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
    let cfg = BacktestConfig { initial_deposit: 10_000.0, spread_points: 0.0, slippage_points: 0.0, commission_per_lot: 0.0, intrabar_up_first: false };
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

#[test]
fn tick_precision_resolves_sl_first() {
    use sqx_backtest::backtest_ticks;
    use sqx_core::Tick;
    use sqx_strategy::{ProfitTarget, StopLoss};
    let bars = vec![
        Bar::new(0, 100.0, 100.0, 100.0, 100.0, 1.0),
        Bar::new(60_000, 100.0, 100.0, 100.0, 100.0, 1.0),
        Bar::new(120_000, 100.0, 120.0, 80.0, 100.0, 1.0),
        Bar::new(180_000, 100.0, 100.0, 100.0, 100.0, 1.0),
    ];
    let mut s = always_long_strategy();
    s.stop_loss = Some(StopLoss::Points(100.0));
    s.profit_target = Some(ProfitTarget::Points(200.0));
    let ticks = vec![
        Tick { time: 120_001, bid: 99.90, ask: 99.91 },
        Tick { time: 120_002, bid: 100.05, ask: 100.06 },
    ];
    let cfg = BacktestConfig { initial_deposit: 10_000.0, spread_points: 0.0, slippage_points: 0.0, commission_per_lot: 0.0, intrabar_up_first: false };
    let r = backtest_ticks(&s, &bars, &ticks, &Instrument::fx("EURUSD"), &cfg);
    assert!(!r.trades.is_empty(), "at least one trade expected");
    assert_eq!(r.trades[0].exit_reason, sqx_backtest::ExitReason::StopLoss, "ticks hit SL first");
}
