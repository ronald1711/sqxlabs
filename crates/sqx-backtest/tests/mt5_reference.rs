//! Independent MT5 calculation snapshots, not fill or strategy-timing certification.
use sqx_backtest::{backtest,AccountConfig,BacktestConfig};
use sqx_core::{Bar,Instrument};
use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Comparator,Operand,Join,MoneyManagement};
#[test]
fn eurusd_eur_profit_and_margin_match_mt5_calculation_snapshots() {
    let reference:serde_json::Value=serde_json::from_str(include_str!("fixtures/mt5_eurusd_eur_calculation.json")).unwrap();
    assert_eq!(reference["account"]["currency"],"EUR");
    let cases=reference["cases"].as_array().unwrap();assert_eq!(cases.len(),4);
    for case in cases {
        let opening=case["open"].as_f64().unwrap();let closing=case["close"].as_f64().unwrap();
        let long=case["direction"]=="buy";
        let mut strategy=Strategy::new("mt5-calculation-reference");strategy.long=long;strategy.short=!long;
        strategy.money_management=MoneyManagement::FixedSize{size:case["lots"].as_f64().unwrap()};
        strategy.signals.push(Signal::new(if long {SignalKind::LongEntry}else{SignalKind::ShortEntry},vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));
        let bars=[Bar::new(0,opening,opening,opening,opening,1.0),Bar::new(60000,opening,opening,opening,opening,1.0),Bar::new(120000,closing,closing,closing,closing,1.0)];
        let config=BacktestConfig{account:AccountConfig{currency:"EUR".into(),max_leverage:reference["account"]["leverage"].as_f64().unwrap(),..Default::default()},..Default::default()};
        let result=backtest(&strategy,&bars,&Instrument::fx("EURUSD"),&config);
        assert!(result.errors.is_empty(),"{:?}",result.errors);assert_eq!(result.trades.len(),1);
        let expected=case["profit_account"].as_f64().unwrap();
        assert!((result.trades[0].pnl-expected).abs()<=0.005000001,"case {case}: actual {}",result.trades[0].pnl);
        assert!((result.final_equity-(config.initial_deposit+expected)).abs()<=0.005000001);
        let margin=result.execution.entry_attempts.iter().find_map(|a|a.margin_check.as_ref()).unwrap();
        assert!((margin.required_margin-case["margin_account"].as_f64().unwrap()).abs()<=0.005000001);
    }
}

#[test]
fn usdjpy_eur_margin_uses_independent_base_conversion_from_mt5_snapshot() {
    let reference:serde_json::Value=serde_json::from_str(include_str!("fixtures/mt5_usdjpy_eur_calculation.json")).unwrap();
    check_usdjpy_reference(&reference,4);
}

#[test]
fn usdjpy_eur_volume_and_signed_profit_match_sixteen_mt5_calculations(){
    let reference:serde_json::Value=serde_json::from_str(include_str!("fixtures/mt5_usdjpy_eur_volume_calculation.json")).unwrap();
    for case in reference["cases"].as_array().unwrap(){assert_eq!(case["quotes_before"],case["quotes_after"]);assert_eq!(case["quotes_unchanged"],true);}
    check_usdjpy_reference(&reference,16);
}

fn check_usdjpy_reference(reference:&serde_json::Value,count:usize){
    use sqx_backtest::{DirectionalConversionRate,ProfitConversionModel,MarginConversionRate,MarginModel};
    assert_eq!(reference["account"]["currency"],"EUR");
    let cases=reference["cases"].as_array().unwrap();assert_eq!(cases.len(),count);
    for case in cases {
        let opening=case["open"].as_f64().unwrap();let closing=case["close"].as_f64().unwrap();
        let long=case["direction"]=="buy";
        let quotes=&case["quotes_before"];
        let profit_long=1.0/quotes["EURJPY"]["bid"].as_f64().unwrap();
        let profit_short=1.0/quotes["EURJPY"]["ask"].as_f64().unwrap();
        let margin_long=1.0/quotes["EURUSD"]["bid"].as_f64().unwrap();
        let margin_short=1.0/quotes["EURUSD"]["ask"].as_f64().unwrap();
        let mut strategy=Strategy::new("mt5-usdjpy-margin-reference");strategy.long=long;strategy.short=!long;
        strategy.money_management=MoneyManagement::FixedSize{size:case["lots"].as_f64().unwrap()};
        strategy.signals.push(Signal::new(if long {SignalKind::LongEntry}else{SignalKind::ShortEntry},vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));
        let bars=[Bar::new(0,opening,opening,opening,opening,1.0),Bar::new(60000,opening,opening,opening,opening,1.0),Bar::new(120000,closing,closing,closing,closing,1.0)];
        let config=BacktestConfig{account:AccountConfig{currency:"EUR".into(),max_leverage:reference["account"]["leverage"].as_f64().unwrap(),margin_model:MarginModel::ForexBase,
            profit_conversion_model:ProfitConversionModel::Directional,
            profit_conversion_rates:vec![DirectionalConversionRate{time:0,from:"JPY".into(),to:"EUR".into(),long_rate:profit_long,short_rate:profit_short}],
            margin_conversion_rates:vec![MarginConversionRate{time:0,from:"USD".into(),to:"EUR".into(),long_rate:margin_long,short_rate:margin_short}],..Default::default()},..Default::default()};
        let result=backtest(&strategy,&bars,&Instrument::fx("USDJPY"),&config);
        assert!(result.errors.is_empty(),"{:?}",result.errors);assert_eq!(result.trades.len(),1);
        assert!((result.trades[0].pnl-case["profit_account"].as_f64().unwrap()).abs()<=0.005000001,"case {case}: pnl {}",result.trades[0].pnl);
        assert!((result.final_equity-config.initial_deposit-case["profit_account"].as_f64().unwrap()).abs()<=0.005000001);
        let expected=case["margin_account"].as_f64().unwrap();
        let admission=result.execution.entry_attempts.iter().find_map(|a|a.margin_check.as_ref()).unwrap();
        assert!((admission.required_margin-expected).abs()<=0.005000001);
        for sample in result.execution.account_snapshots.iter().filter(|s|s.trade_id.is_some()) {
            assert!((sample.required_margin-expected).abs()<=0.005000001,"constant base-currency margin must not vary with USDJPY price");
        }
    }
}
