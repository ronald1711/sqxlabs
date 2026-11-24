//! # sqx-mcp
//!
//! A minimal Model Context Protocol (MCP) server over stdio (JSON-RPC 2.0),
//! exposing the SqxLabs engine as MCP tools so an LLM client can backtest,
//! generate, build portfolios and run robustness checks — from CSV files or
//! directly from a QuestDB instance (the `candles` table).

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use sqx_backtest::{backtest, BacktestConfig};
use sqx_core::{Bar, Instrument, Timeframe};
use sqx_data::{read_csv_bars, DataStore, QuestdbClient, QuestdbConfig};
use sqx_metrics::{compute as compute_metrics, Metrics};
use sqx_optimizer::{rank, random_strategy, walk_forward, Fitness, GeneratorConfig};
use sqx_robustness::{monte_carlo_retest, monte_carlo_trade_manipulation, what_if};
use sqx_strategy::Strategy;

mod generation_archive;

const PROTOCOL: &str = "2024-11-05";

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                writeln!(stdout, "{}", error_json(Value::Null, -32700, "parse error")).ok();
                stdout.flush().ok();
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let has_id = req.get("id").is_some();
        match method {
            "initialize" => send(&mut stdout, id, json!({
                "protocolVersion": PROTOCOL,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "sqxlabs", "version": "0.1.0" }
            })),
            "notifications/initialized" => {},
            "tools/list" => send(&mut stdout, id, json!({ "tools": tools_list() })),
            "tools/call" => {
                let params = req.get("params").cloned().unwrap_or(json!({}));
                let result = call_tool(&params);
                send(&mut stdout, id, result);
            }
            _ => {
                if has_id {
                    send_error(&mut stdout, id, -32601, "method not found");
                }
            }
        }
    }
}

fn send(out: &mut std::io::Stdout, id: Value, result: Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "result": result });
    writeln!(out, "{}", msg).ok();
    out.flush().ok();
}

fn send_error(out: &mut std::io::Stdout, id: Value, code: i64, message: &str) {
    let msg = error_json(id, code, message);
    writeln!(out, "{}", msg).ok();
    out.flush().ok();
}

fn error_json(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tools_list() -> Vec<Value> {
    let mut tools=vec![
        json!({"name":"generation_resume","description":"Replay recorded candidates and finish missing candidates of one frozen MCP search. Requires the original engine; refuses an active search. Repeated calls verify completed work without creating a new search. Completion is coverage, not independent validation.","inputSchema":{"type":"object","properties":{"search_id":{"type":"string"}},"required":["search_id"],"additionalProperties":false}}),
        json!({"name":"generation_trial","description":"Replay a saved MCP generation trial result, or explicitly restart one trial checkpoint with restart=true. Requires the original running engine; never executes stored binaries, advances other candidates or completes a search.","inputSchema":{"type":"object","properties":{"run_id":{"type":"string"},"restart":{"type":"boolean","default":false}},"required":["run_id"],"additionalProperties":false}}),
        json!({"name":"generation_import","description":"Validate and restore a whole generation backup to the operator-configured archive, preserving conflicts and incomplete status. Does not execute strategies or restore engine binaries. Storage errors can leave a partial import; retry the same backup.","inputSchema":{"type":"object","properties":{"backup":{"type":"object"}},"required":["backup"],"additionalProperties":false}}),
        json!({"name":"generation_export","description":"Export all linked records of one frozen generation search under its writer lock. Includes failed/rejected candidates and incomplete status; excludes engine binaries and unrelated experiments. Does not execute or independently validate strategies.","inputSchema":{"type":"object","properties":{"search_id":{"type":"string"}},"required":["search_id"],"additionalProperties":false}}),
        json!({"name":"generation_history","description":"Inspect saved MCP generation searches and recorded trial progress without executing strategies. Pending records do not prove process liveness. The storage directory is configured by the server operator.","inputSchema":{"type":"object","properties":{}}}),
        json!({
            "name": "list_indicators",
            "description": "List the indicator building-block catalog.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "list_symbols",
            "description": "List symbols (CSV files) in a data directory.",
            "inputSchema": { "type": "object", "properties": { "data_dir": { "type": "string" } }, "required": ["data_dir"] }
        }),
        json!({
            "name": "questdb_symbols",
            "description": "List the symbols available in QuestDB (the candles table).",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "questdb_bars",
            "description": "Fetch OHLCV bars for a symbol/timeframe from QuestDB and return them as CSV text.",
            "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" }, "timeframe": { "type": "string" }, "limit": { "type": "integer" } }, "required": ["symbol"] }
        }),
        json!({
            "name": "backtest",
            "description": "Backtest a strategy JSON on CSV (data path, symbol and timeframe required) or QuestDB (symbol, optional timeframe default M5).",
            "inputSchema": { "type": "object", "properties": { "source": {"type":"string","enum":["csv","questdb"],"description":"CSV when data is present; otherwise QuestDB. CSV requires explicit symbol and timeframe."}, "config": { "type": "object", "description": "BacktestConfig: initial_deposit, spread_points, slippage_points, commission_per_lot and account (currency, max_leverage, stop_out_ratio, financing and historical conversion_rates)." }, "data": { "type": "string" }, "symbol": { "type": "string" }, "timeframe": { "type": "string" }, "limit": { "type": "integer" }, "strategy": { "type": "object" } }, "required": ["strategy"] }
        }),
        json!({
            "name": "generate",
            "description": "Generate random strategies, backtest and rank them on CSV or QuestDB data.",
            "inputSchema": { "type": "object", "properties": { "source": {"type":"string","enum":["csv","questdb"],"description":"CSV when data is present; otherwise QuestDB. CSV requires explicit symbol and timeframe."}, "config": { "type": "object", "description": "BacktestConfig: initial_deposit, spread_points, slippage_points, commission_per_lot and account (currency, max_leverage, stop_out_ratio, financing and historical conversion_rates)." }, "data": { "type": "string" }, "symbol": { "type": "string" }, "timeframe": { "type": "string" }, "limit": { "type": "integer" }, "count": { "type": "integer" }, "seed": { "type": "integer" } } }
        }),
        json!({
            "name": "portfolio",
            "description": "Build a portfolio from generated strategies (best subset, correlation) on CSV or QuestDB data.",
            "inputSchema": { "type": "object", "properties": { "source": {"type":"string","enum":["csv","questdb"],"description":"CSV when data is present; otherwise QuestDB. CSV requires explicit symbol and timeframe."}, "config": { "type": "object", "description": "BacktestConfig: initial_deposit, spread_points, slippage_points, commission_per_lot and account (currency, max_leverage, stop_out_ratio, financing and historical conversion_rates)." }, "data": { "type": "string" }, "symbol": { "type": "string" }, "timeframe": { "type": "string" }, "limit": { "type": "integer" }, "count": { "type": "integer" }, "seed": { "type": "integer" } } }
        }),
        json!({
            "name": "robustness",
            "description": "Run the robustness cross-check set (what-if, MC trade, MC retest, walk-forward) on CSV or QuestDB data.",
            "inputSchema": { "type": "object", "properties": { "source": {"type":"string","enum":["csv","questdb"],"description":"CSV when data is present; otherwise QuestDB. CSV requires explicit symbol and timeframe."}, "config": { "type": "object", "description": "BacktestConfig: initial_deposit, spread_points, slippage_points, commission_per_lot and account (currency, max_leverage, stop_out_ratio, financing and historical conversion_rates)." }, "data": { "type": "string" }, "symbol": { "type": "string" }, "timeframe": { "type": "string" }, "limit": { "type": "integer" }, "strategy": { "type": "object" } }, "required": ["strategy"] }
        }),
    ];
    for tool in &mut tools {
        if matches!(tool["name"].as_str(),Some("backtest"|"generate"|"portfolio"|"robustness")) {
            let schema=&mut tool["inputSchema"];
            let mut required=schema["required"].as_array().cloned().unwrap_or_default();
            required.push(json!("symbol"));schema["required"]=json!(required);
            schema["properties"]["calendar"]=json!({"type":"object","description":"Explicit UTC market calendar: name, weekly sessions (weekday Monday=0, start_minute, end_minute), optional date_overrides."});
            schema["properties"]["allow_data_gaps"]=json!({"type":"boolean","default":false,"description":"Acknowledge unresolved gaps for exploratory research; never overrides invalid or incomplete candles."});
            schema["allOf"]=json!([{"if":{"required":["data"]},"then":{"required":["timeframe"]}}]);
        }
    }
    tools
}

fn text_result(s: String) -> Value {
    json!({ "content": [ { "type": "text", "text": s } ] })
}

fn err_result(s: String) -> Value {
    json!({ "isError": true, "content": [ { "type": "text", "text": s } ] })
}

fn call_tool(params: &Value) -> Value {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    match name {
        "list_indicators" => {
            let names: Vec<String> = sqx_strategy::indicator_catalog().iter().map(|s| s.name.to_string()).collect();
            text_result(names.join(", "))
        }
        "list_symbols" => tool_list_symbols(&args),
        "questdb_symbols" => tool_questdb_symbols(),
        "questdb_bars" => tool_questdb_bars(&args),
        "backtest" => tool_backtest(&args),
        "generate" => tool_generate(&args),
        "generation_resume" => {
            if !args.as_object().is_some_and(|object|object.len()==1 && object.contains_key("search_id")){return err_result("Only search_id is accepted".into());}
            let id=match args["search_id"].as_str(){Some(id)=>id,None=>return err_result("search_id must be a string".into())};
            match generation_archive::resume_search(&generation_archive::configured_archive(),id) {
                Ok(report)=>{let invalid=report["invalid"].as_u64().unwrap_or(1)>0;let mut result=text_result(report.to_string());if invalid{result["isError"]=json!(true);}result},
                Err(error)=>err_result(error),
            }
        },
        "generation_trial" => {
            if !args.as_object().is_some_and(|object|object.keys().all(|key|matches!(key.as_str(),"run_id"|"restart"))) {
                return err_result("Only run_id and restart are accepted; frozen inputs cannot be overridden".into());
            }
            let id=match args.get("run_id").and_then(Value::as_str){Some(id)=>id,None=>return err_result("run_id is required".into())};
            let restart=match args.get("restart"){None=>false,Some(Value::Bool(value))=>*value,_=>return err_result("restart must be a boolean".into())};
            match generation_archive::replay_trial(&generation_archive::configured_archive(),id,restart) {
                Ok(report)=>{let invalid=report["matches"]==false || report["execution_valid"]==false;let mut result=text_result(report.to_string());if invalid{result["isError"]=json!(true);}result},
                Err(error)=>err_result(error),
            }
        },
        "generation_import" => {
            if !args.as_object().is_some_and(|object|object.len()==1 && object.contains_key("backup")){return err_result("Only backup is accepted".into());}
            match generation_archive::import_search(&generation_archive::configured_archive(),&args["backup"]) {
                Ok(report)=>text_result(report.to_string()),Err(error)=>err_result(error),
            }
        },
        "generation_export" => {
            if !args.as_object().is_some_and(|object|object.len()==1 && object.contains_key("search_id")){return err_result("Only search_id is accepted".into());}
            let id=match args["search_id"].as_str(){Some(id)=>id,None=>return err_result("search_id must be a string".into())};
            match generation_archive::export_compact(&generation_archive::configured_archive(),id) {
                Ok(backup)=>text_result(backup.to_string()),Err(error)=>err_result(error),
            }
        },
        "generation_history" => match generation_archive::history(&generation_archive::configured_archive()) {
            Ok(history)=>text_result(history.to_string()),Err(error)=>err_result(error),
        },
        "portfolio" => tool_portfolio(&args),
        "robustness" => tool_robustness(&args),
        _ => err_result(format!("unknown tool: {name}")),
    }
}

fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn tool_list_symbols(args: &Value) -> Value {
    let dir = match str_arg(args, "data_dir") {
        Some(d) => d,
        None => return err_result("missing data_dir".to_string()),
    };
    let mut store = DataStore::new();
    if let Err(e) = store.import_dir(&dir, Timeframe::M1) {
        return err_result(e);
    }
    let list: Vec<String> = store.list().iter().map(|s| format!("{} ({} bars)", s.symbol, s.bars)).collect();
    text_result(list.join("
"))
}

fn tool_questdb_symbols() -> Value {
    let client = QuestdbClient::new(&QuestdbConfig::from_env());
    match client.list_symbols() {
        Ok(syms) => text_result(syms.join("
")),
        Err(e) => err_result(e),
    }
}

fn tool_questdb_bars(args: &Value) -> Value {
    let symbol = match str_arg(args, "symbol") {
        Some(s) => s,
        None => return err_result("missing symbol".to_string()),
    };
    let tf: Timeframe = match str_arg(args, "timeframe").unwrap_or_else(|| "M5".to_string()).parse() {
        Ok(t) => t,
        Err(e) => return err_result(format!("bad timeframe: {e}")),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5000).clamp(1, 200_000) as usize;
    let client = QuestdbClient::new(&QuestdbConfig::from_env());
    let bars = match client.fetch_bars(&symbol, tf, None, None, Some(limit)) {
        Ok(b) => b,
        Err(e) => return err_result(e),
    };
    let mut out = String::from("timestamp,open,high,low,close,volume
");
    for b in &bars {
        out.push_str(&format!("{},{},{},{},{},{}
", b.time / 1000, b.open, b.high, b.low, b.close, b.volume));
    }
    text_result(out)
}

/// A local path selects CSV; symbol describes the instrument in either source.
#[derive(Debug,PartialEq)]
struct SourceRequest { data:Option<String>, symbol:String, timeframe:Timeframe }
fn source_request(args:&Value)->Result<SourceRequest,String> {
    let field=|key:&str|->Result<Option<String>,String>{
        match args.get(key) {
            None=>Ok(None),
            Some(Value::String(s)) if !s.trim().is_empty()=>Ok(Some(s.trim().to_string())),
            _=>Err(format!("{key} must be a nonempty string")),
        }
    };
    let data=field("data")?;
    let source=field("source")?;
    match source.as_deref() {
        Some("csv") if data.is_none()=>return Err("CSV source requires data path".into()),
        Some("questdb") if data.is_some()=>return Err("QuestDB source conflicts with CSV data path".into()),
        Some("csv"|"questdb")|None=>{},
        Some(_)=>return Err("source must be csv or questdb".into()),
    }
    let symbol=field("symbol")?.ok_or("symbol is required for the source instrument")?;
    let timeframe=match field("timeframe")? {
        Some(label)=>label.parse().map_err(|e|format!("bad timeframe: {e}"))?,
        None if data.is_some()=>return Err("CSV source requires an explicit timeframe".into()),
        None=>Timeframe::M5,
    };
    if timeframe==Timeframe::Tick{return Err("Tick cannot describe OHLC source candles".into());}
    Ok(SourceRequest{data,symbol,timeframe})
}

fn resolve_bars(args:&Value)->Result<(Vec<Bar>,String,Timeframe,Value),String>{
    let request=source_request(args)?;
    let bars=if let Some(data)=&request.data {
        read_csv_bars(data)?
    }else{
        let limit=args.get("limit").and_then(|v|v.as_u64()).unwrap_or(5000).clamp(1,200000) as usize;
        QuestdbClient::new(&QuestdbConfig::from_env()).fetch_bars(&request.symbol,request.timeframe,None,None,Some(limit))?
    };
    let quality=research_quality(args,&bars,request.timeframe,sqx_data::quality::now_ms())?;
    Ok((bars,request.symbol,request.timeframe,quality))
}

fn research_quality(args:&Value,bars:&[Bar],timeframe:Timeframe,as_of_ms:i64)->Result<Value,String>{
    let calendar:Option<sqx_data::quality::MarketCalendar>=match args.get("calendar") {
        None=>None,
        Some(value)=>Some(serde_json::from_value(value.clone()).map_err(|e|format!("invalid calendar: {e}"))?),
    };
    let allow=match args.get("allow_data_gaps") {
        None=>false,Some(Value::Bool(value))=>*value,
        Some(_)=>return Err("allow_data_gaps must be a boolean".into()),
    };
    let quality=sqx_data::quality::audit(bars,timeframe,as_of_ms,calendar.as_ref());
    let error=if !quality.errors.is_empty(){Some(quality.errors.join("; "))}
        else if !allow && (quality.missing_open_intervals>0 || quality.unclassified_intervals>0 || quality.irregular_steps>0){Some("Unresolved data gaps; supply a calendar or explicitly allow_data_gaps for exploratory research".into())}
        else{None};
    let context=json!({"data_quality":quality,"calendar":calendar,"as_of_ms":as_of_ms,"data_gaps_acknowledged":allow});
    if let Some(error)=error{return Err(json!({"error":error,"research_context":context}).to_string());}
    Ok(context)
}

fn research_result(summary:String,mut context:Value,symbol:&str,timeframe:Timeframe,config:&BacktestConfig)->Value{
    context["symbol"]=json!(symbol);context["timeframe"]=json!(timeframe);context["config"]=json!(config);
    context["scope"]=json!("Exploratory research; quality checks do not certify execution parity, untouched holdout or complete trial accounting.");
    text_result(format!("{summary}\nresearch_context: {context}"))
}

fn simulation_config(args:&Value,timeframe:Timeframe)->Result<BacktestConfig,String>{
    let mut config:BacktestConfig=match args.get("config") {
        Some(value)=>serde_json::from_value(value.clone()).map_err(|e|format!("invalid backtest config: {e}"))?,
        None=>BacktestConfig::default(),
    };
    let duration=if timeframe==Timeframe::MN1 {None}else{timeframe.minutes().and_then(|n|n.checked_mul(60000))};
    if config.bar_duration_ms.is_some() && config.bar_duration_ms!=duration {
        return Err(format!("bar_duration_ms conflicts with source timeframe {timeframe}"));
    }
    config.bar_duration_ms=duration;
    config.account.validate()?;
    Ok(config)
}

fn instrument_for(symbol: &str) -> Instrument {
    Instrument::fx(symbol)
}

fn pyp_for(bars: &[Bar]) -> f64 {
    if bars.len() < 2 {
        return 252.0;
    }
    let mut diffs: Vec<i64> = bars.windows(2).map(|w| (w[1].time - w[0].time) / 1000).collect();
    diffs.sort_unstable();
    let secs = diffs[diffs.len() / 2] as f64;
    if secs > 0.0 {
        (365.25 * 24.0 * 3600.0 / secs).min(bars.len() as f64).max(1.0)
    } else {
        252.0
    }
}

fn metrics_summary(m: &Metrics) -> String {
    format!(
        "net_profit={:.2} profit_factor={:.2} max_dd_pct={:.2} return_dd={:.2} trades={:.0} win_pct={:.1} sharpe={:.2} sqn={:.2}",
        m.net_profit, m.profit_factor, m.max_drawdown_pct, m.return_dd_ratio, m.number_of_trades, m.winning_pct, m.sharpe_ratio, m.sqn
    )
}

fn tool_backtest(args: &Value) -> Value {
    let strat_val = match args.get("strategy") {
        Some(v) => v.clone(),
        None => return err_result("missing strategy".to_string()),
    };
    let strategy: Strategy = match serde_json::from_value(strat_val) {
        Ok(s) => s,
        Err(e) => return err_result(format!("bad strategy: {e}")),
    };
    let (bars, symbol, timeframe, quality) = match resolve_bars(args) {
        Ok(v) => v,
        Err(e) => return err_result(e),
    };
    let inst = instrument_for(&symbol);
    let pyp = pyp_for(&bars);
    let cfg=match simulation_config(args,timeframe){Ok(config)=>config,Err(error)=>return err_result(error)};
    if let Err(error)=sqx_backtest::validate_backtest_inputs(&bars,&inst,&cfg){return err_result(error);}
    let r = backtest(&strategy, &bars, &inst, &cfg);
    if !r.errors.is_empty() { return err_result(r.errors.join("; ")); }
    let m = compute_metrics(&r, bars.len(), pyp);
    research_result(metrics_summary(&m),quality,&symbol,timeframe,&cfg)
}

fn tool_generate(args: &Value) -> Value {
    let (bars, symbol, timeframe, quality) = match resolve_bars(args) {
        Ok(v) => v,
        Err(e) => return err_result(e),
    };
    let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(200).clamp(1, 2000) as usize;
    let seed = args.get("seed").and_then(|v| v.as_u64()).unwrap_or(42);
    let gen = GeneratorConfig::default();
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);
    let strategies: Vec<Strategy> = (0..count).map(|i| random_strategy(&mut rng, &format!("s{i:04}"), &gen)).collect();
    let inst = instrument_for(&symbol);
    let pyp = pyp_for(&bars);
    let cfg=match simulation_config(args,timeframe){Ok(config)=>config,Err(error)=>return err_result(error)};
    if let Err(error)=sqx_backtest::validate_backtest_inputs(&bars,&inst,&cfg){return err_result(error);}
    let mut quality=quality;quality["timeframe"]=json!(timeframe);
    let archive=generation_archive::configured_archive();
    let (evaluations,search_id,completion_id)=match generation_archive::execute(&archive,&strategies,&bars,&inst,&cfg,pyp,seed,&quality) {
        Ok(value)=>value,Err(error)=>return err_result(format!("Generation evidence could not be recorded: {error}")),
    };
    quality["generation_archive"]=json!({"search_id":search_id,"completion_id":completion_id,"root":archive.root(),"inspection_tool":"generation_history"});
    generation_result(evaluations,seed,quality,&symbol,timeframe,&cfg)
}

fn generation_result(evaluations:Vec<sqx_optimizer::BatchEvaluation>,seed:u64,mut context:Value,symbol:&str,timeframe:Timeframe,cfg:&BacktestConfig)->Value {
    let attempted=evaluations.len();
    let mut failures=Vec::new();let mut scored=Vec::new();
    for (candidate_index,evaluation) in evaluations.into_iter().enumerate() {
        if !evaluation.errors.is_empty() {
            failures.push(json!({"candidate_index":candidate_index,"strategy":evaluation.strategy,"errors":evaluation.errors}));
        } else {scored.push((evaluation.strategy,evaluation.metrics));}
    }
    let ranked=rank(scored,Fitness::ProfitFactor,5);
    let invalid=failures.len();let eligible=ranked.len();
    let mut lines=vec![format!("Evaluation attempts: {attempted}; invalid: {invalid}; eligible for ranking: {eligible}; below ranking criteria: {}. These counts are not independent trials.",attempted-invalid-eligible)];
    if invalid>0 {lines.push("Search incomplete: candidate execution failed. Any displayed ranking includes valid candidates only and is not a successful complete search.".into());}
    for (i,(s,m)) in ranked.iter().take(10).enumerate() {
        lines.push(format!("{}. {} net={:.2} pf={:.2} trades={:.0}",i+1,s.name,m.net_profit,m.profit_factor,m.number_of_trades));
    }
    context["generation"]=json!({"seed":seed,"attempted":attempted,"invalid":invalid,"eligible":eligible,"below_ranking_criteria":attempted-invalid-eligible,"min_trades":5,"fitness":"ProfitFactor","candidate_failures":failures,"scope":"Evaluation counts are not independent trials. When generation_archive is present, full candidate checkpoints/results were saved before ranking; a pending checkpoint is not proof of process liveness or completion."});
    let mut response=research_result(lines.join("\n"),context,symbol,timeframe,cfg);
    if invalid>0 {response["isError"]=json!(true);}
    response
}

fn tool_portfolio(args: &Value) -> Value {
    let (bars, symbol, timeframe, quality) = match resolve_bars(args) {
        Ok(v) => v,
        Err(e) => return err_result(e),
    };
    let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(300).clamp(10, 2000) as usize;
    let seed = args.get("seed").and_then(|v| v.as_u64()).unwrap_or(42);
    let gen = GeneratorConfig::default();
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);
    let strategies: Vec<Strategy> = (0..count).map(|i| random_strategy(&mut rng, &format!("p{i:04}"), &gen)).collect();
    let inst = instrument_for(&symbol);
    let pyp = pyp_for(&bars);
    let cfg=match simulation_config(args,timeframe){Ok(config)=>config,Err(error)=>return err_result(error)};
    if let Err(error)=sqx_backtest::validate_backtest_inputs(&bars,&inst,&cfg){return err_result(error);}
    let results = sqx_portfolio::backtest_all(&strategies, &bars, &inst, &cfg);
    portfolio_result(&strategies,&results,bars.len(),pyp,seed,quality,&symbol,timeframe,&cfg)
}

#[allow(clippy::too_many_arguments)]
fn portfolio_result(strategies:&[Strategy],results:&[sqx_backtest::BacktestResult],bar_count:usize,pyp:f64,seed:u64,mut context:Value,symbol:&str,timeframe:Timeframe,cfg:&BacktestConfig)->Value {
    let mut failures=Vec::new();
    for (candidate_index,(strategy,result)) in strategies.iter().zip(results).enumerate() {
        let mut errors=result.errors.clone();
        if let Err(error)=sqx_metrics::validate_observations(result){errors.push(error);}
        if !errors.is_empty(){failures.push(json!({"candidate_index":candidate_index,"strategy":strategy,"errors":errors}));}
    }
    context["portfolio_search"]=json!({"seed":seed,"attempted":results.len(),"requested_candidates":strategies.len(),"invalid":failures.len(),"candidate_failures":failures,"min_trades":5,"fitness":"ProfitFactor","shortlist_limit":12,"subset_size_min":2,"subset_size_max":4,"scope":"Exploratory selection on the supplied data, not independent validation or a durable trial ledger."});
    if !failures.is_empty() || strategies.len()!=results.len() {
        let mut response=research_result("Portfolio search incomplete: candidate execution failed or results are missing. No portfolio selection was performed.".into(),context,symbol,timeframe,cfg);
        response["isError"]=json!(true);return response;
    }
    let names:Vec<String>=strategies.iter().map(|s|s.name.clone()).collect();
    let mets: Vec<Metrics> = results.iter().map(|r| compute_metrics(r, bar_count, pyp)).collect();
    let mut order: Vec<usize> = (0..results.len()).collect();
    order.sort_by(|&a, &b| mets[b].profit_factor.partial_cmp(&mets[a].profit_factor).unwrap_or(std::cmp::Ordering::Equal));
    order.retain(|&i| mets[i].number_of_trades >= 5.0 && mets[i].profit_factor.is_finite());
    context["portfolio_search"]["eligible"]=json!(order.len());
    context["portfolio_search"]["below_ranking_criteria"]=json!(results.len()-order.len());
    order.truncate(12);
    context["portfolio_search"]["shortlisted_candidate_indices"]=json!(order);
    if order.len() < 2 {
        let mut response=research_result("Not enough qualifying strategies to form a portfolio.".into(),context,symbol,timeframe,cfg);
        response["isError"]=json!(true);return response;
    }
    let sel: Vec<sqx_backtest::BacktestResult> = order.iter().map(|&i| results[i].clone()).collect();
    let sel_names: Vec<String> = order.iter().map(|&i| names[i].clone()).collect();
    let subset = sqx_portfolio::best_subset(&sel, 2, 4, cfg.initial_deposit);
    let comp: Vec<sqx_backtest::BacktestResult> = subset.iter().map(|&i| sel[i].clone()).collect();
    let comp_names: Vec<String> = subset.iter().map(|&i| sel_names[i].clone()).collect();
    let weights = sqx_portfolio::equal_weights(comp.len());
    context["portfolio_search"]["selected_candidate_indices"]=json!(subset.iter().map(|&i|order[i]).collect::<Vec<_>>());
    let pf = sqx_portfolio::combine(&comp_names, &comp, &weights, cfg.initial_deposit, bar_count, pyp);
    if !pf.errors.is_empty(){
        context["portfolio_search"]["combination_errors"]=json!(pf.errors);
        let mut response=research_result("Portfolio combination failed.".into(),context,symbol,timeframe,cfg);
        response["isError"]=json!(true);return response;
    }
    let mut lines = vec![format!("portfolio: {} strategies | {}", comp.len(), metrics_summary(&pf.metrics))];
    for c in &pf.components {
        lines.push(format!("  {} w={:.2} net={:.2} dd={:.2}%", c.name, c.weight, c.net_profit, c.max_drawdown_pct));
    }
    research_result(lines.join("\n"),context,symbol,timeframe,cfg)
}

fn tool_robustness(args: &Value) -> Value {
    let strat_val = match args.get("strategy") {
        Some(v) => v.clone(),
        None => return err_result("missing strategy".to_string()),
    };
    let strategy: Strategy = match serde_json::from_value(strat_val) {
        Ok(s) => s,
        Err(e) => return err_result(format!("bad strategy: {e}")),
    };
    let (bars, symbol, timeframe, quality) = match resolve_bars(args) {
        Ok(v) => v,
        Err(e) => return err_result(e),
    };
    let inst = instrument_for(&symbol);
    let pyp = pyp_for(&bars);
    let cfg=match simulation_config(args,timeframe){Ok(config)=>config,Err(error)=>return err_result(error)};
    if let Err(error)=sqx_backtest::validate_backtest_inputs(&bars,&inst,&cfg){return err_result(error);}
    let base = backtest(&strategy, &bars, &inst, &cfg);
    if !base.errors.is_empty(){return err_result(base.errors.join("; "));}
    let base_m = compute_metrics(&base, bars.len(), pyp);
    let mut lines = vec![format!("base: {}", metrics_summary(&base_m))];
    let scenarios = [(1.0, 0.0, 0.0), (3.0, 1.0, 5.0), (5.0, 2.0, 10.0)];
    let mut errors=Vec::new();
    for (index,r) in what_if(&strategy, &bars, &inst, &cfg, &scenarios).into_iter().enumerate() {
        errors.extend(r.errors.iter().map(|error|format!("What-if scenario {index}: {error}")));
        if !r.errors.is_empty(){lines.push(format!("invalid what-if: {}",r.errors.join("; ")));continue;}
        lines.push(format!("what-if spread={:.0} slip={:.0} comm={:.0} -> net {:.2}", r.spread, r.slippage, r.commission, r.net_profit));
    }
    let mc = monte_carlo_trade_manipulation(&base.trades, 500, 0.1, 42);
    lines.push(sqx_robustness::render(&mc));
    let rt = monte_carlo_retest(&strategy, &bars, &inst, &cfg, 200, 7);
    lines.push(format!("mc_retest assumptions: seed {}; {}",rt.seed,rt.warmup_policy));
    lines.push(format!("mc_retest coverage: requested {}, skipped {}, invalid {}; issues: {:?}",rt.requested_runs,rt.skipped_runs,rt.invalid_runs,rt.errors));
    lines.push(format!("mc_retest: median_net={:.2} losing={:.1}%", rt.net_profit_50pct, rt.losing_runs_pct));
    let gen = GeneratorConfig::default();
    let wf = walk_forward(&bars, &inst, &cfg, pyp, &gen, 4, 0.25, 20, 42);
    lines.push(format!("walk_forward (trade-count evidence is not a robustness certificate): {}", serde_json::to_string(&wf).unwrap_or_else(|e|format!("report serialization failed: {e}"))));
    errors.extend(mc.errors.iter().map(|error|format!("Monte Carlo: {error}")));
    errors.extend(rt.errors.iter().map(|error|format!("MC retest: {error}")));
    errors.extend(wf.errors.iter().map(|error|format!("Walk-forward: {error}")));
    let mut context=quality;
    context["errors"]=json!(errors);
    let mut response=research_result(lines.join("\n"),context,&symbol,timeframe,&cfg);
    if !errors.is_empty(){response["isError"]=json!(true);}
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trial_recovery_rejects_ambiguous_modes_and_input_overrides(){
        for arguments in [json!({"run_id":"unused","restart":"true"}),json!({"run_id":"unused","config":{}}),json!({"restart":true})] {
            let result=call_tool(&json!({"name":"generation_trial","arguments":arguments}));
            assert_eq!(result["isError"],true);
            assert!(!result.to_string().contains("No such file"),"reject arguments before accessing records");
        }
    }
    #[test]
    fn portfolio_failures_stop_selection_and_keep_research_context() {
        let strategy=Strategy::new("portfolio fixture");
        let cfg=BacktestConfig::default();
        let bars=vec![Bar::new(0,1.0,1.1,0.9,1.0,1.0),Bar::new(60000,1.0,1.1,0.9,1.0,1.0)];
        let valid=backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&cfg);
        assert!(valid.errors.is_empty());
        let mut failed=valid.clone();failed.errors.push("candidate execution failed".into());
        let response=portfolio_result(&[strategy.clone(),strategy.clone()],&[valid.clone(),failed],2,252.0,42,json!({"data_quality":{"bars":2}}),"EURUSD",Timeframe::M1,&cfg);
        assert_eq!(response["isError"],true);
        let text=response["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("No portfolio selection was performed"));
        let context:Value=serde_json::from_str(text.split_once("research_context: ").unwrap().1).unwrap();
        assert_eq!(context["data_quality"]["bars"],2);assert_eq!(context["symbol"],"EURUSD");
        let evidence=&context["portfolio_search"];
        assert_eq!(evidence["attempted"],2);assert_eq!(evidence["invalid"],1);assert_eq!(evidence["seed"],42);
        assert_eq!(evidence["candidate_failures"][0]["candidate_index"],1);
        assert!(evidence.get("selected_candidate_indices").is_none());
        let no_trades=portfolio_result(&[strategy.clone()],&[valid.clone()],2,252.0,42,json!({}),"EURUSD",Timeframe::M1,&cfg);
        let text=no_trades["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Not enough qualifying"));
        let context:Value=serde_json::from_str(text.split_once("research_context: ").unwrap().1).unwrap();
        assert_eq!(context["portfolio_search"]["invalid"],0);assert_eq!(context["portfolio_search"]["below_ranking_criteria"],1);
        let mut invalid_observation=valid;invalid_observation.final_equity=f64::NAN;
        let response=portfolio_result(&[strategy],&[invalid_observation],2,252.0,42,json!({}),"EURUSD",Timeframe::M1,&cfg);
        assert_eq!(response["isError"],true);assert!(response.to_string().contains("No portfolio selection was performed"));
    }
    #[test]
    fn generation_failures_cannot_rank_or_disappear_as_low_fitness() {
        let evaluation=|name:&str,trades:f64,pf:f64,errors:Vec<String>|sqx_optimizer::BatchEvaluation {
            strategy:Strategy::new(name),metrics:Metrics{number_of_trades:trades,profit_factor:pf,..Default::default()},errors,
        };
        let response=generation_result(vec![
            evaluation("invalid-but-profitable",20.0,99.0,vec!["execution fixture failed".into()]),
            evaluation("valid-selected",10.0,2.0,vec![]),
            evaluation("too-few-trades",4.0,5.0,vec![]),
            evaluation("undefined-ratio",10.0,f64::NAN,vec![]),
        ],42,json!({}),"EURUSD",Timeframe::M1,&BacktestConfig::default());
        assert_eq!(response["isError"],true);
        let text=response["content"][0]["text"].as_str().unwrap();
        let (summary,context)=text.split_once("research_context: ").unwrap();
        assert!(summary.contains("Search incomplete"));assert!(summary.contains("1. valid-selected"));
        assert!(!summary.contains("invalid-but-profitable"));
        let context:Value=serde_json::from_str(context).unwrap();let evidence=&context["generation"];
        assert_eq!(evidence["attempted"],4);assert_eq!(evidence["invalid"],1);
        assert_eq!(evidence["eligible"],1);assert_eq!(evidence["below_ranking_criteria"],2);
        assert_eq!(evidence["candidate_failures"][0]["candidate_index"],0);
        assert_eq!(evidence["candidate_failures"][0]["strategy"]["name"],"invalid-but-profitable");
        assert_eq!(evidence["candidate_failures"][0]["errors"][0],"execution fixture failed");
        assert_eq!(evidence["seed"],42);
        let valid=generation_result(vec![evaluation("below-threshold",0.0,f64::NAN,vec![])],7,json!({}),"EURUSD",Timeframe::M1,&BacktestConfig::default());
        assert!(valid.get("isError").is_none(),"valid no-trade result is not an execution failure");
        let failed=generation_result(vec![evaluation("failed",10.0,3.0,vec!["failed".into()])],7,json!({}),"EURUSD",Timeframe::M1,&BacktestConfig::default());
        assert_eq!(failed["isError"],true);assert!(failed.to_string().contains("eligible for ranking: 0"));
    }
    #[test]
    fn failed_robustness_stages_are_protocol_errors_with_context(){
        let path=std::env::temp_dir().join(format!("sqx-mcp-stress-{}-{}.csv",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::write(&path,"timestamp,open,high,low,close,volume\n1700000000,1,1,1,1,1\n1700000060,1,1,1,1,1\n").unwrap();
        let response=call_tool(&json!({"name":"robustness","arguments":{"data":path,"symbol":"EURUSD","timeframe":"M1","strategy":Strategy::new("no trades")}}));
        std::fs::remove_file(path).unwrap();
        assert_eq!(response["isError"],true,"{response}");
        let text=response["content"][0]["text"].as_str().unwrap();
        let context:Value=serde_json::from_str(text.split("research_context: ").nth(1).unwrap()).unwrap();
        assert_eq!(context["symbol"],"EURUSD");
        assert_eq!(context["config"]["bar_duration_ms"],60000);
        let errors=context["errors"].as_array().unwrap();
        for stage in ["Monte Carlo:","MC retest:","Walk-forward:"] {
            assert!(errors.iter().any(|error|error.as_str().unwrap().starts_with(stage)),"{context}");
        }
    }
    #[test]
    fn source_selection_separates_csv_instrument_from_questdb(){
        let csv=source_request(&json!({"data":"prices.csv","symbol":"USDJPY","timeframe":"M1"})).unwrap();
        assert_eq!(csv,SourceRequest{data:Some("prices.csv".into()),symbol:"USDJPY".into(),timeframe:Timeframe::M1});
        assert_eq!(source_request(&json!({"symbol":"USDJPY"})).unwrap().timeframe,Timeframe::M5);
        for request in [json!({"data":"prices.csv"}),json!({"data":"prices.csv","symbol":"USDJPY"}),json!({"source":"questdb","data":"prices.csv","symbol":"USDJPY","timeframe":"M1"}),json!({"source":"csv","symbol":"USDJPY","timeframe":"M1"}),json!({"source":"other","symbol":"USDJPY"}),json!({"symbol":12}),json!({"symbol":"USDJPY","timeframe":null})] {
            assert!(source_request(&request).is_err(),"{request}");
        }
    }
    #[test]
    fn csv_symbol_and_duration_reach_all_research_tools_without_network(){
        let path=std::env::temp_dir().join(format!("sqx-mcp-source-{}-{}.csv",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::write(&path,"timestamp,open,high,low,close,volume\n1700000000,150,151,149,150,1\n1700000060,150,151,149,150,1\n").unwrap();
        let mut request=json!({"source":"csv","data":path,"symbol":"USDJPY","timeframe":"M1","strategy":Strategy::new("source fixture"),"count":10,"config":{"bar_duration_ms":30000}});
        let (bars,symbol,timeframe,_)=resolve_bars(&request).unwrap();assert_eq!(bars.len(),2);assert_eq!(symbol,"USDJPY");assert_eq!(timeframe,Timeframe::M1);
        for tool in [tool_backtest,tool_generate,tool_portfolio,tool_robustness] {
            let response=tool(&request);assert_eq!(response["isError"],true,"{response}");assert!(response.to_string().contains("conflicts with source timeframe"),"{response}");
        }
        request["config"]=json!({});
        assert_eq!(simulation_config(&request,timeframe).unwrap().bar_duration_ms,Some(60000));
        assert_eq!(simulation_config(&request,Timeframe::MN1).unwrap().bar_duration_ms,None);
        let valid=tool_backtest(&request);
        assert!(valid.get("isError").is_none(),"{valid}");
        assert!(valid.to_string().contains("research_context"));
        std::fs::write(&path,"timestamp,open,high,low,close,volume\n1700000000,150,151,149,150,1\n1700000120,150,151,149,150,1\n").unwrap();
        for tool in [tool_backtest,tool_generate,tool_portfolio,tool_robustness] {
            let response=tool(&request);assert_eq!(response["isError"],true,"{response}");assert!(response.to_string().contains("Unresolved data gaps"),"{response}");
        }
        request["allow_data_gaps"]=json!(true);
        let exploratory=tool_backtest(&request);assert!(exploratory.get("isError").is_none(),"{exploratory}");
        let context:Value=serde_json::from_str(exploratory["content"][0]["text"].as_str().unwrap().split("research_context: ").nth(1).unwrap()).unwrap();
        assert_eq!(context["symbol"],"USDJPY");assert_eq!(context["timeframe"],"M1");
        assert_eq!(context["config"]["bar_duration_ms"],60000);assert_eq!(context["data_quality"]["unclassified_intervals"],1);
        assert_eq!(context["data_gaps_acknowledged"],true);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn quality_gate_distinguishes_market_closures_from_missing_and_invalid_data(){
        let bar=|time|Bar::new(time,1.0,1.1,0.9,1.0,1.0);
        let bars=[bar(0),bar(120000)];
        let denied=research_quality(&json!({}),&bars,Timeframe::M1,180000).unwrap_err();
        assert!(denied.contains("Unresolved data gaps"));
        let allowed=research_quality(&json!({"allow_data_gaps":true}),&bars,Timeframe::M1,180000).unwrap();
        assert_eq!(allowed["data_quality"]["unclassified_intervals"],1);
        assert_eq!(allowed["data_gaps_acknowledged"],true);
        let request=json!({"calendar":{"name":"Thursday split session","weekly":[{"weekday":3,"start_minute":0,"end_minute":1},{"weekday":3,"start_minute":2,"end_minute":3}]}});
        let closed=research_quality(&request,&bars,Timeframe::M1,180000).unwrap();
        assert_eq!(closed["data_quality"]["scheduled_closed_intervals"],1);
        assert_eq!(closed["data_quality"]["unclassified_intervals"],0);
        assert_eq!(closed["calendar"]["weekly"],request["calendar"]["weekly"]);
        for invalid in [vec![bar(0),bar(0)],vec![bar(0),bar(30000)]] {
            assert!(research_quality(&json!({"allow_data_gaps":true}),&invalid,Timeframe::M1,180000).is_err());
        }
        assert!(research_quality(&json!({"allow_data_gaps":true}),&bars,Timeframe::M1,179999).unwrap_err().contains("not closed"));
        for request in [json!({"allow_data_gaps":"true"}),json!({"calendar":null}),json!({"calendar":{"name":"bad","weekly":[{"weekday":7,"start_minute":0,"end_minute":1}]}})] {
            assert!(research_quality(&request,&bars,Timeframe::M1,180000).is_err());
        }
    }

}
