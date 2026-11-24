//! The bar-by-bar simulation loop.

use sqx_core::{Bar, Direction, Instrument, Tick, Timestamp};
use sqx_strategy::{ExitType, MoneyManagement, ProfitTarget, StopLoss, Strategy};

use crate::evaluate::compile;
use crate::{AccountConfig, ExecutionEvidence, BarAmbiguity, BacktestResult, ExitReason, OrderRejection, Trade};

/// Explicit OHLC path assumption. AdverseFirst is side-aware, not a guarantee
/// of the worst achievable result when dynamic exits or later trades are involved.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntrabarPolicy {
    #[default]
    AdverseFirst,
    FavorableFirst,
    HighFirst,
    LowFirst,
}
impl IntrabarPolicy {
    fn high_first(self, direction: Direction) -> bool {
        match self {
            Self::HighFirst => true, Self::LowFirst => false,
            Self::AdverseFirst => direction == Direction::Short,
            Self::FavorableFirst => direction == Direction::Long,
        }
    }
}

/// Backtest configuration: costs and execution assumptions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BacktestConfig {
    pub account: AccountConfig,
    /// Declared fixed duration of each supplied candle. Required in tick mode;
    /// when present, validated for overlap and timestamp overflow in every mode.
    pub bar_duration_ms: Option<i64>,
    /// Explicit opt-in to OHLC execution in empty tick intervals; reported as hybrid.
    pub allow_tick_fallback: bool,
    /// Initial account balance.
    pub initial_deposit: f64,
    /// Bid/ask spread in points. Bar OHLC values are bid quotes.
    pub spread_points: f64,
    /// Slippage in points.
    pub slippage_points: f64,
    /// Round-trip commission per lot, in account currency.
    pub commission_per_lot: f64,
    pub intrabar_policy: IntrabarPolicy,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            account: AccountConfig::default(),
            bar_duration_ms: None, allow_tick_fallback: false,
            initial_deposit: 10_000.0,
            spread_points: 0.0,
            slippage_points: 0.0,
            commission_per_lot: 0.0,
            intrabar_policy: IntrabarPolicy::AdverseFirst,
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
    financing: f64,
    last_rollover_day: i64,
}

/// Run a backtest of strategy over bars for instrument.
pub fn backtest(
    strategy: &Strategy,
    bars: &[Bar],
    instrument: &Instrument,
    config: &BacktestConfig,
) -> BacktestResult {
    run(strategy, bars, None, instrument, config, 0)
}

/// Warm indicators on the full prefix, but create no positions/equity samples
/// before `trade_start`. A signal at trade_start-1 may fill at trade_start open.
pub fn backtest_window(strategy: &Strategy, bars: &[Bar], trade_start: usize, instrument: &Instrument, config: &BacktestConfig) -> BacktestResult {
    run(strategy, bars, None, instrument, config, trade_start)
}

/// Re-run the complete strategy under four explicit OHLC execution scenarios.
/// These are sensitivity scenarios, not exhaustive bounds over all possible paths.
pub fn backtest_path_scenarios(strategy: &Strategy, bars: &[Bar], instrument: &Instrument, config: &BacktestConfig) -> Vec<(IntrabarPolicy, BacktestResult)> {
    [IntrabarPolicy::AdverseFirst, IntrabarPolicy::FavorableFirst, IntrabarPolicy::HighFirst, IntrabarPolicy::LowFirst]
        .into_iter().map(|policy| {
            let mut scenario = config.clone(); scenario.intrabar_policy = policy;
            (policy, backtest(strategy, bars, instrument, &scenario))
        }).collect()
}

/// Tick-precision backtest: SL/PT are resolved against the ticks inside each bar.
pub fn backtest_ticks(
    strategy: &Strategy,
    bars: &[Bar],
    ticks: &[Tick],
    instrument: &Instrument,
    config: &BacktestConfig,
) -> BacktestResult {
    run(strategy, bars, Some(ticks), instrument, config, 0)
}

fn run(
    strategy: &Strategy,
    bars: &[Bar],
    ticks: Option<&[Tick]>,
    instrument: &Instrument,
    config: &BacktestConfig,
    trade_start: usize,
) -> BacktestResult {
    let invalid = |error: String| BacktestResult {
        errors: vec![error], rejected_orders: Vec::new(), ambiguous_bars: Vec::new(), execution: ExecutionEvidence {account_currency:Some(config.account.currency.clone()),..Default::default()}, trades: Vec::new(), equity: Vec::new(),
        initial_deposit: config.initial_deposit, final_equity: f64::NAN,
    };
    if trade_start > 0 && trade_start >= bars.len() { return invalid("trading window starts beyond available data".into()); }
    if let Err(error) = validate_backtest_inputs(bars, instrument, config) { return invalid(error); }
    if let Err(error) = strategy.validate_execution_settings() { return invalid(error); }
    let options = &strategy.trading_options;
    if (options.exit_at_end_of_day || options.exit_on_friday)
        && !options.session_close_utc_minute.is_some_and(|m| m < 1440) {
        return invalid("scheduled exits require session_close_utc_minute in 0..1440; a session close cannot be inferred from future candles".into());
    }
    if bars.is_empty() {
        return BacktestResult { errors: Vec::new(), rejected_orders: Vec::new(), ambiguous_bars: Vec::new(), execution: ExecutionEvidence {account_currency:Some(config.account.currency.clone()),..Default::default()},
            trades: Vec::new(), equity: Vec::new(), initial_deposit: config.initial_deposit,
            final_equity: config.initial_deposit };
    }
    if let Some(ticks) = ticks {
        let Some(duration) = config.bar_duration_ms.filter(|d| *d > 0) else {
            return invalid("tick execution requires a positive bar_duration_ms".into());
        };
        if bars.windows(2).any(|b| b[0].time.saturating_add(duration) > b[1].time) {
            return invalid("bar_duration_ms overlaps successive candles".into());
        }
        if ticks.iter().enumerate().any(|(i,t)| !t.bid.is_finite() || !t.ask.is_finite() || t.bid <= config.slippage_points * instrument.point || t.ask < t.bid || (i > 0 && t.time < ticks[i-1].time)) {
            return invalid("invalid or unsorted bid/ask ticks".into());
        }
    }
    let signals = compile(strategy, bars);
    if !signals.errors.is_empty(){return invalid(signals.errors.join("; "));}
    let tick_ranges: Option<Vec<(usize, usize)>> = ticks.map(|ts| {
        let mut ranges = Vec::with_capacity(bars.len());
        let mut j = 0usize;
        for i in 0..bars.len() {
            let t0 = bars[i].time;
            let t1 = t0.saturating_add(config.bar_duration_ms.expect("validated tick interval"));
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
    let mut execution = ExecutionEvidence { account_evidence_schema:1, account_currency:Some(config.account.currency.clone()), mode: "ohlc_bars".into(), bar_duration_ms: config.bar_duration_ms, ..Default::default() };
    if let (Some(ts), Some(ranges)) = (ticks, &tick_ranges) {
        execution.supplied_ticks = ts.len();
        if ranges.iter().map(|(a,b)| b-a).sum::<usize>() != ts.len() {
            return invalid("ticks fall outside the supplied candle intervals".into());
        }
        execution.missing_tick_intervals = ranges.iter().enumerate().filter(|(_, (a,b))| a==b).map(|(i,_)| bars[i].time).collect();
        if !config.allow_tick_fallback && ranges.iter().skip(1).any(|(a,b)| a==b) {
            return invalid("missing ticks in executable candle intervals; explicitly enable allow_tick_fallback for a hybrid run".into());
        }
        execution.mode = if ranges.iter().skip(1).any(|(a,b)| a==b) { "hybrid_ticks_ohlc" } else { "supplied_ticks" }.into();
        for tick in ts {
            for direction in [Direction::Long,Direction::Short] {
                if let Err(error)=config.account.profit_rate(instrument,tick.time,tick.bid,direction) {return invalid(error);}
                if let Err(error)=config.account.margin_base_rate(instrument,tick.time,tick.bid,direction) {return invalid(error);}
            }
        }
    }
    let point = instrument.point;
    let point_value = instrument.contract_size * point; // money per point per 1.0 lot
    let atr_sl = match strategy.stop_loss {
        Some(StopLoss::Atr { period, .. }) => Some(sqx_indicators::atr(bars, period)),
        _ => None,
    };
    let atr_pt = match strategy.profit_target {
        Some(ProfitTarget::Atr { period, .. }) => Some(sqx_indicators::atr(bars, period)),
        _ => None,
    };
    let atr_series = match strategy.money_management {
        MoneyManagement::AtrRiskBased { atr_period, .. } => Some(sqx_indicators::atr(bars, atr_period)),
        _ => None,
    };

    let mut confirmation_sources=None;
    let conf_ok: Option<Vec<bool>> = if let Some(confirmation)=&strategy.confirmation {
        let tf=confirmation.timeframe;
        if let Some(duration)=config.bar_duration_ms {
            for (index,bar) in bars.iter().enumerate() {
                let Some((_,end))=tf.bucket_bounds(bar.time) else {
                    return invalid(format!("confirmation bucket bounds unavailable at bar {index}"));
                };
                if bar.time.checked_add(duration).is_none_or(|close|close>end) {
                    return invalid(format!("source candle at bar {index} crosses confirmation bucket; finer source data required"));
                }
            }
        }
        let mut rb=match sqx_core::resample(bars,tf){Ok(bars)=>bars,Err(error)=>return invalid(error)};
        if rb.first().zip(bars.first()).is_some_and(|(aggregate,source)|aggregate.time<source.time){
            execution.skipped_leading_confirmation_bucket=Some(rb.remove(0).time);
        }
        if let (Some(duration),Some(last))=(config.bar_duration_ms,bars.last()) {
            let mut source=0usize;
            for aggregate in &rb {
                let (_,end)=tf.bucket_bounds(aggregate.time).expect("validated confirmation bounds");
                if end>last.time {break;} // This bucket cannot yet inform any decision.
                while source<bars.len() && bars[source].time<aggregate.time {source+=1;}
                let mut expected=aggregate.time;
                while source<bars.len() && bars[source].time<end {
                    if bars[source].time!=expected {
                        return invalid(format!("incomplete confirmation bucket at {}: missing source interval at {expected}; continuous coverage required without a market-calendar contract",aggregate.time));
                    }
                    expected=bars[source].time.checked_add(duration).expect("validated source end");
                    source+=1;
                }
                if expected!=end {
                    return invalid(format!("incomplete confirmation bucket at {}: source coverage ends at {expected}, expected {end}; continuous coverage required without a market-calendar contract",aggregate.time));
                }
            }
        }
        let mut confirmation_strategy=Strategy::new("confirmation evidence");confirmation_strategy.signals.push(confirmation.signal.clone());
        let compiled=compile(&confirmation_strategy,&rb);
        if !compiled.errors.is_empty(){return invalid(format!("confirmation: {}",compiled.errors.join("; ")));}
        let sigs=match confirmation.signal.kind {sqx_strategy::SignalKind::LongEntry=>&compiled.long_entry,sqx_strategy::SignalKind::ShortEntry=>&compiled.short_entry,sqx_strategy::SignalKind::LongExit=>&compiled.long_exit,sqx_strategy::SignalKind::ShortExit=>&compiled.short_exit};
        let mut active=vec![false;bars.len()];let mut sources=vec![None;bars.len()];let mut j=0usize;
        for (i,bar) in bars.iter().enumerate(){
            while j<rb.len() && tf.bucket_bounds(rb[j].time).is_some_and(|(_,end)|end<=bar.time){j+=1;}
            if j>0 {active[i]=sigs[j-1];sources[i]=Some(j-1);}
        }
        confirmation_sources=Some((compiled,rb,sources,tf,confirmation.signal.kind));
        Some(active)
    }else{None};

    let mut cash = config.initial_deposit;
    let mut position: Option<Position> = None;
    let mut pending: Option<Direction> = if trade_start > 0 {
        if strategy.long && signals.long_entry[trade_start - 1] { Some(Direction::Long) }
        else if strategy.short && signals.short_entry[trade_start - 1] { Some(Direction::Short) }
        else { None }
    } else { None };
    let mut pending_exit: Option<ExitReason> = None;
    let mut trades: Vec<Trade> = Vec::new();
    let mut rejected_orders = Vec::new();
    let mut ambiguous_bars = Vec::new();
    let mut equity: Vec<(Timestamp, f64)> = Vec::with_capacity(bars.len());
    let mut trade_id = 0usize;
    let mut trades_today: usize = 0;
    let mut current_day: i64 = i64::MIN;
    let mut mart_size: f64 = match &strategy.money_management {
        MoneyManagement::SimpleMartingale { lots_start, .. } => *lots_start,
        _ => 0.0,
    };

    for i in trade_start..bars.len() {
        let bar = bars[i];
        let interval_ticks = match (ticks, &tick_ranges) {
            (Some(ts), Some(ranges)) => &ts[ranges[i].0..ranges[i].1],
            _ => &[],
        };
        let entry_time = interval_ticks.first().map_or(bar.time, |t| t.time);
        // In tick mode the first executable quote can be on the next UTC day
        // even though its containing candle opened before midnight.
        let day = entry_time.div_euclid(86_400_000);
        if day != current_day {
            current_day = day;
            trades_today = 0;
        }

        // 1. Execute a pending entry at this bar's open.
        if let Some(dir) = pending.take() {
            let hour = (entry_time.div_euclid(3_600_000).rem_euclid(24)) as u8;
            let within_hours = match strategy.trading_options.limit_hours {
                Some((a, b)) if a>b => hour >= a || hour < b,
                Some((a, b)) => hour >= a && hour < b,
                None => true,
            };
            let under_cap = match strategy.trading_options.max_trades_per_day {
                Some(m) => trades_today < m,
                None => true,
            };
            let mut blocked_by=Vec::new();
            if !(cash>0.0){blocked_by.push("nonpositive_balance".into());}
            if !(i>0 && [&atr_sl,&atr_pt,&atr_series].iter().all(|series|series.as_ref().map_or(true,|a|a[i-1].is_finite()&&a[i-1]>0.0))){blocked_by.push("atr_not_ready".into());}
            if session_entry_closed(entry_time,options){blocked_by.push("session_closed".into());}
            if !within_hours{blocked_by.push("outside_trading_hours".into());}
            if !under_cap{blocked_by.push("daily_entry_limit".into());}
            if !conf_ok.as_ref().map_or(true,|c|c.get(i).copied().unwrap_or(false)){blocked_by.push("confirmation_not_satisfied".into());}
            let confirmation=confirmation_sources.as_ref().map(|(compiled,rb,sources,tf,kind)|{
                let source=sources[i];
                crate::ConfirmationEvidence{timeframe:*tf,source_bar_open_time:source.map(|j|rb[j].time),available_from:source.and_then(|j|tf.bucket_bounds(rb[j].time).map(|(_,end)|end)),signal:source.and_then(|j|compiled.explain(*kind,j))}
            });
            execution.entry_attempts.push(crate::EntryAttempt{initial_protection:None,margin_check:None,confirmation,signal:signals.explain(if dir==Direction::Long{sqx_strategy::SignalKind::LongEntry}else{sqx_strategy::SignalKind::ShortEntry},i.saturating_sub(1)),signal_bar_index:i.saturating_sub(1),signal_bar_open_time:bars[i.saturating_sub(1)].time,attempt_time:entry_time,direction:dir,blocked_by,requested_lots:None,filled_lots:None,fill_price:None,trade_id:None});
            let attempt=execution.entry_attempts.last_mut().unwrap();
            if attempt.blocked_by.is_empty() {
                let quote = interval_ticks.first().map_or_else(
                    || bar.open + if dir == Direction::Long { config.spread_points * point } else { 0.0 },
                    |t| if dir == Direction::Long { t.ask } else { t.bid });
                let fill = quote + dir.sign() * config.slippage_points * point;
                let quote_rate=match config.account.profit_rate(instrument,entry_time,fill,dir){Ok(rate)=>rate,Err(error)=>return invalid(format!("entry valuation: {error}"))};
                let margin_rate=match config.account.margin_base_rate(instrument,entry_time,fill,dir){Ok(rate)=>rate,Err(error)=>return invalid(format!("entry margin: {error}"))};
                let (sl, pt) = stop_profit_levels(strategy, &atr_sl, &atr_pt, i - 1, fill, dir, point);
                let sl_points = sl_distance_points(sl, fill, point);
                let size = match &strategy.money_management {
                    MoneyManagement::SimpleMartingale { lots_start, lots_reset, .. } => {
                        let s = if mart_size <= 0.0 { *lots_start } else { mart_size };
                        if s > *lots_reset { *lots_start } else { s }
                    }
                    _ => position_size(strategy, cash, sl_points, point_value * quote_rate, point, &atr_series, i - 1, fill),
                };
                let requested = size;
                attempt.requested_lots=Some(requested);
                let size = config.account.lots(requested);
                let allowed = size.filter(|size| {
                    let margin = size * instrument.contract_size * margin_rate / config.account.max_leverage;
                    let spread = interval_ticks.first().map_or(config.spread_points * point, |t| t.ask-t.bid);
                    let spread_and_slippage_cost = (spread + config.slippage_points * point) * instrument.contract_size * size * quote_rate;
                    let commission=config.commission_per_lot * size;
                    let entry_cost=spread_and_slippage_cost+commission;
                    let equity_after_entry_costs=cash-entry_cost;
                    let accepted=equity_after_entry_costs>=margin;
                    attempt.margin_check=Some(crate::MarginCheck{balance:cash,normalized_lots:*size,proposed_fill_price:fill,quote_to_account_rate:quote_rate,margin_base_to_account_rate:margin_rate,max_leverage:config.account.max_leverage,required_margin:margin,spread_and_slippage_cost,commission,equity_after_entry_costs,free_margin_after_entry:equity_after_entry_costs-margin,accepted});
                    accepted
                });
                if let Some(size) = allowed {
                attempt.filled_lots=Some(size);attempt.fill_price=Some(fill);attempt.trade_id=Some(trade_id);
                trades_today += 1;
                let trailing_distance = (fill - sl).abs();
                let trailing_enabled=strategy.exits.iter().any(|e|matches!(e,ExitType::TrailingStop));
                let break_even_enabled=strategy.exits.iter().any(|e|matches!(e,ExitType::MoveToBreakEven));
                attempt.initial_protection=Some(crate::InitialProtection{stop_loss:sl.is_finite().then_some(sl),profit_target:pt.is_finite().then_some(pt),trailing_enabled,break_even_enabled,dynamic_distance:((trailing_enabled||break_even_enabled)&&trailing_distance.is_finite()).then_some(trailing_distance),reference_bar_index:i-1,reference_bar_open_time:bars[i-1].time});
                position = Some(Position {
                    direction: dir,
                    size,
                    open_price: fill,
                    open_time: entry_time,
                    open_bar: i,
                    stop_loss: sl,
                    profit_target: pt,
                    trailing: trailing_enabled,
                    trailing_stop: sl,
                    trailing_distance,
                    move_to_be: break_even_enabled,
                    be_applied: false,
                    exit_after_bars: strategy.exits.iter().find_map(|e| match e {
                        ExitType::ExitAfterBars(n) => Some(*n),
                        _ => None,
                    }),
                    mae_points: 0.0,
                    mfe_points: 0.0,
                    financing: 0.0, last_rollover_day: day,
                });
                } else {
                    let reason=if size.is_none(){"invalid_volume"}else{"insufficient_margin"};
                    attempt.blocked_by.push(reason.into());
                    rejected_orders.push(OrderRejection { time: entry_time, direction: dir,
                        requested_lots: requested, reason: reason.into() });
                }
            }
        }

        // 2. Manage the open position intra-bar (SL / PT / trailing / BE).
        if let Some(mut pos) = position.take() {
            // Short positions liquidate at ask. Tick prices already carry their own spread.
            let mut bar = bar;
            if pos.direction == Direction::Short {
                let spread = config.spread_points * point;
                bar.open += spread; bar.high += spread; bar.low += spread; bar.close += spread;
            }
            accrue_financing(&mut pos,entry_time,&config.account,trade_id,&mut execution.financing_accruals);
            let risk = AccountRisk { cash, config, instrument, time: entry_time };
            let mut close_time = entry_time;
            let opening_price = interval_ticks.first().map_or(bar.open, |t| if pos.direction == Direction::Long {t.bid} else {t.ask});
            let queued_reason = pending_exit.take();
            if queued_reason.is_some() {
                if let Some(decision)=execution.scheduled_exit_decisions.last_mut() {
                    if decision.trade_id==trade_id {decision.executed_time=Some(entry_time);}
                }
            }
            let opening_exit = queued_reason.or_else(|| session_exit_due(pos.open_time, entry_time, options).then_some(ExitReason::ExitAtEndOfDay)).map(|reason| (opening_price, reason));
            let mut closed = opening_exit;
            if closed.is_none() {
                if let (Some(ticks), Some(ranges)) = (ticks, &tick_ranges) {
                    let (a, b) = ranges[i];
                    if a < b {
                        // Tick chronology is authoritative; never overwrite a no-exit result
                        // with a contradictory OHLC extreme from this same interval.
                        if let Some((price, reason, time)) = tick_exit(&ticks[a..b], &mut pos, point, &risk, options,trade_id,&mut execution.financing_accruals,&mut execution.stop_changes) {
                            closed = Some((price, reason)); close_time = time;
                        }
                    } else {
                        closed = resolve_bar(&bar, &mut pos, point, config.intrabar_policy, &risk, &mut ambiguous_bars,trade_id,&mut execution.stop_changes);
                    }
                } else {
                    closed = resolve_bar(&bar, &mut pos, point, config.intrabar_policy, &risk, &mut ambiguous_bars,trade_id,&mut execution.stop_changes);
                }
            }

            if closed.is_none() {
                let mut reasons=Vec::new();
                // The entry candle counts as one occupied candle.
                if pos.exit_after_bars.is_some_and(|n|i-pos.open_bar>=n-1) {
                    reasons.push(ExitReason::ExitAfterBars);
                }
                let exit_sig=match pos.direction {Direction::Long=>signals.long_exit[i],Direction::Short=>signals.short_exit[i]};
                if exit_sig {reasons.push(ExitReason::ExitSignal);}
                // Preserve the existing signal-over-time reporting precedence, but
                // retain every simultaneous reason for the same next-open request.
                if let Some(&chosen_reason)=reasons.last() {
                    pending_exit=Some(chosen_reason);
                    execution.scheduled_exit_decisions.push(crate::ScheduledExitDecision {
                        signal:if exit_sig{signals.explain(if pos.direction==Direction::Long{sqx_strategy::SignalKind::LongExit}else{sqx_strategy::SignalKind::ShortExit},i)}else{None},
                        trade_id,decision_bar_index:i,decision_bar_open_time:bars[i].time,
                        reasons,chosen_reason,executed_time:None,
                    });
                }
            }

            match closed {
                Some((px, reason)) => {
                    pending_exit = None;
                    let before_slippage=px;
                    let px = if reason == ExitReason::ProfitTarget { px } else { px - pos.direction.sign() * config.slippage_points * point };
                    if reason==ExitReason::StopOut {
                        let risk_at_close=AccountRisk{time:close_time,..risk};
                        let rate=match config.account.profit_rate(instrument,close_time,before_slippage,pos.direction){Ok(rate)=>rate,Err(error)=>return invalid(format!("stop-out valuation: {error}"))};
                        let unrealized=(before_slippage-pos.open_price)*pos.direction.sign()/point*pos.size*point_value*rate;
                        let commission=config.commission_per_lot*pos.size;
                        let margin_rate=match config.account.margin_base_rate(instrument,close_time,before_slippage,pos.direction){Ok(rate)=>rate,Err(error)=>return invalid(format!("stop-out margin: {error}"))};
                        let margin=pos.size*instrument.contract_size*margin_rate/config.account.max_leverage;
                        execution.stop_out_decisions.push(crate::StopOutDecision{trade_id,time:close_time,liquidation_price_before_slippage:before_slippage,fill_price:px,threshold_price:risk_at_close.stop_out_price(&pos,before_slippage),quote_to_account_rate:rate,margin_base_to_account_rate:margin_rate,balance:cash,unrealized_profit_account:unrealized,commission_reserve:commission,financing_accrued:pos.financing,equity_before_slippage:cash+unrealized-commission-pos.financing,required_margin:margin,stop_out_ratio:config.account.stop_out_ratio,minimum_equity:margin*config.account.stop_out_ratio});
                    }
                    let commission = config.commission_per_lot * pos.size;
                    let pnl_points = (px - pos.open_price) * pos.direction.sign() / point;
                    let rate = match config.account.profit_rate(instrument,close_time,px,pos.direction){Ok(rate)=>rate,Err(error)=>return invalid(format!("exit valuation: {error}"))};
                    let pnl = pnl_points * pos.size * point_value * rate - commission - pos.financing;
                    let balance_before=cash;
                    cash += pnl;
                    execution.cash_settlements.push(crate::CashSettlement {
                        quote_currency:instrument.quote_currency.clone(),account_currency:config.account.currency.clone(),
                        trade_id,time:close_time,exit_reason:reason,
                        gross_profit_quote:pnl_points * pos.size * point_value,quote_to_account_rate:rate,
                        gross_profit_account:pnl_points * pos.size * point_value * rate,
                        commission,financing:pos.financing,net_change:pnl,balance_before,balance_after:cash,
                    });
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
                        close_time,
                        open_price: pos.open_price,
                        close_price: px,
                        size: pos.size,
                        pnl,
                        pnl_points,
                        commission, financing: pos.financing,
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
        let mut snapshot=crate::AccountSnapshot{phase:"bar_close",bar_open_time:bar.time,quote_time:interval_ticks.last().map(|tick|tick.time),trade_id:None,liquidation_price:None,quote_to_account_rate:None,margin_base_to_account_rate:None,balance:cash,gross_unrealized:0.0,commission_reserve:0.0,financing_accrued:0.0,equity:cash,required_margin:0.0,free_margin:cash};
        if let Some(p)=position.as_ref() {
            let close = interval_ticks.last().map_or_else(
                || bar.close + if p.direction == Direction::Short { config.spread_points * point } else { 0.0 },
                |t| if p.direction == Direction::Long { t.bid } else { t.ask });
            let rate=match config.account.profit_rate(instrument,interval_ticks.last().map_or(bar.time,|t|t.time),close,p.direction){Ok(rate)=>rate,Err(error)=>return invalid(format!("equity valuation: {error}"))};
            snapshot.trade_id=Some(trade_id);snapshot.liquidation_price=Some(close);snapshot.quote_to_account_rate=Some(rate);
            snapshot.gross_unrealized=(close-p.open_price)*p.direction.sign()/point*p.size*point_value*rate;
            snapshot.commission_reserve=config.commission_per_lot*p.size;snapshot.financing_accrued=p.financing;
            snapshot.equity=cash+(snapshot.gross_unrealized-snapshot.commission_reserve-p.financing);
            let margin_rate=match config.account.margin_base_rate(instrument,interval_ticks.last().map_or(bar.time,|t|t.time),close,p.direction){Ok(rate)=>rate,Err(error)=>return invalid(format!("account margin: {error}"))};
            snapshot.margin_base_to_account_rate=Some(margin_rate);
            snapshot.required_margin=p.size*instrument.contract_size*margin_rate/config.account.max_leverage;
            snapshot.free_margin=snapshot.equity-snapshot.required_margin;
        }
        equity.push((bar.time,snapshot.equity));
        execution.account_snapshots.push(snapshot);
    }

    if let Some(direction)=pending {
        let index=bars.len()-1;
        execution.unexecuted_entry=Some(crate::UnexecutedEntry{signal_bar_index:index,signal_bar_open_time:bars[index].time,direction,signal:signals.explain(if direction==Direction::Long{sqx_strategy::SignalKind::LongEntry}else{sqx_strategy::SignalKind::ShortEntry},index),reason:"no_following_opening_quote"});
    }

    // Force-close any open position at the last bar.
    if let Some(pos) = position.take() {
        let last = bars.last().unwrap();
        let last_tick = match (ticks, &tick_ranges) { (Some(ts), Some(ranges)) => ts[ranges[bars.len()-1].0..ranges[bars.len()-1].1].last(), _ => None };
        let close_time = last_tick.map_or(last.time, |t|t.time);
        let px = last_tick.map_or_else(|| last.close + if pos.direction == Direction::Short { config.spread_points * point } else {0.0},
            |t| if pos.direction == Direction::Long {t.bid} else {t.ask}) - pos.direction.sign() * config.slippage_points * point;
        let commission = config.commission_per_lot * pos.size;
        let pnl_points = (px - pos.open_price) * pos.direction.sign() / point;
        let rate = match config.account.profit_rate(instrument,close_time,px,pos.direction){Ok(rate)=>rate,Err(error)=>return invalid(format!("exit valuation: {error}"))};
        let pnl = pnl_points * pos.size * point_value * rate - commission - pos.financing;
        let balance_before=cash;
        cash += pnl;
        execution.cash_settlements.push(crate::CashSettlement {
                        quote_currency:instrument.quote_currency.clone(),account_currency:config.account.currency.clone(),
            trade_id,time:close_time,exit_reason:ExitReason::EndOfData,
            gross_profit_quote:pnl_points * pos.size * point_value,quote_to_account_rate:rate,
            gross_profit_account:pnl_points * pos.size * point_value * rate,
            commission,financing:pos.financing,net_change:pnl,balance_before,balance_after:cash,
        });
        execution.account_snapshots.push(crate::AccountSnapshot{phase:"terminal_liquidation",bar_open_time:last.time,quote_time:last_tick.map(|tick|tick.time),trade_id:None,liquidation_price:None,quote_to_account_rate:None,margin_base_to_account_rate:None,balance:cash,gross_unrealized:0.0,commission_reserve:0.0,financing_accrued:0.0,equity:cash,required_margin:0.0,free_margin:cash});
        trades.push(Trade {
            id: trade_id,
            direction: pos.direction,
            open_time: pos.open_time,
            close_time,
            open_price: pos.open_price,
            close_price: px,
            size: pos.size,
            pnl,
            pnl_points,
            commission, financing: pos.financing,
            bars: bars.len() - 1 - pos.open_bar,
            mae_points: pos.mae_points,
            mfe_points: pos.mfe_points,
            exit_reason: ExitReason::EndOfData,
        });
    }

    if let Some(last) = equity.last_mut() { last.1 = cash; }
    let result=BacktestResult {
        errors: Vec::new(), rejected_orders, ambiguous_bars, execution,
        final_equity: cash,
        trades,
        equity,
        initial_deposit: config.initial_deposit,
    };
    if let Err(error)=crate::validate_account_evidence(&result){return invalid(format!("account evidence: {error}"));}
    result
}

fn stop_profit_levels(
    s: &Strategy,
    atr_sl: &Option<Vec<f64>>,
    atr_pt: &Option<Vec<f64>>,
    i: usize,
    fill: f64,
    dir: Direction,
    point: f64,
) -> (f64, f64) {
    let sl = match s.stop_loss {
        Some(StopLoss::Points(p)) => fill - dir.sign() * p * point,
        Some(StopLoss::Atr { mult, .. }) => fill - dir.sign() * atr_value(atr_sl, i) * mult,
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
        Some(ProfitTarget::Atr { mult, .. }) => fill + dir.sign() * atr_value(atr_pt, i) * mult,
        None => f64::NAN,
    };
    (sl, pt)
}

fn atr_value(atr: &Option<Vec<f64>>, i: usize) -> f64 {
    atr.as_ref()
        .and_then(|a| if i < a.len() && !a[i].is_nan() { Some(a[i]) } else { None })
        .unwrap_or(0.0)
}

// Update only after the price has actually been reached. This prevents a later
// high/low from retroactively activating a trailing or break-even stop.
fn observe_price(pos: &mut Position, price: f64, point: f64) {
    let movement = (price - pos.open_price) * pos.direction.sign();
    pos.mfe_points = pos.mfe_points.max(movement / point);
    pos.mae_points = pos.mae_points.min(movement / point);
    if pos.trailing {
        if pos.direction == Direction::Long {
            pos.trailing_stop = pos.trailing_stop.max(price - pos.trailing_distance);
        } else {
            pos.trailing_stop = pos.trailing_stop.min(price + pos.trailing_distance);
        }
    }
    if pos.move_to_be && !pos.be_applied && movement >= pos.trailing_distance {
        pos.stop_loss = pos.open_price;
        pos.be_applied = true;
    }
}

fn observe_price_recorded(pos:&mut Position,price:f64,point:f64,time:Timestamp,trade_id:usize,source:&'static str,path_step:Option<usize>,events:&mut Vec<crate::StopChange>){
    let before=effective_stop(pos);
    observe_price(pos,price,point);
    let after=effective_stop(pos);
    if before!=after {
        events.push(crate::StopChange{trade_id,time,observed_price:price,source,path_step,previous_stop:before.0.is_finite().then_some(before.0),new_stop:after.0.is_finite().then_some(after.0),previous_reason:before.1,new_reason:after.1});
    }
}

fn effective_stop(pos: &Position) -> (f64, ExitReason) {
    let tighter = pos.trailing && (pos.trailing_stop - pos.stop_loss) * pos.direction.sign() > 0.0;
    if tighter { (pos.trailing_stop, ExitReason::TrailingStop) }
    else { (pos.stop_loss, if pos.be_applied { ExitReason::MoveToBreakEven } else { ExitReason::StopLoss }) }
}

fn hit_at_price(pos: &Position, price: f64, gap: bool, risk: &AccountRisk) -> Option<(f64, ExitReason)> {
    let sign = pos.direction.sign();
    let (mut stop, mut reason) = effective_stop(pos);
    let stop_out = risk.stop_out_price(pos, price);
    if stop_out.is_finite() && (stop_out - stop) * sign > 0.0 {
        stop = stop_out; reason = ExitReason::StopOut;
    }
    if (price - stop) * sign <= 0.0 {
        return Some((if gap { price } else { stop }, reason));
    }
    if pos.profit_target.is_finite() && (price - pos.profit_target) * sign >= 0.0 {
        return Some((if gap { price } else { pos.profit_target }, ExitReason::ProfitTarget));
    }
    None
}

fn resolve_bar(bar: &Bar, pos: &mut Position, point: f64, policy: IntrabarPolicy, risk: &AccountRisk, ambiguities: &mut Vec<BarAmbiguity>,trade_id:usize,events:&mut Vec<crate::StopChange>) -> Option<(f64, ExitReason)> {
    let mut high = *pos;
    let mut low = *pos;
    let mut high_changes=Vec::new();let mut low_changes=Vec::new();
    let high_exit = bar_exit(bar, &mut high, point, true, risk,trade_id,&mut high_changes);
    let low_exit = bar_exit(bar, &mut low, point, false, risk,trade_id,&mut low_changes);
    if high_exit != low_exit || effective_stop(&high) != effective_stop(&low) {
        ambiguities.push(BarAmbiguity { time: bar.time, direction: pos.direction,
            high_first_exit: high_exit, low_first_exit: low_exit,
            high_first_stop: effective_stop(&high).0, low_first_stop: effective_stop(&low).0 });
    }
    if policy.high_first(pos.direction) { *pos = high; events.extend(high_changes); high_exit }
    else { *pos = low; events.extend(low_changes); low_exit }
}

fn bar_exit(bar: &Bar, pos: &mut Position, point: f64, up_first: bool, risk: &AccountRisk,trade_id:usize,events:&mut Vec<crate::StopChange>) -> Option<(f64, ExitReason)> {
    let path = if up_first { [bar.open, bar.high, bar.low, bar.close] }
        else { [bar.open, bar.low, bar.high, bar.close] };
    for (i, price) in path.into_iter().enumerate() {
        if let Some(exit) = hit_at_price(pos, price, i == 0, risk) {
            observe_price(pos, exit.0, point);
            return Some(exit);
        }
        observe_price_recorded(pos,price,point,bar.time,trade_id,if up_first{"ohlc_high_first"}else{"ohlc_low_first"},Some(i),events);
    }
    None
}

// Charge crossed UTC rollovers before evaluating the next executable quote.
// Calling repeatedly within the same day does not charge twice.
fn accrue_financing(pos:&mut Position,time:Timestamp,account:&AccountConfig,trade_id:usize,events:&mut Vec<crate::FinancingAccrual>){
    let day=time.div_euclid(86_400_000);
    for rollover in (pos.last_rollover_day+1)..=day {
        let weekday=(rollover+3).rem_euclid(7) as u8;
        let multiplier=if account.triple_rollover_weekday==Some(weekday){3.0}else{1.0};
        let cost=if pos.direction==Direction::Long{account.long_financing_per_lot}else{account.short_financing_per_lot};
        let charge=pos.size*cost*multiplier;
        let accrued_before=pos.financing;
        pos.financing+=charge;
        if charge!=0.0 {events.push(crate::FinancingAccrual{trade_id,rollover_utc_day:rollover,applied_at:time,lots:pos.size,daily_charge_per_lot:cost,multiplier:multiplier as u8,charge,accrued_before,accrued_after:pos.financing});}
    }
    pos.last_rollover_day=pos.last_rollover_day.max(day);
}

/// Stop-market orders execute at the first observed quote, including adverse gaps.
fn tick_exit(ticks: &[Tick], pos: &mut Position, point: f64, risk: &AccountRisk, options: &sqx_strategy::TradingOptions,trade_id:usize,events:&mut Vec<crate::FinancingAccrual>,stop_changes:&mut Vec<crate::StopChange>) -> Option<(f64, ExitReason, Timestamp)> {
    for tick in ticks {
        accrue_financing(pos,tick.time,&risk.config.account,trade_id,events);
        let price = if pos.direction == Direction::Long { tick.bid } else { tick.ask };
        if let Some(exit) = hit_at_price(pos, price, true, &AccountRisk { time: tick.time, ..*risk }) {
            observe_price(pos, exit.0, point);
            return Some((exit.0, exit.1, tick.time));
        }
        if session_exit_due(pos.open_time, tick.time, options) {
            observe_price(pos, price, point);
            return Some((price, ExitReason::ExitAtEndOfDay, tick.time));
        }
        observe_price_recorded(pos,price,point,tick.time,trade_id,"observed_tick",None,stop_changes);
    }
    None
}

fn sl_distance_points(sl: f64, fill: f64, point: f64) -> f64 {
    if sl.is_finite() {
        (fill - sl).abs() / point
    } else {
        f64::NAN
    }
}

// Deadlines depend only on the entry timestamp and declared schedule, never on
// a later candle or the end of the supplied dataset. Missing quotes delay execution.
fn session_exit_due(open: i64, now: i64, options: &sqx_strategy::TradingOptions) -> bool {
    let Some(minute) = options.session_close_utc_minute else { return false; };
    let day = open.div_euclid(86_400_000);
    let mut deadline_day = day;
    if !options.exit_at_end_of_day {
        if !options.exit_on_friday { return false; }
        deadline_day += (1 - day.rem_euclid(7)).rem_euclid(7);
    }
    let mut deadline = deadline_day as i128 * 86_400_000 + minute as i128 * 60_000;
    if deadline <= open as i128 {
        deadline += if options.exit_at_end_of_day {86_400_000} else {7 * 86_400_000};
    }
    now as i128 >= deadline
}

fn session_entry_closed(time: i64, options: &sqx_strategy::TradingOptions) -> bool {
    let Some(minute) = options.session_close_utc_minute else { return false; };
    let applies = options.exit_at_end_of_day || (options.exit_on_friday && time.div_euclid(86_400_000).rem_euclid(7) == 1);
    applies && time.rem_euclid(86_400_000) >= minute as i64 * 60_000
}

fn position_size(
    s: &Strategy,
    balance: f64,
    sl_points: f64,
    point_value: f64,
    point: f64,
    atr: &Option<Vec<f64>>,
    i: usize,
    fill: f64,
) -> f64 {
    match &s.money_management {
        MoneyManagement::FixedSize { size } => *size,
        MoneyManagement::FixedAmount { amount } | MoneyManagement::CryptoFixedAmount { amount } => {
            let denom = (sl_points * point_value).max(1e-9);
            amount / denom
        }
        MoneyManagement::RiskFixedBalancePct { risk } | MoneyManagement::CryptoFixedBalancePct { risk } => {
            let risk_money = balance * risk / 100.0;
            let denom = (sl_points * point_value).max(1e-9);
            risk_money / denom
        }
        MoneyManagement::RiskFixedPctOfAccount { risk, stop_loss_points } => {
            let risk_money = balance * risk / 100.0;
            let denom = (stop_loss_points * point_value).max(1e-9);
            risk_money / denom
        }
        MoneyManagement::AtrRiskBased { risk, atr_mult, .. } => {
            let dist = atr_value(atr, i) * atr_mult;
            let risk_money = balance * risk / 100.0;
            // ATR is a price distance; point_value is money per point.
            let denom = (dist / point * point_value).max(1e-9);
            risk_money / denom
        }
        MoneyManagement::StocksSizeByPrice { risk } | MoneyManagement::CryptoSizeByPrice { risk } => {
            let risk_money = balance * risk / 100.0;
            // Engine sizes are lots, not underlying units. point_value already
            // includes contract size and quote-to-account conversion at entry.
            let account_value_per_lot=fill * (point_value / point);
            if account_value_per_lot.is_finite() && account_value_per_lot>0.0 {
                risk_money / account_value_per_lot
            }else{f64::NAN}
        }
        MoneyManagement::SimpleMartingale { lots_start, .. } => *lots_start,
    }
}


struct AccountRisk<'a> {
    cash: f64,
    config: &'a BacktestConfig,
    instrument: &'a Instrument,
    time: Timestamp,
}

impl AccountRisk<'_> {
    // Solve equity(price) = margin(price) * stop_out_ratio. Conversion is either
    // reciprocal for a base-currency account or constant for quote/external rates.
    fn stop_out_price(&self, pos: &Position, price: f64) -> f64 {
        let account = &self.config.account;
        let units = pos.size * self.instrument.contract_size;
        let sign = pos.direction.sign();
        let balance = self.cash - self.config.commission_per_lot * pos.size - pos.financing;
        let fraction = account.stop_out_ratio / account.max_leverage;
        if self.instrument.base_currency == account.currency && self.instrument.quote_currency != account.currency {
            units * sign * pos.open_price / (balance + units * sign - fraction * units)
        } else {
            let rate = account.profit_rate(self.instrument, self.time, price, pos.direction).expect("validated conversion");
            if account.margin_model==crate::MarginModel::ForexBase && self.instrument.quote_currency!=account.currency {
                let margin_rate=account.margin_base_rate(self.instrument,self.time,price,pos.direction).expect("validated margin conversion");
                pos.open_price+(fraction*units*margin_rate-balance)/(units*sign*rate)
            } else {
                (units * sign * pos.open_price * rate - balance) / (units * rate * (sign - fraction))
            }
        }
    }
}

pub fn validate_backtest_inputs(bars: &[Bar], instrument: &Instrument, config: &BacktestConfig) -> Result<(), String> {
    config.account.validate()?;
    if let Some(duration)=config.bar_duration_ms {
        if duration<=0{return Err("bar_duration_ms must be positive".into());}
        if bars.iter().any(|bar|bar.time.checked_add(duration).is_none()){return Err("bar_duration_ms overflows candle end timestamp".into());}
        if bars.windows(2).any(|pair|pair[0].time.checked_add(duration).is_some_and(|end|end>pair[1].time)){return Err("bar_duration_ms overlaps successive candles".into());}
    }
    if !config.initial_deposit.is_finite() || config.initial_deposit <= 0.0
        || [config.spread_points, config.slippage_points, config.commission_per_lot].iter().any(|v| !v.is_finite() || *v < 0.0)
        || !instrument.point.is_finite() || instrument.point <= 0.0
        || !instrument.contract_size.is_finite() || instrument.contract_size <= 0.0
        || instrument.base_currency.is_empty() || instrument.quote_currency.is_empty() {
        return Err("invalid deposit, costs or instrument specification".into());
    }
    for (i, bar) in bars.iter().enumerate() {
        if [bar.open, bar.high, bar.low, bar.close].iter().any(|p| !p.is_finite() || *p <= 0.0)
            || !bar.volume.is_finite() || bar.volume < 0.0
            || bar.low > bar.open.min(bar.close) || bar.high < bar.open.max(bar.close)
            || bar.low <= config.slippage_points * instrument.point
            || !(bar.high + (config.spread_points + config.slippage_points) * instrument.point).is_finite()
            || bar.low > bar.high || (i > 0 && bar.time <= bars[i - 1].time) {
            return Err(format!("invalid or non-increasing bar at index {i}"));
        }
        for direction in [Direction::Long,Direction::Short] {config.account.profit_rate(instrument,bar.time,bar.open,direction)?;config.account.margin_base_rate(instrument,bar.time,bar.open,direction)?;}
    }
    Ok(())
}
