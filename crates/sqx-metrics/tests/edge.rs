use sqx_backtest::BacktestResult;
use sqx_metrics::compute;

#[test]
fn zero_trades_metrics_are_safe() {
    let result = BacktestResult {
        trades: vec![],
        equity: vec![(0, 10_000.0)],
        initial_deposit: 10_000.0,
        final_equity: 10_000.0,
    };
    let m = compute(&result, 100, 252.0);
    assert_eq!(m.number_of_trades, 0.0);
    assert!(m.profit_factor.is_nan() || m.profit_factor.is_infinite());
    assert!(m.net_profit.abs() < 1e-9);
}
