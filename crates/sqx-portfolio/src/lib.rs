//! # sqx-portfolio
//!
//! Portfolio construction: backtest a set of strategies, combine their equity
//! curves with weights, report portfolio metrics and the pairwise correlation
//! of component returns, and search for the best subset by return/drawdown.

use sqx_backtest::{backtest, BacktestConfig, BacktestResult, Trade};
use sqx_core::{Bar, Instrument};
use sqx_metrics::{compute, Metrics};
use sqx_strategy::Strategy;

/// One component of a portfolio.
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    pub name: String,
    pub weight: f64,
    pub net_profit: f64,
    pub max_drawdown_pct: f64,
    pub sharpe: f64,
    pub trades: usize,
}

/// The combined portfolio result.
#[derive(Debug, Clone)]
pub struct PortfolioResult {
    pub metrics: Metrics,
    pub equity: Vec<(i64, f64)>,
    /// Pairwise Pearson correlation of component equity returns.
    pub correlation: Vec<Vec<f64>>,
    pub components: Vec<ComponentInfo>,
}

/// Backtest every strategy on the same bars.
pub fn backtest_all(
    strategies: &[Strategy],
    bars: &[Bar],
    instrument: &Instrument,
    config: &BacktestConfig,
) -> Vec<BacktestResult> {
    strategies.iter().map(|s| backtest(s, bars, instrument, config)).collect()
}

/// Equal weights (normalised).
pub fn equal_weights(n: usize) -> Vec<f64> {
    if n == 0 { return Vec::new(); }
    let w = 1.0 / n as f64;
    vec![w; n]
}

/// Combine backtest results with the given weights into one portfolio.
pub fn combine(
    names: &[String],
    results: &[BacktestResult],
    weights: &[f64],
    initial: f64,
    bar_count: usize,
    periods_per_year: f64,
) -> PortfolioResult {
    let n = results.len().min(weights.len());
    let len = results.iter().map(|r| r.equity.len()).min().unwrap_or(0);

    // Combined equity = initial + sum(w_i * (equity_i - initial)).
    let mut equity: Vec<(i64, f64)> = Vec::with_capacity(len);
    for i in 0..len {
        let t = results[0].equity[i].0;
        let mut val = initial;
        for j in 0..n {
            val += weights[j] * (results[j].equity[i].1 - initial);
        }
        equity.push((t, val));
    }

    // Weighted trades, re-time-sorted.
    let mut trades: Vec<Trade> = Vec::new();
    for j in 0..n {
        for tr in &results[j].trades {
            let mut t = tr.clone();
            t.pnl *= weights[j];
            t.commission *= weights[j];
            trades.push(t);
        }
    }
    trades.sort_by_key(|t| t.close_time);

    let final_equity = equity.last().map(|(_, e)| *e).unwrap_or(initial);
    let combined = BacktestResult {
        trades,
        equity: equity.clone(),
        initial_deposit: initial,
        final_equity,
    };
    let metrics = compute(&combined, bar_count, periods_per_year);

    // Correlation of bar-to-bar returns.
    let rets: Vec<Vec<f64>> = results.iter().map(|r| returns(&r.equity)).collect();
    let mut correlation = vec![vec![f64::NAN; n]; n];
    for i in 0..n {
        correlation[i][i] = 1.0;
        for j in (i + 1)..n {
            let c = pearson(&rets[i], &rets[j]);
            correlation[i][j] = c;
            correlation[j][i] = c;
        }
    }

    let mut components = Vec::with_capacity(n);
    for j in 0..n {
        let m = compute(&results[j], bar_count, periods_per_year);
        components.push(ComponentInfo {
            name: names[j].clone(),
            weight: weights[j],
            net_profit: m.net_profit,
            max_drawdown_pct: m.max_drawdown_pct,
            sharpe: m.sharpe_ratio,
            trades: results[j].trades.len(),
        });
    }

    PortfolioResult { metrics, equity, correlation, components }
}

/// Search for the subset (size min_k..=max_k) with the best return/drawdown.
/// Only the first 12 results are considered to bound the search.
pub fn best_subset(results: &[BacktestResult], min_k: usize, max_k: usize, initial: f64) -> Vec<usize> {
    let n = results.len().min(12);
    if n == 0 { return Vec::new(); }
    let max_k = max_k.min(n);
    let min_k = min_k.max(2).min(max_k);
    let mut best_score = f64::NEG_INFINITY;
    let mut best: Vec<usize> = (0..n.min(3)).collect();
    for k in min_k..=max_k {
        for combo in gen_combos(n, k) {
            let w = 1.0 / combo.len() as f64;
            let (np, dd) = combo_stats(results, &combo, w, initial);
            let score = if dd > 0.0 { np / dd } else if np > 0.0 { f64::INFINITY } else { f64::NEG_INFINITY };
            if score > best_score {
                best_score = score;
                best = combo;
            }
        }
    }
    best
}

fn combo_stats(results: &[BacktestResult], combo: &[usize], w: f64, initial: f64) -> (f64, f64) {
    let len = combo.iter().map(|&j| results[j].equity.len()).min().unwrap_or(0);
    let mut peak = initial;
    let mut max_dd = 0.0;
    let mut last = initial;
    for i in 0..len {
        let mut val = initial;
        for &j in combo {
            val += w * (results[j].equity[i].1 - initial);
        }
        if val > peak { peak = val; }
        let dd = peak - val;
        if dd > max_dd { max_dd = dd; }
        last = val;
    }
    (last - initial, max_dd)
}

fn returns(equity: &[(i64, f64)]) -> Vec<f64> {
    let mut out = Vec::with_capacity(equity.len());
    for wd in equity.windows(2) {
        if wd[0].1 > 0.0 {
            out.push((wd[1].1 - wd[0].1) / wd[0].1);
        } else {
            out.push(0.0);
        }
    }
    out
}

fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    if n < 2 { return f64::NAN; }
    let ma = a[..n].iter().sum::<f64>() / n as f64;
    let mb = b[..n].iter().sum::<f64>() / n as f64;
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for i in 0..n {
        cov += (a[i] - ma) * (b[i] - mb);
        va += (a[i] - ma).powi(2);
        vb += (b[i] - mb).powi(2);
    }
    if va == 0.0 || vb == 0.0 { f64::NAN } else { cov / (va.sqrt() * vb.sqrt()) }
}

fn gen_combos(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut result = Vec::new();
    let mut current = Vec::new();
    fn rec(start: usize, n: usize, k: usize, cur: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if cur.len() == k {
            out.push(cur.clone());
            return;
        }
        for i in start..n {
            cur.push(i);
            rec(i + 1, n, k, cur, out);
            cur.pop();
        }
    }
    rec(0, n, k, &mut current, &mut result);
    result
}
