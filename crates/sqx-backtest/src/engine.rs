//! The bar-by-bar simulation loop.

use sqx_core::{Bar, Direction, Instrument, Timestamp};
use sqx_strategy::{ExitType, MoneyManagement, ProfitTarget, StopLoss, Strategy};

use crate::evaluate::compile;
use crate::{BacktestResult, ExitReason, Trade};

/// Backtest configuration: costs and execution assumptions.
#[derive(Debug, Clone)]
pub struct BacktestConfig {
    /// Initial account balance.
    pub initial_deposit: f64,
    /// Spread in points added to entries/exits.
    pub spread_points: f64,
    /// Slippage in points.
    pub slippage_points: f64,
    /// Round-trip commission per lot, in account currency.
    pub commission_per_lot: f64,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            initial_deposit: 10_000.0,
            spread_points: 0.0,
            slippage_points: 0.0,
            commission_per_lot: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Position {
    direction: Direction,
    size: f64,
    open_price: f64,
    open_time: Timestamp,
    open_bar: usize,
    stop_loss: f64,
    profit_target: f64,
    trailing: bool,
    trailing_stop: f64,
    trailing_distance: f64,
    move_to_be: bool,
    be_applied: bool,
    exit_after_bars: Option<usize>,
    mae_points: f64,
    mfe_points: f64,
}

/// Run a backtest of strategy over bars for instrument.
pub fn backtest(
    strategy: &Strategy,
    bars: &[Bar],
    instrument: &Instrument,
    config: &BacktestConfig,
) -> BacktestResult {
    if bars.len() < 2 {
        return BacktestResult {
            trades: Vec::new(),
            equity: Vec::new(),
            initial_deposit: config.initial_deposit,
            final_equity: config.initial_deposit,
        };
    }
    let signals = compile(strategy, bars);
    let point = instrument.point;
    let point_value = instrument.contract_size * point; // money per point per 1.0 lot
    let atr_series = if uses_atr(strategy) {
        Some(sqx_indicators::atr(bars, atr_period(strategy)))
    } else {
        None
    };

    let mut cash = config.initial_deposit;
    let mut position: Option<Position> = None;
    let mut pending: Option<Direction> = None;
    let mut trades: Vec<Trade> = Vec::new();
    let mut equity: Vec<(Timestamp, f64)> = Vec::with_capacity(bars.len());
    let mut trade_id = 0usize;

    for i in 0..bars.len() {
        let bar = bars[i];

        // 1. Execute a pending entry at this bar's open.
        if let Some(dir) = pending.take() {
            let fill = bar.open + dir.sign() * config.slippage_points * point;
            let (sl, pt) = stop_profit_levels(strategy, &atr_series, i, fill, dir, point);
            let sl_points = sl_distance_points(sl, fill, point);
            let size = position_size(strategy, cash, sl_points, point_value, &atr_series, i);
            let trailing_distance = (fill - sl).abs();
            position = Some(Position {
                direction: dir,
                size,
                open_price: fill,
                open_time: bar.time,
                open_bar: i,
                stop_loss: sl,
                profit_target: pt,
                trailing: strategy.exits.iter().any(|e| matches!(e, ExitType::TrailingStop)),
                trailing_stop: sl,
                trailing_distance,
                move_to_be: strategy.exits.iter().any(|e| matches!(e, ExitType::MoveToBreakEven)),
                be_applied: false,
                exit_after_bars: strategy.exits.iter().find_map(|e| match e {
                    ExitType::ExitAfterBars(n) => Some(*n),
                    _ => None,
                }),
                mae_points: 0.0,
                mfe_points: 0.0,
            });
        }

        // 2. Manage the open position intra-bar (SL / PT / trailing / BE).
        if let Some(mut pos) = position.take() {
            let sign = pos.direction.sign();
            let hi_move = (bar.high - pos.open_price) * sign;
            let lo_move = (bar.low - pos.open_price) * sign;
            let favorable = hi_move.max(lo_move).max(0.0);
            let adverse = hi_move.min(lo_move).min(0.0);
            pos.mfe_points = pos.mfe_points.max(favorable / point);
            pos.mae_points = pos.mae_points.min(adverse / point);

            if pos.trailing {
                if pos.direction == Direction::Long {
                    pos.trailing_stop = pos.trailing_stop.max(bar.high - pos.trailing_distance);
                } else {
                    pos.trailing_stop = pos.trailing_stop.min(bar.low + pos.trailing_distance);
                }
            }

            // Move to break-even once price has moved favorably by the SL distance.
            if pos.move_to_be && !pos.be_applied && favorable >= pos.trailing_distance {
                pos.stop_loss = pos.open_price;
                pos.be_applied = true;
            }

            let mut closed: Option<(f64, ExitReason)> = None;
            if pos.direction == Direction::Long {
                if pos.trailing && bar.low <= pos.trailing_stop {
                    closed = Some((pos.trailing_stop.min(bar.open), ExitReason::TrailingStop));
                } else if bar.low <= pos.stop_loss {
                    let px = if bar.open < pos.stop_loss { bar.open } else { pos.stop_loss };
                    closed = Some((px, ExitReason::StopLoss));
                } else if bar.high >= pos.profit_target && pos.profit_target.is_finite() {
                    let px = if bar.open > pos.profit_target { bar.open } else { pos.profit_target };
                    closed = Some((px, ExitReason::ProfitTarget));
                }
            } else {
                if pos.trailing && bar.high >= pos.trailing_stop {
                    closed = Some((pos.trailing_stop.max(bar.open), ExitReason::TrailingStop));
                } else if bar.high >= pos.stop_loss {
                    let px = if bar.open > pos.stop_loss { bar.open } else { pos.stop_loss };
                    closed = Some((px, ExitReason::StopLoss));
                } else if bar.low <= pos.profit_target && pos.profit_target.is_finite() {
                    let px = if bar.open < pos.profit_target { bar.open } else { pos.profit_target };
                    closed = Some((px, ExitReason::ProfitTarget));
                }
            }

            if closed.is_none() {
                if let Some(n) = pos.exit_after_bars {
                    if i - pos.open_bar >= n {
                        closed = Some((bar.close, ExitReason::ExitAfterBars));
                    }
                }
            }

            if closed.is_none() {
                let exit_sig = match pos.direction {
                    Direction::Long => signals.long_exit[i],
                    Direction::Short => signals.short_exit[i],
                };
                if exit_sig {
                    closed = Some((bar.close, ExitReason::ExitSignal));
                }
            }

            match closed {
                Some((px, reason)) => {
                    let commission = config.commission_per_lot * pos.size;
                    let pnl_points = (px - pos.open_price) * pos.direction.sign() / point;
                    let pnl = pnl_points * pos.size * point_value - commission;
                    cash += pnl;
                    trades.push(Trade {
                        id: trade_id,
                        direction: pos.direction,
                        open_time: pos.open_time,
                        close_time: bar.time,
                        open_price: pos.open_price,
                        close_price: px,
                        size: pos.size,
                        pnl,
                        pnl_points,
                        commission,
                        bars: i - pos.open_bar,
                        mae_points: pos.mae_points,
                        mfe_points: pos.mfe_points,
                        exit_reason: reason,
                    });
                    trade_id += 1;
                    position = None;
                }
                None => position = Some(pos),
            }
        }

        // 3. Signal new entries (executed next bar).
        if position.is_none() {
            if strategy.long && signals.long_entry[i] {
                pending = Some(Direction::Long);
            } else if strategy.short && signals.short_entry[i] {
                pending = Some(Direction::Short);
            }
        }

        // 4. Sample the equity curve at close.
        let unrealized = position
            .as_ref()
            .map(|p| (bar.close - p.open_price) * p.direction.sign() / point * p.size * point_value)
            .unwrap_or(0.0);
        equity.push((bar.time, cash + unrealized));
    }

    // Force-close any open position at the last bar.
    if let Some(pos) = position.take() {
        let last = bars.last().unwrap();
        let px = last.close;
        let commission = config.commission_per_lot * pos.size;
        let pnl_points = (px - pos.open_price) * pos.direction.sign() / point;
        let pnl = pnl_points * pos.size * point_value - commission;
        cash += pnl;
        trades.push(Trade {
            id: trade_id,
            direction: pos.direction,
            open_time: pos.open_time,
            close_time: last.time,
            open_price: pos.open_price,
            close_price: px,
            size: pos.size,
            pnl,
            pnl_points,
            commission,
            bars: bars.len() - 1 - pos.open_bar,
            mae_points: pos.mae_points,
            mfe_points: pos.mfe_points,
            exit_reason: ExitReason::EndOfData,
        });
    }

    BacktestResult {
        final_equity: cash,
        trades,
        equity,
        initial_deposit: config.initial_deposit,
    }
}

fn uses_atr(s: &Strategy) -> bool {
    matches!(s.stop_loss, Some(StopLoss::Atr { .. }))
        || matches!(s.profit_target, Some(ProfitTarget::Atr { .. }))
        || matches!(s.money_management, MoneyManagement::AtrRiskBased { .. })
}

fn atr_period(s: &Strategy) -> usize {
    match (&s.stop_loss, &s.profit_target, &s.money_management) {
        (Some(StopLoss::Atr { period, .. }), _, _) => *period,
        (_, Some(ProfitTarget::Atr { period, .. }), _) => *period,
        (_, _, MoneyManagement::AtrRiskBased { atr_period, .. }) => *atr_period,
        _ => 14,
    }
}

fn stop_profit_levels(
    s: &Strategy,
    atr: &Option<Vec<f64>>,
    i: usize,
    fill: f64,
    dir: Direction,
    point: f64,
) -> (f64, f64) {
    let sl = match s.stop_loss {
        Some(StopLoss::Points(p)) => fill - dir.sign() * p * point,
        Some(StopLoss::Atr { mult, .. }) => fill - dir.sign() * atr_value(atr, i) * mult,
        None => {
            if dir == Direction::Long {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }
        }
    };
    let pt = match s.profit_target {
        Some(ProfitTarget::Points(p)) => fill + dir.sign() * p * point,
        Some(ProfitTarget::Atr { mult, .. }) => fill + dir.sign() * atr_value(atr, i) * mult,
        None => f64::NAN,
    };
    (sl, pt)
}

fn atr_value(atr: &Option<Vec<f64>>, i: usize) -> f64 {
    atr.as_ref()
        .and_then(|a| if i < a.len() && !a[i].is_nan() { Some(a[i]) } else { None })
        .unwrap_or(0.0)
}

fn sl_distance_points(sl: f64, fill: f64, point: f64) -> f64 {
    if sl.is_finite() {
        (fill - sl).abs() / point
    } else {
        100.0
    }
}

fn position_size(
    s: &Strategy,
    balance: f64,
    sl_points: f64,
    point_value: f64,
    atr: &Option<Vec<f64>>,
    i: usize,
) -> f64 {
    match &s.money_management {
        MoneyManagement::FixedSize { size } => *size,
        MoneyManagement::FixedAmount { amount } => {
            let denom = (sl_points * point_value).max(1e-9);
            (amount / denom).max(0.01)
        }
        MoneyManagement::RiskFixedBalancePct { risk } => {
            let risk_money = balance * risk / 100.0;
            let denom = (sl_points * point_value).max(1e-9);
            (risk_money / denom).max(0.01)
        }
        MoneyManagement::RiskFixedPctOfAccount { risk, stop_loss_points } => {
            let risk_money = balance * risk / 100.0;
            let denom = (stop_loss_points * point_value).max(1e-9);
            (risk_money / denom).max(0.01)
        }
        MoneyManagement::AtrRiskBased { risk, atr_mult, .. } => {
            let dist = atr_value(atr, i) * atr_mult;
            let risk_money = balance * risk / 100.0;
            let denom = (dist * point_value).max(1e-9);
            (risk_money / denom).max(0.01)
        }
    }
}
