//! # sqx-mcp
//!
//! A minimal Model Context Protocol (MCP) server over stdio (JSON-RPC 2.0),
//! exposing the SqxLabs engine as MCP tools so an LLM client can backtest,
//! generate, build portfolios and run robustness checks.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use sqx_backtest::{backtest, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_data::{read_csv_bars, DataStore};
use sqx_metrics::{compute as compute_metrics, Metrics};
use sqx_optimizer::{backtest_batch, rank, random_strategy, walk_forward, Fitness, GeneratorConfig};
use sqx_robustness::{monte_carlo_retest, monte_carlo_trade_manipulation, what_if};
use sqx_strategy::Strategy;

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
    vec![
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
            "name": "backtest",
            "description": "Backtest a strategy JSON on a CSV data file and return metrics.",
            "inputSchema": { "type": "object", "properties": { "data": { "type": "string" }, "strategy": { "type": "object" } }, "required": ["data", "strategy"] }
        }),
        json!({
            "name": "generate",
            "description": "Generate random strategies, backtest and rank them.",
            "inputSchema": { "type": "object", "properties": { "data": { "type": "string" }, "count": { "type": "integer" }, "seed": { "type": "integer" } }, "required": ["data"] }
        }),
        json!({
            "name": "portfolio",
            "description": "Build a portfolio from generated strategies (best subset, correlation).",
            "inputSchema": { "type": "object", "properties": { "data": { "type": "string" }, "count": { "type": "integer" }, "seed": { "type": "integer" } }, "required": ["data"] }
        }),
        json!({
            "name": "robustness",
            "description": "Run the robustness cross-check set (what-if, MC trade, MC retest, walk-forward).",
            "inputSchema": { "type": "object", "properties": { "data": { "type": "string" }, "strategy": { "type": "object" } }, "required": ["data", "strategy"] }
        }),
    ]
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
        "backtest" => tool_backtest(&args),
        "generate" => tool_generate(&args),
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
    if let Err(e) = store.import_dir(&dir, sqx_core::Timeframe::M1) {
        return err_result(e);
    }
    let list: Vec<String> = store.list().iter().map(|s| format!("{} ({} bars)", s.symbol, s.bars)).collect();
    text_result(list.join("\n"))
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
    let data = match str_arg(args, "data") {
        Some(d) => d,
        None => return err_result("missing data".to_string()),
    };
    let strat_val = match args.get("strategy") {
        Some(v) => v.clone(),
        None => return err_result("missing strategy".to_string()),
    };
    let strategy: Strategy = match serde_json::from_value(strat_val) {
        Ok(s) => s,
        Err(e) => return err_result(format!("bad strategy: {e}")),
    };
    let bars = match read_csv_bars(&data) {
        Ok(b) => b,
        Err(e) => return err_result(e),
    };
    let inst = instrument_for("EURUSD");
    let pyp = pyp_for(&bars);
    let cfg = BacktestConfig::default();
    let r = backtest(&strategy, &bars, &inst, &cfg);
    let m = compute_metrics(&r, bars.len(), pyp);
    text_result(metrics_summary(&m))
}

fn tool_generate(args: &Value) -> Value {
    let data = match str_arg(args, "data") {
        Some(d) => d,
        None => return err_result("missing data".to_string()),
    };
    let bars = match read_csv_bars(&data) {
        Ok(b) => b,
        Err(e) => return err_result(e),
    };
    let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(200).clamp(1, 2000) as usize;
    let seed = args.get("seed").and_then(|v| v.as_u64()).unwrap_or(42);
    let gen = GeneratorConfig::default();
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);
    let strategies: Vec<Strategy> = (0..count).map(|i| random_strategy(&mut rng, &format!("s{i:04}"), &gen)).collect();
    let inst = instrument_for("EURUSD");
    let pyp = pyp_for(&bars);
    let cfg = BacktestConfig::default();
    let scored = backtest_batch(&strategies, &bars, &inst, &cfg, pyp);
    let ranked = rank(scored, Fitness::ProfitFactor, 5);
    let mut lines = Vec::new();
    for (i, (s, m)) in ranked.iter().take(10).enumerate() {
        lines.push(format!("{}. {} net={:.2} pf={:.2} trades={:.0}", i + 1, s.name, m.net_profit, m.profit_factor, m.number_of_trades));
    }
    text_result(lines.join("\n"))
}

fn tool_portfolio(args: &Value) -> Value {
    let data = match str_arg(args, "data") {
        Some(d) => d,
        None => return err_result("missing data".to_string()),
    };
    let bars = match read_csv_bars(&data) {
        Ok(b) => b,
        Err(e) => return err_result(e),
    };
    let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(300).clamp(10, 2000) as usize;
    let seed = args.get("seed").and_then(|v| v.as_u64()).unwrap_or(42);
    let gen = GeneratorConfig::default();
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);
    let strategies: Vec<Strategy> = (0..count).map(|i| random_strategy(&mut rng, &format!("p{i:04}"), &gen)).collect();
    let names: Vec<String> = strategies.iter().map(|s| s.name.clone()).collect();
    let inst = instrument_for("EURUSD");
    let pyp = pyp_for(&bars);
    let cfg = BacktestConfig::default();
    let results = sqx_portfolio::backtest_all(&strategies, &bars, &inst, &cfg);
    let mets: Vec<Metrics> = results.iter().map(|r| compute_metrics(r, bars.len(), pyp)).collect();
    let mut order: Vec<usize> = (0..results.len()).collect();
    order.sort_by(|&a, &b| mets[b].profit_factor.partial_cmp(&mets[a].profit_factor).unwrap_or(std::cmp::Ordering::Equal));
    order.retain(|&i| mets[i].number_of_trades >= 5.0 && mets[i].profit_factor.is_finite());
    order.truncate(12);
    if order.len() < 2 {
        return err_result("not enough qualifying strategies".to_string());
    }
    let sel: Vec<sqx_backtest::BacktestResult> = order.iter().map(|&i| results[i].clone()).collect();
    let sel_names: Vec<String> = order.iter().map(|&i| names[i].clone()).collect();
    let subset = sqx_portfolio::best_subset(&sel, 2, 4, cfg.initial_deposit);
    let comp: Vec<sqx_backtest::BacktestResult> = subset.iter().map(|&i| sel[i].clone()).collect();
    let comp_names: Vec<String> = subset.iter().map(|&i| sel_names[i].clone()).collect();
    let weights = sqx_portfolio::equal_weights(comp.len());
    let pf = sqx_portfolio::combine(&comp_names, &comp, &weights, cfg.initial_deposit, bars.len(), pyp);
    let mut lines = vec![format!("portfolio: {} strategies | {}", comp.len(), metrics_summary(&pf.metrics))];
    for c in &pf.components {
        lines.push(format!("  {} w={:.2} net={:.2} dd={:.2}%", c.name, c.weight, c.net_profit, c.max_drawdown_pct));
    }
    text_result(lines.join("\n"))
}

fn tool_robustness(args: &Value) -> Value {
    let data = match str_arg(args, "data") {
        Some(d) => d,
        None => return err_result("missing data".to_string()),
    };
    let strat_val = match args.get("strategy") {
        Some(v) => v.clone(),
        None => return err_result("missing strategy".to_string()),
    };
    let strategy: Strategy = match serde_json::from_value(strat_val) {
        Ok(s) => s,
        Err(e) => return err_result(format!("bad strategy: {e}")),
    };
    let bars = match read_csv_bars(&data) {
        Ok(b) => b,
        Err(e) => return err_result(e),
    };
    let inst = instrument_for("EURUSD");
    let pyp = pyp_for(&bars);
    let cfg = BacktestConfig::default();
    let base = backtest(&strategy, &bars, &inst, &cfg);
    let base_m = compute_metrics(&base, bars.len(), pyp);
    let mut lines = vec![format!("base: {}", metrics_summary(&base_m))];
    let scenarios = [(1.0, 0.0, 0.0), (3.0, 1.0, 5.0), (5.0, 2.0, 10.0)];
    for r in what_if(&strategy, &bars, &inst, &cfg, &scenarios) {
        lines.push(format!("what-if spread={:.0} slip={:.0} comm={:.0} -> net {:.2}", r.spread, r.slippage, r.commission, r.net_profit));
    }
    let mc = monte_carlo_trade_manipulation(&base.trades, 500, 0.1, 42);
    lines.push(format!("mc_trade: losing_runs={:.1}%", mc.losing_runs_pct));
    let rt = monte_carlo_retest(&strategy, &bars, &inst, &cfg, 200, 7);
    lines.push(format!("mc_retest: median_net={:.2} losing={:.1}%", rt.net_profit_50pct, rt.losing_runs_pct));
    let gen = GeneratorConfig::default();
    let wf = walk_forward(&bars, &inst, &cfg, pyp, &gen, 4, 0.25, 20, 42);
    lines.push(format!("walk_forward: windows={} oos_net={:.2} oos_pf={:.2}", wf.windows, wf.oos_net_profit, wf.oos_profit_factor));
    text_result(lines.join("\n"))
}
