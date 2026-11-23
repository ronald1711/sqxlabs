//! LLM assistant layer: talk to any OpenAI-compatible endpoint (e.g. a free
//! provider from awesome-free-llm-apis) and let the model drive the engine
//! through a small tool set.

use rand::{rngs::StdRng, SeedableRng};
use serde_json::{json, Value};
use sqx_backtest::{backtest, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_optimizer::{random_strategy, GeneratorConfig};
use sqx_strategy::Strategy;
use std::io::Read;
use std::time::{Duration, Instant};

// Limits are enforced locally, regardless of provider instructions or tool schema.
const SESSION_TIME: Duration = Duration::from_secs(300);
const MAX_ROUNDS: usize = 16;
const MAX_TOOL_CALLS_PER_ROUND: usize = 8;
const MAX_TOOL_CALLS: usize = 64;
const MAX_BACKTESTS: usize = 200;
const MAX_BARS: usize = 100_000;
const MAX_QUESTION_BYTES: usize = 32_768;

const SYSTEM_PROMPT: &str = "You are the SqxLabs strategy assistant. Use the provided tools to generate strategies and inspect their backtests on the loaded market data. Never invent numbers: call a tool. Answer briefly in the user's language.";

#[derive(Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub max_rounds: usize,
}

impl std::fmt::Debug for LlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmConfig").field("model", &self.model)
            .field("base_url", &"[configured endpoint]").field("api_key", &"[redacted]")
            .field("max_rounds", &self.max_rounds).finish()
    }
}

pub struct Assistant {
    pub cfg: LlmConfig,
    pub symbol: String,
    pub bars: Vec<Bar>,
    pub instrument: Instrument,
    pub periods_per_year: f64,
    backtest_config: BacktestConfig,
    generated: Vec<Strategy>,
    candidate_evaluations: Vec<Value>,
    detail_evaluations: Vec<Value>,
    trial_executions: Vec<Value>,
    trial_archive: Option<TrialArchive>,
    storage_error: Option<String>,
    tool_calls_used: usize,
    backtests_used: usize,
    started_at: Option<Instant>,
}

struct TrialArchive {
    archive: sqx_project::archive::Archive,
    engine_id: String,
    session_id: String,
    source_context: Value,
}

impl Assistant {
    pub fn new(cfg: LlmConfig, symbol: &str, bars: Vec<Bar>, instrument: Instrument, periods_per_year: f64) -> Self {
        Self { cfg, symbol: symbol.to_string(), bars, instrument, periods_per_year, backtest_config: BacktestConfig::default(), generated: Vec::new(), candidate_evaluations: Vec::new(), detail_evaluations: Vec::new(), trial_executions: Vec::new(), trial_archive: None, storage_error: None, tool_calls_used: 0, backtests_used: 0, started_at: None }
    }

    /// Freeze explicit simulation assumptions for this assistant instance.
    pub fn with_backtest_config(mut self, config: BacktestConfig) -> Self {
        self.backtest_config = config;
        self
    }

    /// Enable immutable per-trial checkpoints before execution and full results
    /// immediately afterwards. A storage failure permanently stops this session.
    pub fn with_trial_archive(mut self, archive: sqx_project::archive::Archive, engine_id: String, source_context: Value) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let identity = format!("{}-{:?}-{}", std::process::id(), std::time::SystemTime::now(), NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed));
        self.trial_archive = Some(TrialArchive { archive, engine_id, session_id: sqx_project::archive::content_id(identity.as_bytes()), source_context });
        self
    }

    fn execute_trial(&mut self, tool: &str, index: usize, strategy: &Strategy) -> Result<sqx_backtest::BacktestResult,String> {
        if let Some(error)=&self.storage_error { return Err(error.clone()); }
        let dataset=if self.trial_archive.is_some() {json!({"symbol":self.symbol,"bars":self.bars,"instrument":self.instrument})} else {Value::Null};
        let mut request=json!({"research_kind":"assistant_trial_started","tool":tool,"tool_call":self.tool_calls_used,"index":index,"config":self.backtest_config,"periods_per_year":self.periods_per_year,"seed":7});
        let started=if let Some(journal)=&self.trial_archive {
            request["session_id"]=json!(journal.session_id);
            request["source_context"]=journal.source_context.clone();
            match journal.archive.save(journal.engine_id.clone(),dataset.clone(),json!(strategy),request.clone(),json!({"status":"execution_not_recorded"})) {
                Ok(id)=>Some(id),
                Err(error)=>{let error=format!("Assistant evidence could not be archived before execution: {error}");self.storage_error=Some(error.clone());return Err(error);}
            }
        } else {None};
        let result=backtest(strategy,&self.bars,&self.instrument,&self.backtest_config);
        let mut record=json!({"tool":tool,"tool_call":self.tool_calls_used,"index":index,"strategy":strategy,"result":result});
        if let (Some(journal),Some(started))=(&self.trial_archive,started) {
            request["research_kind"]=json!("assistant_trial");
            request["started_run_id"]=json!(started);
            record["started_run_id"]=json!(started);
            match journal.archive.save(journal.engine_id.clone(),dataset,json!(strategy),request,json!(result)) {
                Ok(id)=>record["run_id"]=json!(id),
                Err(error)=>{
                    let error=format!("Assistant evidence could not be archived after execution (checkpoint {started}): {error}");
                    self.storage_error=Some(error.clone());
                    self.trial_executions.push(record);
                    return Err(error);
                }
            }
        }
        self.trial_executions.push(record);
        Ok(result)
    }

    /// All generation outcomes, in candidate order, before top-five filtering.
    /// Session-local evidence, not a durable archive or a record of details reruns.
    pub fn candidate_evaluations(&self) -> &[Value] { &self.candidate_evaluations }

    /// Every executed details rerun, including failed strategy evaluations.
    pub fn detail_evaluations(&self) -> &[Value] { &self.detail_evaluations }

    /// Full engine results for every executed generation or details trial.
    /// Kept separate from provider-facing summaries. Storage is the caller's responsibility.
    pub fn trial_executions(&self) -> &[Value] { &self.trial_executions }

    pub fn research_context(&self) -> Value {
        json!({"symbol":self.symbol,"bars":self.bars.len(),"config":self.backtest_config,
            "seed":7,"backtests_attempted":self.backtests_used,"trial_checkpointing":self.trial_archive.is_some(),"trial_session_id":self.trial_archive.as_ref().map(|j|&j.session_id),
            "scope":"Exploratory in-sample research on the full supplied dataset; no reserved final test and no independent validation. Full trial/execution records require archive receipts to confirm storage; per-trial checkpoints do not resume the provider conversation."})
    }

    fn remaining_time(&self) -> Result<Duration, String> {
        SESSION_TIME.checked_sub(self.started_at.map(|start|start.elapsed()).unwrap_or_default())
            .filter(|remaining| !remaining.is_zero()).ok_or_else(|| "assistant session time budget exceeded (5 minutes)".into())
    }

    pub fn run(&mut self, question: &str) -> Result<String, String> {
        if let Some(error)=&self.storage_error { return Err(error.clone()); }
        self.started_at.get_or_insert_with(Instant::now);
        self.remaining_time()?;
        if !(1..=MAX_ROUNDS).contains(&self.cfg.max_rounds) {
            return Err(format!("max_rounds must be between 1 and {MAX_ROUNDS}"));
        }
        if question.len() > MAX_QUESTION_BYTES { return Err("question exceeds 32768 bytes".into()); }
        if self.bars.len() > MAX_BARS { return Err("assistant supports at most 100000 bars per session".into()); }
        sqx_backtest::validate_backtest_inputs(&self.bars, &self.instrument, &self.backtest_config)?;
        let tools = tool_defs();
        let mut messages = vec![
            json!({"role": "system", "content": SYSTEM_PROMPT}),
            json!({"role":"system","content":format!("Research assumptions: {}. Report these limitations; do not describe candidates as validated or deployment-ready.",self.research_context())}),
            json!({"role": "user", "content": question}),
        ];
        for _ in 0..self.cfg.max_rounds {
            let body = json!({
                "model": self.cfg.model,
                "messages": messages,
                "tools": tools,
                "tool_choice": "auto",
                "temperature": 0.2,
            });
            let resp = post_json_with_timeout(&self.cfg.base_url, &self.cfg.api_key, &body.to_string(),self.remaining_time()?.min(Duration::from_secs(180)))?;
            self.remaining_time()?;
            let v: Value = serde_json::from_str(&resp).map_err(|e| format!("invalid JSON from LLM: {e}: {}", sanitize(&self.cfg.api_key, &resp)))?;
            // Some providers answer HTTP 200 with an error object; surface it
            // instead of silently returning an empty answer.
            if let Some(err) = v.get("error") {
                let msg = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| err.to_string());
                return Err(format!("LLM error: {}", sanitize(&self.cfg.api_key, &msg)));
            }
            if v["choices"].as_array().map(|a| a.is_empty()).unwrap_or(true) {
                return Err(format!("LLM returned no choices: {}", sanitize(&self.cfg.api_key, &resp)));
            }
            let msg = v["choices"][0]["message"].clone();
            let tool_calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
            if tool_calls.is_empty() {
                let content = msg["content"].as_str().unwrap_or("").to_string();
                if content.trim().is_empty() {
                    return Err("LLM returned an empty answer (check the model name, base URL and key)".to_string());
                }
                return Ok(content);
            }
            if tool_calls.len() > MAX_TOOL_CALLS_PER_ROUND || self.tool_calls_used.saturating_add(tool_calls.len()) > MAX_TOOL_CALLS {
                return Err("assistant tool-call budget exceeded".into());
            }
            messages.push(msg);
            for tc in tool_calls {
                let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                let result = match tc["function"]["arguments"].as_str().and_then(|raw| serde_json::from_str::<Value>(raw).ok()) {
                    Some(args) => self.dispatch(&name, &args),
                    None => { self.tool_calls_used += 1; json!({"error":"tool arguments must be valid JSON"}).to_string() }
                };
                if let Some(error)=&self.storage_error { return Err(error.clone()); }
                messages.push(json!({"role": "tool", "tool_call_id": tc["id"], "content": result}));
            }
        }
        Err(format!("assistant did not finish within {} tool rounds", self.cfg.max_rounds))
    }

    fn reserve_backtests(&mut self, count: usize) -> Result<(), &'static str> {
        if self.bars.len() > MAX_BARS { return Err("assistant supports at most 100000 bars per session"); }
        if self.backtests_used.saturating_add(count) > MAX_BACKTESTS { return Err("assistant backtest budget exceeded (200 per session)"); }
        if sqx_backtest::validate_backtest_inputs(&self.bars, &self.instrument, &self.backtest_config).is_err() { return Err("invalid simulation inputs; check data, costs and account configuration"); }
        self.backtests_used += count;
        Ok(())
    }

    pub fn dispatch(&mut self, name: &str, args: &Value) -> String {
        if let Some(error)=&self.storage_error { return json!({"error":error}).to_string(); }
        self.started_at.get_or_insert_with(Instant::now);
        if let Err(error)=self.remaining_time() { return json!({"error":error}).to_string(); }
        if self.tool_calls_used >= MAX_TOOL_CALLS { return json!({"error":"assistant tool-call budget exceeded"}).to_string(); }
        self.tool_calls_used += 1;
        let Some(object) = args.as_object() else { return json!({"error":"tool arguments must be an object"}).to_string(); };
        let allowed: &[&str] = match name { "generate"=>&["count"], "details"=>&["index"], _=>&[] };
        if object.keys().any(|key| !allowed.contains(&key.as_str())) { return json!({"error":"unknown tool argument"}).to_string(); }
        match name {
            "list_indicators" => {
                let names: Vec<&str> = sqx_strategy::indicator_catalog().iter().map(|s| s.name).collect();
                json!({"indicators": names}).to_string()
            }
            "generate" => {
                let Some(count) = args["count"].as_u64().filter(|n| (1..=200).contains(n)) else {
                    return json!({"error":"count must be an integer between 1 and 200"}).to_string();
                };
                let count = count as usize;
                if let Err(error) = self.reserve_backtests(count) { return json!({"error":error}).to_string(); }
                let mut rng = StdRng::seed_from_u64(7);
                let cfg = GeneratorConfig {
                    min_conditions: 1,
                    max_conditions: 3,
                    sl_points: (0.0, 200.0),
                    pt_points: (0.0, 400.0),
                    fixed_size: 0.1,
                    long: true,
                    short: true,
                    param_ranges: Vec::new(),
                };
                // Candidate indices identify strategies for the entire session,
                // including references in earlier model/tool messages.
                let start_index=self.generated.len();
                self.generated.extend((start_index..start_index+count)
                    .map(|i| random_strategy(&mut rng, &format!("llm-{i:04}"), &cfg)));
                let mut rows = Vec::new();
                for i in start_index..start_index+count {
                    let s=self.generated[i].clone();
                    let row=if let Err(error)=self.remaining_time() {
                        json!({"index":i,"name":s.name,"status":"not_evaluated","error":error})
                    } else {
                        match self.execute_trial("generate",i,&s) {
                            Err(error)=>json!({"index":i,"name":s.name,"status":"storage_error","error":error}),
                            Ok(r) if !r.errors.is_empty()=>json!({"index":i,"name":s.name,"status":"invalid","error":r.errors.join("; ")}),
                            Ok(r)=>{
                                let m=sqx_metrics::compute(&r,self.bars.len(),self.periods_per_year);
                                json!({"index":i,"status":"evaluated","name":s.name,"net_profit":m.net_profit,"profit_factor":m.profit_factor,"trades":r.trades.len(),"max_dd_pct":m.max_drawdown_pct})
                            }
                        }
                    };
                    rows.push(row);
                }
                self.candidate_evaluations.extend(rows.iter().zip(&self.generated[start_index..]).map(|(row,strategy)| {
                    json!({"strategy":strategy,"outcome":row,"seed":7,"generation_start_index":start_index})
                }));
                let invalid:Vec<Value> = rows.iter().filter(|row|row.get("error").is_some()).cloned().collect();
                rows.retain(|row|row.get("error").is_none());
                rows.sort_by(|a, b| {
                    b["net_profit"]
                        .as_f64()
                        .unwrap_or(0.0)
                        .partial_cmp(&a["net_profit"].as_f64().unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                rows.truncate(5);
                json!({"generated": count, "top": rows,"invalid":invalid,"research_context":self.research_context()}).to_string()
            }
            "details" => {
                let Some(idx) = args["index"].as_u64().and_then(|n| usize::try_from(n).ok()) else {
                    return json!({"error":"index must be a non-negative integer"}).to_string();
                };
                if idx >= self.generated.len() { return json!({"error":"index out of range"}).to_string(); }
                if let Err(error) = self.reserve_backtests(1) { return json!({"error":error}).to_string(); }
                let s=self.generated[idx].clone();
                let r=match self.execute_trial("details",idx,&s) {Ok(r)=>r,Err(error)=>return json!({"error":error}).to_string()};
                let outcome=if !r.errors.is_empty() {
                    json!({"research_context":self.research_context(),"index":idx,"strategy":s,"status":"invalid","error":r.errors.join("; ")})
                } else {
                    let m = sqx_metrics::compute(&r, self.bars.len(), self.periods_per_year);
                    json!({
                        "research_context":self.research_context(),"index":idx,"strategy":s,"status":"evaluated",
                        "net_profit":m.net_profit,"profit_factor":m.profit_factor,"max_drawdown_pct":m.max_drawdown_pct,
                        "trades":r.trades.len(),"winning_pct":m.winning_pct,"sharpe":m.sharpe_ratio,"sqn":m.sqn
                    })
                };
                self.detail_evaluations.push(json!({"tool_call":self.tool_calls_used,"outcome":outcome}));
                outcome.to_string()
            }
            _ => json!({"error": format!("unknown tool {name}")}).to_string(),
        }
    }
}

const MAX_RESPONSE_BYTES: usize = 1_048_576;
const MAX_REQUEST_BYTES: usize = 4_194_304;

#[cfg(test)]
fn post_json(base_url: &str, api_key: &str, body: &str) -> Result<String, String> {
    post_json_with_timeout(base_url,api_key,body,Duration::from_secs(180))
}

fn post_json_with_timeout(base_url: &str, api_key: &str, body: &str, timeout: Duration) -> Result<String, String> {
    if timeout.is_zero() { return Err("assistant session time budget exceeded".into()); }
    if body.len() > MAX_REQUEST_BYTES { return Err("LLM request exceeds 4 MiB".into()); }
    if api_key.bytes().any(|b| b < 32 || b == 127) { return Err("API key contains invalid control characters".into()); }
    let mut url=url::Url::parse(base_url).map_err(|_| "invalid LLM endpoint URL".to_string())?;
    if !matches!(url.scheme(),"http"|"https") || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err("LLM endpoint must be an HTTP(S) URL without embedded credentials or a fragment".into());
    }
    if !url.path().trim_end_matches('/').ends_with("/chat/completions") {
        url.set_path(&format!("{}/chat/completions",url.path().trim_end_matches('/')));
    }
    // No subprocess or command-line credential. Do not follow provider redirects,
    // even to the same host: endpoint binding must not silently change.
    let agent=ureq::AgentBuilder::new().redirects(0)
        .timeout_connect(timeout.min(Duration::from_secs(10))).timeout(timeout).build();
    let mut request=agent.post(url.as_str()).set("Content-Type","application/json");
    if !api_key.is_empty() { request=request.set("Authorization",&format!("Bearer {api_key}")); }
    let response=match request.send_string(body) {
        Ok(response)=>response,
        Err(ureq::Error::Status(_,response))=>response,
        Err(ureq::Error::Transport(_))=>return Err("LLM transport failed (connection, TLS or timeout); check the endpoint".into()),
    };
    let status=response.status();
    if (300..400).contains(&status) { return Err(format!("LLM redirect refused (HTTP {status}); configure the final endpoint explicitly")); }
    let mut bytes=Vec::new();
    response.into_reader().take((MAX_RESPONSE_BYTES+1) as u64).read_to_end(&mut bytes)
        .map_err(|_| "LLM response could not be read completely".to_string())?;
    if bytes.len()>MAX_RESPONSE_BYTES { return Err("LLM response exceeds 1 MiB".into()); }
    let text=String::from_utf8(bytes).map_err(|_| "LLM response is not valid UTF-8".to_string())?;
    if !(200..300).contains(&status) { return Err(format!("LLM request failed (HTTP {status}): {}",sanitize(api_key,&text))); }
    Ok(text)
}

/// Redact a credential from a diagnostic string and cap its length, so a
/// provider's response body can never leak the API key or flood the log.
fn sanitize(secret: &str, text: &str) -> String {
    let t = if secret.is_empty() { text.to_string() } else { text.replace(secret, "***") };
    // Cap at 500 CHARS (not bytes): String::truncate requires a UTF-8 char
    // boundary and would panic on a multibyte sequence straddling the cut.
    if t.chars().count() > 500 {
        let mut s: String = t.chars().take(500).collect();
        s.push('…');
        s
    } else {
        t
    }
}

fn tool_defs() -> Value {
    json!([
        {"type": "function", "function": {
            "name": "list_indicators",
            "description": "List the available indicator names.",
            "parameters": {"type": "object", "properties": {}, "required": []}
        }},
        {"type": "function", "function": {
            "name": "generate",
            "description": "Generate random strategies on the loaded data and backtest them; returns the top five by net profit.",
            "parameters": {"type": "object", "properties": {"count": {"type": "integer", "description": "number of strategies, 1-200", "minimum":1,"maximum":200}}, "required": ["count"], "additionalProperties":false}
        }},
        {"type": "function", "function": {
            "name": "details",
            "description": "Backtest one previously generated strategy by index and return its metrics and definition.",
            "parameters": {"type": "object", "properties": {"index": {"type": "integer", "description": "session-stable index from a generate result; earlier indices remain valid after another generate call"}}, "required": ["index"], "additionalProperties":false}
        }}
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn list_indicators_tool_returns_names() {
        let bars = vec![Bar::new(0, 1.0, 1.0, 1.0, 1.0, 1.0)];
        let cfg = LlmConfig { base_url: "http://x".into(), model: "m".into(), api_key: "k".into(), max_rounds: 1 };
        let mut a = Assistant::new(cfg, "EURUSD", bars, Instrument::fx("EURUSD"), 252.0);
        let out = a.dispatch("list_indicators", &json!({}));
        assert!(out.contains("RSI"));
    }

    fn bounded_assistant() -> Assistant {
        Assistant::new(LlmConfig{base_url:"http://127.0.0.1:1".into(),model:"test".into(),api_key:"".into(),max_rounds:1},"EURUSD",vec![Bar::new(0,1.0,1.0,1.0,1.0,1.0)],Instrument::fx("EURUSD"),252.0)
    }

    #[test]
    fn details_use_explicit_simulation_costs_and_report_their_scope() {
        use sqx_strategy::{Signal,SignalKind,Condition,Operand,PriceField,Comparator,Join,MoneyManagement};
        let mut strategy=Strategy::new("cost fixture");strategy.long=true;strategy.short=false;
        strategy.money_management=MoneyManagement::FixedSize{size:0.1};
        strategy.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Price(PriceField::Close,0),Comparator::Gt,Operand::Const(0.0))],Join::And));
        let cfg=BacktestConfig{initial_deposit:4321.0,spread_points:20.0,slippage_points:3.0,commission_per_lot:7.0,..Default::default()};
        let mut a=bounded_assistant().with_backtest_config(cfg.clone());
        a.bars=(0..6).map(|i|{let p=1.1+i as f64*0.001;Bar::new(1700000000000+i*60000,p,p+0.0005,p-0.0005,p+0.0002,1.0)}).collect();
        let expected=backtest(&strategy,&a.bars,&a.instrument,&cfg);
        assert_eq!(expected.trades.len(),1);assert!(expected.errors.is_empty());
        let default=backtest(&strategy,&a.bars,&a.instrument,&BacktestConfig::default());
        assert!(expected.net_profit()<default.net_profit());
        a.generated.push(strategy);
        let result:Value=serde_json::from_str(&a.dispatch("details",&json!({"index":0}))).unwrap();
        assert_eq!(result["net_profit"].as_f64().unwrap(),expected.net_profit());
        assert_eq!(result["research_context"]["config"],serde_json::to_value(cfg).unwrap());
        assert!(result["research_context"]["scope"].as_str().unwrap().contains("no reserved final test"));
    }

    #[test]
    fn generation_evidence_keeps_candidates_outside_top_five_and_session_failure() {
        let mut a=bounded_assistant();
        let first:Value=serde_json::from_str(&a.dispatch("generate",&json!({"count":8}))).unwrap();
        assert_eq!(first["top"].as_array().unwrap().len(),5);
        assert_eq!(a.candidate_evaluations().len(),8);
        a.dispatch("generate",&json!({"count":2}));
        assert_eq!(a.candidate_evaluations().len(),10);
        for (index,evidence) in a.candidate_evaluations().iter().enumerate() {
            assert_eq!(evidence["outcome"]["index"],index);
            assert_eq!(evidence["outcome"]["status"],"evaluated");
            assert_eq!(evidence["strategy"],serde_json::to_value(&a.generated[index]).unwrap());
            assert_eq!(evidence["generation_start_index"],if index<8{0}else{8});
        }
        assert_eq!(a.trial_executions().len(),10);
        for (index,record) in a.trial_executions().iter().enumerate() {
            assert_eq!(record["tool"],"generate");
            assert_eq!(record["index"],index);
            let expected=backtest(&a.generated[index],&a.bars,&a.instrument,&a.backtest_config);
            assert_eq!(record["result"],serde_json::to_value(expected).unwrap());
            assert!(record.get("api_key").is_none());
        }
        assert!(first["top"].as_array().unwrap().iter().all(|row|row.get("result").is_none()),"full records must not enter provider-facing tool summaries");
        let before=a.candidate_evaluations().to_vec();
        // Deterministic pre-provider failure must not erase completed evidence.
        a.cfg.max_rounds=0;
        assert!(a.run("continue").is_err());
        assert_eq!(a.candidate_evaluations(),before);
        a.dispatch("generate",&json!({"count":0}));
        assert_eq!(a.candidate_evaluations(),before);
    }

    #[test]
    fn details_evidence_records_each_execution_and_preserves_failures(){
        let mut a=bounded_assistant();a.dispatch("generate",&json!({"count":1}));
        for _ in 0..2 {let value:Value=serde_json::from_str(&a.dispatch("details",&json!({"index":0}))).unwrap();assert_eq!(value["status"],"evaluated");}
        assert_eq!(a.detail_evaluations().len(),2);
        assert!(a.detail_evaluations()[0]["tool_call"].as_u64()<a.detail_evaluations()[1]["tool_call"].as_u64());
        a.generated[0].money_management=sqx_strategy::MoneyManagement::FixedSize{size:0.0};
        let failed:Value=serde_json::from_str(&a.dispatch("details",&json!({"index":0}))).unwrap();
        assert_eq!(failed["status"],"invalid");assert!(failed["error"].is_string());
        assert_eq!(a.detail_evaluations()[2]["outcome"],failed);
        a.dispatch("details",&json!({"index":99}));
        assert_eq!(a.detail_evaluations().len(),3,"invalid index does not execute a backtest");
        assert_eq!(a.backtests_used,4);
        assert_eq!(a.trial_executions().len(),4);
        assert_eq!(a.trial_executions()[3]["tool"],"details");
        assert_eq!(a.trial_executions()[3]["index"],0);
        assert!(!a.trial_executions()[3]["result"]["errors"].as_array().unwrap().is_empty());
        assert_eq!(a.trial_executions()[3]["strategy"],json!(a.generated[0]));
    }

    #[test]
    fn trial_archive_failure_prevents_execution_and_permanently_stops_session() {
        let root=std::env::temp_dir().join(format!("sqx-ai-storage-{}-{:?}",std::process::id(),std::time::SystemTime::now()));
        std::fs::write(&root,"blocked destination").unwrap();
        let mut a=bounded_assistant().with_trial_archive(sqx_project::archive::Archive::new(&root),"fixture-engine".into(),json!({"fixture":true}));
        let response:Value=serde_json::from_str(&a.dispatch("generate",&json!({"count":2}))).unwrap();
        assert_eq!(response["top"].as_array().unwrap().len(),0);
        assert_eq!(a.trial_executions().len(),0,"checkpoint failure must precede engine execution");
        assert!(a.storage_error.as_ref().unwrap().contains("before execution"));
        std::fs::remove_file(&root).unwrap();
        // Repairing storage must not silently continue a session after a lost record.
        let response:Value=serde_json::from_str(&a.dispatch("details",&json!({"index":0}))).unwrap();
        assert!(response["error"].as_str().unwrap().contains("could not be archived"));
        assert!(a.run("continue").unwrap_err().contains("could not be archived"));
        assert!(!root.exists());
        assert_eq!(a.trial_executions().len(),0);
    }

    #[test]
    fn trial_archive_retains_invalid_results_with_their_checkpoint() {
        let root=std::env::temp_dir().join(format!("sqx-ai-invalid-{}-{:?}",std::process::id(),std::time::SystemTime::now()));
        let archive=sqx_project::archive::Archive::new(&root);
        let mut a=bounded_assistant().with_trial_archive(archive.clone(),"fixture-engine".into(),json!({"fixture":true}));
        a.dispatch("generate",&json!({"count":1}));
        a.generated[0].money_management=sqx_strategy::MoneyManagement::FixedSize{size:0.0};
        a.dispatch("details",&json!({"index":0}));
        let record=&a.trial_executions()[1];
        let result=archive.load(record["run_id"].as_str().unwrap()).unwrap();
        assert!(!result.run.result["errors"].as_array().unwrap().is_empty());
        assert_eq!(result.run.result,record["result"]);
        let checkpoint=archive.load(record["started_run_id"].as_str().unwrap()).unwrap();
        assert_eq!(checkpoint.strategy,json!(a.generated[0]));
        assert_eq!(checkpoint.run.result["status"],"execution_not_recorded");
        assert_eq!(archive.list().unwrap().len(),4);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_indices_remain_stable_across_generation_calls() {
        let mut a=bounded_assistant();
        let first:Value=serde_json::from_str(&a.dispatch("generate",&json!({"count":2}))).unwrap();
        assert_eq!(first["generated"],2);
        let prior=serde_json::to_value(&a.generated).unwrap();
        let second:Value=serde_json::from_str(&a.dispatch("generate",&json!({"count":1}))).unwrap();
        assert_eq!(a.generated.len(),3,"later searches must retain earlier candidates");
        assert_eq!(serde_json::to_value(&a.generated[..2]).unwrap(),prior);
        assert_eq!(second["top"][0]["index"],2);
        let old:Value=serde_json::from_str(&a.dispatch("details",&json!({"index":1}))).unwrap();
        assert_eq!(old["strategy"],prior[1]);
        let new:Value=serde_json::from_str(&a.dispatch("details",&json!({"index":2}))).unwrap();
        assert_eq!(new["strategy"]["name"],"llm-0002");
    }

    #[test]
    fn invalid_simulation_assumptions_are_not_ranked_as_zero_profit() {
        let mut a=bounded_assistant().with_backtest_config(BacktestConfig{spread_points:-1.0,..Default::default()});
        let result:Value=serde_json::from_str(&a.dispatch("generate",&json!({"count":1}))).unwrap();
        assert!(result["error"].is_string());assert!(result.get("top").is_none());assert_eq!(a.backtests_used,0);
        assert!(a.run("test").is_err());
    }

    #[test]
    fn expired_session_cannot_restart_through_run_or_dispatch() {
        let mut a=bounded_assistant();
        a.started_at=Some(Instant::now()-SESSION_TIME-Duration::from_secs(1));
        assert!(a.run("test").unwrap_err().contains("time budget exceeded"));
        let result:Value=serde_json::from_str(&a.dispatch("generate",&json!({"count":1}))).unwrap();
        assert!(result["error"].as_str().unwrap().contains("time budget exceeded"));
        assert_eq!(a.backtests_used,0);assert!(a.generated.is_empty());
        assert!(post_json_with_timeout("http://127.0.0.1:1","","{}",Duration::ZERO).unwrap_err().contains("time budget exceeded"));
    }

    #[test]
    fn invalid_arguments_never_start_or_replace_a_search() {
        let mut a=bounded_assistant();
        for args in [json!({}),json!({"count":0}),json!({"count":201}),json!({"count":-1}),json!({"count":1.5}),json!({"count":"2"}),json!({"count":1,"extra":true}),json!([])] {
            let result:Value=serde_json::from_str(&a.dispatch("generate",&args)).unwrap();
            assert!(result["error"].is_string(),"{args}: {result}");
        }
        assert_eq!(a.backtests_used,0);assert!(a.generated.is_empty());
        a.dispatch("generate",&json!({"count":1}));
        let before=serde_json::to_value(&a.generated).unwrap();
        a.dispatch("generate",&json!({"count":0}));
        assert_eq!(serde_json::to_value(&a.generated).unwrap(),before);
        let result:Value=serde_json::from_str(&a.dispatch("details",&json!({}))).unwrap();
        assert!(result["error"].is_string());assert_eq!(a.backtests_used,1);
    }

    #[test]
    fn aggregate_backtest_budget_includes_details_and_cannot_be_reset_by_generate() {
        let mut a=bounded_assistant();
        let first:Value=serde_json::from_str(&a.dispatch("generate",&json!({"count":199}))).unwrap();
        assert_eq!(first["generated"],199);
        let detail:Value=serde_json::from_str(&a.dispatch("details",&json!({"index":0}))).unwrap();
        assert!(detail.get("strategy").is_some());assert_eq!(a.backtests_used,200);
        for (tool,args) in [("generate",json!({"count":1})),("details",json!({"index":0}))] {
            let result:Value=serde_json::from_str(&a.dispatch(tool,&args)).unwrap();
            assert!(result["error"].as_str().unwrap().contains("budget exceeded"));
        }
        assert_eq!(a.generated.len(),199);assert_eq!(a.backtests_used,200);
    }

    #[test]
    fn tool_budget_counts_invalid_and_read_only_calls() {
        let mut a=bounded_assistant();
        for _ in 0..MAX_TOOL_CALLS { a.dispatch("unknown",&json!({})); }
        let result:Value=serde_json::from_str(&a.dispatch("list_indicators",&json!({}))).unwrap();
        assert!(result["error"].as_str().unwrap().contains("budget exceeded"));
        assert_eq!(a.backtests_used,0);
    }

    #[test]
    fn oversized_inputs_are_refused_before_network_or_computation() {
        let mut a=bounded_assistant();
        a.cfg.max_rounds=MAX_ROUNDS+1;
        assert!(a.run("test").unwrap_err().contains("max_rounds"));
        a.cfg.max_rounds=1;
        assert!(a.run(&"x".repeat(MAX_QUESTION_BYTES+1)).unwrap_err().contains("question exceeds"));
        a.bars=vec![a.bars[0];MAX_BARS+1];
        assert!(a.run("test").unwrap_err().contains("100000 bars"));
        let result:Value=serde_json::from_str(&a.dispatch("generate",&json!({"count":1}))).unwrap();
        assert!(result["error"].as_str().unwrap().contains("100000 bars"));
        assert_eq!(a.backtests_used,0);
    }

    #[test]
    fn sanitize_redacts_the_secret_and_truncates() {
        assert_eq!(sanitize("sk-abc", "err: sk-abc leaked"), "err: *** leaked");
        assert_eq!(sanitize("", "no secret"), "no secret");
        let long = "x".repeat(1000);
        let s = sanitize("", &long);
        assert!(s.chars().count() == 501, "500 chars + ellipsis, got {}", s.chars().count());
        assert!(s.ends_with('…'));
        // Multibyte straddling the cut must not panic and must stay valid UTF-8.
        let mb = "é".repeat(600);
        let s2 = sanitize("", &mb);
        assert!(s2.chars().count() == 501, "got {} chars", s2.chars().count());
        assert!(s2.ends_with('…'));
    }

    /// Spin up a one-shot HTTP server that returns a canned response, then call
    /// post_json against it. Returns the post_json result.
    fn post_json_against(status_line: &str, body: &str) -> Result<String, String> {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = body.to_string();
        let status = status_line.to_string();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut received=Vec::new();
            loop {
                let mut buf=[0u8;4096];
                let n=stream.read(&mut buf).unwrap();
                assert!(n>0,"client closed before completing request");
                received.extend_from_slice(&buf[..n]);
                assert!(received.len()<65536,"unexpected fixture request size");
                if let Some(end)=received.windows(4).position(|w|w==b"\r\n\r\n") {
                    let headers=std::str::from_utf8(&received[..end]).unwrap();
                    let length=headers.lines().filter_map(|line|line.split_once(':'))
                        .find(|(name,_)|name.eq_ignore_ascii_case("content-length"))
                        .map(|(_,value)|value.trim().parse::<usize>().unwrap()).unwrap_or(0);
                    if received.len()>=end+4+length { break; }
                }
            }
            let resp = format!(
                "{status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        });
        let url = format!("http://{addr}");
        let r = post_json(&url, "sk-test", "{}");
        handle.join().unwrap();
        r
    }

    #[test]
    fn transport_caps_success_and_error_bodies() {
        let oversized="x".repeat(MAX_RESPONSE_BYTES+1);
        for status in ["HTTP/1.1 200 OK","HTTP/1.1 500 Internal Server Error"] {
            assert!(post_json_against(status,&oversized).unwrap_err().contains("exceeds 1 MiB"));
        }
        assert!(post_json("http://127.0.0.1:1","",&"x".repeat(MAX_REQUEST_BYTES+1)).unwrap_err().contains("exceeds 4 MiB"));
    }

    #[test]
    fn transport_refuses_redirect_without_contacting_destination() {
        let destination=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let status=format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/leak",destination.local_addr().unwrap());
        assert!(post_json_against(&status,"").unwrap_err().contains("redirect refused"));
        assert_eq!(destination.accept().unwrap_err().kind(),std::io::ErrorKind::WouldBlock);
    }

    #[test]
    fn transport_redacts_errors_and_rejects_unsafe_endpoint_syntax() {
        assert!(!post_json_against("HTTP/1.1 401 Unauthorized","bad key sk-test").unwrap_err().contains("sk-test"));
        for endpoint in ["file:///tmp/private","http://user:secret@localhost/","http://localhost/#fragment"] {
            assert!(post_json(endpoint,"secret","{}").is_err());
        }
        assert!(post_json("http://localhost/","bad\r\nInjected: true","{}").unwrap_err().contains("control characters"));
        let mut a=bounded_assistant();a.cfg.api_key="private-credential".into();
        assert!(!format!("{:?}",a.cfg).contains("private-credential"));
    }

    #[test]
    fn post_json_accepts_http_200() {
        let r = post_json_against("HTTP/1.1 200 OK", "{\"choices\":[{\"message\":{\"content\":\"hi\"}}]}");
        assert!(r.is_ok(), "HTTP 200 must succeed: {r:?}");
        assert!(r.unwrap().contains("hi"));
    }

    #[test]
    fn post_json_fails_on_http_500() {
        // HTTP error bodies remain available as bounded, redacted diagnostics.
        let r = post_json_against("HTTP/1.1 500 Internal Server Error", "{\"error\":{\"message\":\"boom\"}}");
        assert!(r.is_err(), "HTTP 500 must fail, got {r:?}");
        assert!(r.unwrap_err().contains("boom"), "error should carry the response body");
    }
}
