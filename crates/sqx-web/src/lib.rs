//! # sqx-web
//!
//! A dependency-free embedded HTTP server that serves a single-page dashboard
//! over the trading engine, with multi-symbol data from the Data Manager.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, RwLock};

use rand::SeedableRng;
use serde_json::json;

use sqx_backtest::{backtest, BacktestConfig, BacktestResult};
use sqx_core::{Bar, Instrument};
use sqx_data::{DataStore, SymbolSeries};
use sqx_metrics::{compute as compute_metrics, Metrics};
use sqx_optimizer::{backtest_batch, rank, random_strategy, walk_forward, Fitness, GeneratorConfig};
use sqx_robustness::{monte_carlo_trade_manipulation, what_if};
use sqx_strategy::{render as render_pseudocode, Strategy};

const INDEX: &str = include_str!("../assets/index.html");

struct State {
    store: RwLock<DataStore>,
    databank: RwLock<Vec<serde_json::Value>>,
}

pub fn serve(addr: &str, store: DataStore) -> std::io::Result<()> {
    println!("SqxLabs web app: http://{addr} ({} symbols)", store.len());
    let databank = std::fs::read_to_string("data/databank.json")
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<serde_json::Value>>(&t).ok())
        .unwrap_or_default();
    let state = Arc::new(State { store: RwLock::new(store), databank: RwLock::new(databank) });
    let listener = TcpListener::bind(addr)?;
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let st = Arc::clone(&state);
                std::thread::spawn(move || {
                    let _ = handle(s, &st);
                });
            }
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream, state: &State) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let k = stream.read(&mut tmp)?;
        if k == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..k]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let hdr_end = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or(buf.len());
    let head = String::from_utf8_lossy(&buf[..hdr_end]).to_string();
    let mut body = buf[hdr_end.saturating_add(4).min(buf.len())..].to_vec();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut rl = request_line.split_whitespace();
    let method = rl.next().unwrap_or("GET").to_string();
    let path = rl.next().unwrap_or("/").to_string();
    let mut content_length = 0usize;
    for l in lines {
        if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }
    while body.len() < content_length {
        let k = stream.read(&mut tmp)?;
        if k == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..k]);
    }
    let body_str = String::from_utf8_lossy(&body).to_string();

    let (status, ctype, payload) = route(&method, &path, &body_str, state);
    let resp = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}", payload.len());
    stream.write_all(resp.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn query_symbol(path: &str) -> (&str, String) {
    match path.split_once('?') {
        Some((p, q)) => {
            let sym = q
                .split('&')
                .find_map(|kv| kv.strip_prefix("symbol="))
                .map(|s| s.to_string())
                .unwrap_or_default();
            (p, sym)
        }
        None => (path, String::new()),
    }
}

fn query_param(path: &str, key: &str) -> String {
    match path.split_once('?') {
        Some((_, q)) => q
            .split('&')
            .find_map(|kv| kv.strip_prefix(&format!("{key}=")))
            .map(|s| s.to_string())
            .unwrap_or_default(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_name_strips_separators() {
        assert_eq!(safe_name("my strategy/v1"), "my_strategy_v1");
        assert_eq!(safe_name("ok-name_1"), "ok-name_1");
    }

    #[test]
    fn query_param_reads_values() {
        assert_eq!(query_param("/x?name=foo&a=1", "name"), "foo");
        assert_eq!(query_param("/x", "name"), "");
        assert_eq!(query_param("/x?name=", "name"), "");
    }

    #[test]
    fn dedupe_push_keeps_unique_strategies() {
        let mut db = Vec::new();
        let row = json!({ "name": "a", "strategy": { "name": "a" } });
        let row2 = json!({ "name": "b", "strategy": { "name": "b" } });
        dedupe_push(&mut db, &[row.clone(), row.clone(), row2.clone()]);
        assert_eq!(db.len(), 2);
        dedupe_push(&mut db, &[row]);
        assert_eq!(db.len(), 2);
    }

    #[test]
    fn export_json_produces_code() {
        let body = r#"{"engine":"mql5","strategy":{"name":"t","long":true,"short":false,"signals":[],"exits":[],"stop_loss":null,"profit_target":null,"money_management":{"FixedSize":{"size":0.1}}}}"#;
        let out = export_json(body);
        assert!(out.contains("code"), "export should return code: {out}");
    }
}

fn route(method: &str, full_path: &str, body: &str, state: &State) -> (&'static str, &'static str, String) {
    let (path, qsym) = query_symbol(full_path);
    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => ("200 OK", "text/html; charset=utf-8", INDEX.to_string()),
        ("GET", "/api/indicators") => ("200 OK", "application/json", indicators_json()),
        ("GET", "/api/catalog") => ("200 OK", "application/json", catalog_json()),
        ("GET", "/api/datasources") => ("200 OK", "application/json", datasources_json(state)),
        ("GET", "/api/data") => ("200 OK", "application/json", data_json(state, &qsym)),
        ("POST", "/api/backtest") => ("200 OK", "application/json", backtest_json(body, state)),
        ("POST", "/api/generate") => ("200 OK", "application/json", generate_json(body, state)),
        ("POST", "/api/portfolio") => ("200 OK", "application/json", portfolio_json(body, state)),
        ("POST", "/api/pseudocode") => ("200 OK", "application/json", pseudocode_json(body)),
        ("POST", "/api/robustness") => ("200 OK", "application/json", robustness_json(body, state)),
        ("POST", "/api/optimize") => ("200 OK", "application/json", optimize_json(body, state)),
        ("POST", "/api/pipeline") => ("200 OK", "application/json", pipeline_json(body, state)),
        ("POST", "/api/export") => ("200 OK", "application/json", export_json(body)),
        ("POST", "/api/data/import") => ("200 OK", "application/json", import_json(body, state)),
        ("POST", "/api/data/delete") => ("200 OK", "application/json", delete_json(body, state)),
        ("POST", "/api/assistant") => ("200 OK", "application/json", assistant_json(body, state)),
        ("GET", "/api/assistant/config") => ("200 OK", "application/json", assistant_config_json()),
        ("GET", "/api/databank") => ("200 OK", "application/json", databank_json(state)),
        ("POST", "/api/databank/clear") => ("200 OK", "application/json", databank_clear(state)),
        ("GET", "/api/project/list") => ("200 OK", "application/json", project_list_json()),
        ("GET", "/api/project/load") => ("200 OK", "application/json", project_load_json(&query_param(full_path, "name"))),
        ("POST", "/api/project/save") => ("200 OK", "application/json", project_save_json(body)),
        ("POST", "/api/portfolio/databank") => ("200 OK", "application/json", portfolio_databank_json(body, state)),
        ("GET", "/api/profiles") => ("200 OK", "application/json", profiles_json()),
        ("POST", "/api/profiles/save") => ("200 OK", "application/json", profile_save_json(body)),
        ("POST", "/api/profiles/delete") => ("200 OK", "application/json", profile_delete_json(body)),
        _ => ("404 Not Found", "text/plain", "not found".to_string()),
    }
}

fn find_series(state: &State, symbol: &str) -> Option<SymbolSeries> {
    let st = state.store.read().unwrap();
    if symbol.is_empty() {
        st.series.first().cloned()
    } else {
        st.get(symbol).cloned().or_else(|| st.series.first().cloned())
    }
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

fn indicators_json() -> String {
    let list: Vec<_> = sqx_strategy::indicator_catalog()
        .iter()
        .map(|s| json!({ "name": s.name, "label": s.label, "oscillator": s.is_oscillator }))
        .collect();
    json!({ "indicators": list }).to_string()
}

fn catalog_json() -> String {
    let indicators: Vec<_> = sqx_strategy::indicator_catalog()
        .iter()
        .map(|s| {
            let params: Vec<_> = s.params.iter().map(|(n, v)| json!({ "name": n, "value": v })).collect();
            json!({ "name": s.name, "label": s.label, "oscillator": s.is_oscillator, "params": params })
        })
        .collect();
    let mm = json!([
        {"type": "FixedSize", "label": "Fixed size", "params": [{"name": "size", "value": 0.1}]},
        {"type": "CryptoFixedBalancePct", "label": "Crypto fixed % balance", "params": [{"name": "risk", "value": 5}]},
        {"type": "RiskFixedBalancePct", "label": "Risk fixed % balance", "params": [{"name": "risk", "value": 5}]},
        {"type": "AtrRiskBased", "label": "ATR Risk-Based Sizing", "params": [{"name": "risk", "value": 1}, {"name": "atr_period", "value": 14}, {"name": "atr_mult", "value": 2}]},
        {"type": "RiskFixedPctOfAccount", "label": "Risk fixed % of account", "params": [{"name": "risk", "value": 5}, {"name": "stop_loss_points", "value": 100}]},
        {"type": "CryptoFixedAmount", "label": "Crypto fixed amount", "params": [{"name": "amount", "value": 500}]},
        {"type": "FixedAmount", "label": "Fixed amount", "params": [{"name": "amount", "value": 500}]},
        {"type": "CryptoSizeByPrice", "label": "Crypto size by price", "params": [{"name": "risk", "value": 5}]},
        {"type": "StocksSizeByPrice", "label": "Stocks size by price", "params": [{"name": "risk", "value": 5}]},
        {"type": "SimpleMartingale", "label": "Simple Martingale MM", "params": [{"name": "lots_start", "value": 0.1}, {"name": "lots_multiplier", "value": 2}, {"name": "lots_reset", "value": 1}]}
    ]);
    json!({
        "indicators": indicators,
        "comparators": ["Gt", "Gte", "Lt", "Lte", "Eq", "Neq", "CrossAbove", "CrossBelow", "Rising", "Falling"],
        "price_fields": ["Open", "High", "Low", "Close", "Typical", "Volume"],
        "signal_kinds": ["LongEntry", "ShortEntry", "LongExit", "ShortExit"],
        "money_management": mm,
        "trading_options": {"exit_at_end_of_day": false, "exit_on_friday": false, "max_trades_per_day": null, "limit_hours": null}
    })
    .to_string()
}

fn datasources_json(state: &State) -> String {
    let st = state.store.read().unwrap();
    let list: Vec<_> = st
        .list()
        .iter()
        .map(|s| json!({ "symbol": s.symbol, "timeframe": s.timeframe, "bars": s.bars, "first": s.first, "last": s.last }))
        .collect();
    let default = st.series.first().map(|s| s.symbol.clone()).unwrap_or_default();
    json!({ "symbols": list, "default": default }).to_string()
}

fn strategy_key(r: &serde_json::Value) -> Option<String> {
    r.get("strategy").and_then(|s| serde_json::to_string(s).ok())
}

fn dedupe_push(db: &mut Vec<serde_json::Value>, rows: &[serde_json::Value]) {
    let mut seen: std::collections::HashSet<String> = db.iter().filter_map(strategy_key).collect();
    for r in rows {
        if let Some(key) = strategy_key(r) {
            if seen.insert(key) {
                db.push(r.clone());
            }
        }
    }
}

fn databank_push(state: &State, rows: &[serde_json::Value]) {
    let mut db = state.databank.write().unwrap();
    dedupe_push(&mut db, rows);
    drop(db);
    persist_databank(state);
}

fn persist_databank(state: &State) {
    let rows = state.databank.read().unwrap();
    let _ = std::fs::create_dir_all("data");
    let _ = std::fs::write(
        "data/databank.json",
        serde_json::to_string_pretty(&*rows).unwrap_or_default(),
    );
}

fn portfolio_databank_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let cfg = config_from(&val);
    let top = val.get("top").and_then(|v| v.as_u64()).unwrap_or(5).clamp(2, 20) as usize;
    let rows = state.databank.read().unwrap().clone();
    let mut strategies: Vec<(String, Strategy)> = Vec::new();
    for r in rows.iter() {
        let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if let Ok(s) = serde_json::from_value::<Strategy>(r.get("strategy").cloned().unwrap_or(json!({}))) {
            strategies.push((name, s));
        }
        if strategies.len() >= top {
            break;
        }
    }
    if strategies.len() < 2 {
        return json!({ "error": "need at least 2 databank strategies" }).to_string();
    }
    let names: Vec<String> = strategies.iter().map(|(n, _)| n.clone()).collect();
    let results: Vec<BacktestResult> = strategies
        .iter()
        .map(|(_, s)| backtest(s, &series.bars, &inst, &cfg))
        .collect();
    let k = strategies.len();
    let weights = vec![1.0 / k as f64; k];
    let pf = sqx_portfolio::combine(&names, &results, &weights, cfg.initial_deposit, series.bars.len(), pyp);
    json!({
        "symbol": series.symbol,
        "components": names,
        "metrics": metrics_json(&pf.metrics),
        "correlation": pf.correlation,
        "equity": pf.equity
    })
    .to_string()
}

fn profiles_load() -> serde_json::Value {
    std::fs::read_to_string("data/profiles.json")
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .unwrap_or_else(|| json!({ "brokers": [], "sessions": [] }))
}

fn profiles_store(v: &serde_json::Value) {
    let _ = std::fs::create_dir_all("data");
    let _ = std::fs::write(
        "data/profiles.json",
        serde_json::to_string_pretty(v).unwrap_or_default(),
    );
}

fn profiles_json() -> String {
    profiles_load().to_string()
}

fn profile_kind_key(kind: &str) -> &'static str {
    if kind == "session" { "sessions" } else { "brokers" }
}

fn profile_save_json(body: &str) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let kind = val.get("kind").and_then(|v| v.as_str()).unwrap_or("broker").to_string();
    let profile = match val.get("profile") {
        Some(p) => p.clone(),
        None => return json!({ "error": "profile required" }).to_string(),
    };
    let name = profile.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if name.is_empty() {
        return json!({ "error": "name required" }).to_string();
    }
    let key = profile_kind_key(&kind);
    let mut store = profiles_load();
    if !store.get(key).map(|v| v.is_array()).unwrap_or(false) {
        store[key] = json!([]);
    }
    if let Some(arr) = store.get_mut(key).and_then(|v| v.as_array_mut()) {
        arr.retain(|x| x.get("name").and_then(|v| v.as_str()) != Some(name.as_str()));
        arr.push(profile);
    }
    profiles_store(&store);
    json!({ "ok": true, "name": name }).to_string()
}

fn profile_delete_json(body: &str) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let kind = val.get("kind").and_then(|v| v.as_str()).unwrap_or("broker").to_string();
    let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let key = profile_kind_key(&kind);
    let mut store = profiles_load();
    if let Some(arr) = store.get_mut(key).and_then(|v| v.as_array_mut()) {
        arr.retain(|x| x.get("name").and_then(|v| v.as_str()) != Some(name.as_str()));
    }
    profiles_store(&store);
    json!({ "ok": true }).to_string()
}

fn databank_json(state: &State) -> String {
    let rows = state.databank.read().unwrap();
    json!({ "rows": *rows, "count": rows.len() }).to_string()
}

fn databank_clear(state: &State) -> String {
    state.databank.write().unwrap().clear();
    persist_databank(state);
    json!({ "ok": true }).to_string()
}

fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn project_list_json() -> String {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir("data/projects") {
        for e in entries.flatten() {
            if let Some(stem) = e.path().file_stem().and_then(|s| s.to_str()) {
                out.push(stem.to_string());
            }
        }
    }
    out.sort();
    json!({ "projects": out }).to_string()
}

fn project_load_json(name: &str) -> String {
    let safe = safe_name(name);
    match std::fs::read_to_string(format!("data/projects/{safe}.json")) {
        Ok(text) => text,
        Err(e) => json!({ "error": e.to_string() }).to_string(),
    }
}

fn project_save_json(body: &str) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if name.is_empty() {
        return json!({ "error": "name required" }).to_string();
    }
    let strategy = match val.get("strategy") {
        Some(v) => v.clone(),
        None => return json!({ "error": "strategy required" }).to_string(),
    };
    let dir = std::path::Path::new("data/projects");
    if let Err(e) = std::fs::create_dir_all(dir) {
        return json!({ "error": e.to_string() }).to_string();
    }
    let path = dir.join(format!("{}.json", safe_name(&name)));
    match std::fs::write(&path, serde_json::to_string_pretty(&strategy).unwrap_or_default()) {
        Ok(_) => json!({ "ok": true, "name": name }).to_string(),
        Err(e) => json!({ "error": e.to_string() }).to_string(),
    }
}

fn export_json(body: &str) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let label = val.get("engine").and_then(|v| v.as_str()).unwrap_or("mql5");
    let engine = sqx_export::Engine::from_label(label).unwrap_or(sqx_export::Engine::Mql5);
    let strategy_val = match val.get("strategy") {
        Some(v) => v.clone(),
        None => return json!({ "error": "strategy required" }).to_string(),
    };
    let s: Strategy = match serde_json::from_value(strategy_val) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let code = sqx_export::export(&s, engine);
    json!({ "engine": engine.label(), "code": code }).to_string()
}

fn pipeline_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let cfg = config_from(&val);
    let count = val.get("count").and_then(|v| v.as_u64()).unwrap_or(300).clamp(50, 2000) as usize;
    let keep = val.get("keep").and_then(|v| v.as_u64()).unwrap_or(20).clamp(1, 200) as usize;
    let pipeline = sqx_pipeline::default_pipeline(count, keep);
    let report = sqx_pipeline::run(
        &pipeline,
        &series.bars,
        &inst,
        &cfg,
        pyp,
        &GeneratorConfig::default(),
        42,
    );
    let rows: Vec<serde_json::Value> = report
        .databank
        .iter()
        .map(|s| {
            let r = backtest(s, &series.bars, &inst, &cfg);
            let m = compute_metrics(&r, series.bars.len(), pyp);
            json!({
                "name": s.name,
                "profit_factor": m.profit_factor,
                "net_profit": m.net_profit,
                "trades": r.trades.len(),
                "dd_pct": m.max_drawdown_pct,
                "strategy": s
            })
        })
        .collect();
    databank_push(state, &rows);
    json!({ "log": report.log, "databank": rows.len() }).to_string()
}

fn optimize_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let kind = val.get("kind").and_then(|v| v.as_str()).unwrap_or("simple").to_string();
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let cfg = config_from(&val);
    let count = val.get("count").and_then(|v| v.as_u64()).unwrap_or(200).clamp(10, 2000) as usize;
    let seed = val.get("seed").and_then(|v| v.as_u64()).unwrap_or(42);
    let gen = gen_config_from(&val);
    match kind.as_str() {
        "walk-forward" => {
            let windows = val.get("windows").and_then(|v| v.as_u64()).unwrap_or(5).clamp(2, 12) as usize;
            let oos = val.get("oos").and_then(|v| v.as_f64()).unwrap_or(0.3).clamp(0.1, 0.6);
            let r = walk_forward(&series.bars, &inst, &cfg, pyp, &gen, windows, oos, count.min(200), seed);
            json!({
                "kind": "walk-forward",
                "windows": r.windows,
                "oos_trades": r.oos_trades,
                "oos_net_profit": r.oos_net_profit,
                "oos_profit_factor": r.oos_profit_factor,
                "oos_winning_pct": r.oos_winning_pct,
                "oos_max_dd_pct": r.oos_max_dd_pct
            })
            .to_string()
        }
        "wf-matrix" => {
            let mut rows = Vec::new();
            for w in [3usize, 4, 5, 6] {
                for oos in [0.2f64, 0.3, 0.4] {
                    let r = walk_forward(&series.bars, &inst, &cfg, pyp, &gen, w, oos, 50, seed);
                    rows.push(json!({
                        "windows": w, "oos": oos,
                        "net": r.oos_net_profit, "pf": r.oos_profit_factor,
                        "trades": r.oos_trades, "dd_pct": r.oos_max_dd_pct
                    }));
                }
            }
            json!({ "kind": "wf-matrix", "rows": rows }).to_string()
        }
        _ => {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let strategies: Vec<Strategy> = (0..count)
                .map(|i| random_strategy(&mut rng, &format!("s{i:04}"), &gen))
                .collect();
            let scored = backtest_batch(&strategies, &series.bars, &inst, &cfg, pyp);
            let ranked = rank(scored, Fitness::ProfitFactor, 5);
            let rows: Vec<_> = ranked
                .iter()
                .take(20)
                .map(|(s, m)| json!({
                    "name": s.name, "profit_factor": m.profit_factor,
                    "net_profit": m.net_profit, "trades": m.number_of_trades,
                    "dd_pct": m.max_drawdown_pct, "strategy": s
                }))
                .collect();
            databank_push(state, &rows);
            json!({ "kind": "simple", "results": rows }).to_string()
        }
    }
}

fn import_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let csv = val.get("csv").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if symbol.is_empty() || csv.is_empty() {
        return json!({ "error": "symbol and csv are required" }).to_string();
    }
    let bars = match sqx_data::read_csv_bars_str(&csv) {
        Ok(b) => b,
        Err(e) => return json!({ "error": e }).to_string(),
    };
    let n = bars.len();
    state.store.write().unwrap().add(&symbol, sqx_core::Timeframe::M1, bars);
    json!({ "ok": true, "symbol": symbol, "bars": n }).to_string()
}

fn delete_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if symbol.is_empty() {
        return json!({ "error": "symbol required" }).to_string();
    }
    state.store.write().unwrap().series.retain(|s| s.symbol != symbol);
    json!({ "ok": true }).to_string()
}

fn data_json(state: &State, symbol: &str) -> String {
    match find_series(state, symbol) {
        Some(s) => json!({
            "symbol": s.symbol,
            "bars": s.bars.len(),
            "first_time": s.bars.first().map(|b| b.time).unwrap_or(0),
            "last_time": s.bars.last().map(|b| b.time).unwrap_or(0)
        })
        .to_string(),
        None => json!({ "error": "no data" }).to_string(),
    }
}

fn metrics_json(m: &Metrics) -> serde_json::Value {
    json!({
        "net_profit": m.net_profit,
        "profit_factor": m.profit_factor,
        "return_dd": m.return_dd_ratio,
        "max_drawdown_pct": m.max_drawdown_pct,
        "number_of_trades": m.number_of_trades,
        "winning_pct": m.winning_pct,
        "avg_trade": m.avg_trade,
        "sharpe": m.sharpe_ratio,
        "sqn": m.sqn,
        "cagr": m.cagr,
        "stability": m.stability,
        "exposure_pct": m.exposure_pct
    })
}

fn gen_config_from(val: &serde_json::Value) -> GeneratorConfig {
    let f = |k: &str, d: f64| val.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
    let u = |k: &str, d: usize| val.get(k).and_then(|v| v.as_u64()).unwrap_or(d as u64) as usize;
    let param_ranges = val
        .get("param_ranges")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    Some((
                        r.get("indicator")?.as_str()?.to_string(),
                        r.get("param")?.as_str()?.to_string(),
                        r.get("min")?.as_f64()?,
                        r.get("max")?.as_f64()?,
                    ))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    GeneratorConfig {
        min_conditions: u("min_conditions", 1).clamp(1, 6),
        max_conditions: u("max_conditions", 3).clamp(1, 6),
        sl_points: (f("sl_min", 0.0), f("sl_max", 200.0)),
        pt_points: (f("pt_min", 0.0), f("pt_max", 400.0)),
        fixed_size: f("size", 0.1),
        long: val.get("long").and_then(|v| v.as_bool()).unwrap_or(true),
        short: val.get("short").and_then(|v| v.as_bool()).unwrap_or(true),
        param_ranges,
    }
}

fn config_from(val: &serde_json::Value) -> BacktestConfig {
    BacktestConfig {
        initial_deposit: val.get("deposit").and_then(|v| v.as_f64()).unwrap_or(10_000.0),
        spread_points: val.get("spread").and_then(|v| v.as_f64()).unwrap_or(0.0),
        slippage_points: val.get("slippage").and_then(|v| v.as_f64()).unwrap_or(0.0),
        commission_per_lot: val.get("commission").and_then(|v| v.as_f64()).unwrap_or(0.0),
        intrabar_up_first: false,
    }
}

fn robustness_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let s: Strategy = match serde_json::from_str(body) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let inst = instrument_for(&series.symbol);
    let base = config_from(&val);
    let scenarios = [
        (0.0, 0.0, 0.0),
        (10.0, 0.0, 0.0),
        (0.0, 5.0, 0.0),
        (10.0, 5.0, 0.0),
        (10.0, 5.0, 7.0),
        (20.0, 10.0, 10.0),
    ];
    let rows = what_if(&s, &series.bars, &inst, &base, &scenarios);
    let r = backtest(&s, &series.bars, &inst, &base);
    let mc = monte_carlo_trade_manipulation(&r.trades, 1000, 0.1, 42);
    json!({
        "what_if": rows.iter().map(|w| json!({
            "spread": w.spread, "slippage": w.slippage, "commission": w.commission,
            "net_profit": w.net_profit, "profit_factor": w.profit_factor
        })).collect::<Vec<_>>(),
        "mc": {
            "runs": mc.runs,
            "net_p5": mc.net_profit_5pct,
            "net_p50": mc.net_profit_50pct,
            "net_p95": mc.net_profit_95pct,
            "losing_pct": mc.losing_runs_pct,
            "max_dd_95": mc.max_drawdown_95pct
        },
        "trades": r.trades.len()
    })
    .to_string()
}

fn llm_config() -> sqx_assistant::LlmConfig {
    let base_url = std::env::var("SQX_LLM_BASE_URL").unwrap_or_else(|_| {
        let addr = "127.0.0.1:11434".parse().unwrap();
        if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(250)).is_ok() {
            "http://127.0.0.1:11434/v1".to_string()
        } else {
            "https://api.groq.com/openai/v1".to_string()
        }
    });
    let model = std::env::var("SQX_LLM_MODEL").unwrap_or_else(|_| {
        if base_url.contains("11434") {
            "llama3.1".to_string()
        } else {
            "llama-3.3-70b-versatile".to_string()
        }
    });
    let api_key = std::env::var("SQX_LLM_API_KEY").unwrap_or_default();
    sqx_assistant::LlmConfig { base_url, model, api_key, max_rounds: 6 }
}

fn assistant_config_json() -> String {
    let cfg = llm_config();
    json!({
        "base_url": cfg.base_url,
        "model": cfg.model,
        "api_key_set": !cfg.api_key.is_empty(),
    })
    .to_string()
}

fn assistant_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let question = val.get("question").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if question.is_empty() {
        return json!({ "error": "missing question" }).to_string();
    }
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let cfg = llm_config();
    let mut a = sqx_assistant::Assistant::new(cfg, &series.symbol, series.bars.clone(), inst, pyp);
    match a.run(&question) {
        Ok(answer) => json!({ "answer": answer }).to_string(),
        Err(e) => json!({ "error": e }).to_string(),
    }
}

fn backtest_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let s: Strategy = match serde_json::from_str(body) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let cfg = config_from(&val);
    let result = backtest(&s, &series.bars, &inst, &cfg);
    let m = compute_metrics(&result, series.bars.len(), pyp);
    let equity = result.equity.clone();
    json!({
        "metrics": metrics_json(&m),
        "final_equity": result.final_equity,
        "equity": equity,
        "trades": result.trades.len(),
        "pseudocode": render_pseudocode(&s),
        "symbol": series.symbol
    })
    .to_string()
}

fn generate_json(body: &str, state: &State) -> String {
    #[derive(serde::Deserialize)]
    struct Req {
        count: usize,
        seed: u64,
        #[serde(default)]
        symbol: String,
        #[serde(default)]
        deposit: Option<f64>,
        #[serde(default)]
        spread: Option<f64>,
        #[serde(default)]
        slippage: Option<f64>,
        #[serde(default)]
        commission: Option<f64>,
        #[serde(default)]
        min_conditions: Option<usize>,
        #[serde(default)]
        max_conditions: Option<usize>,
        #[serde(default)]
        sl_min: Option<f64>,
        #[serde(default)]
        sl_max: Option<f64>,
        #[serde(default)]
        pt_min: Option<f64>,
        #[serde(default)]
        pt_max: Option<f64>,
        #[serde(default)]
        size: Option<f64>,
        #[serde(default)]
        long: Option<bool>,
        #[serde(default)]
        short: Option<bool>,
        #[serde(default)]
        param_ranges: Vec<ParamRange>,
    }
    #[derive(serde::Deserialize)]
    struct ParamRange {
        indicator: String,
        param: String,
        min: f64,
        max: f64,
    }
    let req: Req = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let series = match find_series(state, &req.symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let count = req.count.clamp(1, 2000);
    let gen = GeneratorConfig {
        min_conditions: req.min_conditions.unwrap_or(1).clamp(1, 6),
        max_conditions: req.max_conditions.unwrap_or(3).clamp(1, 6),
        sl_points: (req.sl_min.unwrap_or(0.0), req.sl_max.unwrap_or(200.0)),
        pt_points: (req.pt_min.unwrap_or(0.0), req.pt_max.unwrap_or(400.0)),
        fixed_size: req.size.unwrap_or(0.1),
        long: req.long.unwrap_or(true),
        short: req.short.unwrap_or(true),
        param_ranges: req.param_ranges.into_iter().map(|r| (r.indicator, r.param, r.min, r.max)).collect(),
    };
    let mut rng = rand::rngs::StdRng::seed_from_u64(req.seed);
    let strategies: Vec<Strategy> = (0..count)
        .map(|i| random_strategy(&mut rng, &format!("s{i:04}"), &gen))
        .collect();
    let cfg = BacktestConfig {
        initial_deposit: req.deposit.unwrap_or(10_000.0),
        spread_points: req.spread.unwrap_or(0.0),
        slippage_points: req.slippage.unwrap_or(0.0),
        commission_per_lot: req.commission.unwrap_or(0.0),
        intrabar_up_first: false,
    };
    let scored = backtest_batch(&strategies, &series.bars, &inst, &cfg, pyp);
    let ranked = rank(scored, Fitness::ProfitFactor, 5);
    let rows: Vec<_> = ranked
        .iter()
        .take(20)
        .map(|(s, m)| {
            json!({
                "name": s.name,
                "profit_factor": m.profit_factor,
                "net_profit": m.net_profit,
                "trades": m.number_of_trades,
                "dd_pct": m.max_drawdown_pct,
                "strategy": s
            })
        })
        .collect();
    databank_push(state, &rows);
    json!({ "results": rows, "symbol": series.symbol }).to_string()
}

fn portfolio_json(body: &str, state: &State) -> String {
    #[derive(serde::Deserialize)]
    struct Req {
        count: usize,
        seed: u64,
        top: usize,
        min_k: usize,
        max_k: usize,
        #[serde(default)]
        symbol: String,
    }
    let req: Req = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let series = match find_series(state, &req.symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let count = req.count.clamp(10, 2000);
    let gen = GeneratorConfig::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(req.seed);
    let strategies: Vec<Strategy> = (0..count)
        .map(|i| random_strategy(&mut rng, &format!("p{i:04}"), &gen))
        .collect();
    let names: Vec<String> = strategies.iter().map(|s| s.name.clone()).collect();
    let cfg = BacktestConfig::default();
    let results = sqx_portfolio::backtest_all(&strategies, &series.bars, &inst, &cfg);
    let mets: Vec<Metrics> = results
        .iter()
        .map(|r| compute_metrics(r, series.bars.len(), pyp))
        .collect();
    let mut order: Vec<usize> = (0..results.len()).collect();
    order.sort_by(|&a, &b| {
        mets[b].profit_factor.partial_cmp(&mets[a].profit_factor).unwrap_or(std::cmp::Ordering::Equal)
    });
    order.retain(|&i| mets[i].number_of_trades >= 5.0 && mets[i].profit_factor.is_finite());
    order.truncate(req.top.clamp(2, 15));
    if order.len() < 2 {
        return json!({ "error": "not enough qualifying strategies" }).to_string();
    }
    let sel: Vec<sqx_backtest::BacktestResult> = order.iter().map(|&i| results[i].clone()).collect();
    let sel_names: Vec<String> = order.iter().map(|&i| names[i].clone()).collect();
    let subset = sqx_portfolio::best_subset(&sel, req.min_k, req.max_k, cfg.initial_deposit);
    let comp: Vec<sqx_backtest::BacktestResult> = subset.iter().map(|&i| sel[i].clone()).collect();
    let comp_names: Vec<String> = subset.iter().map(|&i| sel_names[i].clone()).collect();
    let weights = sqx_portfolio::equal_weights(comp.len());
    let pf = sqx_portfolio::combine(&comp_names, &comp, &weights, cfg.initial_deposit, series.bars.len(), pyp);
    let comps: Vec<_> = pf
        .components
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "weight": c.weight,
                "net_profit": c.net_profit,
                "max_drawdown_pct": c.max_drawdown_pct,
                "trades": c.trades
            })
        })
        .collect();
    json!({
        "metrics": metrics_json(&pf.metrics),
        "equity": pf.equity,
        "correlation": pf.correlation,
        "components": comps,
        "symbol": series.symbol
    })
    .to_string()
}

fn pseudocode_json(body: &str) -> String {
    let s: Strategy = match serde_json::from_str(body) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    json!({ "pseudocode": render_pseudocode(&s) }).to_string()
}
