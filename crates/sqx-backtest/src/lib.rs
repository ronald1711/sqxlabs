//! # sqx-backtest
//!
//! A deterministic, event-driven backtest engine. Signals are evaluated at the
//! close of each bar and executed at the next bar's open (no look-ahead);
//! stop-loss / profit-target / trailing-stop are checked intra-bar using
//! high/low with gap handling.

mod engine;
mod evaluate;
mod registry;

pub use engine::{backtest, BacktestConfig};
pub use evaluate::CompiledSignals;

use sqx_core::{Direction, Timestamp};

/// Why a trade was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    StopLoss,
    ProfitTarget,
    TrailingStop,
    MoveToBreakEven,
    ExitSignal,
    ExitAfterBars,
    ReverseSignal,
    EndOfData,
}

/// A closed trade.
#[derive(Debug, Clone, PartialEq)]
pub struct Trade {
    pub id: usize,
    pub direction: Direction,
    pub open_time: Timestamp,
    pub close_time: Timestamp,
    pub open_price: f64,
    pub close_price: f64,
    pub size: f64,
    pub pnl: f64,
    pub pnl_points: f64,
    pub commission: f64,
    pub bars: usize,
    /// Maximum adverse excursion in points (<= 0).
    pub mae_points: f64,
    /// Maximum favorable excursion in points (>= 0).
    pub mfe_points: f64,
    pub exit_reason: ExitReason,
}

/// The result of a backtest run.
#[derive(Debug, Clone)]
pub struct BacktestResult {
    pub trades: Vec<Trade>,
    /// Equity curve: (timestamp, equity) sampled at each bar close.
    pub equity: Vec<(Timestamp, f64)>,
    pub initial_deposit: f64,
    pub final_equity: f64,
}

impl BacktestResult {
    /// Realized profit/loss of all trades (account currency).
    pub fn net_profit(&self) -> f64 {
        self.trades.iter().map(|t| t.pnl).sum()
    }
}
