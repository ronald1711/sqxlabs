use sqx_backtest::{AccountConfig,BacktestConfig,DirectionalConversionRate,ProfitConversionModel,MarginModel,MarginConversionRate,backtest};
use sqx_core::{Bar,Direction,Instrument};
use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Comparator,Operand,Join,MoneyManagement,ExitType,PriceField};
fn account()->AccountConfig {
    AccountConfig{currency:"EUR".into(),profit_conversion_model:ProfitConversionModel::Directional,
        profit_conversion_rates:vec![DirectionalConversionRate{time:0,from:"JPY".into(),to:"EUR".into(),long_rate:0.006,short_rate:0.004}],
        margin_model:MarginModel::ForexBase,margin_conversion_rates:vec![MarginConversionRate{time:0,from:"USD".into(),to:"EUR".into(),long_rate:0.9,short_rate:0.89}],..Default::default()}
}
#[test]
fn directional_rates_are_causal_and_do_not_fall_back_to_single_rate() {
    let instrument=Instrument::fx("USDJPY");let mut a=account();
    assert_eq!(a.profit_rate(&instrument,0,150.0,Direction::Long).unwrap(),0.006);
    assert_eq!(a.profit_rate(&instrument,0,150.0,Direction::Short).unwrap(),0.004);
    assert!(a.quote_rate(&instrument,0,150.0).is_err());
    a.conversion_rates.push(sqx_backtest::ConversionRate{time:0,from:"JPY".into(),to:"EUR".into(),rate:0.005});
    a.profit_conversion_rates[0].time=1;
    assert!(a.profit_rate(&instrument,0,150.0,Direction::Long).is_err());
    a.profit_conversion_rates[0].time=0;a.max_conversion_age_ms=10;
    assert!(a.profit_rate(&instrument,11,150.0,Direction::Short).is_err());
    a.profit_conversion_rates.push(a.profit_conversion_rates[0].clone());assert!(a.validate().is_err());
    a.profit_conversion_rates.pop();a.profit_conversion_rates[0].long_rate=f64::INFINITY;assert!(a.validate().is_err());
}
#[test]
fn mixed_direction_strategy_uses_position_rate_in_each_cash_settlement() {
    let mut strategy=Strategy::new("mixed-conversion");strategy.long=true;strategy.short=true;
    strategy.money_management=MoneyManagement::FixedSize{size:1.0};strategy.exits=vec![ExitType::ExitAfterBars(1)];
    for (kind,comparison) in [(SignalKind::LongEntry,Comparator::Gt),(SignalKind::ShortEntry,Comparator::Lt)] {
        strategy.signals.push(Signal::new(kind,vec![Condition::new(Operand::Price(PriceField::Close,0),comparison,Operand::Const(150.0))],Join::And));
    }
    let bars:Vec<_>=[151.0,151.0,152.0,149.0,148.0,149.0,151.0,151.0].into_iter().enumerate().map(|(i,p)|Bar::new(i as i64*60000,p,p,p,p,1.0)).collect();
    let config=BacktestConfig{account:account(),..Default::default()};
    let result=backtest(&strategy,&bars,&Instrument::fx("USDJPY"),&config);
    assert!(result.errors.is_empty(),"{:?}",result.errors);
    for direction in [Direction::Long,Direction::Short] {assert!(result.trades.iter().any(|t|t.direction==direction&&t.pnl!=0.0));}
    for (trade,settlement) in result.trades.iter().zip(&result.execution.cash_settlements) {
        let rate=if trade.direction==Direction::Long {0.006}else{0.004};
        let expected=(trade.close_price-trade.open_price)*trade.direction.sign()*100000.0*rate;
        assert!((trade.pnl-expected).abs()<1e-8);
        assert_eq!(settlement.quote_to_account_rate,rate);
    }
    for snapshot in result.execution.account_snapshots.iter().filter(|s|s.trade_id.is_some()) {
        let trade=&result.trades[snapshot.trade_id.unwrap()];
        assert_eq!(snapshot.quote_to_account_rate,Some(if trade.direction==Direction::Long{0.006}else{0.004}));
    }
}

#[test]
fn profit_conversion_change_is_applied_only_at_available_tick() {
    let mut strategy=Strategy::new("profit-fx-tick");strategy.long=true;strategy.short=false;
    strategy.money_management=MoneyManagement::FixedSize{size:1.0};
    strategy.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));
    let mut config=BacktestConfig{bar_duration_ms:Some(60000),account:account(),..Default::default()};
    config.account.profit_conversion_rates.push(DirectionalConversionRate{time:120500,from:"JPY".into(),to:"EUR".into(),long_rate:0.01,short_rate:0.009});
    let bars=[Bar::new(0,150.0,150.0,150.0,150.0,1.0),Bar::new(60000,150.0,150.0,150.0,150.0,1.0),Bar::new(120000,140.0,140.0,140.0,140.0,1.0)];
    let ticks=[sqx_core::Tick{time:60000,bid:150.0,ask:150.0},sqx_core::Tick{time:120000,bid:140.0,ask:140.0},sqx_core::Tick{time:120500,bid:140.0,ask:140.0}];
    let result=sqx_backtest::backtest_ticks(&strategy,&bars,&ticks,&Instrument::fx("USDJPY"),&config);
    assert!(result.errors.is_empty(),"{:?}",result.errors);assert_eq!(result.trades.len(),1);
    assert_eq!(result.trades[0].close_time,120500);assert_eq!(result.trades[0].exit_reason,sqx_backtest::ExitReason::StopOut);
    assert_eq!(result.execution.stop_out_decisions[0].quote_to_account_rate,0.01);
    assert_eq!(result.execution.stop_out_decisions[0].margin_base_to_account_rate,0.9);
    assert_eq!(result.execution.cash_settlements[0].quote_to_account_rate,0.01);
    assert_eq!(result.final_equity,0.0);
}
