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
