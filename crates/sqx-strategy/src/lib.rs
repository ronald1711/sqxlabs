//! # sqx-strategy
//!
//! The strategy object model: a transparent IF-THEN rule tree built from
//! building blocks, plus pseudo-code rendering and a catalog of the shipped
//! building blocks used by the generator. This crate is pure data + rendering;
//! the backtest engine (sqx-backtest) turns a [`Strategy`] into trades.

mod catalog;
mod model;
mod pseudocode;

pub use catalog::*;
pub use model::*;
pub use pseudocode::*;
