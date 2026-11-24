//! Hand-calculated execution-contract reference, not a broker fill report.
//! Prices are specified in integer 1e-5 units. 0.1 EURUSD lot = 10,000 EUR.
//! Entry and exit each suffer 10 units (one pip) adverse slippage. Commission
//! is $7 per round-trip lot, hence $0.70. Both gaps lose 240 units * $0.10
//! per unit + $0.70 = $24.70. Tick ask already includes the 20-unit spread.
use sqx_backtest::{backtest_ticks, BacktestConfig, ExitReason};
use sqx_core::{Bar, Instrument, Tick};
use sqx_strategy::{Comparator, Condition, Join, MoneyManagement, Operand, Signal, SignalKind, StopLoss, Strategy};

#[test]
fn dynamic_stops_wait_for_observed_ticks_on_both_sides() {
    use sqx_strategy::ExitType;
    // Integer-price reference: adverse excursion comes BEFORE the favourable
    // tick. A later candle extremum must not retroactively protect that excursion.
    // Trailing cases earn $6; break-even gaps lose $2 before $0.70 commission.
    for (short,trailing,quotes,expected_stop,expected_exit,expected_net) in [
        (false,true, [(110000,110020),(109960,109980),(110200,110220),(110080,110100)],110100,110080,5.30),
        (true, true, [(110000,110020),(110040,110060),(109800,109820),(109920,109940)],109920,109940,5.30),
        (false,false,[(110000,110020),(109960,109980),(110130,110150),(110000,110020)],110020,110000,-2.70),
        (true, false,[(110000,110020),(110040,110060),(109870,109890),(110000,110020)],110000,110020,-2.70),
    ] {
        let price=|units:i64|units as f64/100000.0;
        let mut strategy=Strategy::new("causal dynamic-stop reference");
        strategy.long=!short;strategy.short=short;
        strategy.money_management=MoneyManagement::FixedSize{size:0.1};
        strategy.stop_loss=Some(StopLoss::Points(10.0));
        strategy.exits=vec![if trailing{ExitType::TrailingStop}else{ExitType::MoveToBreakEven}];
        strategy.signals.push(Signal::new(if short{SignalKind::ShortEntry}else{SignalKind::LongEntry},
            vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));
        let ticks:Vec<_>=quotes.into_iter().zip([60001,60010,60020,60030])
            .map(|((bid,ask),time)|Tick{time,bid:price(bid),ask:price(ask)}).collect();
        let bars=[Bar::new(0,1.1,1.1,1.1,1.1,1.0),Bar::new(60000,1.1,1.102,1.098,ticks[3].bid,4.0)];
        let config=BacktestConfig{bar_duration_ms:Some(60000),commission_per_lot:7.0,..Default::default()};
        let result=backtest_ticks(&strategy,&bars,&ticks,&Instrument::fx("EURUSD"),&config);
        assert!(result.errors.is_empty(),"{:?}",result.errors);
        assert_eq!(result.trades.len(),1);
        let trade=&result.trades[0];
        assert_eq!((trade.open_time,trade.close_time),(60001,60030));
        assert_eq!(trade.exit_reason,if trailing{ExitReason::TrailingStop}else{ExitReason::MoveToBreakEven});
        assert!((trade.close_price-price(expected_exit)).abs()<1e-12);
        assert!((trade.pnl-expected_net).abs()<1e-8);
        assert!((result.final_equity-(10000.0+expected_net)).abs()<1e-8);
        assert_eq!(result.execution.stop_changes.len(),1);
        let change=&result.execution.stop_changes[0];
        assert_eq!(change.time,60020);
        assert!((change.new_stop.unwrap()-price(expected_stop)).abs()<1e-12);
    }
}

#[test]
fn long_and_short_stop_gaps_pay_executable_quote_slippage_and_one_commission() {
    // side, opening bid/ask, gap bid/ask, executed entry/exit
    for (short, bid, ask, gap_bid, gap_ask, entry, exit) in [
        (false,110000,110020,109800,109820,110030,109790),
        (true, 110000,110020,110200,110220,109990,110230),
    ] {
        let price=|units:i64|units as f64/100000.0;
        let mut strategy=Strategy::new("integer-price stop-gap reference");
        strategy.long=!short;strategy.short=short;
        strategy.money_management=MoneyManagement::FixedSize{size:0.1};
        strategy.stop_loss=Some(StopLoss::Points(10.0));
        strategy.signals.push(Signal::new(if short{SignalKind::ShortEntry}else{SignalKind::LongEntry},
            vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));
        let bars=[Bar::new(0,1.1,1.1,1.1,1.1,1.0),
            Bar::new(60000,1.1,1.102,1.098,price(gap_bid),2.0)];
        let ticks=[Tick{time:60001,bid:price(bid),ask:price(ask)},
            Tick{time:60020,bid:price(gap_bid),ask:price(gap_ask)}];
        // Deliberately contradictory synthetic spread: supplied bid/ask must win.
        let config=BacktestConfig{bar_duration_ms:Some(60000),spread_points:50.0,
            slippage_points:1.0,commission_per_lot:7.0,..Default::default()};
        let result=backtest_ticks(&strategy,&bars,&ticks,&Instrument::fx("EURUSD"),&config);
        assert!(result.errors.is_empty(),"{:?}",result.errors);
        assert_eq!(result.trades.len(),1);
        let trade=&result.trades[0];
        assert_eq!(trade.exit_reason,ExitReason::StopLoss);
        assert_eq!((trade.open_time,trade.close_time),(60001,60020));
        assert!((trade.open_price-price(entry)).abs()<1e-12);
        assert!((trade.close_price-price(exit)).abs()<1e-12);
        assert!((trade.commission-0.70).abs()<1e-12);
        assert!((trade.pnl+24.70).abs()<1e-8);
        assert!((result.final_equity-9975.30).abs()<1e-8);
        assert_eq!(result.execution.cash_settlements.len(),1);
        assert!((result.execution.cash_settlements[0].net_change+24.70).abs()<1e-8);
    }
}
