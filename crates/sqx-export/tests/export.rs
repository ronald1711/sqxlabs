use sqx_export::{export, Engine};
use sqx_strategy::{Comparator, Condition, Join, MoneyManagement, Operand, PriceField, Signal, SignalKind, Strategy};

fn rsi_strategy() -> Strategy {
    let mut s = Strategy::new("rsi-test");
    s.long = true;
    s.short = false;
    s.money_management = MoneyManagement::FixedSize { size: 0.1 };
    s.signals.push(Signal::new(
        SignalKind::LongEntry,
        vec![Condition::new(
            Operand::Indicator("RSI".into(), vec![sqx_strategy::Param::new("period", 14.0)]),
            Comparator::CrossBelow,
            Operand::Const(30.0),
        )],
        Join::And,
    ));
    s
}

#[test]
fn mql5_exports_rsi_cross() {
    let code = export(&rsi_strategy(), Engine::Mql5);
    assert!(code.contains("iRSI"), "MQL5 should use iRSI, got: {code}");
    assert!(code.contains("LongEntry"), "MQL5 should define LongEntry");
}

#[test]
fn pine_exports_ta_rsi() {
    let code = export(&rsi_strategy(), Engine::PineScript);
    assert!(code.contains("ta.rsi(close, 14)"), "Pine should use ta.rsi, got: {code}");
    assert!(code.contains("ta.crossunder"), "Pine cross-below should use ta.crossunder");
}

#[test]
fn pine_stoch_d_preserves_fixed_k_period_and_variable_smoothing() {
    for (params, smoothing) in [
        (vec![], 3),
        (vec![sqx_strategy::Param::new("period", 3.0)], 3),
        (vec![sqx_strategy::Param::new("period", 7.0)], 7),
        (vec![sqx_strategy::Param::new("period", 7.6)], 8),
    ] {
        let mut strategy = rsi_strategy();
        strategy.signals[0].conditions[0].left = Operand::Indicator("StochD".into(), params);
        let report = sqx_export::export_report(&strategy, Engine::PineScript);
        let code = report.code.expect("StochD has a Pine reference mapping");
        assert!(code.contains(&format!("ta.sma(ta.stoch(close, high, low, 14), {smoothing})")), "{code}");
        assert_eq!(report.status, "reference_draft");
    }
}

#[test]
fn mql4_exports_order_send() {
    let code = export(&rsi_strategy(), Engine::Mql4);
    assert!(code.contains("OrderSend"), "MQL4 should emit OrderSend");
}

#[test]
fn pseudocode_roundtrips() {
    let code = export(&rsi_strategy(), Engine::PseudoCode);
    assert!(code.contains("RSI(period=14)"), "pseudo-code should render the RSI condition");
}

#[test]
fn ninjascript_exports_csharp() {
    let code = export(&rsi_strategy(), Engine::NinjaTrader);
    assert!(code.contains("public class SqxLabsStrategy"));
    assert!(code.contains("RSI(14)"));
    assert!(code.contains("OnBarUpdate"));
}

#[test]
fn easylanguage_exports_cross() {
    let code = export(&rsi_strategy(), Engine::EasyLanguage);
    assert!(code.contains("RSI(Close, 14)"));
    assert!(code.contains("crosses under"));
}

#[test]
fn unsupported_features_never_emit_partial_platform_code() {
    use sqx_export::export_report;
    for engine in [Engine::Mql4,Engine::Mql5,Engine::PineScript,Engine::NinjaTrader,Engine::EasyLanguage] {
        let mut s = rsi_strategy();
        s.trading_options.exit_on_friday = true;
        s.trading_options.session_close_utc_minute = Some(1260);
        let report = export_report(&s,engine);
        assert_eq!(report.status,"blocked");
        assert!(report.code.is_none());
        assert!(report.blockers.iter().any(|e|e.contains("Scheduled")));
        s.trading_options = Default::default();
        s.signals[0].conditions[0].left = Operand::Indicator("UnmappedTestIndicator".into(),vec![]);
        let report = export_report(&s,engine);
        assert!(report.code.is_none(),"{engine:?} must not substitute SMA or zero");
        assert!(report.blockers.iter().any(|e|e.contains("indicator")));
        s = rsi_strategy(); s.stop_loss = Some(sqx_strategy::StopLoss::Points(100.0));
        assert!(export_report(&s,engine).code.is_none(),"must not silently omit protection");
    }
}

#[test]
fn draft_limitations_travel_with_downloaded_source() {
    use sqx_export::export_report;
    let report = export_report(&rsi_strategy(),Engine::Mql5);
    assert_eq!(report.status,"reference_draft");
    let code = report.code.unwrap();
    assert!(code.contains("REFERENCE DRAFT ONLY"));
    assert!(code.contains("order actions are placeholders"));
    let mut s = rsi_strategy();
    s.trading_options.exit_at_end_of_day = true;
    s.trading_options.session_close_utc_minute = Some(1200);
    let rules = export_report(&s,Engine::PseudoCode).code.unwrap();
    assert!(rules.contains("session_close_utc_minute: Some(1200)"));
    assert!(rules.contains("size: 0.1"));
}
