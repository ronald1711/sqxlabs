use serde::{Deserialize, Serialize};

use crate::Timestamp;

/// A single OHLCV bar. Prices are absolute quote prices; `volume` is in the
/// instrument's native volume unit (contracts/shares) and may be 0 for forex.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub time: Timestamp,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Bar {
    pub fn new(time: Timestamp, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Self {
        Self { time, open, high, low, close, volume }
    }

    /// Typical price (H+L+C)/3.
    pub fn typical(&self) -> f64 {
        (self.high + self.low + self.close) / 3.0
    }

    /// Range of the bar.
    pub fn range(&self) -> f64 {
        self.high - self.low
    }
}
