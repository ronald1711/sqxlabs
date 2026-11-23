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
            commission: 0.0, financing: 0.0,
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
    BacktestResult { errors: Vec::new(), rejected_orders: Vec::new(), ambiguous_bars: Vec::new(), execution: Default::default(), trades, equity, initial_deposit: 10_000.0, final_equity: 10_000.0 + pnls.iter().sum::<f64>() }
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

#[test]
fn flat_trades_are_neither_winners_nor_losers_and_break_streaks() {
    let r=result_with_trades(&[10.0,0.0,20.0,-5.0,-0.0,-15.0]);
    let m=compute(&r,6,252.0);
    assert_eq!(m.number_of_trades,6.0);assert_eq!(m.number_of_profits,2.0);assert_eq!(m.number_of_losses,2.0);
    assert!(approx(m.winning_pct,100.0/3.0));
    assert_eq!(m.avg_win,15.0);assert_eq!(m.avg_loss,-10.0);
    assert_eq!(m.max_consec_wins,1.0);assert_eq!(m.max_consec_losses,1.0);
    let flat=compute(&result_with_trades(&[0.0,-0.0]),2,252.0);
    assert_eq!(flat.winning_pct,0.0);assert_eq!(flat.number_of_losses,0.0);
    assert!(flat.avg_win.is_nan());assert!(flat.avg_loss.is_nan());assert!(flat.profit_factor.is_nan());
}

#[test]
fn corrupt_trade_observations_never_produce_rankable_metrics(){
    for field in 0..5 {
        let mut run=result_with_trades(&[100.0,-10.0]);
        match field {0=>run.trades[0].pnl=f64::NAN,1=>run.trades[0].commission=f64::INFINITY,2=>run.trades[0].financing=f64::NEG_INFINITY,3=>run.trades[0].size=f64::NAN,_=>run.equity[0].1=f64::NAN}
        assert!(sqx_metrics::validate_observations(&run).is_err());
        let m=compute(&run,3,252.0);
        assert!(m.net_profit.is_nan());assert!(m.profit_factor.is_nan());assert!(m.number_of_trades.is_nan());
    }
    let mut overflow=result_with_trades(&[1.0,1.0]);
    for trade in &mut overflow.trades{trade.pnl=f64::MAX;}
    assert!(sqx_metrics::validate_observations(&overflow).is_err());
    let mut duration=result_with_trades(&[1.0,1.0]);duration.trades[0].bars=usize::MAX;
    assert!(sqx_metrics::validate_observations(&duration).is_err());
    assert!(sqx_metrics::validate_observations(&result_with_trades(&[1.0,0.0])).is_ok());
}
