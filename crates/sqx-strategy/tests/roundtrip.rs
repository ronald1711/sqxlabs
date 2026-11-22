use sqx_strategy::{
    Comparator, Condition, Join, MoneyManagement, Operand, PriceField, ProfitTarget, Signal,
    SignalKind, StopLoss, Strategy,
};

#[test]
fn strategy_json_roundtrip() {
    let mut s = Strategy::new("roundtrip");
    s.long = true;
    s.signals.push(Signal::new(
        SignalKind::LongEntry,
        vec![Condition::new(
            Operand::Price(PriceField::Close, 0),
            Comparator::Gt,
            Operand::Const(0.0),
        )],
        Join::And,
    ));
    s.stop_loss = Some(StopLoss::Points(100.0));
    s.profit_target = Some(ProfitTarget::Points(200.0));
    s.money_management = MoneyManagement::FixedSize { size: 0.1 };

    let json = serde_json::to_string(&s).expect("serialize");
    let back: Strategy = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.name, "roundtrip");
    assert_eq!(back.signals.len(), 1);
    assert_eq!(back.stop_loss, Some(StopLoss::Points(100.0)));
    assert_eq!(back.profit_target, Some(ProfitTarget::Points(200.0)));
}
