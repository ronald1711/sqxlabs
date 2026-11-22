//! # sqx-core
//!
//! Shared data model for the StrategyQuant open-source port.
//! Holds the vocabulary every other crate talks in: market bars,
//! timeframes, tradable symbols, and trading directions.

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
