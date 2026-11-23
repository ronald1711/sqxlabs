//! The equality comparators share a NaN trap that every other comparator in
//! evaluate.rs already guards against: NaN compares unequal to everything,
//! itself included, so an unguarded Neq is true for every bar of an
//! indicator's warm-up window. The engine reads that as a valid entry signal
//! and fabricates trades before the strategy is looking at real data.

use sqx_backtest::compile_signal;
use sqx_core::Bar;
use sqx_strategy::{Comparator, Condition, Join, Operand, Param, PriceField, Signal, SignalKind};

fn bars(n: usize) -> Vec<Bar> {
    let mut close = 100.0;
    (0..n)
        .map(|i| {
            close += 1.0;
            Bar::new(i as i64 * 60_000, close - 0.5, close + 1.0, close - 1.0, close, 1000.0)
        })
        .collect()
}

fn signal(cmp: Comparator, right: Operand) -> Signal {
    let sma = Operand::Indicator("SMA".to_string(), vec![Param::new("period", 20.0)]);
    Signal::new(
        SignalKind::LongEntry,
        vec![Condition::new(sma, cmp, right)],
        Join::And,
    )
}

/// SMA(20) is NaN for bars 0..=18. A "not equal" against any constant must be
/// false across that whole stretch, not true.
#[test]
fn neq_is_false_while_an_operand_is_warming_up() {
    let b = bars(60);
    let sig = signal(Comparator::Neq, Operand::Const(-999.0));
    let out = compile_signal(&sig, &b);
    assert!(
        out[..19].iter().all(|v| !v),
        "Neq fired during the SMA warm-up: {:?}",
        &out[..19]
    );
    assert!(
        out[25..].iter().all(|v| *v),
        "Neq should be true once the SMA is defined"
    );
}

/// Same warm-up, mirror image: nothing equals NaN either, but the guard must
/// keep Eq from silently reading as false only by accident.
#[test]
fn eq_is_false_while_an_operand_is_warming_up() {
    let b = bars(60);
    let sig = signal(Comparator::Eq, Operand::Const(-999.0));
    let out = compile_signal(&sig, &b);
    assert!(out.iter().all(|v| !v), "Eq must never hold against -999");
}

/// A constant on the left is the other operand order.
#[test]
fn neq_guards_nan_on_either_side() {
    let b = bars(60);
    let sma = Operand::Indicator("SMA".to_string(), vec![Param::new("period", 20.0)]);
    let sig = Signal::new(
        SignalKind::LongEntry,
        vec![Condition::new(Operand::Const(0.0), Comparator::Neq, sma)],
        Join::And,
    );
    let out = compile_signal(&sig, &b);
    assert!(
        out[..19].iter().all(|v| !v),
        "Neq fired during the SMA warm-up with the indicator on the right: {:?}",
        &out[..19]
    );
}

/// Every comparator must agree that an undefined operand produces no signal,
/// except the unary Rising/Falling forms which use only the left operand.
#[test]
fn every_binary_comparator_is_silent_during_warmup() {
    let b = bars(60);
    let sma = Operand::Indicator("SMA".to_string(), vec![Param::new("period", 20.0)]);
    let close = Operand::Price(PriceField::Close, 0);
    for cmp in [
        Comparator::Gt,
        Comparator::Gte,
        Comparator::Lt,
        Comparator::Lte,
        Comparator::Eq,
        Comparator::Neq,
        Comparator::CrossAbove,
        Comparator::CrossBelow,
    ] {
        let sig = Signal::new(
            SignalKind::LongEntry,
            vec![Condition::new(sma.clone(), cmp, close.clone())],
            Join::And,
        );
        let out = compile_signal(&sig, &b);
        assert!(
            out[..19].iter().all(|v| !v),
            "{cmp:?} fired during the SMA warm-up: {:?}",
            &out[..19]
        );
    }
}
