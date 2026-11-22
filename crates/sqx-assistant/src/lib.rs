//! LLM assistant layer: talk to any OpenAI-compatible endpoint (e.g. a free
//! provider from awesome-free-llm-apis) and let the model drive the engine
//! through a small tool set.

use rand::{rngs::StdRng, SeedableRng};
use serde_json::{json, Value};
use sqx_backtest::{backtest, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_optimizer::{random_strategy, GeneratorConfig};
use sqx_strategy::Strategy;
use std::process::Command;

const SYSTEM_PROMPT: &str = "You are the SqxLabs strategy assistant. Use the provided tools to generate strategies and inspect their backtests on the loaded market data. Never invent numbers: call a tool. Answer briefly in the user's language.";

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub max_rounds: usize,
}

pub struct Assistant {
    pub cfg: LlmConfig,
    pub symbol: String,
    pub bars: Vec<Bar>,
    pub instrument: Instrument,
    pub periods_per_year: f64,
    generated: Vec<Strategy>,
}

impl Assistant {
    pub fn new(cfg: LlmConfig, symbol: &str, bars: Vec<Bar>, instrument: Instrument, periods_per_year: f64) -> Self {
        Self { cfg, symbol: symbol.to_string(), bars, instrument, periods_per_year, generated: Vec::new() }
    }

    pub fn run(&mut self, question: &str) -> Result<String, String> {
        let tools = tool_defs();
        let mut messages = vec![
            json!({"role": "system", "content": SYSTEM_PROMPT}),
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
            let resp = post_json(&self.cfg.base_url, &self.cfg.api_key, &body.to_string())?;
            let v: Value = serde_json::from_str(&resp).map_err(|e| format!("invalid JSON from LLM: {e}: {resp}"))?;
            let msg = v["choices"][0]["message"].clone();
            let tool_calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
            if tool_calls.is_empty() {
                return Ok(msg["content"].as_str().unwrap_or("").to_string());
            }
            messages.push(msg);
            for tc in tool_calls {
                let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                let args: Value = serde_json::from_str(tc["function"]["arguments"].as_str().unwrap_or("{}"))
                    .unwrap_or_else(|_| json!({}));
                let result = self.dispatch(&name, &args);
                messages.push(json!({"role": "tool", "tool_call_id": tc["id"], "content": result}));
            }
        }
        Err(format!("assistant did not finish within {} tool rounds", self.cfg.max_rounds))
    }

    pub fn dispatch(&mut self, name: &str, args: &Value) -> String {
        match name {
            "list_indicators" => {
                let names: Vec<&str> = sqx_strategy::indicator_catalog().iter().map(|s| s.name).collect();
                json!({"indicators": names}).to_string()
            }
            "generate" => {
                let count = args["count"].as_u64().unwrap_or(20).clamp(1, 200) as usize;
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
                self.generated = (0..count)
                    .map(|i| random_strategy(&mut rng, &format!("llm-{i:04}"), &cfg))
                    .collect();
                let mut rows: Vec<Value> = self
                    .generated
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let r = backtest(s, &self.bars, &self.instrument, &BacktestConfig::default());
                        let m = sqx_metrics::compute(&r, self.bars.len(), self.periods_per_year);
                        json!({
                            "index": i,
                            "name": s.name,
                            "net_profit": m.net_profit,
                            "profit_factor": m.profit_factor,
                            "trades": r.trades.len(),
                            "max_dd_pct": m.max_drawdown_pct
                        })
                    })
                    .collect();
                rows.sort_by(|a, b| {
                    b["net_profit"]
                        .as_f64()
                        .unwrap_or(0.0)
                        .partial_cmp(&a["net_profit"].as_f64().unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                rows.truncate(5);
                json!({"generated": count, "top": rows}).to_string()
            }
            "details" => {
                let idx = args["index"].as_u64().unwrap_or(0) as usize;
                match self.generated.get(idx) {
                    Some(s) => {
                        let r = backtest(s, &self.bars, &self.instrument, &BacktestConfig::default());
                        let m = sqx_metrics::compute(&r, self.bars.len(), self.periods_per_year);
                        json!({
                            "index": idx,
                            "net_profit": m.net_profit,
                            "profit_factor": m.profit_factor,
                            "max_drawdown_pct": m.max_drawdown_pct,
                            "trades": r.trades.len(),
                            "winning_pct": m.winning_pct,
                            "sharpe": m.sharpe_ratio,
                            "sqn": m.sqn,
                            "strategy": s
                        })
                        .to_string()
                    }
                    None => json!({"error": "index out of range"}).to_string(),
                }
            }
            _ => json!({"error": format!("unknown tool {name}")}).to_string(),
        }
    }
}

fn post_json(base_url: &str, api_key: &str, body: &str) -> Result<String, String> {
    let url = if base_url.contains("/chat/completions") {
        base_url.to_string()
    } else {
        format!("{}/chat/completions", base_url.trim_end_matches('/'))
    };
    let out = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "180",
            "-X",
            "POST",
            &url,
            "-H",
            "Content-Type: application/json",
            "-H",
            &format!("Authorization: Bearer {api_key}"),
            "-d",
            body,
        ])
        .output()
        .map_err(|e| format!("curl failed: {e} (is curl installed?)"))?;
    if !out.status.success() {
        return Err(format!("curl error: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
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
            "parameters": {"type": "object", "properties": {"count": {"type": "integer", "description": "number of strategies, 1-200"}}, "required": ["count"]}
        }},
        {"type": "function", "function": {
            "name": "details",
            "description": "Backtest one previously generated strategy by index and return its metrics and definition.",
            "parameters": {"type": "object", "properties": {"index": {"type": "integer", "description": "index from the generate result"}}, "required": ["index"]}
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
}
