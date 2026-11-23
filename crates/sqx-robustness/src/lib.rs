//! # sqx-robustness
//!
//! Robustness tests mirroring StrategyQuant's Retester: Monte Carlo
//! trade-manipulation (reshuffling order and randomly skipping trades) to gauge
//! how much a result depends on trade order or catching every trade.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use sqx_backtest::{backtest, backtest_window, BacktestConfig, Trade};
use sqx_core::{Bar, Instrument};
use sqx_strategy::{ProfitTarget, StopLoss, Strategy};

/// One what-if scenario: re-run the backtest with different costs.
#[derive(Debug, Clone)]
pub struct WhatIfRow {
    pub errors: Vec<String>,
    pub spread: f64,
    pub slippage: f64,
    pub commission: f64,
    pub net_profit: f64,
    pub profit_factor: f64,
}

/// Re-run the strategy under a set of cost scenarios (spread, slippage, commission).
pub fn what_if(
    strategy: &Strategy,
    bars: &[Bar],
    instrument: &Instrument,
    base: &BacktestConfig,
    scenarios: &[(f64, f64, f64)],
) -> Vec<WhatIfRow> {
    let mut out = Vec::new();
    for &(spread, slippage, commission) in scenarios {
        let cfg = BacktestConfig {
            spread_points: spread,
            slippage_points: slippage,
            commission_per_lot: commission,
            ..base.clone()
        };
        let r = backtest(strategy, bars, instrument, &cfg);
        out.push(summarize_cost_scenario(&r,spread,slippage,commission));
    }
    out
}

fn summarize_cost_scenario(result:&sqx_backtest::BacktestResult,spread:f64,slippage:f64,commission:f64)->WhatIfRow {
    let mut errors=result.errors.clone();
    if let Err(error)=sqx_metrics::validate_observations(result){errors.push(error);}
    let np:f64=result.trades.iter().map(|trade|trade.pnl).sum();
    if !np.is_finite(){errors.push("Net trade PNL aggregation overflow".into());}
    if !errors.is_empty(){return WhatIfRow{spread,slippage,commission,net_profit:f64::NAN,profit_factor:f64::NAN,errors};}
    let gp:f64=result.trades.iter().filter(|trade|trade.pnl>0.0).map(|trade|trade.pnl).sum();
    let gl:f64=result.trades.iter().filter(|trade|trade.pnl<0.0).map(|trade|trade.pnl).sum();
    let pf=if gl!=0.0{gp / -gl}else if gp>0.0{f64::INFINITY}else{f64::NAN};
    WhatIfRow{spread,slippage,commission,net_profit:np,profit_factor:pf,errors}
}

/// Distribution summary of a Monte Carlo retest.
#[derive(Debug, Clone)]
pub struct RetestReport {
    /// Seed and sampled offsets identify the experiment, including skipped runs.
    pub seed: u64,
    pub start_offsets: Vec<usize>,
    /// Historical bars warm indicators; no trades or equity precede the offset.
    pub warmup_policy: &'static str,
    pub requested_runs: usize,
    pub skipped_runs: usize,
    pub invalid_runs: usize,
    pub errors: Vec<String>,
    pub runs: usize,
    pub net_profit_5pct: f64,
    pub net_profit_50pct: f64,
    pub net_profit_95pct: f64,
    pub losing_runs_pct: f64,
    pub max_dd_95pct: f64,
}

/// Re-backtest with a random trading start and slight SL/PT jitter.
/// Earlier bars remain available for causal indicator warmup. Each scenario starts
/// flat with the configured deposit; no pre-window positions are carried in.
pub fn monte_carlo_retest(
    strategy: &Strategy,
    bars: &[Bar],
    instrument: &Instrument,
    base: &BacktestConfig,
    runs: usize,
    seed: u64,
) -> RetestReport {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut skipped_runs=0;let mut invalid_runs=0;let mut errors=Vec::new();
    let mut start_offsets=Vec::new();
    let mut nets = Vec::new();
    let mut dds = Vec::new();
    let skip_max = (bars.len() / 10).max(1);
    for _ in 0..runs {
        let skip = rng.gen_range(0..skip_max);
        start_offsets.push(skip);
        if bars.len()-skip < 50 {
            skipped_runs+=1;
            continue;
        }
        let mut s = strategy.clone();
        if let Some(sl) = s.stop_loss {
            s.stop_loss = Some(jitter_sl(sl, &mut rng));
        }
        if let Some(pt) = s.profit_target {
            s.profit_target = Some(jitter_pt(pt, &mut rng));
        }
        let r = backtest_window(&s, bars, skip, instrument, base);
        let mut run_errors=r.errors.clone();
        if let Err(error)=sqx_metrics::validate_observations(&r){run_errors.push(error);}
        if !run_errors.is_empty() {
            invalid_runs+=1;
            for error in run_errors { if !errors.contains(&error) {errors.push(error);} }
            continue;
        }
        let net=r.trades.iter().map(|t| t.pnl).sum::<f64>();
        let drawdown=max_drawdown_equity(&r.equity, base.initial_deposit);
        if !net.is_finite() || !drawdown.is_finite() || !r.final_equity.is_finite() {
            invalid_runs+=1;
            let error="Retest produced nonfinite accounting or drawdown".to_string();
            if !errors.contains(&error){errors.push(error);}
            continue;
        }
        nets.push(net);
        dds.push(drawdown);
    }
    nets.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    dds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pct = |v: &[f64], p: f64| {
        if invalid_runs>0 || skipped_runs>0 || v.is_empty() {
            f64::NAN
        } else {
            v[((v.len() - 1) as f64 * p).round() as usize]
        }
    };
    let runs_n = nets.len();
    let losing = if runs_n > 0 && invalid_runs==0 && skipped_runs==0 {
        nets.iter().filter(|n| **n < 0.0).count() as f64 / runs_n as f64 * 100.0
    } else {
        f64::NAN
    };
    if skipped_runs>0 {errors.push(format!("Incomplete retest coverage: {skipped_runs} of {runs} sampled windows have fewer than 50 bars; distribution unavailable"));}
    if runs_n==0 && errors.is_empty() {errors.push("No retest runs could be evaluated; at least 50 bars per sampled window are required".into());}
    RetestReport {
        seed,start_offsets,warmup_policy:"historical prefix; flat at trading start; previous-close signal may fill at start open",
        requested_runs:runs,skipped_runs,invalid_runs,errors,
        runs: runs_n,
        net_profit_5pct: pct(&nets, 0.05),
        net_profit_50pct: pct(&nets, 0.50),
        net_profit_95pct: pct(&nets, 0.95),
        losing_runs_pct: losing,
        max_dd_95pct: pct(&dds, 0.95),
    }
}

fn jitter_sl(sl: StopLoss, rng: &mut StdRng) -> StopLoss {
    let f = rng.gen_range(0.9..1.1);
    match sl {
        StopLoss::Points(p) => StopLoss::Points(p * f),
        StopLoss::Atr { period, mult } => StopLoss::Atr { period, mult: mult * f },
    }
}

fn jitter_pt(pt: ProfitTarget, rng: &mut StdRng) -> ProfitTarget {
    let f = rng.gen_range(0.9..1.1);
    match pt {
        ProfitTarget::Points(p) => ProfitTarget::Points(p * f),
        ProfitTarget::Atr { period, mult } => ProfitTarget::Atr { period, mult: mult * f },
    }
}

fn max_drawdown_equity(equity: &[(i64, f64)], initial: f64) -> f64 {
    if !initial.is_finite(){return f64::NAN;}
    let mut peak = initial;
    let mut dd = 0.0;
    for (_, e) in equity {
        if !e.is_finite(){return f64::NAN;}
        if *e > peak {
            peak = *e;
        }
        let d = peak - *e;
        if d > dd {
            dd = d;
        }
    }
    dd
}

/// One resampled path, in generation order (zero-based run index).
#[derive(Debug, Clone, PartialEq)]
pub struct MonteCarloSample {
    pub run_index: usize,
    pub included_trades: usize,
    pub net_profit: f64,
    pub max_drawdown: f64,
}

/// Summary of a Monte Carlo trade-manipulation experiment.
#[derive(Debug, Clone)]
pub struct MonteCarloReport {
    pub samples: Vec<MonteCarloSample>,
    pub errors: Vec<String>,
    pub runs: usize,
    pub net_profit_5pct: f64,
    pub net_profit_50pct: f64,
    pub net_profit_95pct: f64,
    pub worst_net_profit: f64,
    pub best_net_profit: f64,
    pub losing_runs_pct: f64,
    pub positive_runs_pct: f64,
    pub flat_runs_pct: f64,
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
    let mut errors=Vec::new();
    if trades.is_empty(){errors.push("No trades available for Monte Carlo resampling".into());}
    if runs==0{errors.push("Monte Carlo requires at least one run".into());}
    if !skip_prob.is_finite() || !(0.0..=1.0).contains(&skip_prob){errors.push("skip_prob must be finite and between zero and one".into());}
    if trades.iter().any(|trade| !trade.pnl.is_finite()){errors.push("Trade PnL must be finite".into());}
    if !errors.is_empty() { return invalid_monte_carlo(errors); }
    let mut rng = StdRng::seed_from_u64(seed);
    let keep = (1.0 - skip_prob).clamp(0.0, 1.0);
    let mut nets = Vec::with_capacity(runs);
    let mut dds = Vec::with_capacity(runs);
    let mut samples = Vec::with_capacity(runs);
    for run_index in 0..runs {
        let mut seq: Vec<f64> = trades.iter().filter(|_| rng.gen_bool(keep)).map(|t| t.pnl).collect();
        seq.shuffle(&mut rng);
        let net: f64 = seq.iter().sum();
        let drawdown=max_drawdown(&seq);
        if !net.is_finite() || !drawdown.is_finite(){return invalid_monte_carlo(vec!["Monte Carlo arithmetic overflow; no valid distribution produced".into()]);}
        samples.push(MonteCarloSample{run_index,included_trades:seq.len(),net_profit:net,max_drawdown:drawdown});
        nets.push(net);
        dds.push(drawdown);
    }
    nets.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    dds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let pct = |v: &[f64], p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    let losing = nets.iter().filter(|n| **n < 0.0).count() as f64 / runs as f64 * 100.0;

    MonteCarloReport {
        samples,errors:Vec::new(),runs,
        net_profit_5pct: pct(&nets, 0.05),
        net_profit_50pct: pct(&nets, 0.50),
        net_profit_95pct: pct(&nets, 0.95),
        worst_net_profit: *nets.first().unwrap(),
        best_net_profit: *nets.last().unwrap(),
        losing_runs_pct: losing,
        positive_runs_pct:nets.iter().filter(|n|**n>0.0).count() as f64/runs as f64*100.0,
        flat_runs_pct:nets.iter().filter(|n|**n==0.0).count() as f64/runs as f64*100.0,
        max_drawdown_95pct: pct(&dds, 0.95),
    }
}

fn invalid_monte_carlo(errors:Vec<String>)->MonteCarloReport {
    MonteCarloReport{samples:Vec::new(),errors,runs:0,net_profit_5pct:f64::NAN,net_profit_50pct:f64::NAN,net_profit_95pct:f64::NAN,
        worst_net_profit:f64::NAN,best_net_profit:f64::NAN,losing_runs_pct:f64::NAN,positive_runs_pct:f64::NAN,
        flat_runs_pct:f64::NAN,max_drawdown_95pct:f64::NAN}
}

impl MonteCarloReport {
    /// Strictly profitable runs only. Zero-PnL/skipped-all paths are not wins.
    pub fn meets_positive_threshold(&self,minimum:f64)->bool {
        self.errors.is_empty() && self.runs>0 && minimum.is_finite() && (0.0..=100.0).contains(&minimum)
            && self.positive_runs_pct.is_finite() && self.positive_runs_pct>=minimum
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
    if !r.errors.is_empty(){return format!("Monte Carlo unavailable: {}",r.errors.join("; "));}
    s.push_str(&format!("Monte Carlo trade-manipulation ({} runs)\n", r.runs));
    s.push_str(&format!("  Net profit  5th pct : {:.2}\n", r.net_profit_5pct));
    s.push_str(&format!("  Net profit 50th pct : {:.2}\n", r.net_profit_50pct));
    s.push_str(&format!("  Net profit 95th pct : {:.2}\n", r.net_profit_95pct));
    s.push_str(&format!("  Worst net profit    : {:.2}\n", r.worst_net_profit));
    s.push_str(&format!("  Best net profit     : {:.2}\n", r.best_net_profit));
    s.push_str(&format!("  Positive runs       : {:.1}%\n", r.positive_runs_pct));
    s.push_str(&format!("  Flat runs           : {:.1}%\n", r.flat_runs_pct));
    s.push_str(&format!("  Losing runs         : {:.1}%\n", r.losing_runs_pct));
    s.push_str(&format!("  Max DD (95th pct)   : {:.2}\n", r.max_drawdown_95pct));
    s
}

#[cfg(test)]
mod accounting_tests {
    use super::max_drawdown_equity;
    #[test]
    fn cost_scenarios_reject_invalid_observations_and_aggregation(){
        let bars=vec![sqx_core::Bar::new(0,1.0,1.1,0.9,1.0,1.0),sqx_core::Bar::new(60000,1.0,1.1,0.9,1.0,1.0)];
        let valid=sqx_backtest::backtest(&sqx_strategy::Strategy::new("fixture"),&bars,&sqx_core::Instrument::fx("EURUSD"),&sqx_backtest::BacktestConfig::default());
        assert!(valid.errors.is_empty());
        let row=super::summarize_cost_scenario(&valid,0.0,0.0,0.0);assert!(row.errors.is_empty());assert_eq!(row.net_profit,0.0);assert!(row.profit_factor.is_nan());
        let mut invalid=valid.clone();invalid.equity[0].1=f64::NAN;
        let row=super::summarize_cost_scenario(&invalid,10.0,5.0,7.0);assert!(!row.errors.is_empty());assert!(row.net_profit.is_nan());assert!(row.profit_factor.is_nan());assert_eq!((row.spread,row.slippage,row.commission),(10.0,5.0,7.0));
        let trade=sqx_backtest::Trade{id:0,direction:sqx_core::Direction::Long,open_time:0,close_time:60000,open_price:1.0,close_price:1.1,size:1.0,pnl:f64::MAX,pnl_points:1.0,commission:0.0,financing:0.0,bars:1,mae_points:0.0,mfe_points:1.0,exit_reason:sqx_backtest::ExitReason::EndOfData};
        let mut overflow=valid.clone();overflow.trades=vec![trade.clone(),trade];
        let row=super::summarize_cost_scenario(&overflow,0.0,0.0,0.0);assert!(row.errors.iter().any(|e|e.contains("overflow")));assert!(row.net_profit.is_nan());
        let mut failed=valid;failed.errors.push("execution failed".into());
        assert!(super::summarize_cost_scenario(&failed,0.0,0.0,0.0).errors.contains(&"execution failed".to_string()));
    }
    #[test]
    fn equity_drawdown_preserves_initial_loss_and_rejects_invalid_observations(){
        assert_eq!(max_drawdown_equity(&[(0,90.0),(1,110.0),(2,80.0)],100.0),30.0);
        for invalid in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY]{
            assert!(max_drawdown_equity(&[(0,invalid),(1,100.0)],100.0).is_nan());
            assert!(max_drawdown_equity(&[(0,100.0)],invalid).is_nan());
        }
        assert!(!max_drawdown_equity(&[(0,-f64::MAX)],f64::MAX).is_finite());
    }
}
