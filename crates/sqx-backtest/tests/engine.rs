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
    let cfg = BacktestConfig { initial_deposit: 10_000.0, spread_points: 0.0, slippage_points: 0.0, commission_per_lot: 0.0 };
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
