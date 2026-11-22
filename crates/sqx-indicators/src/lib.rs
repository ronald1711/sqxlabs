//! # sqx-indicators
//!
//! A dependency-free technical-indicator library. Every indicator returns a
//! [`Series`] (`Vec<f64>`) aligned with the input bars; leading values are
//! `f64::NAN` during the warm-up window. Functions operate on price slices or
//! on `&[Bar]` where high/low/volume are required.
//!
//! The library is intentionally side-effect free and deterministic so that
//! backtests reproduce byte-for-byte.

mod extra;
mod ma;
mod osc;
mod signal;
mod trend;
mod util;
mod volatility;

pub use extra::*;
pub use ma::*;
pub use osc::*;
pub use signal::*;
pub use trend::*;
pub use util::*;
pub use volatility::*;

use sqx_core::Bar;

/// A numeric series aligned with the input. `f64::NAN` marks "undefined".
pub type Series = Vec<f64>;

/// Extract the close prices from bars.
pub fn closes(bars: &[Bar]) -> Vec<f64> {
    bars.iter().map(|b| b.close).collect()
}

/// Extract the open prices from bars.
pub fn opens(bars: &[Bar]) -> Vec<f64> {
    bars.iter().map(|b| b.open).collect()
}

/// Extract the high prices from bars.
pub fn highs(bars: &[Bar]) -> Vec<f64> {
    bars.iter().map(|b| b.high).collect()
}

/// Extract the low prices from bars.
pub fn lows(bars: &[Bar]) -> Vec<f64> {
    bars.iter().map(|b| b.low).collect()
}

/// Extract the typical prices from bars.
pub fn typicals(bars: &[Bar]) -> Vec<f64> {
    bars.iter().map(|b| b.typical()).collect()
}
