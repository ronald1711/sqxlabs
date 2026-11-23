use sqx_backtest::{backtest, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_strategy::{Comparator, Condition, Join, MoneyManagement, Operand, PriceField, Signal, SignalKind, Strategy};

fn rising_bars() -> Vec<Bar> {
    // 6 bars, open rises linearly, close above open.
    (0..6)
        .map(|i| {
            let o = 1.0 + i as f64 * 0.1;
            Bar::new(1_700_000_000_000 + i as i64 * 60_000, o, o + 0.05, o - 0.01, o + 0.03, 10.0)
        })
        .collect()
}

fn always_long_strategy() -> Strategy {
    let mut s = Strategy::new("always-long");
    s.long = true;
    s.short = false;
    s.money_management = MoneyManagement::FixedSize { size: 0.1 };
    // Close > 0 is always true.
    s.signals.push(Signal::new(
        SignalKind::LongEntry,
        vec![Condition::new(Operand::Price(PriceField::Close, 0), Comparator::Gt, Operand::Const(0.0))],
        Join::And,
    ));
    s
}

#[test]
fn always_long_holds_one_trade() {
    let bars = rising_bars();
    let inst = Instrument::fx("EURUSD");
    let cfg = BacktestConfig { initial_deposit: 10_000.0, spread_points: 0.0, slippage_points: 0.0, commission_per_lot: 0.0, bar_duration_ms: None, allow_tick_fallback: false, intrabar_policy: sqx_backtest::IntrabarPolicy::AdverseFirst, account: Default::default() };
    let result = backtest(&always_long_strategy(), &bars, &inst, &cfg);
    assert_eq!(result.trades.len(), 1, "expected exactly one trade");
    let t = &result.trades[0];
    assert_eq!(t.direction, sqx_core::Direction::Long);
    // Entry at bar[1].open, exit at last close.
    assert!(approx(t.open_price, bars[1].open));
    assert!(approx(t.close_price, bars[5].close));
    assert!(t.pnl > 0.0, "rising market long should profit");
}

#[test]
fn no_signal_produces_no_trades() {
    let bars = rising_bars();
    let inst = Instrument::fx("EURUSD");
    let cfg = BacktestConfig::default();
    let mut s = Strategy::new("empty");
    s.signals.push(Signal::new(SignalKind::LongEntry, vec![Condition::new(Operand::Price(PriceField::Close, 0), Comparator::Gt, Operand::Const(9999.0))], Join::And));
    let result = backtest(&s, &bars, &inst, &cfg);
    assert_eq!(result.trades.len(), 0);
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn tick_precision_resolves_sl_first() {
    use sqx_backtest::backtest_ticks;
    use sqx_core::Tick;
    use sqx_strategy::{ProfitTarget, StopLoss};
    let bars = vec![
        Bar::new(0, 100.0, 100.0, 100.0, 100.0, 1.0),
        Bar::new(60_000, 100.0, 100.0, 100.0, 100.0, 1.0),
        Bar::new(120_000, 100.0, 120.0, 80.0, 100.0, 1.0),
        Bar::new(180_000, 100.0, 100.0, 100.0, 100.0, 1.0),
    ];
    let mut s = always_long_strategy();
    s.stop_loss = Some(StopLoss::Points(100.0));
    s.profit_target = Some(ProfitTarget::Points(200.0));
    let ticks = vec![
        Tick { time: 120_001, bid: 99.90, ask: 99.91 },
        Tick { time: 120_002, bid: 100.05, ask: 100.06 },
    ];
    let cfg = BacktestConfig { initial_deposit: 10_000.0, spread_points: 0.0, slippage_points: 0.0, commission_per_lot: 0.0, bar_duration_ms: Some(60000), allow_tick_fallback: true, intrabar_policy: sqx_backtest::IntrabarPolicy::AdverseFirst, account: sqx_backtest::AccountConfig { max_leverage: 1000.0, ..Default::default() } };
    let r = backtest_ticks(&s, &bars, &ticks, &Instrument::fx("EURUSD"), &cfg);
    assert!(!r.trades.is_empty(), "at least one trade expected");
    assert_eq!(r.trades[0].exit_reason, sqx_backtest::ExitReason::StopLoss, "ticks hit SL first");
}

#[test]
fn spread_and_slippage_reduce_flat_round_trip_by_known_amount() {
    let bars: Vec<_> = (0..4).map(|i| Bar::new(i * 60_000, 1.0, 1.0, 1.0, 1.0, 1.0)).collect();
    for short in [false, true] {
        let mut s = always_long_strategy();
        s.long = !short;
        s.short = short;
        s.signals[0].kind = if short { SignalKind::ShortEntry } else { SignalKind::LongEntry };
        let cfg = BacktestConfig { spread_points: 2.0, slippage_points: 1.0, commission_per_lot: 5.0, ..Default::default() };
        let result = backtest(&s, &bars, &Instrument::fx("EURUSD"), &cfg);
        // 0.1 lot: $1/pip; 2 pip spread + 1 pip on each market fill + $0.50 commission.
        assert!((result.trades[0].pnl + 4.5).abs() < 1e-8);
        assert!((result.equity.last().unwrap().1 - result.final_equity).abs() < 1e-8);
    }
}

#[test]
fn close_signal_exits_at_next_open_not_known_close() {
    let mut s = always_long_strategy();
    s.signals.push(Signal::new(SignalKind::LongExit,
        vec![Condition::new(Operand::Price(PriceField::Close, 0), Comparator::Gt, Operand::Const(1.05))], Join::And));
    let bars = vec![
        Bar::new(0, 1.0, 1.0, 1.0, 1.0, 1.0),
        Bar::new(60_000, 1.0, 1.1, 1.0, 1.1, 1.0),
        Bar::new(120_000, 0.9, 0.9, 0.9, 0.9, 1.0),
    ];
    let result = backtest(&s, &bars, &Instrument::fx("EURUSD"), &BacktestConfig::default());
    assert_eq!(result.trades[0].close_time, 120_000);
    assert_eq!(result.trades[0].close_price, 0.9);
    assert_eq!(result.trades[0].exit_reason, sqx_backtest::ExitReason::ExitSignal);
}

#[test]
fn atr_stop_at_open_is_independent_of_that_bars_future_extremes() {
    let mut s = always_long_strategy();
    s.stop_loss = Some(sqx_strategy::StopLoss::Atr { period: 2, mult: 2.0 });
    let mut bars = vec![
        Bar::new(0, 1.0, 1.01, 0.99, 1.0, 1.0),
        Bar::new(60_000, 1.0, 1.01, 0.99, 1.0, 1.0),
        Bar::new(120_000, 1.0, 1.01, 0.99, 1.0, 1.0),
        Bar::new(180_000, 1.0, 1.01, 0.8, 1.0, 1.0),
    ];
    let a = backtest(&s, &bars, &Instrument::fx("EURUSD"), &BacktestConfig::default());
    bars[3].high = 2.0;
    let b = backtest(&s, &bars, &Instrument::fx("EURUSD"), &BacktestConfig::default());
    assert_eq!(a.trades[0].open_time, 180_000, "wait for completed ATR warmup");
    assert!((a.trades[0].close_price - 0.96).abs() < 1e-9);
    assert_eq!(a.trades[0].close_price, b.trades[0].close_price);
    let entry=a.execution.entry_attempts.iter().find(|e|e.trade_id==Some(0)).unwrap();
    let protection=entry.initial_protection.as_ref().unwrap();
    assert!((protection.stop_loss.unwrap()-0.96).abs()<1e-9);
    assert_eq!(protection.reference_bar_index,2);assert_eq!(protection.reference_bar_open_time,120000);
    let changed=b.execution.entry_attempts.iter().find(|e|e.trade_id==Some(0)).unwrap();
    assert_eq!(entry.initial_protection,changed.initial_protection);
}

#[test]
fn later_high_cannot_retroactively_move_a_stop() {
    for exit in [sqx_strategy::ExitType::TrailingStop, sqx_strategy::ExitType::MoveToBreakEven] {
        let mut s = always_long_strategy();
        s.stop_loss = Some(sqx_strategy::StopLoss::Points(100.0));
        s.exits.push(exit);
        let bars = vec![
            Bar::new(0, 1.0, 1.0, 1.0, 1.0, 1.0),
            Bar::new(60_000, 1.0, 1.02, 0.98, 1.01, 1.0),
        ];
        let result = backtest(&s, &bars, &Instrument::fx("EURUSD"), &BacktestConfig::default());
        assert_eq!(result.trades[0].exit_reason, sqx_backtest::ExitReason::StopLoss);
        assert!((result.trades[0].close_price - 0.99).abs() < 1e-9);
        assert_eq!(result.trades[0].mfe_points, 0.0);
        assert!(result.execution.stop_changes.is_empty(),"alternative high-first amendments must not leak into adverse-first execution");
    }
}

#[test]
fn tick_gap_fills_at_available_quote_and_no_hit_is_authoritative() {
    use sqx_backtest::backtest_ticks;
    use sqx_core::Tick;
    let mut s = always_long_strategy();
    s.stop_loss = Some(sqx_strategy::StopLoss::Points(100.0));
    let bars = vec![
        Bar::new(0, 1.0, 1.0, 1.0, 1.0, 1.0),
        Bar::new(60_000, 1.0, 1.0, 0.9, 1.0, 1.0),
        Bar::new(120_000, 1.0, 1.0, 1.0, 1.0, 1.0),
    ];
    let cfg = BacktestConfig { bar_duration_ms: Some(60000), allow_tick_fallback: true, ..Default::default() };
    let ticks = [Tick { time: 60_000, bid: 1.0, ask: 1.0 }, Tick { time: 60_001, bid: 0.98, ask: 0.981 }];
    let r = backtest_ticks(&s, &bars, &ticks, &Instrument::fx("EURUSD"), &cfg);
    assert_eq!(r.trades[0].close_price, 0.98);
    let ticks = [Tick { time: 60_001, bid: 1.0, ask: 1.001 }];
    let r = backtest_ticks(&s, &bars, &ticks, &Instrument::fx("EURUSD"), &cfg);
    assert_eq!(r.trades[0].exit_reason, sqx_backtest::ExitReason::EndOfData);
}

#[test]
fn ambiguous_sl_tp_is_adverse_for_both_sides_and_reported() {
    use sqx_backtest::{ExitReason, IntrabarPolicy};
    use sqx_strategy::{StopLoss, ProfitTarget};
    for short in [false,true] {
        let mut s=always_long_strategy();s.long=!short;s.short=short;
        s.signals[0].kind=if short {SignalKind::ShortEntry} else {SignalKind::LongEntry};
        s.stop_loss=Some(StopLoss::Points(100.0));
        s.profit_target=Some(ProfitTarget::Points(100.0));
        let bars=[Bar::new(0,1.0,1.0,1.0,1.0,1.0),Bar::new(60000,1.0,1.02,0.98,1.0,1.0)];
        let inst=Instrument::fx("EURUSD");
        let mut cfg=BacktestConfig::default();
        let adverse=backtest(&s,&bars,&inst,&cfg);
        assert_eq!(adverse.trades[0].exit_reason,ExitReason::StopLoss);
        assert!((adverse.trades[0].pnl+100.0).abs()<1e-8);
        assert_eq!(adverse.ambiguous_bars.len(),1);
        let ambiguity=&adverse.ambiguous_bars[0];
        assert_ne!(ambiguity.low_first_exit,ambiguity.high_first_exit);
        cfg.intrabar_policy=IntrabarPolicy::FavorableFirst;
        let favorable=backtest(&s,&bars,&inst,&cfg);
        assert_eq!(favorable.trades[0].exit_reason,ExitReason::ProfitTarget);
        assert!((favorable.trades[0].pnl-100.0).abs()<1e-8);
        assert_eq!(favorable.trades,backtest(&s,&bars,&inst,&cfg).trades);
    }
}

#[test]
fn later_candles_cannot_change_an_already_executed_exit() {
    let mut s=always_long_strategy();
    s.stop_loss=Some(sqx_strategy::StopLoss::Points(100.0));
    let mut bars=vec![Bar::new(0,1.0,1.0,1.0,1.0,1.0),Bar::new(60000,1.0,1.0,0.98,0.99,1.0),Bar::new(120000,1.0,1.0,1.0,1.0,1.0)];
    let cfg=BacktestConfig::default(); let inst=Instrument::fx("EURUSD");
    let a=backtest(&s,&bars,&inst,&cfg);
    bars[2]=Bar::new(120000,2.0,3.0,0.5,2.0,999.0);
    let b=backtest(&s,&bars,&inst,&cfg);
    assert_eq!(a.trades[0],b.trades[0]);
    assert_eq!(&a.equity[..2],&b.equity[..2]);
}

#[test]
fn full_path_scenarios_keep_inputs_fixed_and_recompute_executions() {
    let mut s=always_long_strategy();
    s.stop_loss=Some(sqx_strategy::StopLoss::Points(100.0));
    s.profit_target=Some(sqx_strategy::ProfitTarget::Points(100.0));
    let bars=[Bar::new(0,1.0,1.0,1.0,1.0,1.0),Bar::new(60000,1.0,1.02,0.98,1.0,1.0)];
    let cfg=BacktestConfig::default();
    let scenarios=sqx_backtest::backtest_path_scenarios(&s,&bars,&Instrument::fx("EURUSD"),&cfg);
    assert_eq!(scenarios.len(),4);
    for (_,run) in &scenarios {assert_eq!(run.initial_deposit,cfg.initial_deposit);assert!(run.errors.is_empty());}
    assert!((scenarios[0].1.final_equity-9900.0).abs()<1e-8);
    assert!((scenarios[1].1.final_equity-10100.0).abs()<1e-8);
    assert_eq!(scenarios[1].1.trades,scenarios[2].1.trades);
    assert_eq!(scenarios[0].1.trades,scenarios[3].1.trades);
}

#[test]
fn trading_window_warms_from_history_without_pre_window_positions() {
    use sqx_strategy::Param;
    let bars:Vec<_>=(0..40).map(|i|{let p=1.0+i as f64*0.001;Bar::new(i*60000,p,p+0.0005,p-0.0005,p,1.0)}).collect();
    let mut s=always_long_strategy();
    s.signals[0].conditions[0].left=Operand::Indicator("SMA".into(),vec![Param::new("period",30.0)]);
    let cfg=BacktestConfig::default();let inst=Instrument::fx("EURUSD");
    let warm=sqx_backtest::backtest_window(&s,&bars,35,&inst,&cfg);
    assert!(warm.errors.is_empty());assert_eq!(warm.trades.len(),1);
    assert_eq!(warm.trades[0].open_time,bars[35].time);
    assert_eq!(warm.equity.len(),5);
    assert!(warm.equity.iter().all(|(t,_)|*t>=bars[35].time));
    let cold=backtest(&s,&bars[35..],&inst,&cfg);
    assert!(cold.trades.is_empty(),"short cold segment has no SMA warmup");
}

#[test]
fn scheduled_exit_uses_declared_deadline_not_next_candle() {
    let mut strategy = always_long_strategy();
    strategy.trading_options.exit_at_end_of_day = true;
    let inst = Instrument::fx("EURUSD");
    let cfg = BacktestConfig::default();
    let bars: Vec<_> = [0, 60_000, 120_000, 180_000, 86_400_000].into_iter()
        .enumerate().map(|(i,t)| Bar::new(t, 1.0+i as f64*0.001, 1.01, 0.99, 1.005, 1.0)).collect();
    assert!(!backtest(&strategy, &bars, &inst, &cfg).errors.is_empty());
    strategy.trading_options.session_close_utc_minute = Some(3);
    let full = backtest(&strategy, &bars, &inst, &cfg);
    assert!(full.errors.is_empty(), "{:?}", full.errors);
    assert_eq!(full.trades[0].close_time, 180_000);
    assert_eq!(full.trades[0].close_price, bars[3].open);
    assert_eq!(full.trades[0].exit_reason, sqx_backtest::ExitReason::ExitAtEndOfDay);
    // Moving a future candle to another day cannot retroactively close at bar 2.
    let mut changed = bars.clone(); changed[4].time = 172_800_000;
    let other = backtest(&strategy, &changed, &inst, &cfg);
    assert_eq!(full.trades[0].close_time, other.trades[0].close_time);
    assert_eq!(full.trades[0].pnl, other.trades[0].pnl);
    // Missing the deadline quote delays execution to the next observed open.
    let sparse = vec![bars[0],bars[1],bars[4]];
    let delayed = backtest(&strategy, &sparse, &inst, &cfg);
    assert_eq!(delayed.trades[0].close_time, 86_400_000);
    assert_eq!(delayed.trades[0].close_price, bars[4].open);
}

#[test]
fn friday_cutoff_is_weekly_and_blocks_late_friday_entries() {
    let mut strategy = always_long_strategy();
    strategy.trading_options.exit_on_friday = true;
    strategy.trading_options.session_close_utc_minute = Some(3);
    // Unix day 0 is Thursday. Thursday's cutoff must not trigger Friday-only exits.
    let bars: Vec<_> = [0, 60_000, 180_000, 86_580_000, 86_640_000].into_iter()
        .map(|t| Bar::new(t,1.0,1.01,0.99,1.0,1.0)).collect();
    let result = backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default());
    assert!(result.errors.is_empty(),"{:?}",result.errors);
    assert_eq!(result.trades.len(),1);
    assert_eq!(result.trades[0].close_time,86_580_000);
    assert_eq!(result.trades[0].exit_reason,sqx_backtest::ExitReason::ExitAtEndOfDay);
    strategy.trading_options.session_close_utc_minute = Some(1440);
    assert!(!backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default()).errors.is_empty());
}

#[test]
fn malformed_sizing_and_stops_fail_before_trading(){
    let base=always_long_strategy();let mut invalid=Vec::new();
    for size in [0.0,-1.0,f64::NAN,f64::INFINITY] {let mut s=base.clone();s.money_management=MoneyManagement::FixedSize{size};invalid.push(s);}
    for stop in [sqx_strategy::StopLoss::Points(-10.0),sqx_strategy::StopLoss::Atr{period:0,mult:2.0}] {let mut s=base.clone();s.stop_loss=Some(stop);invalid.push(s);}
    let mut s=base.clone();s.money_management=MoneyManagement::RiskFixedPctOfAccount{risk:2.0,stop_loss_points:0.0};invalid.push(s);
    let mut s=base.clone();s.profit_target=Some(sqx_strategy::ProfitTarget::Points(f64::NAN));invalid.push(s);
    let mut s=base.clone();s.exits.push(sqx_strategy::ExitType::ExitAfterBars(0));invalid.push(s);
    for strategy in invalid {
        let result=backtest(&strategy,&rising_bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(!result.errors.is_empty(),"{strategy:?}");assert!(result.trades.is_empty());assert!(result.final_equity.is_nan());
    }
    assert!(backtest(&base,&rising_bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default()).errors.is_empty());
}

#[test]
fn ambiguous_or_invalid_signal_settings_fail_explicitly(){
    let base=always_long_strategy();let mut invalid=Vec::new();
    for threshold in [0.0,-0.1,1.1,f64::NAN] {let mut s=base.clone();s.signals[0].fuzzy=Some(threshold);invalid.push(s);}
    let mut s=base.clone();s.signals.push(s.signals[0].clone());invalid.push(s);
    let mut s=base.clone();s.signals[0].conditions[0].left=Operand::Price(PriceField::Close,-1);invalid.push(s);
    let mut s=base.clone();s.signals[0].conditions[0].right=Operand::Const(f64::INFINITY);invalid.push(s);
    let mut s=base.clone();s.signals[0].conditions[0].left=Operand::Indicator("SMA".into(),vec![sqx_strategy::Param::new("period",2.0),sqx_strategy::Param::new("period",10.0)]);invalid.push(s);
    for strategy in invalid {
        let run=backtest(&strategy,&rising_bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(!run.errors.is_empty(),"{strategy:?}");assert!(run.trades.is_empty());
    }
    let mut valid=base;valid.signals[0].fuzzy=Some(1.0);
    assert!(backtest(&valid,&rising_bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default()).errors.is_empty());
}

#[test]
fn unknown_indicator_names_and_parameters_are_errors(){
    for operand in [Operand::Indicator("SMMA_typo".into(),vec![]),Operand::Indicator("SMA".into(),vec![sqx_strategy::Param::new("peroid",10.0)])] {
        let mut strategy=always_long_strategy();strategy.signals[0].conditions[0].left=operand;
        let run=backtest(&strategy,&rising_bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(run.errors[0].contains("Unknown"));assert!(run.trades.is_empty());
    }
    for name in ["KAMA","TEMA","SMA"] {
        let mut strategy=always_long_strategy();strategy.signals[0].conditions[0].left=Operand::Indicator(name.into(),vec![]);
        assert!(strategy.validate_execution_settings().is_ok());
    }
}

#[test]
fn indicator_periods_are_not_silently_rounded_or_defaulted(){
    for value in [0.0,-1.0,2.5,usize::MAX as f64] {
        let mut s=always_long_strategy();s.signals[0].conditions[0].left=Operand::Indicator("SMA".into(),vec![sqx_strategy::Param::new("period",value)]);
        let run=backtest(&s,&rising_bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(!run.errors.is_empty(),"period {value}");assert!(run.trades.is_empty());
    }
}

#[test]
fn tick_confirmation_is_not_silently_treated_as_true(){
    let mut strategy=always_long_strategy();
    let mut signal=strategy.signals[0].clone();signal.conditions[0].right=Operand::Const(1e9);
    strategy.confirmation=Some(sqx_strategy::Confirmation{timeframe:sqx_core::Timeframe::Tick,signal});
    let run=backtest(&strategy,&rising_bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default());
    assert!(run.errors[0].contains("Tick confirmation"));assert!(run.trades.is_empty());
}

#[test]
fn confirmation_waits_for_completed_bucket_and_ignores_future_buckets(){
    let mut strategy=always_long_strategy();
    strategy.confirmation=Some(sqx_strategy::Confirmation{timeframe:sqx_core::Timeframe::M5,signal:strategy.signals[0].clone()});
    let bars:Vec<_>=(0..15).map(|i|Bar::new(i*60_000,1.1,1.11,1.09,1.1,1.0)).collect();
    let cfg=BacktestConfig::default();let inst=Instrument::fx("EURUSD");
    let run=backtest(&strategy,&bars,&inst,&cfg);
    assert!(run.errors.is_empty());assert!(!run.trades.is_empty());
    assert!(run.trades[0].open_time>=5*60_000);
    let unavailable=run.execution.entry_attempts[0].confirmation.as_ref().unwrap();assert_eq!(unavailable.source_bar_open_time,None);assert_eq!(unavailable.available_from,None);assert!(unavailable.signal.is_none());
    let filled=run.execution.entry_attempts.iter().find(|a|a.trade_id.is_some()).unwrap();let evidence=filled.confirmation.as_ref().unwrap();assert_eq!(evidence.source_bar_open_time,Some(0));assert_eq!(evidence.available_from,Some(300000));assert!(evidence.available_from.unwrap()<=filled.attempt_time);let signal=evidence.signal.as_ref().unwrap();assert!(signal.matched);assert_eq!(signal.conditions[0].left,Some(1.1));

    let mut changed=bars.clone();for bar in &mut changed[10..]{bar.open=2.0;bar.high=2.1;bar.low=1.9;bar.close=2.0;}
    let altered=backtest(&strategy,&changed,&inst,&cfg);
    assert_eq!(run.trades[0].open_time,altered.trades[0].open_time);
    assert_eq!(run.trades[0].open_price,altered.trades[0].open_price);
    assert_eq!(&run.equity[..10],&altered.equity[..10]);
    assert_eq!(run.execution.entry_attempts,altered.execution.entry_attempts);
}

#[test]
fn monthly_confirmation_becomes_available_at_calendar_month_end(){
    let mut strategy=always_long_strategy();
    strategy.confirmation=Some(sqx_strategy::Confirmation{timeframe:sqx_core::Timeframe::MN1,signal:strategy.signals[0].clone()});
    let february_first=1_706_745_600_000i64;
    let bars:Vec<_>=(0..32).map(|day|Bar::new(february_first+day*86_400_000,1.1,1.11,1.09,1.1,1.0)).collect();
    let run=backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default());
    assert!(run.errors.is_empty());assert!(!run.trades.is_empty());
    assert_eq!(run.trades[0].open_time,1_709_251_200_000,"February 2024 closes on March 1, after 29 days");
    let attempt=run.execution.entry_attempts.iter().find(|a|a.trade_id.is_some()).unwrap();let evidence=attempt.confirmation.as_ref().unwrap();assert_eq!(evidence.source_bar_open_time,Some(february_first));assert_eq!(evidence.available_from,Some(1_709_251_200_000));
}

#[test]
fn truncated_leading_confirmation_candle_cannot_authorize_entry(){
    let mut strategy=always_long_strategy();
    strategy.confirmation=Some(sqx_strategy::Confirmation{timeframe:sqx_core::Timeframe::M5,signal:strategy.signals[0].clone()});
    let bars:Vec<_>=(2..16).map(|i|Bar::new(i*60_000,1.1,1.11,1.09,1.1,1.0)).collect();
    let result=backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default());
    assert!(result.errors.is_empty());assert!(!result.trades.is_empty());
    assert_eq!(result.execution.skipped_leading_confirmation_bucket,Some(0));
    assert_eq!(result.trades[0].open_time,10*60_000,"partial 00:00-00:05 bucket is unavailable; first complete bucket closes 00:10");
    let attempt=result.execution.entry_attempts.iter().find(|a|a.trade_id.is_some()).unwrap();let evidence=attempt.confirmation.as_ref().unwrap();assert_eq!(evidence.source_bar_open_time,Some(300000));assert_eq!(evidence.available_from,Some(600000));
}

#[test]
fn stop_based_sizing_requires_a_real_stop_instead_of_a_hidden_distance(){
    for sizing in [MoneyManagement::FixedAmount{amount:100.0},MoneyManagement::CryptoFixedAmount{amount:100.0},MoneyManagement::RiskFixedBalancePct{risk:2.0},MoneyManagement::CryptoFixedBalancePct{risk:2.0}] {
        let mut strategy=always_long_strategy();strategy.money_management=sizing;
        let missing=backtest(&strategy,&rising_bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(missing.errors[0].contains("explicit stop-loss"));assert!(missing.trades.is_empty());
        strategy.stop_loss=Some(sqx_strategy::StopLoss::Points(100.0));
        assert!(backtest(&strategy,&rising_bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default()).errors.is_empty());
    }
    // Fixed lots and independently specified sizing distances do not infer a stop.
    let mut strategy=always_long_strategy();
    assert!(strategy.validate_execution_settings().is_ok());
    strategy.money_management=MoneyManagement::RiskFixedPctOfAccount{risk:2.0,stop_loss_points:100.0};
    assert!(strategy.validate_execution_settings().is_ok());
}

#[test]
fn dynamic_exits_cannot_silently_use_an_infinite_distance(){
    use sqx_strategy::{ExitType,StopLoss};
    for exit in [ExitType::TrailingStop,ExitType::MoveToBreakEven] {
        let mut strategy=always_long_strategy();strategy.exits=vec![exit];
        let run=backtest(&strategy,&rising_bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(run.errors[0].contains("initial stop-loss"));assert!(run.trades.is_empty());
        strategy.stop_loss=Some(StopLoss::Points(100.0));
        assert!(strategy.validate_execution_settings().is_ok());
    }
    let mut strategy=always_long_strategy();strategy.exits=vec![ExitType::ExitAfterBars(2),ExitType::ExitAfterBars(9)];
    assert!(strategy.validate_execution_settings().unwrap_err().contains("Duplicate exit"));
}

#[test]
fn overnight_entry_hours_use_execution_time_and_exclusive_end(){
    let mut strategy=always_long_strategy();strategy.trading_options.limit_hours=Some((22,2));
    for (hour,allowed) in [(21,false),(22,true),(23,true),(0,true),(1,true),(2,false)] {
        let time=86_400_000+hour*3_600_000;
        let bars=[Bar::new(time-60_000,1.1,1.1,1.1,1.1,1.0),Bar::new(time,1.1,1.1,1.1,1.1,1.0)];
        let run=backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(run.errors.is_empty());assert_eq!(!run.trades.is_empty(),allowed,"execution hour {hour}");
    }
    for hours in [(24,2),(22,25),(0,0),(12,12)] {
        strategy.trading_options.limit_hours=Some(hours);
        assert!(strategy.validate_execution_settings().is_err());
    }
    strategy.trading_options.limit_hours=Some((0,24));assert!(strategy.validate_execution_settings().is_ok());
}

#[test]
fn slippage_that_makes_execution_prices_invalid_returns_errors_not_panics(){
    let mut long=always_long_strategy();long.money_management=MoneyManagement::FixedSize{size:0.01};
    let cfg=BacktestConfig{slippage_points:20000.0,bar_duration_ms:Some(60000),..Default::default()};
    let bars=[Bar::new(0,10.0,10.0,10.0,10.0,1.0),Bar::new(60000,10.0,10.0,10.0,10.0,1.0)];
    let ticks=[sqx_core::Tick{time:60000,bid:1.1,ask:1.1}];
    let run=sqx_backtest::backtest_ticks(&long,&bars,&ticks,&Instrument::fx("EURUSD"),&cfg);
    assert!(!run.errors.is_empty());assert!(run.final_equity.is_nan());
    let mut short=long;short.long=false;short.short=true;short.signals[0].kind=SignalKind::ShortEntry;
    let run=sqx_backtest::backtest_ticks(&short,&bars,&ticks,&Instrument::fx("EURUSD"),&cfg);
    assert!(!run.errors.is_empty());assert!(run.final_equity.is_nan());
}

#[test]
fn overflowing_tick_fill_is_reported_without_panicking(){
    let cfg=BacktestConfig{slippage_points:1e308,bar_duration_ms:Some(60000),..Default::default()};
    let bars=[Bar::new(0,1e305,1e305,1e305,1e305,1.0),Bar::new(60000,1e305,1e305,1e305,1e305,1.0)];
    let ticks=[sqx_core::Tick{time:60000,bid:1e305,ask:f64::MAX}];
    let run=sqx_backtest::backtest_ticks(&always_long_strategy(),&bars,&ticks,&Instrument::fx("EURUSD"),&cfg);
    assert!(run.errors[0].contains("entry valuation"));assert!(run.final_equity.is_nan());
}

#[test]
fn exit_after_bars_counts_the_entry_candle_as_the_first_held_bar(){
    use sqx_backtest::ExitReason;
    use sqx_strategy::ExitType;
    let bars:Vec<_>=(0..8).map(|i|{let p=1.0+i as f64*0.001;Bar::new(i*60000,p,p+0.0005,p-0.0005,p+0.0002,1.0)}).collect();
    for held in [1usize,2,3] {
        let mut strategy=always_long_strategy();strategy.exits=vec![ExitType::ExitAfterBars(held)];
        let run=backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(run.errors.is_empty(),"{:?}",run.errors);
        let trade=&run.trades[0];assert_eq!(trade.open_time,60000);
        assert_eq!(trade.exit_reason,ExitReason::ExitAfterBars);
        assert_eq!(trade.close_time,(1+held) as i64*60000,"{held} fully held candles must exit at the following open");
        assert_eq!(trade.close_price,bars[1+held].open);
        assert_eq!(trade.bars,held);
    }
}

#[test]
fn last_candle_time_exit_is_not_filled_using_its_close_signal(){
    let mut strategy=always_long_strategy();strategy.exits=vec![sqx_strategy::ExitType::ExitAfterBars(1)];
    let bars=[Bar::new(0,1.0,1.001,0.999,1.0,1.0),Bar::new(60000,1.0,1.003,0.999,1.002,1.0)];
    let run=backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default());
    assert!(run.errors.is_empty());assert_eq!(run.trades.len(),1);
    // The close-derived time exit has no following bar to execute on. Only the
    // explicit end-of-data reporting liquidation is available, not a live fill.
    assert_eq!(run.trades[0].exit_reason,sqx_backtest::ExitReason::EndOfData);
    assert_eq!(run.trades[0].close_price,bars[1].close);
}

#[test]
fn scheduled_exit_evidence_retains_simultaneous_reasons_and_unfilled_requests(){
    use sqx_backtest::ExitReason;
    let mut strategy=always_long_strategy();strategy.exits=vec![sqx_strategy::ExitType::ExitAfterBars(1)];
    strategy.signals.push(Signal::new(SignalKind::LongExit,vec![Condition::new(Operand::Price(PriceField::Close,0),Comparator::Gt,Operand::Const(0.0))],Join::And));
    let bars:Vec<_>=(0..3).map(|i|Bar::new(i*60000,1.0,1.001,0.999,1.0,1.0)).collect();
    let cfg=BacktestConfig::default();let instrument=Instrument::fx("EURUSD");
    let complete=backtest(&strategy,&bars,&instrument,&cfg);assert!(complete.errors.is_empty());
    let decision=&complete.execution.scheduled_exit_decisions[0];assert_eq!(decision.trade_id,complete.trades[0].id);assert_eq!(decision.decision_bar_index,1);assert_eq!(decision.decision_bar_open_time,60000);
    assert_eq!(decision.reasons,vec![ExitReason::ExitAfterBars,ExitReason::ExitSignal]);assert_eq!(decision.chosen_reason,ExitReason::ExitSignal);assert_eq!(decision.executed_time,Some(120000));assert_eq!(complete.trades[0].close_time,decision.executed_time.unwrap());
    let terminal=backtest(&strategy,&bars[..2],&instrument,&cfg);assert!(terminal.errors.is_empty());let unfilled=&terminal.execution.scheduled_exit_decisions[0];assert_eq!(unfilled.executed_time,None);assert_eq!(unfilled.reasons,decision.reasons);assert_eq!(terminal.trades[0].exit_reason,ExitReason::EndOfData);
}

#[test]
fn entry_attempt_evidence_links_fills_and_explains_silent_gates(){
    let bars:Vec<_>=(0..5).map(|i|Bar::new(i*60000,1.0,1.001,0.999,1.0,1.0)).collect();
    let mut strategy=always_long_strategy();strategy.exits=vec![sqx_strategy::ExitType::ExitAfterBars(1)];strategy.trading_options.max_trades_per_day=Some(1);
    let instrument=Instrument::fx("EURUSD");let cfg=BacktestConfig::default();
    let run=backtest(&strategy,&bars,&instrument,&cfg);assert!(run.errors.is_empty());assert_eq!(run.trades.len(),1);
    let entry=&run.execution.entry_attempts[0];assert_eq!(entry.signal_bar_index,0);assert_eq!(entry.attempt_time,60000);assert_eq!(entry.trade_id,Some(run.trades[0].id));assert_eq!(entry.fill_price,Some(run.trades[0].open_price));assert_eq!(entry.filled_lots,Some(run.trades[0].size));assert!(entry.blocked_by.is_empty());
    let capped=&run.execution.entry_attempts[1];assert_eq!(capped.attempt_time,180000);assert!(capped.blocked_by.iter().any(|r|r=="daily_entry_limit"));assert_eq!(capped.requested_lots,None);assert_eq!(capped.trade_id,None);
    strategy.trading_options.limit_hours=Some((1,2));
    let closed=backtest(&strategy,&bars,&instrument,&cfg);assert!(closed.errors.is_empty());assert!(closed.trades.is_empty());assert!(closed.execution.entry_attempts.iter().all(|a|a.blocked_by.iter().any(|r|r=="outside_trading_hours")&&a.fill_price.is_none()));
}

#[test]
fn overflowed_signal_operand_invalidates_research_instead_of_triggering_entries(){
    let bars:Vec<_>=(0..3).map(|i|Bar::new(i*60000,1e308,1e308,1e308,1e308,1.0)).collect();
    let mut strategy=always_long_strategy();strategy.signals[0].conditions[0].left=Operand::Price(PriceField::Typical,0);
    let compiled=sqx_backtest::compile_signal(&strategy.signals[0],&bars);
    assert!(compiled.iter().all(|v|!*v),"overflow must not become a true comparison");
    let run=backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default());
    assert!(run.errors.iter().any(|e|e.contains("nonfinite signal operand")),"{:?}",run.errors);assert!(run.trades.is_empty());
}

#[test]
fn declared_candle_duration_rejects_overlap_and_end_overflow_in_bar_mode(){
    let bar=|time|Bar::new(time,1.0,1.01,0.99,1.0,1.0);
    let instrument=Instrument::fx("EURUSD");let mut cfg=BacktestConfig{bar_duration_ms:Some(120000),..Default::default()};
    let overlapping=backtest(&always_long_strategy(),&[bar(0),bar(60000)],&instrument,&cfg);assert!(overlapping.errors.iter().any(|e|e.contains("overlaps")));assert!(overlapping.trades.is_empty());
    cfg.bar_duration_ms=Some(60000);assert!(backtest(&always_long_strategy(),&[bar(0),bar(60000)],&instrument,&cfg).errors.is_empty());
    let overflow=backtest(&always_long_strategy(),&[bar(i64::MAX-1)],&instrument,&cfg);assert!(overflow.errors.iter().any(|e|e.contains("overflows")));
    cfg.bar_duration_ms=Some(0);assert!(backtest(&always_long_strategy(),&[bar(0)],&instrument,&cfg).errors.iter().any(|e|e.contains("positive")));
}

#[test]
fn confirmation_cannot_split_a_declared_source_candle_across_buckets(){
    let mut strategy=always_long_strategy();strategy.confirmation=Some(sqx_strategy::Confirmation{timeframe:sqx_core::Timeframe::M1,signal:strategy.signals[0].clone()});
    let bars:Vec<_>=(0..4).map(|i|Bar::new(i*120000,1.0,1.01,0.99,1.0,1.0)).collect();
    let config=BacktestConfig{bar_duration_ms:Some(120000),..Default::default()};
    let run=backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&config);
    assert!(run.errors.iter().any(|e|e.contains("crosses confirmation bucket")),"{:?}",run.errors);assert!(run.trades.is_empty());
}

#[test]
fn confirmation_requires_declared_source_intervals_to_fit_even_when_shorter(){
    let mut strategy=always_long_strategy();strategy.confirmation=Some(sqx_strategy::Confirmation{timeframe:sqx_core::Timeframe::M5,signal:strategy.signals[0].clone()});
    let bars:Vec<_>=(0..12).map(|i|Bar::new(i*60000,1.0,1.01,0.99,1.0,1.0)).collect();
    let config=BacktestConfig{bar_duration_ms:Some(60000),..Default::default()};
    let aligned=backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&config);
    assert!(aligned.errors.is_empty(),"{:?}",aligned.errors);
    let shifted:Vec<_>=bars.iter().map(|b|Bar::new(b.time+30000,b.open,b.high,b.low,b.close,b.volume)).collect();
    let run=backtest(&strategy,&shifted,&Instrument::fx("EURUSD"),&config);
    assert!(run.errors.iter().any(|e|e.contains("crosses confirmation bucket")),"{:?}",run.errors);assert!(run.trades.is_empty());
}

#[test]
fn declared_confirmation_rejects_missing_source_intervals(){
    let mut strategy=always_long_strategy();
    strategy.confirmation=Some(sqx_strategy::Confirmation{timeframe:sqx_core::Timeframe::M5,signal:strategy.signals[0].clone()});
    let config=BacktestConfig{bar_duration_ms:Some(60000),..Default::default()};
    let instrument=Instrument::fx("EURUSD");
    // Missing beginning, middle or end of the second bucket all alter its OHLC.
    for missing in [5,7,9] {
        let bars:Vec<_>=(0..12).filter(|i|*i!=missing).map(|i|Bar::new(i*60000,1.1,1.11,1.09,1.1,1.0)).collect();
        let run=backtest(&strategy,&bars,&instrument,&config);
        assert!(run.errors.iter().any(|e|e.contains("incomplete confirmation bucket")),"missing {missing}: {:?}",run.errors);
        assert!(run.trades.is_empty());
    }
    // An unfinished final bucket is never available and needs no invented bars.
    let bars:Vec<_>=(2..12).map(|i|Bar::new(i*60000,1.1,1.11,1.09,1.1,1.0)).collect();
    let run=backtest(&strategy,&bars,&instrument,&config);
    assert!(run.errors.is_empty(),"{:?}",run.errors);
    assert_eq!(run.execution.skipped_leading_confirmation_bucket,Some(0));
    assert!(!run.trades.is_empty());
}

#[test]
fn declared_daily_sources_cover_actual_leap_month_confirmation(){
    let mut strategy=always_long_strategy();
    strategy.confirmation=Some(sqx_strategy::Confirmation{timeframe:sqx_core::Timeframe::MN1,signal:strategy.signals[0].clone()});
    let start=1_706_745_600_000i64;
    let bars:Vec<_>=(0..32).map(|day|Bar::new(start+day*86400000,1.1,1.11,1.09,1.1,1.0)).collect();
    let config=BacktestConfig{bar_duration_ms:Some(86400000),..Default::default()};
    let instrument=Instrument::fx("EURUSD");
    let run=backtest(&strategy,&bars,&instrument,&config);
    assert!(run.errors.is_empty(),"{:?}",run.errors);
    assert_eq!(run.trades[0].open_time,1_709_251_200_000);
    let mut missing=bars.clone();missing.remove(28); // Leap day is required, not a nominal 28-day month.
    let run=backtest(&strategy,&missing,&instrument,&config);
    assert!(run.errors.iter().any(|e|e.contains("incomplete confirmation bucket")),"{:?}",run.errors);
    assert!(run.trades.is_empty());
}

#[test]
fn chosen_ohlc_stop_amendments_record_path_order_without_alternative_events(){
    for exit in [sqx_strategy::ExitType::TrailingStop,sqx_strategy::ExitType::MoveToBreakEven] {
        let mut s=always_long_strategy();s.stop_loss=Some(sqx_strategy::StopLoss::Points(100.0));s.exits.push(exit);
        let bars=[Bar::new(0,1.0,1.0,1.0,1.0,1.0),Bar::new(60000,1.0,1.02,0.98,1.01,1.0)];
        let cfg=BacktestConfig{intrabar_policy:sqx_backtest::IntrabarPolicy::HighFirst,..Default::default()};
        let r=backtest(&s,&bars,&Instrument::fx("EURUSD"),&cfg);assert!(r.errors.is_empty());
        assert_eq!(r.execution.stop_changes.len(),1);
        let change=&r.execution.stop_changes[0];assert_eq!(change.trade_id,0);assert_eq!(change.time,60000);
        assert_eq!(change.source,"ohlc_high_first");assert_eq!(change.path_step,Some(1));assert_eq!(change.observed_price,1.02);
        assert!((change.previous_stop.unwrap()-0.99).abs()<1e-12);
        assert_eq!(change.new_reason,r.trades[0].exit_reason);
        assert!((change.new_stop.unwrap()-r.trades[0].close_price).abs()<1e-12);
    }
}

#[test]
fn terminal_entry_signals_are_recorded_without_inventing_attempts_or_trades(){
    for short in [false,true] {
        let mut s=always_long_strategy();s.long=!short;s.short=short;
        s.signals=vec![Signal::new(if short{SignalKind::ShortEntry}else{SignalKind::LongEntry},vec![Condition::new(Operand::Price(PriceField::Close,0),Comparator::Gt,Operand::Const(1.05))],Join::And)];
        for bars in [vec![Bar::new(0,1.1,1.1,1.1,1.1,1.0)],vec![Bar::new(0,1.0,1.0,1.0,1.0,1.0),Bar::new(60000,1.1,1.1,1.1,1.1,1.0)]] {
            let r=backtest(&s,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default());
            assert!(r.errors.is_empty());assert!(r.trades.is_empty());assert!(r.execution.entry_attempts.is_empty());
            assert_eq!(r.final_equity,10000.0);let queued=r.execution.unexecuted_entry.unwrap();
            assert_eq!(queued.signal_bar_index,bars.len()-1);assert_eq!(queued.signal_bar_open_time,bars.last().unwrap().time);
            assert_eq!(queued.direction,if short{sqx_core::Direction::Short}else{sqx_core::Direction::Long});
            assert_eq!(queued.reason,"no_following_opening_quote");assert!(queued.signal.unwrap().matched);
        }
    }
    let r=backtest(&Strategy::new("no signal"),&[Bar::new(0,1.0,1.0,1.0,1.0,1.0)],&Instrument::fx("EURUSD"),&BacktestConfig::default());
    assert!(r.execution.unexecuted_entry.is_none());
}
