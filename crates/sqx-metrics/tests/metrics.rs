use sqx_backtest::{BacktestResult, ExitReason, Trade};
use sqx_core::Direction;
use sqx_metrics::{compute, Metrics};

fn result_with_trades(pnls: &[f64]) -> BacktestResult {
    let trades: Vec<Trade> = pnls
        .iter()
        .enumerate()
        .map(|(i, p)| Trade {
            id: i,
            direction: Direction::Long,
            open_time: i as i64,
            close_time: (i + 1) as i64,
            open_price: 1.0,
            close_price: 1.0 + p / 10_000.0,
            size: 1.0,
            pnl: *p,
            pnl_points: p / 10.0,
            commission: 0.0,
            bars: 1,
            mae_points: 0.0,
            mfe_points: 0.0,
            exit_reason: ExitReason::ExitSignal,
        })
        .collect();
    let equity: Vec<(i64, f64)> = {
        let mut e = 10_000.0;
        let mut out = vec![(0, e)];
        for p in pnls {
            e += p;
            out.push(((out.len()) as i64, e));
        }
        out
    };
    BacktestResult { trades, equity, initial_deposit: 10_000.0, final_equity: 10_000.0 + pnls.iter().sum::<f64>() }
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn profit_metrics() {
    let r = result_with_trades(&[100.0, 200.0, -50.0, 150.0]);
    let m: Metrics = compute(&r, 5, 252.0);
    assert!(approx(m.net_profit, 400.0));
    assert!(approx(m.gross_profit, 450.0));
    assert!(approx(m.gross_loss, -50.0));
    assert!(approx(m.profit_factor, 9.0));
    assert!(approx(m.number_of_trades, 4.0));
    assert!(approx(m.number_of_profits, 3.0));
    assert!(approx(m.number_of_losses, 1.0));
    assert!(approx(m.winning_pct, 75.0));
}

#[test]
fn profit_factor_infinite_without_losses() {
    let r = result_with_trades(&[100.0, 200.0]);
    let m: Metrics = compute(&r, 3, 252.0);
    assert_eq!(m.profit_factor, f64::INFINITY);
}

#[test]
fn drawdown_tracks_peak() {
    let r = result_with_trades(&[100.0, -300.0, 50.0]);
    let m: Metrics = compute(&r, 4, 252.0);
    assert!(m.max_drawdown > 0.0);
    assert!(approx(m.max_drawdown, 300.0), "peak 10100 then 9800 -> dd 300");
}
