//! # sqx-backtest
//!
//! A deterministic, event-driven backtest engine. Signals are evaluated at the
//! close of each bar and executed at the next bar's open (no look-ahead);
//! stop-loss / profit-target / trailing-stop are checked intra-bar using
//! high/low with gap handling.

mod engine;
mod evidence;
pub use evidence::validate_account_evidence;
pub mod account;
pub use account::{AccountConfig, ConversionRate, MarginModel, MarginConversionRate, DirectionalConversionRate, ProfitConversionModel};
mod evaluate;
pub mod registry;

pub use engine::{validate_backtest_inputs, backtest, backtest_window, backtest_path_scenarios, IntrabarPolicy, backtest_ticks, BacktestConfig};
pub use evaluate::{compile_signal, compile_signal_checked};
pub use evaluate::{CompiledSignals, SignalEvidence, ConditionEvidence};
pub use registry::compute;

use sqx_core::{Direction, Timestamp};

/// Why a trade was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ExitReason {
    StopLoss,
    StopOut,
    ProfitTarget,
    TrailingStop,
    MoveToBreakEven,
    ExitSignal,
    ExitAfterBars,
    ReverseSignal,
    ExitAtEndOfDay,
    EndOfData,
}

/// A closed trade.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
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
    /// Signed rollover costs in account currency, already included in pnl.
    pub financing: f64,
    pub bars: usize,
    /// Maximum adverse excursion in points (<= 0).
    pub mae_points: f64,
    /// Maximum favorable excursion in points (>= 0).
    pub mfe_points: f64,
    pub exit_reason: ExitReason,
}

/// The result of a backtest run.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BacktestResult {
    /// Fatal input errors invalidate the entire run.
    pub errors: Vec<String>,
    /// Orders rejected by broker constraints, in submission order.
    pub rejected_orders: Vec<OrderRejection>,
    pub ambiguous_bars: Vec<BarAmbiguity>,
    pub execution: ExecutionEvidence,
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

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OrderRejection {
    pub time: Timestamp,
    pub direction: Direction,
    pub requested_lots: f64,
    pub reason: String,
}

/// Two plausible monotonic OHLC paths produced different execution/state outcomes.
/// These alternatives are local scenarios, not bounds on all possible tick paths.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BarAmbiguity {
    pub time: Timestamp,
    pub direction: Direction,
    pub high_first_exit: Option<(f64, ExitReason)>,
    pub low_first_exit: Option<(f64, ExitReason)>,
    pub high_first_stop: f64,
    pub low_first_stop: f64,
}

#[derive(Debug,Clone,PartialEq,serde::Serialize)]
pub struct ConfirmationEvidence {
    pub timeframe: sqx_core::Timeframe,
    pub source_bar_open_time: Option<Timestamp>,
    pub available_from: Option<Timestamp>,
    pub signal: Option<SignalEvidence>,
}

/// Admission calculation for a normalized order size, in account currency.
/// Costs reserve spread, entry slippage and round-trip commission; this is not
/// a cash debit or a full ongoing margin-level history.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MarginCheck {
    pub balance: f64,
    pub normalized_lots: f64,
    pub proposed_fill_price: f64,
    pub quote_to_account_rate: f64,
    pub margin_base_to_account_rate: f64,
    pub max_leverage: f64,
    pub required_margin: f64,
    pub spread_and_slippage_cost: f64,
    pub commission: f64,
    pub equity_after_entry_costs: f64,
    pub free_margin_after_entry: f64,
    pub accepted: bool,
}

/// Protective levels installed only after entry admission succeeds.
/// Missing levels mean no finite configured level, not a zero-price order.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct InitialProtection {
    pub stop_loss: Option<f64>,
    pub profit_target: Option<f64>,
    pub trailing_enabled: bool,
    pub break_even_enabled: bool,
    pub dynamic_distance: Option<f64>,
    /// Last completed candle used when resolving ATR-based levels.
    pub reference_bar_index: usize,
    pub reference_bar_open_time: Timestamp,
}

/// Entry signal queued at the dataset boundary without a following opening quote.
/// No sizing, admission or execution has occurred for this request.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct UnexecutedEntry {
    pub signal_bar_index: usize,
    pub signal_bar_open_time: Timestamp,
    pub direction: Direction,
    pub signal: Option<SignalEvidence>,
    pub reason: &'static str,
}

/// Evaluation of a previously completed candle's entry signal at an opening quote.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EntryAttempt {
    pub initial_protection: Option<InitialProtection>,
    pub margin_check: Option<MarginCheck>,
    pub confirmation: Option<ConfirmationEvidence>,
    pub signal: Option<SignalEvidence>,
    pub signal_bar_index: usize,
    pub signal_bar_open_time: Timestamp,
    pub attempt_time: Timestamp,
    pub direction: Direction,
    pub blocked_by: Vec<String>,
    pub requested_lots: Option<f64>,
    pub filled_lots: Option<f64>,
    pub fill_price: Option<f64>,
    pub trade_id: Option<usize>,
}

/// Close-derived exit request; candle timestamps label opens, not decision instants.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ScheduledExitDecision {
    pub signal: Option<SignalEvidence>,
    pub trade_id: usize,
    pub decision_bar_index: usize,
    pub decision_bar_open_time: Timestamp,
    pub reasons: Vec<ExitReason>,
    pub chosen_reason: ExitReason,
    /// None if the dataset ended before a next executable opening quote.
    pub executed_time: Option<Timestamp>,
}

/// Account valuation sampled after a candle's execution, or after terminal liquidation.
/// quote_time is present only for an observed tick; bar_open_time is a candle label.
/// This is sampled evidence, not every intrabar margin check.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AccountSnapshot {
    pub phase: &'static str,
    pub bar_open_time: Timestamp,
    pub quote_time: Option<Timestamp>,
    pub trade_id: Option<usize>,
    pub liquidation_price: Option<f64>,
    pub quote_to_account_rate: Option<f64>,
    pub margin_base_to_account_rate: Option<f64>,
    pub balance: f64,
    pub gross_unrealized: f64,
    pub commission_reserve: f64,
    pub financing_accrued: f64,
    pub equity: f64,
    pub required_margin: f64,
    pub free_margin: f64,
}

/// Effective protective-stop change after a reached live-position price.
/// OHLC time labels a candle; path_step gives model order, not an intrabar clock.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct StopChange {
    pub trade_id: usize,
    pub time: Timestamp,
    pub observed_price: f64,
    pub source: &'static str,
    pub path_step: Option<usize>,
    pub previous_stop: Option<f64>,
    pub new_stop: Option<f64>,
    pub previous_reason: ExitReason,
    pub new_reason: ExitReason,
}

/// Valuation at a stop-out liquidation price before execution slippage.
/// For a continuous OHLC segment this is the modelled threshold price, not an
/// observed quote. For a gap/tick it is the executable observed price.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct StopOutDecision {
    pub trade_id: usize,
    pub time: Timestamp,
    pub liquidation_price_before_slippage: f64,
    pub fill_price: f64,
    pub threshold_price: f64,
    pub quote_to_account_rate: f64,
    pub margin_base_to_account_rate: f64,
    pub balance: f64,
    pub unrealized_profit_account: f64,
    pub commission_reserve: f64,
    pub financing_accrued: f64,
    pub equity_before_slippage: f64,
    pub required_margin: f64,
    pub stop_out_ratio: f64,
    pub minimum_equity: f64,
}

/// Nonzero financing accrual recognised before processing an available quote.
/// UTC day is days since Unix epoch; applied_at may be later after a data gap.
/// This changes the position's accrued charge, not cash until settlement.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FinancingAccrual {
    pub trade_id: usize,
    pub rollover_utc_day: i64,
    pub applied_at: Timestamp,
    pub lots: f64,
    pub daily_charge_per_lot: f64,
    pub multiplier: u8,
    pub charge: f64,
    pub accrued_before: f64,
    pub accrued_after: f64,
}

/// Realized cash settlement at liquidation. Commission and accumulated financing
/// are booked here in this model; this is not a broker's daily posting ledger.
/// Spread/slippage are already reflected in the executed trade prices.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CashSettlement {
    pub quote_currency: String,
    pub account_currency: String,
    pub trade_id: usize,
    pub time: Timestamp,
    pub exit_reason: ExitReason,
    pub gross_profit_quote: f64,
    pub quote_to_account_rate: f64,
    pub gross_profit_account: f64,
    pub commission: f64,
    /// Signed charge: negative financing credits the account.
    pub financing: f64,
    pub net_change: f64,
    pub balance_before: f64,
    pub balance_after: f64,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ExecutionEvidence {
    /// 0: legacy/aggregate, 1: engine account ledger with reconciliation support.
    pub account_evidence_schema: u32,
    pub unexecuted_entry: Option<UnexecutedEntry>,
    pub account_snapshots: Vec<AccountSnapshot>,
    pub stop_changes: Vec<StopChange>,
    pub stop_out_decisions: Vec<StopOutDecision>,
    pub cash_settlements: Vec<CashSettlement>,
    pub financing_accruals: Vec<FinancingAccrual>,
    pub entry_attempts: Vec<EntryAttempt>,
    pub scheduled_exit_decisions: Vec<ScheduledExitDecision>,
    /// Confirmation bucket omitted because the dataset begins after its opening.
    pub skipped_leading_confirmation_bucket: Option<Timestamp>,
    /// Account denomination of equity, PnL, commission and financing.
    /// None means provenance is unknown, never an implicit USD default.
    pub account_currency: Option<String>,
    pub mode: String,
    pub bar_duration_ms: Option<i64>,
    pub missing_tick_intervals: Vec<Timestamp>,
    pub supplied_ticks: usize,
}
