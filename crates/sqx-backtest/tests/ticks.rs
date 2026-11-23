use sqx_backtest::{backtest_ticks, BacktestConfig, ExitReason};
use sqx_core::{Bar, Instrument, Tick};
use sqx_strategy::{Comparator, Condition, Join, MoneyManagement, Operand, PriceField, Signal, SignalKind, StopLoss, Strategy};
fn strategy()->Strategy {
    let mut s=Strategy::new("tick execution");s.long=true;s.short=false;
    s.money_management=MoneyManagement::FixedSize{size:0.1};
    s.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Price(PriceField::Close,0),Comparator::Gt,Operand::Const(0.0))],Join::And));s
}
fn bars()->Vec<Bar>{(0..3).map(|i|Bar::new(i*60000,1.0,1.02,0.98,1.0,1.0)).collect()}
fn config()->BacktestConfig{BacktestConfig{bar_duration_ms:Some(60000),spread_points:50.0,..Default::default()}}
#[test]
fn ticks_supply_entry_exit_prices_times_and_last_candle() {
    let ticks=[Tick{time:60007,bid:1.0,ask:1.001},Tick{time:179999,bid:1.002,ask:1.003}];
    let r=backtest_ticks(&strategy(),&bars(),&ticks,&Instrument::fx("EURUSD"),&config());
    assert!(r.errors.is_empty(),"{:?}",r.errors);
    assert_eq!(r.execution.mode,"supplied_ticks");
    assert_eq!(r.trades[0].open_time,60007);
    assert_eq!(r.trades[0].open_price,1.001,"do not add synthetic spread to tick ask");
    assert_eq!(r.trades[0].close_time,179999);
    assert_eq!(r.execution.cash_settlements.len(),1);
    assert_eq!(r.execution.cash_settlements[0].time,179999);
    assert_eq!(r.execution.cash_settlements[0].net_change,r.trades[0].pnl);
    assert_eq!(r.execution.cash_settlements[0].balance_after,r.final_equity);
    assert_eq!(r.trades[0].close_price,1.002);
    let open=r.execution.account_snapshots.iter().find(|row|row.quote_time==Some(179999)&&row.trade_id==Some(0)).unwrap();
    assert_eq!(open.liquidation_price,Some(1.002));
    assert_eq!(r.execution.account_snapshots.last().unwrap().quote_time,Some(179999));
    assert_eq!(r.execution.account_snapshots.last().unwrap().equity,r.final_equity);
    assert!((r.trades[0].pnl-10.0).abs()<1e-8);
}
#[test]
fn tick_stop_records_the_quote_timestamp() {
    let mut s=strategy();s.stop_loss=Some(StopLoss::Points(100.0));
    let ticks=[Tick{time:60007,bid:1.0,ask:1.001},Tick{time:125432,bid:0.98,ask:0.981}];
    let r=backtest_ticks(&s,&bars(),&ticks,&Instrument::fx("EURUSD"),&config());
    assert!(r.errors.is_empty());
    assert_eq!(r.trades[0].exit_reason,ExitReason::StopLoss);
    assert_eq!(r.trades[0].close_time,125432);
    assert_eq!(r.trades[0].close_price,0.98);
}
#[test]
fn missing_ticks_require_explicit_hybrid_opt_in() {
    let ticks=[Tick{time:60007,bid:1.0,ask:1.001}];
    let mut cfg=config();
    assert!(!backtest_ticks(&strategy(),&bars(),&ticks,&Instrument::fx("EURUSD"),&cfg).errors.is_empty());
    cfg.allow_tick_fallback=true;
    let r=backtest_ticks(&strategy(),&bars(),&ticks,&Instrument::fx("EURUSD"),&cfg);
    assert!(r.errors.is_empty());
    assert_eq!(r.execution.mode,"hybrid_ticks_ohlc");
    assert!(r.execution.missing_tick_intervals.contains(&120000));
    cfg.bar_duration_ms=None;
    assert!(!backtest_ticks(&strategy(),&bars(),&ticks,&Instrument::fx("EURUSD"),&cfg).errors.is_empty());
}
#[test]
fn ticks_outside_candle_intervals_are_not_silently_discarded() {
    let ticks=[Tick{time:60007,bid:1.0,ask:1.001},Tick{time:180000,bid:1.0,ask:1.001}];
    let r=backtest_ticks(&strategy(),&bars(),&ticks,&Instrument::fx("EURUSD"),&config());
    assert!(r.errors.iter().any(|e|e.contains("outside")));
}

#[test]
fn scheduled_tick_exit_uses_first_quote_after_cutoff_inside_candle() {
    let mut s = strategy();
    s.trading_options.exit_at_end_of_day = true;
    s.trading_options.session_close_utc_minute = Some(3);
    let bars = [Bar::new(0,1.0,1.02,0.98,1.0,1.0), Bar::new(120000,1.0,1.02,0.98,1.0,1.0)];
    let ticks = [Tick{time:120007,bid:1.0,ask:1.001}, Tick{time:179999,bid:1.002,ask:1.003}, Tick{time:181234,bid:1.004,ask:1.005}, Tick{time:239999,bid:1.009,ask:1.010}];
    let cfg = BacktestConfig{bar_duration_ms:Some(120000),..config()};
    let r = backtest_ticks(&s,&bars,&ticks,&Instrument::fx("EURUSD"),&cfg);
    assert!(r.errors.is_empty(),"{:?}",r.errors);
    assert_eq!(r.trades[0].close_time,181234);
    assert_eq!(r.trades[0].close_price,1.004);
    assert_eq!(r.trades[0].exit_reason,ExitReason::ExitAtEndOfDay);
}

#[test]
fn daily_entry_limit_uses_actual_tick_execution_day(){
    let midnight=86_400_000;
    let candles:Vec<_>=[midnight-90000,midnight-30000,midnight+30000].into_iter().map(|time|Bar::new(time,1.1,1.11,1.08,1.1,1.0)).collect();
    let ticks=[Tick{time:midnight+10000,bid:1.1,ask:1.1},Tick{time:midnight+15000,bid:1.09,ask:1.09},Tick{time:midnight+40000,bid:1.1,ask:1.1}];
    let mut s=strategy();s.stop_loss=Some(StopLoss::Points(10.0));s.trading_options.max_trades_per_day=Some(1);
    let mut cfg=config();cfg.account.long_financing_per_lot=10.0;
    let run=backtest_ticks(&s,&candles,&ticks,&Instrument::fx("EURUSD"),&cfg);
    assert!(run.errors.is_empty(),"{:?}",run.errors);
    assert_eq!(run.trades.len(),1,"both possible entries execute after midnight and share one cap");
    assert_eq!(run.trades[0].open_time,midnight+10000);assert_eq!(run.trades[0].close_time,midnight+15000);
    assert_eq!(run.trades[0].financing,0.0,"position opened after rollover owes no prior-day financing");
}

#[test]
fn financing_follows_tick_rollover_and_stops_at_trade_exit(){
    let midnight=86_400_000;
    let candles:Vec<_>=[midnight-90000,midnight-30000,midnight+30000].into_iter().map(|time|Bar::new(time,1.1,1.11,1.08,1.1,1.0)).collect();
    for (exit_offset,expected_cost) in [(-10000,0.0),(10000,1.0)] {
        let ticks=[Tick{time:midnight-20000,bid:1.1,ask:1.1},Tick{time:midnight+exit_offset,bid:1.09,ask:1.09},Tick{time:midnight+40000,bid:1.1,ask:1.1}];
        let mut s=strategy();s.stop_loss=Some(StopLoss::Points(10.0));
        let mut cfg=config();cfg.account.long_financing_per_lot=10.0;cfg.account.triple_rollover_weekday=None;
        let run=backtest_ticks(&s,&candles,&ticks,&Instrument::fx("EURUSD"),&cfg);
        assert!(run.errors.is_empty());assert_eq!(run.trades[0].close_time,midnight+exit_offset);
        assert_eq!(run.trades[0].financing,expected_cost);
        assert!((run.trades[0].pnl-(-100.0-expected_cost)).abs()<1e-8);
    }
}

#[test]
fn intrabar_financing_can_trigger_stopout_at_first_post_rollover_quote(){
    let midnight=86_400_000;
    let candles:Vec<_>=[midnight-90000,midnight-30000].into_iter().map(|time|Bar::new(time,1.1,1.1,1.1,1.1,1.0)).collect();
    let ticks=[Tick{time:midnight-20000,bid:1.1,ask:1.1},Tick{time:midnight+10000,bid:1.1,ask:1.1}];
    let mut cfg=config();cfg.account.long_financing_per_lot=100000.0;cfg.account.triple_rollover_weekday=None;
    let run=backtest_ticks(&strategy(),&candles,&ticks,&Instrument::fx("EURUSD"),&cfg);
    assert!(run.errors.is_empty());assert_eq!(run.trades.len(),1);
    assert_eq!(run.trades[0].exit_reason,ExitReason::StopOut);
    assert_eq!(run.trades[0].close_time,midnight+10000);
    assert_eq!(run.trades[0].financing,10000.0);assert_eq!(run.final_equity,0.0);
    let event=&run.execution.stop_out_decisions[0];
    assert_eq!(event.time,midnight+10000);assert_eq!(event.liquidation_price_before_slippage,1.1);
    assert_eq!(event.financing_accrued,10000.0);assert_eq!(event.equity_before_slippage,0.0);
    assert!(event.equity_before_slippage<event.minimum_equity);
    assert_eq!(run.execution.financing_accruals[0].applied_at,event.time);
}

#[test]
fn one_bar_time_exit_uses_next_candles_first_quote(){
    let mut s=strategy();s.exits=vec![sqx_strategy::ExitType::ExitAfterBars(1)];
    let ticks=[Tick{time:60007,bid:1.0,ask:1.001},Tick{time:119999,bid:1.002,ask:1.003},Tick{time:120009,bid:1.004,ask:1.005},Tick{time:179999,bid:1.01,ask:1.011}];
    let run=backtest_ticks(&s,&bars(),&ticks,&Instrument::fx("EURUSD"),&config());
    assert!(run.errors.is_empty(),"{:?}",run.errors);
    let trade=&run.trades[0];assert_eq!(trade.open_time,60007);assert_eq!(trade.close_time,120009);assert_eq!(trade.close_price,1.004);assert_eq!(trade.exit_reason,ExitReason::ExitAfterBars);assert_eq!(trade.bars,1);
    let decision=&run.execution.scheduled_exit_decisions[0];assert_eq!(decision.decision_bar_open_time,60000);assert_eq!(decision.executed_time,Some(120009));
}

#[test]
fn rejected_tick_order_records_actual_attempt_time(){
    let mut s=strategy();s.money_management=MoneyManagement::FixedSize{size:1000.0};
    let ticks=[Tick{time:60007,bid:1.0,ask:1.001},Tick{time:120009,bid:1.0,ask:1.001}];
    let run=backtest_ticks(&s,&bars(),&ticks,&Instrument::fx("EURUSD"),&config());
    assert!(run.errors.is_empty());assert!(run.trades.is_empty());assert_eq!(run.rejected_orders[0].time,60007);
    let attempt=&run.execution.entry_attempts[0];assert_eq!(attempt.attempt_time,60007);assert_eq!(attempt.signal_bar_open_time,0);assert_eq!(attempt.requested_lots,Some(1000.0));assert!(attempt.trade_id.is_none());assert!(!attempt.blocked_by.is_empty());
}

#[test]
fn financing_events_do_not_double_charge_opening_quote_or_later_same_day_ticks(){
    let day=86_400_000;
    let data:Vec<_>=[day-120_000,day-60_000,day].into_iter().map(|t|Bar::new(t,1.0,1.0,1.0,1.0,1.0)).collect();
    let ticks=[Tick{time:day-59_993,bid:1.0,ask:1.0},Tick{time:day+7,bid:1.0,ask:1.0},Tick{time:day+900,bid:1.0,ask:1.0}];
    let mut cfg=config();cfg.account.long_financing_per_lot=10.0;
    let result=backtest_ticks(&strategy(),&data,&ticks,&Instrument::fx("EURUSD"),&cfg);
    assert!(result.errors.is_empty(),"{:?}",result.errors);
    let events=&result.execution.financing_accruals;assert_eq!(events.len(),1);
    assert_eq!(events[0].rollover_utc_day,1);assert_eq!(events[0].applied_at,day+7);
    assert_eq!(events[0].charge,1.0);assert_eq!(events[0].accrued_after,1.0);
    assert_eq!(result.execution.cash_settlements[0].financing,1.0);
    assert_eq!(result.execution.cash_settlements[0].time,day+900);
    assert_eq!(result.final_equity,9999.0);
}

#[test]
fn tick_stop_amendments_stop_at_exit_and_preserve_actual_quote_times(){
    for short in [false,true] {
        for exit in [sqx_strategy::ExitType::TrailingStop,sqx_strategy::ExitType::MoveToBreakEven] {
            let mut s=strategy();s.long=!short;s.short=short;s.signals[0].kind=if short{SignalKind::ShortEntry}else{SignalKind::LongEntry};
            s.stop_loss=Some(StopLoss::Points(100.0));s.exits.push(exit);
            let favorable=if short{0.98}else{1.02};
            let data=[Bar::new(0,1.0,1.0,1.0,1.0,1.0),Bar::new(60000,1.0,2.0,0.5,1.0,1.0)];
            let ticks=[Tick{time:60007,bid:1.0,ask:1.0},Tick{time:90000,bid:favorable,ask:favorable},Tick{time:100000,bid:1.0,ask:1.0},Tick{time:110000,bid:if short{0.5}else{2.0},ask:if short{0.5}else{2.0}}];
            let r=backtest_ticks(&s,&data,&ticks,&Instrument::fx("EURUSD"),&config());assert!(r.errors.is_empty(),"{:?}",r.errors);
            assert_eq!(r.trades[0].close_time,100000);assert_eq!(r.execution.stop_changes.len(),1);
            let change=&r.execution.stop_changes[0];assert_eq!(change.time,90000);assert_eq!(change.observed_price,favorable);
            assert_eq!(change.source,"observed_tick");assert_eq!(change.path_step,None);
            assert_eq!(change.new_reason,r.trades[0].exit_reason);
        }
    }
}
