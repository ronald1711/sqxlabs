use serde::{Deserialize, Serialize};

/// A tradable market and its specifications: tick size/value, commission, and
/// the point size used to convert between pips and price.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instrument {
    pub name: String,
    pub tick_size: f64,
    pub tick_value: f64,
    pub point: f64,
    pub commission: f64,
    pub contract_size: f64,
}

impl Default for Instrument {
    fn default() -> Self {
        Self {
            name: "EURUSD".into(),
            tick_size: 0.00001,
            tick_value: 1.0,
            point: 0.0001,
            commission: 0.0,
            contract_size: 1.0,
        }
    }
}

impl Instrument {
    pub fn fx(name: &str) -> Self {
        let point = if name.ends_with("JPY") { 0.01 } else { 0.0001 };
        Self {
            name: name.to_string(),
            tick_size: point / 10.0,
            tick_value: 1.0,
            point,
            commission: 0.0,
            contract_size: 100_000.0,
        }
    }

    /// Convert a price distance to points (== pips for 5-digit FX).
    pub fn to_points(&self, price_diff: f64) -> f64 {
        price_diff / self.point
    }
}
