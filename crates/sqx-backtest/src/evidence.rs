//! Reconciliation of schema-1 engine account evidence, not broker certification.
use crate::BacktestResult;
fn equal(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-10 * a.abs().max(b.abs()).max(1.0)
}
fn require(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

/// Strictly reconcile recorded cash, financing and sampled account observations.
/// Legacy/aggregate results have schema 0 and cannot pass this evidence check.
pub fn validate_account_evidence(result: &BacktestResult) -> Result<(), String> {
    let evidence = &result.execution;
    require(
        evidence.account_evidence_schema == 1,
        "unsupported or missing account evidence schema",
    )?;
    require(
        result.errors.is_empty(),
        "invalid execution cannot certify account evidence",
    )?;
    require(
        evidence
            .account_currency
            .as_ref()
            .is_some_and(|c| !c.is_empty()),
        "missing account currency",
    )?;
    require(
        evidence.cash_settlements.len() == result.trades.len(),
        "settlement count differs from closed trades",
    )?;
    let mut admitted = vec![false; result.trades.len()];
    for attempt in &evidence.entry_attempts {
        require(
            attempt.signal_bar_open_time < attempt.attempt_time,
            "entry precedes completed signal candle",
        )?;
        if let Some(margin) = &attempt.margin_check {
            require(
                [
                    margin.normalized_lots,
                    margin.proposed_fill_price,
                    margin.quote_to_account_rate,
                    margin.max_leverage,
                ]
                .iter()
                .all(|value| value.is_finite() && *value > 0.0),
                "invalid margin admission inputs",
            )?;
            require(
                [
                    margin.required_margin,
                    margin.spread_and_slippage_cost,
                    margin.commission,
                ]
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0),
                "invalid margin admission costs",
            )?;
            require(
                equal(
                    margin.equity_after_entry_costs,
                    margin.balance - (margin.spread_and_slippage_cost + margin.commission),
                ) && equal(
                    margin.free_margin_after_entry,
                    margin.equity_after_entry_costs - margin.required_margin,
                ),
                "margin admission arithmetic differs",
            )?;
            require(
                margin.accepted == (margin.equity_after_entry_costs >= margin.required_margin),
                "margin admission decision contradicts available equity",
            )?;
        }
        if let Some(id) = attempt.trade_id {
            let trade = result
                .trades
                .get(id)
                .ok_or("entry references unknown trade")?;
            require(!admitted[id], "multiple entries reference one trade")?;
            admitted[id] = true;
            require(attempt.blocked_by.is_empty(), "blocked entry has a trade")?;
            require(
                attempt.direction == trade.direction && attempt.attempt_time == trade.open_time,
                "entry identity differs from trade",
            )?;
            require(
                attempt
                    .filled_lots
                    .is_some_and(|size| equal(size, trade.size))
                    && attempt
                        .fill_price
                        .is_some_and(|price| equal(price, trade.open_price)),
                "entry fill differs from trade",
            )?;
            let margin = attempt
                .margin_check
                .as_ref()
                .ok_or("filled entry lacks margin admission")?;
            require(
                margin.accepted
                    && equal(margin.normalized_lots, trade.size)
                    && equal(margin.proposed_fill_price, trade.open_price),
                "filled entry differs from margin admission",
            )?;
            require(
                attempt.initial_protection.is_some(),
                "filled entry lacks initial protection record",
            )?;
        } else {
            require(
                !attempt.blocked_by.is_empty()
                    && attempt.filled_lots.is_none()
                    && attempt.fill_price.is_none()
                    && attempt.initial_protection.is_none(),
                "unfilled entry contains execution or lacks rejection",
            )?;
            require(
                attempt
                    .margin_check
                    .as_ref()
                    .is_none_or(|margin| !margin.accepted),
                "unfilled entry has accepted margin admission",
            )?;
        }
    }
    require(
        admitted.iter().all(|accepted| *accepted),
        "trade lacks an accepted entry",
    )?;
    // Source candle indices include pre-trading warmup; equity indices do not.
    // Match recorded candle timestamps instead of mixing these coordinate systems.
    let mut scheduled = vec![false; result.trades.len()];
    for decision in &evidence.scheduled_exit_decisions {
        let trade = result
            .trades
            .get(decision.trade_id)
            .ok_or("exit decision references unknown trade")?;
        require(
            !scheduled[decision.trade_id],
            "duplicate scheduled exit for trade",
        )?;
        scheduled[decision.trade_id] = true;
        require(
            result
                .equity
                .binary_search_by_key(&decision.decision_bar_open_time, |(time, _)| *time)
                .is_ok(),
            "exit decision candle differs",
        )?;
        require(
            !decision.reasons.is_empty()
                && decision.reasons.iter().all(|reason| {
                    matches!(
                        reason,
                        crate::ExitReason::ExitSignal | crate::ExitReason::ExitAfterBars
                    )
                })
                && decision.reasons.last() == Some(&decision.chosen_reason),
            "invalid scheduled exit reason or precedence",
        )?;
        if let Some(time) = decision.executed_time {
            require(
                time > decision.decision_bar_open_time
                    && time == trade.close_time
                    && decision.chosen_reason == trade.exit_reason,
                "executed exit differs from trade",
            )?;
        } else {
            require(
                trade.exit_reason == crate::ExitReason::EndOfData
                    && result
                        .equity
                        .last()
                        .is_some_and(|(time, _)| *time == decision.decision_bar_open_time),
                "unexecuted exit is not at dataset boundary",
            )?;
        }
    }
    for trade in &result.trades {
        if matches!(
            trade.exit_reason,
            crate::ExitReason::ExitSignal | crate::ExitReason::ExitAfterBars
        ) {
            require(
                scheduled.get(trade.id) == Some(&true),
                "scheduled trade exit lacks decision",
            )?;
        }
    }
    let mut stopped = vec![false; result.trades.len()];
    for decision in &evidence.stop_out_decisions {
        let trade = result
            .trades
            .get(decision.trade_id)
            .ok_or("stop-out references unknown trade")?;
        require(!stopped[decision.trade_id], "duplicate stop-out decision")?;
        stopped[decision.trade_id] = true;
        require(
            trade.exit_reason == crate::ExitReason::StopOut
                && decision.time == trade.close_time
                && equal(decision.fill_price, trade.close_price),
            "stop-out differs from closed trade",
        )?;
        require(
            [
                decision.liquidation_price_before_slippage,
                decision.quote_to_account_rate,
            ]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0),
            "invalid stop-out valuation inputs",
        )?;
        require(
            decision.required_margin.is_finite()
                && decision.required_margin >= 0.0
                && decision.stop_out_ratio.is_finite()
                && (0.0..=1.0).contains(&decision.stop_out_ratio),
            "invalid stop-out margin inputs",
        )?;
        require(
            equal(decision.commission_reserve, trade.commission)
                && equal(decision.financing_accrued, trade.financing),
            "stop-out costs differ from trade",
        )?;
        let settlement = evidence
            .cash_settlements
            .get(decision.trade_id)
            .ok_or("stop-out lacks settlement")?;
        require(
            equal(decision.balance, settlement.balance_before),
            "stop-out balance differs from settlement",
        )?;
        require(
            equal(
                decision.equity_before_slippage,
                decision.balance + decision.unrealized_profit_account
                    - decision.commission_reserve
                    - decision.financing_accrued,
            ) && equal(
                decision.minimum_equity,
                decision.required_margin * decision.stop_out_ratio,
            ),
            "stop-out arithmetic differs",
        )?;
        require(
            decision.equity_before_slippage <= decision.minimum_equity
                || equal(decision.equity_before_slippage, decision.minimum_equity),
            "stop-out above required threshold",
        )?;
    }
    for trade in &result.trades {
        if trade.exit_reason == crate::ExitReason::StopOut {
            require(
                stopped.get(trade.id) == Some(&true),
                "stop-out trade lacks decision",
            )?;
        }
    }
    let mut stops = vec![None; result.trades.len()];
    let mut stop_reasons = vec![crate::ExitReason::StopLoss; result.trades.len()];
    let mut last_stop_time = vec![None; result.trades.len()];
    let mut protection = vec![None; result.trades.len()];
    for attempt in &evidence.entry_attempts {
        if let Some(id) = attempt.trade_id {
            protection[id] = attempt.initial_protection.as_ref();
            stops[id] = attempt
                .initial_protection
                .as_ref()
                .and_then(|p| p.stop_loss);
        }
    }
    for change in &evidence.stop_changes {
        let id = change.trade_id;
        let trade = result
            .trades
            .get(id)
            .ok_or("stop change references unknown trade")?;
        let initial = protection[id].ok_or("stop change lacks initial protection")?;
        let same_stop = match (stops[id], change.previous_stop) {
            (None, None) => true,
            (Some(a), Some(b)) => equal(a, b),
            _ => false,
        };
        require(
            same_stop && stop_reasons[id] == change.previous_reason,
            "stop change chain differs",
        )?;
        require(
            change.time >= trade.open_time
                && change.time <= trade.close_time
                && last_stop_time[id].is_none_or(|time| change.time >= time),
            "stop change time outside position chronology",
        )?;
        require(
            change.observed_price.is_finite()
                && change.observed_price > 0.0
                && change.new_stop.is_some_and(|stop| stop.is_finite()),
            "invalid stop change prices",
        )?;
        if let (Some(old), Some(new)) = (change.previous_stop, change.new_stop) {
            require(
                (new - old) * trade.direction.sign() >= 0.0,
                "stop change weakens protection",
            )?;
        }
        require(
            match change.new_reason {
                crate::ExitReason::TrailingStop => initial.trailing_enabled,
                crate::ExitReason::MoveToBreakEven => initial.break_even_enabled,
                _ => false,
            },
            "stop change uses disabled protection",
        )?;
        require(
            match change.source {
                "observed_tick" => change.path_step.is_none(),
                "ohlc_high_first" | "ohlc_low_first" => {
                    change.path_step.is_some_and(|step| step < 4)
                }
                _ => false,
            },
            "invalid stop change observation source",
        )?;
        stops[id] = change.new_stop;
        stop_reasons[id] = change.new_reason;
        last_stop_time[id] = Some(change.time);
    }
    let mut balance = result.initial_deposit;
    for (index, (entry, trade)) in evidence
        .cash_settlements
        .iter()
        .zip(&result.trades)
        .enumerate()
    {
        require(
            trade.id == index
                && entry.trade_id == trade.id
                && entry.time == trade.close_time
                && entry.exit_reason == trade.exit_reason,
            "settlement trade identity differs",
        )?;
        require(
            Some(&entry.account_currency) == evidence.account_currency.as_ref()
                && !entry.quote_currency.is_empty(),
            "settlement currency differs",
        )?;
        require(
            entry.quote_to_account_rate.is_finite() && entry.quote_to_account_rate > 0.0,
            "invalid settlement conversion rate",
        )?;
        require(
            equal(entry.balance_before, balance),
            "settlement balance chain differs",
        )?;
        require(
            equal(
                entry.gross_profit_account,
                entry.gross_profit_quote * entry.quote_to_account_rate,
            ),
            "settlement currency conversion differs",
        )?;
        require(
            equal(entry.commission, trade.commission) && equal(entry.financing, trade.financing),
            "settlement costs differ from trade",
        )?;
        require(
            equal(
                entry.net_change,
                entry.gross_profit_account - entry.commission - entry.financing,
            ) && equal(entry.net_change, trade.pnl),
            "settlement net change differs",
        )?;
        balance += entry.net_change;
        require(
            equal(entry.balance_after, balance),
            "settlement closing balance differs",
        )?;
    }
    require(
        equal(balance, result.final_equity),
        "final cash differs from settlement chain",
    )?;
    let mut accrued = vec![0.0; result.trades.len()];
    let mut last_day = vec![None; result.trades.len()];
    for event in &evidence.financing_accruals {
        let id = event.trade_id;
        let trade = result
            .trades
            .get(id)
            .ok_or("financing references unknown trade")?;
        require(
            event.applied_at >= trade.open_time && event.applied_at <= trade.close_time,
            "financing timestamp outside trade",
        )?;
        require(
            event.rollover_utc_day <= event.applied_at.div_euclid(86_400_000)
                && event.rollover_utc_day > trade.open_time.div_euclid(86_400_000),
            "financing rollover outside holding interval",
        )?;
        require(
            last_day[id].is_none_or(|day| event.rollover_utc_day > day),
            "duplicate or unordered financing rollover",
        )?;
        require(
            matches!(event.multiplier, 1 | 3) && equal(event.lots, trade.size),
            "invalid financing size or multiplier",
        )?;
        require(
            equal(event.accrued_before, accrued[id])
                && equal(
                    event.charge,
                    event.lots * event.daily_charge_per_lot * event.multiplier as f64,
                ),
            "financing charge or chain differs",
        )?;
        accrued[id] += event.charge;
        last_day[id] = Some(event.rollover_utc_day);
        require(
            equal(event.accrued_after, accrued[id]),
            "financing accumulated total differs",
        )?;
    }
    for (trade, total) in result.trades.iter().zip(accrued) {
        require(
            equal(trade.financing, total),
            "trade financing differs from accrual events",
        )?;
    }
    let snapshots = &evidence.account_snapshots;
    require(!snapshots.is_empty(), "missing sampled account evidence")?;
    let ordinary: Vec<_> = snapshots
        .iter()
        .filter(|s| s.phase == "bar_close")
        .collect();
    require(
        ordinary.len() == result.equity.len(),
        "account sample count differs from equity curve",
    )?;
    // Exactly one ordinary observation per candle, optionally followed by a
    // single liquidation observation. Filtering by phase alone would hide
    // duplicated or prematurely inserted terminal observations.
    require(
        snapshots.len() == ordinary.len() || snapshots.len() == ordinary.len() + 1,
        "unexpected terminal account sample count",
    )?;
    require(
        snapshots
            .iter()
            .take(ordinary.len())
            .all(|s| s.phase == "bar_close"),
        "account sample phases are out of order",
    )?;
    require(
        ordinary
            .windows(2)
            .all(|pair| pair[0].bar_open_time < pair[1].bar_open_time),
        "account sample times are not strictly increasing",
    )?;
    if snapshots.len() > ordinary.len() {
        let terminal = snapshots.last().unwrap();
        let preceding = ordinary
            .last()
            .ok_or("terminal account sample has no preceding candle")?;
        require(
            terminal.phase == "terminal_liquidation" && preceding.trade_id.is_some(),
            "terminal liquidation has no open position",
        )?;
        require(
            terminal.bar_open_time == preceding.bar_open_time
                && terminal.quote_time == preceding.quote_time,
            "terminal liquidation observation time differs",
        )?;
    }
    for snapshot in snapshots {
        require(
            matches!(snapshot.phase, "bar_close" | "terminal_liquidation"),
            "unknown account sample phase",
        )?;
        require(
            equal(
                snapshot.equity,
                snapshot.balance
                    + (snapshot.gross_unrealized
                        - snapshot.commission_reserve
                        - snapshot.financing_accrued),
            ),
            "account equity components differ",
        )?;
        require(
            snapshot.required_margin >= 0.0
                && equal(
                    snapshot.free_margin,
                    snapshot.equity - snapshot.required_margin,
                ),
            "account free margin differs",
        )?;
        if let Some(id) = snapshot.trade_id {
            require(
                result.trades.get(id).is_some(),
                "account sample references unknown trade",
            )?;
            require(
                snapshot
                    .liquidation_price
                    .is_some_and(|p| p.is_finite() && p > 0.0)
                    && snapshot
                        .quote_to_account_rate
                        .is_some_and(|r| r.is_finite() && r > 0.0),
                "missing open-position valuation inputs",
            )?;
        } else {
            require(
                snapshot.liquidation_price.is_none()
                    && snapshot.quote_to_account_rate.is_none()
                    && equal(snapshot.gross_unrealized, 0.0)
                    && equal(snapshot.commission_reserve, 0.0)
                    && equal(snapshot.financing_accrued, 0.0)
                    && equal(snapshot.required_margin, 0.0),
                "flat account retains position valuation",
            )?;
        }
    }
    for (i, (snapshot, (time, value))) in ordinary.iter().zip(&result.equity).enumerate() {
        require(
            snapshot.bar_open_time == *time,
            "account sample timestamp differs from equity curve",
        )?;
        // The equity curve's final value includes forced liquidation, whereas the
        // final ordinary sample deliberately preserves the preceding open mark.
        if i + 1 < ordinary.len() {
            require(
                equal(snapshot.equity, *value),
                "account sample differs from equity curve",
            )?;
        }
    }
    let last = snapshots.last().unwrap();
    require(
        last.trade_id.is_none()
            && equal(last.balance, result.final_equity)
            && equal(last.equity, result.final_equity),
        "terminal account sample differs from final cash",
    )?;
    require(
        result
            .equity
            .last()
            .is_some_and(|(_, value)| equal(*value, result.final_equity)),
        "terminal equity differs from final cash",
    )?;
    Ok(())
}
