//! QuestDB data source over the HTTP REST API (`/exec`).
//!
//! Reads OHLCV candles from the `candles` table written by the AlgoTrader
//! ingester: `candles(symbol, timeframe, ts, open, high, low, close, volume)`.
//! The table stores multiple native timeframes (forex: `1m`..`1d`; crypto:
//! `1h`/`1d`), so a requested timeframe is read directly where possible and
//! only aggregated with `SAMPLE BY` when it is not stored.

use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use sqx_core::{Bar, Timeframe};

/// QuestDB connection settings.
#[derive(Debug, Clone)]
pub struct QuestdbConfig {
    /// Hostname or IP of the QuestDB server.
    pub host: String,
    /// HTTP REST port (default 9000).
    pub port: u16,
    /// Basic-auth user for the HTTP API (default `admin`).
    pub user: Option<String>,
    /// Basic-auth password for the HTTP API (default `quest`).
    pub password: Option<String>,
}

impl Default for QuestdbConfig {
    fn default() -> Self {
        Self {
            host: "192.168.10.33".to_string(),
            port: 9000,
            user: Some("admin".to_string()),
            password: Some("quest".to_string()),
        }
    }
}

impl QuestdbConfig {
    /// Build from `SQX_QUESTDB_HOST`, `SQX_QUESTDB_PORT`, `SQX_QUESTDB_USER`
    /// and `SQX_QUESTDB_PASS` (each optional; unset values keep the default).
    pub fn from_env() -> Self {
        let mut c = Self::default();
        if let Ok(h) = std::env::var("SQX_QUESTDB_HOST") {
            if !h.is_empty() {
                c.host = h;
            }
        }
        if let Ok(p) = std::env::var("SQX_QUESTDB_PORT") {
            if let Ok(p) = p.parse() {
                c.port = p;
            }
        }
        if let Ok(u) = std::env::var("SQX_QUESTDB_USER") {
            c.user = if u.is_empty() { None } else { Some(u) };
        }
        if let Ok(p) = std::env::var("SQX_QUESTDB_PASS") {
            c.password = if p.is_empty() { None } else { Some(p) };
        }
        c
    }

    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

/// Blocking QuestDB client over the HTTP REST API.
pub struct QuestdbClient {
    agent: ureq::Agent,
    base_url: String,
    auth_header: Option<String>,
}

impl QuestdbClient {
    pub fn new(config: &QuestdbConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(60))
            .build();
        let auth_header = match (&config.user, &config.password) {
            (Some(u), Some(p)) => Some(format!("Basic {}", base64(&format!("{u}:{p}")))),
            _ => None,
        };
        Self { agent, base_url: config.base_url(), auth_header }
    }

    /// List distinct symbols in the `candles` table.
    pub fn list_symbols(&self) -> Result<Vec<String>, String> {
        let rows = self.exec("SELECT DISTINCT symbol FROM candles ORDER BY symbol ASC")?;
        let mut out = Vec::new();
        for row in rows {
            if let Some(s) = row.as_array().and_then(|a| a.first()).and_then(|v| v.as_str()) {
                out.push(s.to_string());
            }
        }
        Ok(out)
    }

    /// Fetch bars for a symbol/timeframe.
    ///
    /// `start_ms`/`end_ms` are optional unix-millisecond bounds; `limit`
    /// returns the most recent `limit` bars (ascending).
    pub fn fetch_bars(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_ms: Option<i64>,
        end_ms: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<Bar>, String> {
        let sym = escape_sql_str(symbol);

        let bars = if let Some(label) = direct_label(timeframe) {
            // The requested timeframe is stored natively: reverse scan + LIMIT
            // returns the most recent bars without scanning the whole table.
            let mut where_clause = format!("symbol = '{sym}' AND timeframe = '{label}'");
            if let Some(s) = start_ms {
                where_clause.push_str(&format!(" AND ts >= '{}'", iso_utc(s)));
            }
            if let Some(e) = end_ms {
                where_clause.push_str(&format!(" AND ts <= '{}'", iso_utc(e)));
            }
            let sql = match limit {
                Some(n) => format!(
                    "SELECT ts, open, high, low, close, volume FROM candles WHERE {where_clause} ORDER BY ts DESC LIMIT {n}"
                ),
                None => format!(
                    "SELECT ts, open, high, low, close, volume FROM candles WHERE {where_clause} ORDER BY ts ASC"
                ),
            };
            let rows = self.exec(&sql)?;
            let mut bars: Vec<Bar> = rows.iter().filter_map(row_to_bar).collect();
            if let Some(n) = limit {
                bars.reverse();
                bars.truncate(n);
            }
            bars
        } else if let Some((base, unit)) = aggregate_spec(timeframe) {
            // Not stored natively: aggregate from a coarser/finer base. The 1m
            // base is huge, so bound intraday aggregation by a time window;
            // daily-base aggregation (W1/MN1) is small and left mostly unbounded.
            let end = end_ms.unwrap_or_else(now_ms);
            let mut where_clause = format!("symbol = '{sym}' AND timeframe = '{base}' AND ts <= '{}'", iso_utc(end));
            if base == "1m" {
                // The 1m base is huge, so bound intraday aggregation by a window.
                let start = start_ms.unwrap_or_else(|| match (limit, timeframe.minutes()) {
                    (Some(n), Some(mins)) => end.saturating_sub(n as i64 * mins * 60_000 * 2),
                    _ => end.saturating_sub(90 * 24 * 3_600_000),
                });
                where_clause.push_str(&format!(" AND ts >= '{}'", iso_utc(start)));
            } else if let Some(s) = start_ms {
                // Daily base is small; only bound when the caller asks for it.
                where_clause.push_str(&format!(" AND ts >= '{}'", iso_utc(s)));
            }
            let sql = format!(
                "SELECT ts, first(open) AS open, max(high) AS high, min(low) AS low, last(close) AS close, sum(volume) AS volume \
                 FROM candles WHERE {where_clause} SAMPLE BY {unit} ALIGN TO CALENDAR ORDER BY ts ASC"
            );
            let rows = self.exec(&sql)?;
            let mut bars: Vec<Bar> = rows.iter().filter_map(row_to_bar).collect();
            if let Some(n) = limit {
                let start = bars.len().saturating_sub(n);
                bars = bars[start..].to_vec();
            }
            bars
        } else {
            return Err(format!("timeframe {timeframe} is not supported for QuestDB candles"));
        };

        if bars.is_empty() {
            return Err(format!("no bars for {symbol} {timeframe}"));
        }
        Ok(bars)
    }

    /// Run SQL against `/exec` and return the `dataset` rows.
    fn exec(&self, sql: &str) -> Result<Vec<serde_json::Value>, String> {
        let url = format!("{}/exec?query={}", self.base_url, pct_encode(sql));
        let req = self.agent.get(&url).set("Accept", "application/json");
        let req = match &self.auth_header {
            Some(a) => req.set("Authorization", a),
            None => req,
        };
        let body = match req.call() {
            Ok(resp) => resp.into_string().map_err(|e| e.to_string())?,
            Err(ureq::Error::Status(code, resp)) => {
                let detail = resp.into_string().unwrap_or_default();
                return Err(format!("QuestDB HTTP {code}: {}", detail.trim().chars().take(300).collect::<String>()));
            }
            Err(ureq::Error::Transport(t)) => return Err(format!("QuestDB transport error: {t}")),
        };
        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("QuestDB JSON parse: {e}"))?;
        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            if !err.is_empty() {
                return Err(format!("QuestDB error: {err}"));
            }
        }
        v.get("dataset")
            .and_then(|d| d.as_array())
            .cloned()
            .ok_or_else(|| format!("QuestDB response had no dataset: {}", body.chars().take(300).collect::<String>()))
    }
}

/// Directly-stored timeframe label (reverse-scan read, no aggregation).
fn direct_label(tf: Timeframe) -> Option<&'static str> {
    use Timeframe::*;
    Some(match tf {
        M1 => "1m",
        M5 => "5m",
        M15 => "15m",
        M30 => "30m",
        H1 => "1h",
        H4 => "4h",
        D1 => "1d",
        _ => return None,
    })
}

/// SAMPLE BY aggregation spec for timeframes not stored natively:
/// (base timeframe label, sample unit).
fn aggregate_spec(tf: Timeframe) -> Option<(&'static str, &'static str)> {
    use Timeframe::*;
    Some(match tf {
        M2 => ("1m", "2m"),
        M3 => ("1m", "3m"),
        M10 => ("1m", "10m"),
        H2 => ("1m", "2h"),
        H6 => ("1m", "6h"),
        H8 => ("1m", "8h"),
        H12 => ("1m", "12h"),
        W1 => ("1d", "7d"),
        MN1 => ("1d", "1M"),
        _ => return None,
    })
}

/// Convert one `dataset` row (ts string, o, h, l, c, v) into a `Bar`.
fn row_to_bar(row: &serde_json::Value) -> Option<Bar> {
    let arr = row.as_array()?;
    let ts_str = arr.first()?.as_str()?;
    let ts = DateTime::parse_from_rfc3339(ts_str).ok()?.timestamp_millis();
    let f = |i: usize| arr.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0);
    Some(Bar::new(ts, f(1), f(2), f(3), f(4), f(5)))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn iso_utc(ms: i64) -> String {
    match DateTime::<Utc>::from_timestamp_millis(ms) {
        Some(dt) => dt.to_rfc3339_opts(SecondsFormat::Micros, true),
        None => "1970-01-01T00:00:00.000000Z".to_string(),
    }
}

/// Escape a string literal for SQL single-quote context.
fn escape_sql_str(s: &str) -> String {
    s.replace('\'', "''")
}

/// Percent-encode a string for a URL query component.
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Minimal standard-alphabet base64 for Basic auth credentials.
fn base64(s: &str) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = s.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqx_core::Timeframe::*;

    #[test]
    fn direct_and_aggregate_mapping() {
        assert_eq!(direct_label(M1), Some("1m"));
        assert_eq!(direct_label(M5), Some("5m"));
        assert_eq!(direct_label(H4), Some("4h"));
        assert_eq!(direct_label(D1), Some("1d"));
        assert_eq!(direct_label(W1), None);
        assert_eq!(aggregate_spec(W1), Some(("1d", "7d")));
        assert_eq!(aggregate_spec(MN1), Some(("1d", "1M")));
        assert_eq!(aggregate_spec(M3), Some(("1m", "3m")));
        assert_eq!(direct_label(Tick), None);
        assert_eq!(aggregate_spec(Tick), None);
    }

    #[test]
    fn base64_encodes_basic_auth() {
        assert_eq!(base64("admin:quest"), "YWRtaW46cXVlc3Q=");
    }

    #[test]
    fn pct_encodes_query() {
        assert_eq!(pct_encode("a b'c"), "a%20b%27c");
        assert_eq!(pct_encode("symbol = 'EURUSD'"), "symbol%20%3D%20%27EURUSD%27");
    }

    #[test]
    fn iso_utc_roundtrips() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00.000000Z");
        assert_eq!(iso_utc(1_700_000_000_000), "2023-11-14T22:13:20.000000Z");
    }

    #[test]
    fn row_to_bar_parses() {
        let row = serde_json::json!(["2026-11-20T12:34:56.123456Z", 1.1, 1.2, 1.0, 1.15, 42.0]);
        let b = row_to_bar(&row).unwrap();
        assert!((b.open - 1.1).abs() < 1e-9);
        assert!((b.high - 1.2).abs() < 1e-9);
        assert!((b.low - 1.0).abs() < 1e-9);
        assert!((b.close - 1.15).abs() < 1e-9);
        assert_eq!(b.volume, 42.0);
        assert_eq!(b.time, 1_795_178_096_123);
    }

    #[test]
    fn row_to_bar_rejects_null_volume() {
        let row = serde_json::json!(["2026-11-20T12:34:56Z", 1.0, 1.0, 1.0, 1.0, null]);
        let b = row_to_bar(&row).unwrap();
        assert_eq!(b.volume, 0.0);
    }
}
