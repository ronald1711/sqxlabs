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
