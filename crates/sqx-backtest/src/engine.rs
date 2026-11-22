//! The bar-by-bar simulation loop.

use sqx_core::{Bar, Direction, Instrument, Tick, Timestamp};
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
    /// Intrabar path assumption: true = high reached before low (more optimistic
    /// for longs), false = low before high (conservative).
    pub intrabar_up_first: bool,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            initial_deposit: 10_000.0,
            spread_points: 0.0,
            slippage_points: 0.0,
            commission_per_lot: 0.0,
            intrabar_up_first: false,
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
    run(strategy, bars, None, instrument, config)
}

/// Tick-precision backtest: SL/PT are resolved against the ticks inside each bar.
pub fn backtest_ticks(
    strategy: &Strategy,
    bars: &[Bar],
    ticks: &[Tick],
    instrument: &Instrument,
    config: &BacktestConfig,
) -> BacktestResult {
    run(strategy, bars, Some(ticks), instrument, config)
}

fn run(
    strategy: &Strategy,
    bars: &[Bar],
    ticks: Option<&[Tick]>,
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
    let tick_ranges: Option<Vec<(usize, usize)>> = ticks.map(|ts| {
        let mut ranges = Vec::with_capacity(bars.len());
        let mut j = 0usize;
        for i in 0..bars.len() {
            let t0 = bars[i].time;
            let t1 = if i + 1 < bars.len() { bars[i + 1].time } else { t0 + 1 };
            while j < ts.len() && ts[j].time < t0 {
                j += 1;
            }
            let start = j;
            let mut k = j;
            while k < ts.len() && ts[k].time < t1 {
                k += 1;
            }
            ranges.push((start, k));
        }
        ranges
    });
    let point = instrument.point;
    let point_value = instrument.contract_size * point; // money per point per 1.0 lot
    let atr_series = if uses_atr(strategy) {
        Some(sqx_indicators::atr(bars, atr_period(strategy)))
    } else {
        None
    };

    let conf_ok: Option<Vec<bool>> = strategy.confirmation.as_ref().map(|c| {
        let tf = c.timeframe;
        let bucket_ms = tf.minutes().unwrap_or(0) * 60_000;
        if bucket_ms <= 0 {
            vec![true; bars.len()]
        } else {
            let rb = sqx_core::resample(bars, tf);
            let sigs = crate::evaluate::compile_signal(&c.signal, &rb);
            let mut active = vec![false; bars.len()];
            let mut j = 0usize;
            for i in 0..bars.len() {
                let t = bars[i].time;
                while j < rb.len() && rb[j].time + bucket_ms <= t {
                    j += 1;
                }
                if j > 0 {
                    active[i] = sigs[j - 1];
                }
            }
            active
        }
    });

    let mut cash = config.initial_deposit;
    let mut position: Option<Position> = None;
    let mut pending: Option<Direction> = None;
    let mut trades: Vec<Trade> = Vec::new();
    let mut equity: Vec<(Timestamp, f64)> = Vec::with_capacity(bars.len());
    let mut trade_id = 0usize;
    let mut trades_today: usize = 0;
    let mut current_day: i64 = i64::MIN;
    let mut mart_size: f64 = match &strategy.money_management {
        MoneyManagement::SimpleMartingale { lots_start, .. } => *lots_start,
        _ => 0.0,
    };

    for i in 0..bars.len() {
        let bar = bars[i];
        let day = bar.time.div_euclid(86_400_000);
        if day != current_day {
            current_day = day;
            trades_today = 0;
        }

        // 1. Execute a pending entry at this bar's open.
        if let Some(dir) = pending.take() {
            let hour = (bar.time.div_euclid(3_600_000).rem_euclid(24)) as u8;
            let within_hours = match strategy.trading_options.limit_hours {
                Some((a, b)) => hour >= a && hour < b,
                None => true,
            };
            let under_cap = match strategy.trading_options.max_trades_per_day {
                Some(m) => trades_today < m,
                None => true,
            };
            if within_hours && under_cap && conf_ok.as_ref().map_or(true, |c| c.get(i).copied().unwrap_or(true)) {
                let fill = bar.open + dir.sign() * config.slippage_points * point;
                let (sl, pt) = stop_profit_levels(strategy, &atr_series, i, fill, dir, point);
                let sl_points = sl_distance_points(sl, fill, point);
                let size = match &strategy.money_management {
                    MoneyManagement::SimpleMartingale { lots_start, lots_reset, .. } => {
                        let s = if mart_size <= 0.0 { *lots_start } else { mart_size };
                        if s > *lots_reset { *lots_start } else { s }
                    }
                    _ => position_size(strategy, cash, sl_points, point_value, &atr_series, i, fill),
                };
                trades_today += 1;
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
            if let (Some(ticks), Some(ranges)) = (ticks, &tick_ranges) {
                let (a, b) = ranges[i];
                if a < b {
                    closed = tick_exit(&ticks[a..b], pos.direction, pos.stop_loss, pos.profit_target);
                }
            }
            if closed.is_none() {
                if pos.direction == Direction::Long {
                    let pt_hit = pos.profit_target.is_finite() && bar.high >= pos.profit_target;
                    if pos.trailing && bar.low <= pos.trailing_stop {
                        closed = Some((pos.trailing_stop.min(bar.open), ExitReason::TrailingStop));
                    } else if bar.low <= pos.stop_loss && pt_hit && config.intrabar_up_first {
                        closed = Some((pos.profit_target, ExitReason::ProfitTarget));
                    } else if bar.low <= pos.stop_loss {
                        let px = if bar.open < pos.stop_loss { bar.open } else { pos.stop_loss };
                        closed = Some((px, ExitReason::StopLoss));
                    } else if pt_hit {
                        let px = if bar.open > pos.profit_target { bar.open } else { pos.profit_target };
                        closed = Some((px, ExitReason::ProfitTarget));
                    }
                } else {
                    let pt_hit = pos.profit_target.is_finite() && bar.low <= pos.profit_target;
                    if pos.trailing && bar.high >= pos.trailing_stop {
                        closed = Some((pos.trailing_stop.max(bar.open), ExitReason::TrailingStop));
                    } else if bar.high >= pos.stop_loss && pt_hit && !config.intrabar_up_first {
                        closed = Some((pos.profit_target, ExitReason::ProfitTarget));
                    } else if bar.high >= pos.stop_loss {
                        let px = if bar.open > pos.stop_loss { bar.open } else { pos.stop_loss };
                        closed = Some((px, ExitReason::StopLoss));
                    } else if pt_hit {
                        let px = if bar.open < pos.profit_target { bar.open } else { pos.profit_target };
                        closed = Some((px, ExitReason::ProfitTarget));
                    }
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

            if closed.is_none() {
                let is_last_of_day = i + 1 >= bars.len()
                    || bars[i + 1].time.div_euclid(86_400_000) != day;
                let friday = strategy.trading_options.exit_on_friday && is_friday(bar.time);
                if is_last_of_day && (strategy.trading_options.exit_at_end_of_day || friday) {
                    closed = Some((bar.close, ExitReason::ExitAtEndOfDay));
                }
            }

            match closed {
                Some((px, reason)) => {
                    let commission = config.commission_per_lot * pos.size;
                    let pnl_points = (px - pos.open_price) * pos.direction.sign() / point;
                    let pnl = pnl_points * pos.size * point_value - commission;
                    cash += pnl;
                    if let MoneyManagement::SimpleMartingale { lots_start, lots_multiplier, lots_reset } = &strategy.money_management {
                        if pnl < 0.0 {
                            let next = pos.size * lots_multiplier;
                            mart_size = if next > *lots_reset { *lots_start } else { next };
                        } else {
                            mart_size = *lots_start;
                        }
                    }
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

/// Resolve SL/PT against the ticks inside one bar (tick precision).
fn tick_exit(ticks: &[Tick], dir: Direction, sl: f64, pt: f64) -> Option<(f64, ExitReason)> {
    for t in ticks {
        if dir == Direction::Long {
            if t.bid <= sl {
                return Some((sl, ExitReason::StopLoss));
            }
            if pt.is_finite() && t.bid >= pt {
                return Some((pt, ExitReason::ProfitTarget));
            }
        } else {
            if t.ask >= sl {
                return Some((sl, ExitReason::StopLoss));
            }
            if pt.is_finite() && t.ask <= pt {
                return Some((pt, ExitReason::ProfitTarget));
            }
        }
    }
    None
}

fn sl_distance_points(sl: f64, fill: f64, point: f64) -> f64 {
    if sl.is_finite() {
        (fill - sl).abs() / point
    } else {
        100.0
    }
}

/// True when the timestamp falls on a Friday (unix epoch 1970-01-01 = Thursday).
fn is_friday(time_ms: i64) -> bool {
    time_ms.div_euclid(86_400_000).rem_euclid(7) == 1
}

fn position_size(
    s: &Strategy,
    balance: f64,
    sl_points: f64,
    point_value: f64,
    atr: &Option<Vec<f64>>,
    i: usize,
    fill: f64,
) -> f64 {
    match &s.money_management {
        MoneyManagement::FixedSize { size } => *size,
        MoneyManagement::FixedAmount { amount } | MoneyManagement::CryptoFixedAmount { amount } => {
            let denom = (sl_points * point_value).max(1e-9);
            (amount / denom).max(0.01)
        }
        MoneyManagement::RiskFixedBalancePct { risk } | MoneyManagement::CryptoFixedBalancePct { risk } => {
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
        MoneyManagement::StocksSizeByPrice { risk } | MoneyManagement::CryptoSizeByPrice { risk } => {
            let risk_money = balance * risk / 100.0;
            (risk_money / fill.max(1e-9)).max(0.01)
        }
        MoneyManagement::SimpleMartingale { lots_start, .. } => *lots_start,
    }
}
