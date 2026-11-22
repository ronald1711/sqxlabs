//! # sqx-data
//!
//! The Data Manager: a store of symbol/timeframe bar series, CSV import, and
//! listing. This is the foundation for multi-symbol backtests and multi-market
//! robustness checks.

use sqx_core::{Bar, Tick, Timeframe};

/// One symbol/timeframe bar series.
#[derive(Debug, Clone)]
pub struct SymbolSeries {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub bars: Vec<Bar>,
}

/// Summary of one series for the UI.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub symbol: String,
    pub timeframe: String,
    pub bars: usize,
    pub first: i64,
    pub last: i64,
}

/// A store of symbol/timeframe series.
#[derive(Debug, Clone, Default)]
pub struct DataStore {
    pub series: Vec<SymbolSeries>,
}

impl DataStore {
    pub fn new() -> Self {
        Self { series: Vec::new() }
    }

    /// Add or replace a series.
    pub fn add(&mut self, symbol: &str, timeframe: Timeframe, bars: Vec<Bar>) {
        self.series.retain(|s| !(s.symbol == symbol && s.timeframe == timeframe));
        self.series.push(SymbolSeries { symbol: symbol.to_string(), timeframe, bars });
    }

    /// Import a CSV file as a symbol series.
    pub fn import_csv(&mut self, path: &str, symbol: &str, timeframe: Timeframe) -> Result<usize, String> {
        let bars = read_csv_bars(path)?;
        let n = bars.len();
        self.add(symbol, timeframe, bars);
        Ok(n)
    }

    /// Import every *.csv in a directory; the symbol is the file stem.
    pub fn import_dir(&mut self, dir: &str, timeframe: Timeframe) -> Result<Vec<(String, usize)>, String> {
        let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("csv") {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("symbol").to_string();
                let p = path.to_string_lossy().to_string();
                match self.import_csv(&p, &stem, timeframe) {
                    Ok(n) => out.push((stem, n)),
                    Err(_) => {},
                }
            }
        }
        Ok(out)
    }

    pub fn get(&self, symbol: &str) -> Option<&SymbolSeries> {
        self.series.iter().find(|s| s.symbol == symbol)
    }

    pub fn symbols(&self) -> Vec<String> {
        self.series.iter().map(|s| s.symbol.clone()).collect()
    }

    pub fn list(&self) -> Vec<SymbolInfo> {
        self.series.iter().map(|s| SymbolInfo {
            symbol: s.symbol.clone(),
            timeframe: s.timeframe.label().to_string(),
            bars: s.bars.len(),
            first: s.bars.first().map(|b| b.time).unwrap_or(0),
            last: s.bars.last().map(|b| b.time).unwrap_or(0),
        }).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }

    pub fn len(&self) -> usize {
        self.series.len()
    }
}

/// Deterministic synthetic bars for demos and tests (no data source required).
pub fn synthetic_bars(symbol: &str, n: usize) -> Vec<Bar> {
    let mut out = Vec::with_capacity(n);
    let mut price = if symbol.to_ascii_uppercase().starts_with("USDJPY") { 110.0 } else { 1.10 };
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15
        ^ symbol
            .bytes()
            .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64));
    let mut t = 1_700_000_000_000i64;
    for _ in 0..n {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let r = ((state >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
        let open = price;
        price = (price + r * 0.0004).max(0.01);
        let close = price;
        let hi = open.max(close) + r.abs() * 0.0002;
        let lo = open.min(close) - r.abs() * 0.0002;
        out.push(Bar { time: t, open, high: hi, low: lo, close, volume: 100.0 });
        t += 60_000;
    }
    out
}

/// Parse a tick CSV with header timestamp,bid,ask (time in unix seconds).
pub fn read_ticks_csv(path: &str) -> Result<Vec<Tick>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if idx == 0 && line.starts_with("timestamp") {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 3 {
            continue;
        }
        let time: i64 = cols[0].trim().parse().map_err(|e| format!("bad tick time line {}: {e}", idx + 1))?;
        let bid: f64 = cols[1].trim().parse().map_err(|e| format!("bad bid line {}: {e}", idx + 1))?;
        let ask: f64 = cols[2].trim().parse().map_err(|e| format!("bad ask line {}: {e}", idx + 1))?;
        out.push(Tick { time: time * 1000, bid, ask });
    }
    if out.is_empty() {
        return Err(format!("no ticks in {path}"));
    }
    Ok(out)
}

/// Parse a CSV with header timestamp,open,high,low,close,volume (time in unix seconds).
pub fn read_csv_bars(path: &str) -> Result<Vec<Bar>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut bars = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if idx == 0 && line.starts_with("timestamp") {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 6 {
            continue;
        }
        let time: i64 = cols[0].trim().parse().map_err(|e| format!("bad timestamp line {}: {e}", idx + 1))?;
        let open: f64 = cols[1].trim().parse().map_err(|e| format!("bad open line {}: {e}", idx + 1))?;
        let high: f64 = cols[2].trim().parse().map_err(|e| format!("bad high line {}: {e}", idx + 1))?;
        let low: f64 = cols[3].trim().parse().map_err(|e| format!("bad low line {}: {e}", idx + 1))?;
        let close: f64 = cols[4].trim().parse().map_err(|e| format!("bad close line {}: {e}", idx + 1))?;
        let volume: f64 = cols[5].trim().parse().unwrap_or(0.0);
        bars.push(Bar::new(time * 1000, open, high, low, close, volume));
    }
    if bars.is_empty() {
        return Err(format!("no bars in {path}"));
    }
    Ok(bars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_bars_are_well_formed() {
        let bars = synthetic_bars("EURUSD", 100);
        assert_eq!(bars.len(), 100);
        assert!(bars.iter().all(|b| b.high >= b.low && b.close > 0.0));
        assert!(bars.windows(2).all(|w| w[0].time < w[1].time));
    }
}
