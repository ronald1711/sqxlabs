//! # sqx-robustness
//!
//! Robustness tests mirroring StrategyQuant's Retester: Monte Carlo
//! trade-manipulation (reshuffling order and randomly skipping trades) to gauge
//! how much a result depends on trade order or catching every trade.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use sqx_backtest::Trade;

/// Summary of a Monte Carlo trade-manipulation experiment.
#[derive(Debug, Clone)]
pub struct MonteCarloReport {
    pub runs: usize,
    pub net_profit_5pct: f64,
    pub net_profit_50pct: f64,
    pub net_profit_95pct: f64,
    pub worst_net_profit: f64,
    pub best_net_profit: f64,
    pub losing_runs_pct: f64,
    pub max_drawdown_95pct: f64,
}

/// Run Monte Carlo trade-manipulation over the given trades.
///
/// Each run shuffles the trade order and drops each trade with probability
/// skip_prob, then records the net profit and max drawdown of the remaining
/// sequence.
pub fn monte_carlo_trade_manipulation(
    trades: &[Trade],
    runs: usize,
    skip_prob: f64,
    seed: u64,
) -> MonteCarloReport {
    if trades.is_empty() || runs == 0 {
        return MonteCarloReport {
            runs, net_profit_5pct: f64::NAN, net_profit_50pct: f64::NAN,
            net_profit_95pct: f64::NAN, worst_net_profit: f64::NAN, best_net_profit: f64::NAN,
            losing_runs_pct: f64::NAN, max_drawdown_95pct: f64::NAN,
        };
    }
    let mut rng = StdRng::seed_from_u64(seed);
    let keep = (1.0 - skip_prob).clamp(0.0, 1.0);
    let mut nets = Vec::with_capacity(runs);
    let mut dds = Vec::with_capacity(runs);
    for _ in 0..runs {
        let mut seq: Vec<f64> = trades.iter().filter(|_| rng.gen_bool(keep)).map(|t| t.pnl).collect();
        seq.shuffle(&mut rng);
        let net: f64 = seq.iter().sum();
        nets.push(net);
        dds.push(max_drawdown(&seq));
    }
    nets.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    dds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let pct = |v: &[f64], p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    let losing = nets.iter().filter(|n| **n < 0.0).count() as f64 / runs as f64 * 100.0;

    MonteCarloReport {
        runs,
        net_profit_5pct: pct(&nets, 0.05),
        net_profit_50pct: pct(&nets, 0.50),
        net_profit_95pct: pct(&nets, 0.95),
        worst_net_profit: *nets.first().unwrap(),
        best_net_profit: *nets.last().unwrap(),
        losing_runs_pct: losing,
        max_drawdown_95pct: pct(&dds, 0.95),
    }
}

/// Max drawdown of an equity path starting at zero from a pnl sequence.
fn max_drawdown(pnls: &[f64]) -> f64 {
    let mut eq = 0.0;
    let mut peak = 0.0;
    let mut dd = 0.0;
    for p in pnls {
        eq += p;
        if eq > peak { peak = eq; }
        let cur = peak - eq;
        if cur > dd { dd = cur; }
    }
    dd
}

/// Render a Monte Carlo report as text.
pub fn render(r: &MonteCarloReport) -> String {
    let mut s = String::new();
    s.push_str(&format!("Monte Carlo trade-manipulation ({} runs)\n", r.runs));
    s.push_str(&format!("  Net profit  5th pct : {:.2}\n", r.net_profit_5pct));
    s.push_str(&format!("  Net profit 50th pct : {:.2}\n", r.net_profit_50pct));
    s.push_str(&format!("  Net profit 95th pct : {:.2}\n", r.net_profit_95pct));
    s.push_str(&format!("  Worst net profit    : {:.2}\n", r.worst_net_profit));
    s.push_str(&format!("  Best net profit     : {:.2}\n", r.best_net_profit));
    s.push_str(&format!("  Losing runs         : {:.1}%\n", r.losing_runs_pct));
    s.push_str(&format!("  Max DD (95th pct)   : {:.2}\n", r.max_drawdown_95pct));
    s
}
