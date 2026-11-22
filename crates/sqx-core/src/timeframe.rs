use serde::{Deserialize, Serialize};

/// A bar interval. Minutes per bar is the canonical representation so that
/// multiple labels can map onto the same interval (e.g. "M60" == "H1").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Timeframe {
    Tick, M1, M2, M3, M5, M10, M15, M30, H1, H2, H4, H6, H8, H12, D1, W1, MN1,
}

impl Timeframe {
    /// Minutes per bar. `None` for tick data.
    pub fn minutes(self) -> Option<i64> {
        use Timeframe::*;
        Some(match self {
            Tick => return None,
            M1 => 1, M2 => 2, M3 => 3, M5 => 5, M10 => 10, M15 => 15, M30 => 30,
            H1 => 60, H2 => 120, H4 => 240, H6 => 360, H8 => 480, H12 => 720,
            D1 => 1440, W1 => 10080, MN1 => 43200,
        })
    }

    /// Human-readable label used in logs and reports.
    pub fn label(self) -> &'static str {
        use Timeframe::*;
        match self {
            Tick => "Tick", M1 => "M1", M2 => "M2", M3 => "M3", M5 => "M5",
            M10 => "M10", M15 => "M15", M30 => "M30", H1 => "H1", H2 => "H2",
            H4 => "H4", H6 => "H6", H8 => "H8", H12 => "H12", D1 => "D1",
            W1 => "W1", MN1 => "MN1",
        }
    }
}

impl std::fmt::Display for Timeframe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Parse a timeframe from a label ("M5", "H1", "D1", ...) or a QuestDB-style
/// unit ("1m", "5m", "1h", "1d", ...).
impl std::str::FromStr for Timeframe {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use Timeframe::*;
        let t = s.trim();
        let up = t.to_ascii_uppercase();
        let low = t.to_ascii_lowercase();
        Ok(match (up.as_str(), low.as_str()) {
            ("M1", _) | (_, "1m") => M1,
            ("M2", _) | (_, "2m") => M2,
            ("M3", _) | (_, "3m") => M3,
            ("M5", _) | (_, "5m") => M5,
            ("M10", _) | (_, "10m") => M10,
            ("M15", _) | (_, "15m") => M15,
            ("M30", _) | (_, "30m") => M30,
            ("H1", _) | (_, "1h") => H1,
            ("H2", _) | (_, "2h") => H2,
            ("H4", _) | (_, "4h") => H4,
            ("H6", _) | (_, "6h") => H6,
            ("H8", _) | (_, "8h") => H8,
            ("H12", _) | (_, "12h") => H12,
            ("D1", _) | (_, "1d") => D1,
            ("W1", _) | (_, "1w") | (_, "7d") => W1,
            ("MN1", _) | (_, "1mn") | (_, "1mth") | (_, "1mon") => MN1,
            _ => return Err(format!("unknown timeframe '{t}'")),
        })
    }
}
