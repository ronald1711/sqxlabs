//! # sqx-data
//!
//! The Data Manager: a store of symbol/timeframe bar series, CSV import, and
//! listing. This is the foundation for multi-symbol backtests and multi-market
//! robustness checks.

use sqx_core::{Bar, Tick, Timeframe};

mod questdb;
pub mod quality;

pub use questdb::{QuestdbClient, QuestdbConfig};

/// One symbol/timeframe bar series.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolSeries {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub calendar: Option<quality::MarketCalendar>,
    /// Exact clock-normalization evidence, when supplied; callers must verify against bars.
    pub normalization: Option<normalization::Bundle>,
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
        self.series.push(SymbolSeries { symbol: symbol.to_string(), timeframe, calendar: None, normalization: None, bars });
    }

    /// Import a CSV file as a symbol series.
    pub fn import_csv(&mut self, path: &str, symbol: &str, timeframe: Timeframe) -> Result<usize, String> {
        let bars = read_csv_bars(path)?;
        let quality=quality::audit(&bars,timeframe,quality::now_ms(),None);
        if !quality.errors.is_empty(){return Err(quality.errors.join("; "));}
        let n = bars.len();
        self.add(symbol, timeframe, bars);
        Ok(n)
    }

    /// Import every *.csv in a directory; the symbol is the file stem.
    pub fn import_dir(&mut self, dir: &str, timeframe: Timeframe) -> Result<Vec<(String, usize)>, String> {
        let mut paths=std::fs::read_dir(dir).map_err(|e|e.to_string())?
            .map(|e|e.map(|e|e.path())).collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        paths.sort();
        let mut staged=Vec::new();let cutoff=quality::now_ms();
        for path in paths.into_iter().filter(|p|p.extension().and_then(|e|e.to_str())==Some("csv")) {
            let symbol=path.file_stem().and_then(|s|s.to_str()).ok_or("invalid CSV filename")?.to_string();
            let bars=read_csv_bars(&path.to_string_lossy()).map_err(|e|format!("{}: {e}",path.display()))?;
            let report=quality::audit(&bars,timeframe,cutoff,None);
            if !report.errors.is_empty(){return Err(format!("{}: {}",path.display(),report.errors.join("; ")));}
            staged.push((symbol,bars));
        }
        let mut out=Vec::new();
        for (symbol,bars) in staged {out.push((symbol.clone(),bars.len()));self.add(&symbol,timeframe,bars);}
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
    for i in 0..n {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let r = ((state >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
        let cycle = (i as f64 * 0.02).sin();
        let open = price;
        price = (price + cycle * 0.0005 + r * 0.0002).max(0.01);
        let close = price;
        let hi = open.max(close) + r.abs() * 0.0002;
        let lo = open.min(close) - r.abs() * 0.0002;
        out.push(Bar { time: t, open, high: hi, low: lo, close, volume: 100.0 });
        t += 60_000;
    }
    out
}

// Headerless numeric CSV remains supported. A header is a schema declaration,
// not a comment: silently ignoring a reordered or unit-renamed header corrupts data.
fn csv_header(line:&str,expected:&[&str])->Result<bool,String>{
    let line=line.trim_start_matches('\u{feff}').trim();
    let fields:Vec<_>=line.split(',').map(str::trim).collect();
    if fields.first().is_some_and(|field|matches!(*field,"timestamp_broker"|"timestamp_broker_ms")) {
        return Err("raw broker-clock CSV cannot be imported as UTC; normalize with an explicit clock policy first".into());
    }
    if fields.first().is_some_and(|field|field.starts_with("timestamp")) {
        if fields!=expected{return Err(format!("invalid CSV header at line 1: expected {} (Unix seconds)",expected.join(",")));}
        return Ok(true);
    }
    Ok(false)
}

/// Parse tick CSV: timestamp,bid,ask uses Unix seconds; timestamp_ms,bid,ask
/// explicitly uses Unix milliseconds. Headerless input remains seconds.
pub fn read_ticks_csv(path: &str) -> Result<Vec<Tick>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    read_ticks_csv_str(&text)
}

/// Parse tick CSV text with explicit seconds/milliseconds headers.
pub fn read_ticks_csv_str(text: &str) -> Result<Vec<Tick>, String> {
    let mut out = Vec::new();
    let mut milliseconds=false;
    for (idx, line) in text.lines().enumerate() {
        if idx==0 && line.trim_start_matches('\u{feff}').split(',').map(str::trim).collect::<Vec<_>>()==["timestamp_ms","bid","ask"] {
            milliseconds=true;continue;
        }
        if idx == 0 && csv_header(line,&["timestamp","bid","ask"])? {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() != 3 { return Err(format!("expected three tick columns at line {}",idx+1)); }
        let time: i64 = cols[0].trim().parse().map_err(|e| format!("bad tick time line {}: {e}", idx + 1))?;
        let bid: f64 = cols[1].trim().parse().map_err(|e| format!("bad bid line {}: {e}", idx + 1))?;
        let ask: f64 = cols[2].trim().parse().map_err(|e| format!("bad ask line {}: {e}", idx + 1))?;
        let time=if milliseconds {time}else{time.checked_mul(1000).ok_or_else(||format!("tick timestamp overflow at line {}",idx+1))?};
        if !bid.is_finite() || !ask.is_finite() || bid<=0.0 || ask<bid || out.last().is_some_and(|t:&Tick|t.time>time){return Err(format!("invalid or unsorted tick at line {}",idx+1));}
        out.push(Tick { time, bid, ask });
    }
    if out.is_empty() {
        return Err("no ticks in CSV".into());
    }
    Ok(out)
}

/// Parse a CSV with header timestamp,open,high,low,close,volume (time in unix seconds).
pub fn read_csv_bars(path: &str) -> Result<Vec<Bar>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let bars = read_csv_bars_str(&text)?;
    if bars.is_empty() {
        return Err(format!("no bars in {path}"));
    }
    Ok(bars)
}

/// Parse bar CSV text (header timestamp,open,high,low,close,volume; time in seconds).
pub fn read_csv_bars_str(text: &str) -> Result<Vec<Bar>, String> {
    let mut bars = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if idx == 0 && csv_header(line,&["timestamp","open","high","low","close","volume"])? {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() != 6 { return Err(format!("expected six OHLCV columns at line {}",idx+1)); }
        let time: i64 = cols[0].trim().parse().map_err(|e| format!("bad timestamp line {}: {e}", idx + 1))?;
        let open: f64 = cols[1].trim().parse().map_err(|e| format!("bad open line {}: {e}", idx + 1))?;
        let high: f64 = cols[2].trim().parse().map_err(|e| format!("bad high line {}: {e}", idx + 1))?;
        let low: f64 = cols[3].trim().parse().map_err(|e| format!("bad low line {}: {e}", idx + 1))?;
        let close: f64 = cols[4].trim().parse().map_err(|e| format!("bad close line {}: {e}", idx + 1))?;
        let volume: f64 = cols[5].trim().parse().map_err(|e|format!("bad volume line {}: {e}",idx+1))?;
        let time=time.checked_mul(1000).ok_or_else(||format!("timestamp overflow at line {}",idx+1))?;
        if [open,high,low,close].iter().any(|p|!p.is_finite() || *p<=0.0) || !volume.is_finite() || volume<0.0 || low>open.min(close) || high<open.max(close)
            || bars.last().is_some_and(|b:&Bar|b.time>=time){return Err(format!("invalid OHLCV or duplicate/reversed time at line {}",idx+1));}
        bars.push(Bar::new(time, open, high, low, close, volume));
    }
    Ok(bars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reordered_or_mislabeled_bar_headers_are_rejected(){
        for header in ["timestamp,close,high,low,open,volume","timestamp_ms,open,high,low,close,volume","timestamp,open,high,low,close,turnover"] {
            let text=format!("{header}\n1700000000,1.05,1.1,0.9,1,10\n");
            assert!(read_csv_bars_str(&text).is_err(),"accepted misleading header: {header}");
        }
        let parsed=read_csv_bars_str("timestamp,open,high,low,close,volume\n1700000000,1.05,1.1,0.9,1,10\n").unwrap();
        assert_eq!(parsed[0].open,1.05);assert_eq!(parsed[0].close,1.0);
    }

    #[test]
    fn tick_headers_cannot_swap_quote_sides_or_timestamp_units(){
        let path=std::env::temp_dir().join(format!("sqx-tick-header-{}-{}.csv",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        for header in ["timestamp,ask,bid","timestamp_us,bid,ask","timestamp_ms,ask,bid","timestamp,bid,mid"] {
            std::fs::write(&path,format!("{header}\n1700000000,1,1\n")).unwrap();
            let error=read_ticks_csv(path.to_str().unwrap()).unwrap_err();assert!(error.contains("invalid CSV header"),"{error}");
        }
        std::fs::write(&path,"timestamp,bid,ask\n1700000000,1,1.01\n").unwrap();
        let ticks=read_ticks_csv(path.to_str().unwrap()).unwrap();assert_eq!(ticks[0].time,1700000000000);assert_eq!(ticks[0].bid,1.0);assert_eq!(ticks[0].ask,1.01);
        std::fs::write(&path,"timestamp_ms,bid,ask\n1700000000123,1,1.01\n1700000000456,1.001,1.011\n").unwrap();
        let precise=read_ticks_csv(path.to_str().unwrap()).unwrap();assert_eq!(precise[0].time,1700000000123);assert_eq!(precise[1].time,1700000000456);
        std::fs::write(&path,"timestamp_ms,bid,ask\n1700000000456,1,1.01\n1700000000123,1,1.01\n").unwrap();
        assert!(read_ticks_csv(path.to_str().unwrap()).unwrap_err().contains("unsorted"));
        std::fs::remove_file(path).unwrap();
        assert!(read_csv_bars_str("\u{feff}timestamp, open, high, low, close, volume\n1700000000,1,1.1,0.9,1,1\n").is_ok());
        assert!(read_csv_bars_str("1700000000,1,1.1,0.9,1,1\n").is_ok());
    }

    #[test]
    fn synthetic_bars_are_well_formed() {
        let bars = synthetic_bars("EURUSD", 100);
        assert_eq!(bars.len(), 100);
        assert!(bars.iter().all(|b| b.high >= b.low && b.close > 0.0));
        assert!(bars.windows(2).all(|w| w[0].time < w[1].time));
    }
}

pub mod normalization;
