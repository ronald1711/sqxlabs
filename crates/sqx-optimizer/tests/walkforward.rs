use sqx_backtest::BacktestConfig;
use sqx_core::{Bar, Instrument};
use sqx_optimizer::{walk_forward, GeneratorConfig};

fn bars() -> Vec<Bar> {
    (0..400)
        .map(|i| {
            let x = i as f64;
            let mid = 1.10 + (x * 0.05).sin() * 0.01;
            Bar::new(1_700_000_000_000 + i as i64 * 60_000, mid, mid + 0.002, mid - 0.002, mid + 0.0005, 100.0)
        })
        .collect()
}

#[test]
fn walk_forward_is_deterministic() {
    let b = bars();
    let inst = Instrument::fx("EURUSD");
    let cfg = BacktestConfig::default();
    let gen = GeneratorConfig::default();
    let a = walk_forward(&b, &inst, &cfg, 252.0, &gen, 4, 0.3, 20, 42);
    let c = walk_forward(&b, &inst, &cfg, 252.0, &gen, 4, 0.3, 20, 42);
    assert_eq!(a.windows, c.windows);
    assert_eq!(a.oos_trades, c.oos_trades);
    assert!((a.oos_net_profit - c.oos_net_profit).abs() < 1e-9);
}
