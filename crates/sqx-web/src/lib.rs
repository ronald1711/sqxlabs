//! # sqx-web
//!
//! A dependency-free embedded HTTP server that serves a single-page dashboard
//! over the trading engine. Start it with the CLI serve command.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use rand::SeedableRng;
use serde_json::json;

use sqx_backtest::{backtest, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_metrics::{compute as compute_metrics, Metrics};
use sqx_optimizer::{backtest_batch, rank, random_strategy, Fitness, GeneratorConfig};
use sqx_strategy::{render as render_pseudocode, Strategy};

const INDEX: &str = include_str!("../assets/index.html");

struct State {
    bars: Vec<Bar>,
    instrument: Instrument,
    periods_per_year: f64,
}

pub fn serve(addr: &str, bars: Vec<Bar>, instrument: Instrument) -> std::io::Result<()> {
    let pyp = periods_per_year(&bars);
    let state = Arc::new(State { bars, instrument, periods_per_year: pyp });
    let listener = TcpListener::bind(addr)?;
    println!("SqxLabs web app: http://{addr}");
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

fn periods_per_year(bars: &[Bar]) -> f64 {
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
    eprintln!("[sqx-web] {} {}", method, path);

    let (status, ctype, payload) = route(&method, &path, &body_str, state);
    let resp = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}", payload.len());
    stream.write_all(resp.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn route(method: &str, path: &str, body: &str, state: &State) -> (&'static str, &'static str, String) {
    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => ("200 OK", "text/html; charset=utf-8", INDEX.to_string()),
        ("GET", "/api/indicators") => ("200 OK", "application/json", indicators_json()),
        ("GET", "/api/data") => ("200 OK", "application/json", data_json(state)),
        ("POST", "/api/backtest") => ("200 OK", "application/json", backtest_json(body, state)),
        ("POST", "/api/generate") => ("200 OK", "application/json", generate_json(body, state)),
        ("POST", "/api/pseudocode") => ("200 OK", "application/json", pseudocode_json(body)),
        _ => ("404 Not Found", "text/plain", "not found".to_string()),
    }
}

fn indicators_json() -> String {
    let list: Vec<_> = sqx_strategy::indicator_catalog()
        .iter()
        .map(|s| json!({ "name": s.name, "label": s.label, "oscillator": s.is_oscillator }))
        .collect();
    json!({ "indicators": list }).to_string()
}

fn data_json(state: &State) -> String {
    let first = state.bars.first().map(|b| b.time).unwrap_or(0);
    let last = state.bars.last().map(|b| b.time).unwrap_or(0);
    json!({ "symbol": state.instrument.name, "bars": state.bars.len(), "first_time": first, "last_time": last }).to_string()
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

fn backtest_json(body: &str, state: &State) -> String {
    let s: Strategy = match serde_json::from_str(body) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let cfg = BacktestConfig::default();
    let result = backtest(&s, &state.bars, &state.instrument, &cfg);
    let m = compute_metrics(&result, state.bars.len(), state.periods_per_year);
    let equity = result.equity.clone();
    json!({
        "metrics": metrics_json(&m),
        "final_equity": result.final_equity,
        "equity": equity,
        "trades": result.trades.len(),
        "pseudocode": render_pseudocode(&s)
    }).to_string()
}

fn generate_json(body: &str, state: &State) -> String {
    #[derive(serde::Deserialize)]
    struct Req {
        count: usize,
        seed: u64,
    }
    let req: Req = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let count = req.count.clamp(1, 2000);
    let gen = GeneratorConfig::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(req.seed);
    let strategies: Vec<Strategy> = (0..count)
        .map(|i| random_strategy(&mut rng, &format!("s{i:04}"), &gen))
        .collect();
    let cfg = BacktestConfig::default();
    let scored = backtest_batch(&strategies, &state.bars, &state.instrument, &cfg, state.periods_per_year);
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
                "dd_pct": m.max_drawdown_pct
            })
        })
        .collect();
    json!({ "results": rows }).to_string()
}

fn pseudocode_json(body: &str) -> String {
    let s: Strategy = match serde_json::from_str(body) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    json!({ "pseudocode": render_pseudocode(&s) }).to_string()
}
