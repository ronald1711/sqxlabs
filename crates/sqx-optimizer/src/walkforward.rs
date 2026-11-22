//! Walk-forward optimization: re-optimize on each in-sample window and trade
//! the chosen strategy forward on the out-of-sample window, rolling forward.

use rand::rngs::StdRng;
use rand::SeedableRng;

use sqx_backtest::{backtest, BacktestConfig, BacktestResult};
use sqx_core::{Bar, Instrument};
use sqx_metrics::{compute as compute_metrics, Metrics};
use sqx_strategy::Strategy;

use crate::{backtest_batch, random_strategy, rank, Fitness, GeneratorConfig};

/// Summary of a walk-forward optimization run.
#[derive(Debug, Clone)]
pub struct WalkForwardReport {
    pub windows: usize,
    pub oos_trades: usize,
    pub oos_net_profit: f64,
    pub oos_profit_factor: f64,
    pub oos_winning_pct: f64,
    pub oos_max_dd_pct: f64,
}

#[allow(clippy::too_many_arguments)]
pub fn walk_forward(
    bars: &[Bar],
    instrument: &Instrument,
    config: &BacktestConfig,
    periods_per_year: f64,
    gen_config: &GeneratorConfig,
    windows: usize,
    oos_fraction: f64,
    candidates_per_window: usize,
    seed: u64,
) -> WalkForwardReport {
    let n = bars.len();
    if windows == 0 || n < 100 {
        return WalkForwardReport {
            windows: 0, oos_trades: 0, oos_net_profit: 0.0,
            oos_profit_factor: f64::NAN, oos_winning_pct: f64::NAN, oos_max_dd_pct: f64::NAN,
        };
    }

    let mut all_trades: Vec<sqx_backtest::Trade> = Vec::new();
    let mut equity_path: Vec<(i64, f64)> = Vec::new();
    let mut eq = config.initial_deposit;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut completed = 0usize;

    let window_len = n / windows;
    let oos = oos_fraction.clamp(0.05, 0.5);

    for w in 0..windows {
        let start = w * window_len;
        let end = if w == windows - 1 { n } else { (w + 1) * window_len };
        let split = start + ((end - start) as f64 * (1.0 - oos)).round() as usize;
        let is_bars = &bars[start..split];
        let oos_bars = &bars[split..end];
        if is_bars.len() < 50 || oos_bars.len() < 10 {
            continue;
        }
        let candidates: Vec<Strategy> = (0..candidates_per_window)
            .map(|i| random_strategy(&mut rng, &format!("w{w}-c{i}"), gen_config))
            .collect();
        let scored = backtest_batch(&candidates, is_bars, instrument, config, periods_per_year);
        let ranked = rank(scored, Fitness::ProfitFactor, 3);
        if let Some((best, _)) = ranked.first() {
            let r = backtest(best, oos_bars, instrument, config);
            for t in &r.trades {
                eq += t.pnl;
                equity_path.push((t.close_time, eq));
            }
            all_trades.extend(r.trades);
        }
        completed += 1;
    }

    let result = BacktestResult {
        trades: all_trades,
        equity: equity_path,
        initial_deposit: config.initial_deposit,
        final_equity: eq,
    };
    let m: Metrics = compute_metrics(&result, n, periods_per_year);

    WalkForwardReport {
        windows: completed,
        oos_trades: result.trades.len(),
        oos_net_profit: m.net_profit,
        oos_profit_factor: m.profit_factor,
        oos_winning_pct: m.winning_pct,
        oos_max_dd_pct: m.max_drawdown_pct,
    }
}
