use sqx_backtest::{backtest,AccountConfig,BacktestConfig,ConversionRate,MarginConversionRate,MarginModel,ExitReason};
use sqx_core::{Bar,Direction,Instrument};
use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Comparator,Operand,Join,MoneyManagement};
fn account()->AccountConfig {
    AccountConfig{currency:"EUR".into(),margin_model:MarginModel::ForexBase,
        conversion_rates:vec![ConversionRate{time:0,from:"JPY".into(),to:"EUR".into(),rate:0.006}],
        margin_conversion_rates:vec![MarginConversionRate{time:0,from:"USD".into(),to:"EUR".into(),long_rate:0.9,short_rate:0.89}],..Default::default()}
}
#[test]
fn margin_observations_are_causal_directional_and_validated() {
    let instrument=Instrument::fx("USDJPY");let mut a=account();
    assert_eq!(a.margin_base_rate(&instrument,0,150.0,Direction::Long).unwrap(),0.9);
    assert_eq!(a.margin_base_rate(&instrument,0,160.0,Direction::Short).unwrap(),0.89);
    a.margin_conversion_rates[0].time=1;
    assert!(a.margin_base_rate(&instrument,0,150.0,Direction::Long).is_err());
    a.margin_conversion_rates[0].time=0;a.max_conversion_age_ms=10;
    assert!(a.margin_base_rate(&instrument,11,150.0,Direction::Long).is_err());
    a.margin_conversion_rates.push(a.margin_conversion_rates[0].clone());assert!(a.validate().is_err());
    a.margin_conversion_rates.pop();a.margin_conversion_rates[0].short_rate=f64::NAN;assert!(a.validate().is_err());
}
#[test]
fn constant_base_margin_drives_long_and_short_stopout() {
    for long in [true,false] {
        let mut strategy=Strategy::new("base-margin-stopout");strategy.long=long;strategy.short=!long;
        strategy.money_management=MoneyManagement::FixedSize{size:1.0};
        strategy.signals.push(Signal::new(if long{SignalKind::LongEntry}else{SignalKind::ShortEntry},vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));
        let mut account=account();
        account.profit_conversion_model=sqx_backtest::ProfitConversionModel::Directional;
        account.profit_conversion_rates=vec![sqx_backtest::DirectionalConversionRate{time:0,from:"JPY".into(),to:"EUR".into(),long_rate:0.006,short_rate:0.004}];
        let config=BacktestConfig{account,..Default::default()};
        let bars=[Bar::new(0,150.0,150.0,150.0,150.0,1.0),Bar::new(60000,150.0,150.0,150.0,150.0,1.0),if long{Bar::new(120000,150.0,150.0,120.0,120.0,1.0)}else{Bar::new(120000,150.0,180.0,150.0,180.0,1.0)}];
        let r=backtest(&strategy,&bars,&Instrument::fx("USDJPY"),&config);
        assert!(r.errors.is_empty(),"{:?}",r.errors);assert_eq!(r.trades.len(),1);
        assert_eq!(r.trades[0].exit_reason,ExitReason::StopOut);
        let margin=100000.0*(if long {0.9}else{0.89})/30.0;
        let threshold=150.0+(margin*0.5-10000.0)/(100000.0*(if long{0.006}else{0.004})*(if long{1.0}else{-1.0}));
        let d=&r.execution.stop_out_decisions[0];
        assert!((d.fill_price-threshold).abs()<1e-9);
        assert!((d.required_margin-margin).abs()<1e-9);
        assert!((r.final_equity-margin*0.5).abs()<1e-8);
    }
}

#[test]
fn margin_conversion_change_triggers_stopout_at_its_first_available_tick() {
    let mut strategy=Strategy::new("margin-fx-tick");strategy.long=true;strategy.short=false;
    strategy.money_management=MoneyManagement::FixedSize{size:1.0};
    strategy.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));
    let mut config=BacktestConfig{bar_duration_ms:Some(60000),account:account(),..Default::default()};
    config.account.margin_conversion_rates.push(MarginConversionRate{time:120500,from:"USD".into(),to:"EUR".into(),long_rate:10.0,short_rate:10.0});
    let bars:Vec<_>=(0..3).map(|i|Bar::new(i*60000,150.0,150.0,150.0,150.0,1.0)).collect();
    let ticks:Vec<_>=[60000,120000,120500].into_iter().map(|time|sqx_core::Tick{time,bid:150.0,ask:150.0}).collect();
    let result=sqx_backtest::backtest_ticks(&strategy,&bars,&ticks,&Instrument::fx("USDJPY"),&config);
    assert!(result.errors.is_empty(),"{:?}",result.errors);assert_eq!(result.trades.len(),1);
    let trade=&result.trades[0];assert_eq!(trade.exit_reason,ExitReason::StopOut);assert_eq!(trade.close_time,120500);assert_eq!(trade.close_price,150.0);
    assert_eq!(result.execution.entry_attempts[0].margin_check.as_ref().unwrap().margin_base_to_account_rate,0.9);
    let decision=&result.execution.stop_out_decisions[0];assert_eq!(decision.margin_base_to_account_rate,10.0);
    assert!((decision.required_margin-100000.0*10.0/30.0).abs()<1e-8);
    assert_eq!(result.final_equity,10000.0,"margin conversion can trigger stop-out without a price move or cash charge");
}
