//! # sqx-core
//!
//! Shared data model for the StrategyQuant open-source port.
//! Holds the vocabulary every other crate talks in: market bars,
//! timeframes, tradable symbols, and trading directions.

use serde::{Deserialize, Serialize};

mod bar;
mod direction;
mod instrument;
mod timeframe;

pub use bar::Bar;
pub use direction::{Direction, PositionKind};
pub use instrument::Instrument;
pub use timeframe::Timeframe;

/// A unix timestamp in milliseconds (UTC).
pub type Timestamp = i64;

/// A single tick: bid and ask at a point in time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Tick {
    pub time: Timestamp,
    pub bid: f64,
    pub ask: f64,
}

/// Resample bars to a coarser timeframe by aggregating OHLCV per period.
pub fn resample(bars: &[Bar], to: Timeframe) -> Vec<Bar> {
    let to_min = match to.minutes() {
        Some(m) => m,
        None => return bars.to_vec(),
    };
    let bucket_ms = to_min * 60_000;
    let mut out = Vec::new();
    let mut cur: Option<Bar> = None;
    let mut cur_bucket = i64::MIN;
    for b in bars {
        let bucket = b.time.div_euclid(bucket_ms);
        match cur {
            None => {
                cur = Some(*b);
                cur_bucket = bucket;
            }
            Some(mut c) => {
                if bucket == cur_bucket {
                    c.high = c.high.max(b.high);
                    c.low = c.low.min(b.low);
                    c.close = b.close;
                    c.volume += b.volume;
                    cur = Some(c);
                } else {
                    out.push(c);
                    cur = Some(*b);
                    cur_bucket = bucket;
                }
            }
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    out
}
