//! # sqx-portfolio
//!
//! Portfolio construction: backtest a set of strategies, combine their equity
//! curves with weights, report portfolio metrics and the pairwise correlation
//! of component returns, and search for the best subset by return/drawdown.

mod search;
pub use search::*;

use sqx_backtest::{backtest, BacktestConfig, BacktestResult, Trade};
use sqx_core::{Bar, Instrument};
use sqx_metrics::{compute, Metrics};
use sqx_strategy::Strategy;

/// One component of a portfolio.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentInfo {
    pub name: String,
    pub weight: f64,
    pub net_profit: f64,
    pub max_drawdown_pct: f64,
    pub sharpe: f64,
    pub trades: usize,
}

/// The combined portfolio result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PortfolioResult {
    pub account_currency: Option<String>,
    pub errors: Vec<String>,
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

fn validate_components(results:&[BacktestResult],initial:f64)->Result<(),String> {
    if !initial.is_finite() || initial<=0.0{return Err("Portfolio deposit must be finite and positive".into());}
    let first=results.first().ok_or("Portfolio requires at least one component")?;
    let currency=first.execution.account_currency.as_deref().filter(|c|c.len()==3 && c.bytes().all(|b|b.is_ascii_uppercase())).ok_or("Portfolio account currency is unknown or invalid")?;
    if first.equity.is_empty(){return Err("Portfolio components require equity samples".into());}
    for (index,result) in results.iter().enumerate(){
        if result.execution.account_currency.as_deref()!=Some(currency){return Err(format!("Component {index} has a different or unknown account currency; explicit conversion is required"));}
        if !result.errors.is_empty(){return Err(format!("Invalid component {index}: {}",result.errors.join("; ")));}
        if result.initial_deposit!=initial{return Err(format!("Component {index} has a different initial deposit"));}
        if result.equity.len()!=first.equity.len() || result.equity.iter().zip(&first.equity).any(|(a,b)|a.0!=b.0){return Err(format!("Component {index} equity timestamps are not aligned"));}
        if result.equity.windows(2).any(|pair|pair[0].0>=pair[1].0) || result.equity.iter().any(|(_,value)|!value.is_finite()) || !result.final_equity.is_finite(){return Err(format!("Component {index} has invalid equity samples"));}
        if result.trades.iter().any(|trade|!trade.pnl.is_finite()||!trade.commission.is_finite()||!trade.financing.is_finite()){return Err(format!("Component {index} has nonfinite trade values"));}
    }
    Ok(())
}

fn invalid_portfolio(error:String,initial:f64,bar_count:usize,periods_per_year:f64)->PortfolioResult {
    let errors=vec![error];
    let run=BacktestResult{errors:errors.clone(),rejected_orders:vec![],ambiguous_bars:vec![],execution:Default::default(),trades:vec![],equity:vec![],initial_deposit:initial,final_equity:f64::NAN};
    PortfolioResult{account_currency:None,errors,metrics:compute(&run,bar_count,periods_per_year),equity:vec![],correlation:vec![],components:vec![]}
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
    let validation=validate_components(results,initial).and_then(|()| {
        if names.len()!=results.len() || weights.len()!=results.len(){return Err("Names, weights and components must have identical lengths".into());}
        if weights.iter().any(|w|!w.is_finite()||*w<0.0) || (weights.iter().sum::<f64>()-1.0).abs()>1e-9{return Err("Weights must be finite, nonnegative and sum to one".into());}
        Ok(())
    });
    if let Err(error)=validation{return invalid_portfolio(error,initial,bar_count,periods_per_year);}
    let n = results.len();
    let len = results[0].equity.len();

    // Combined equity = initial + sum(w_i * (equity_i - initial)).
    let mut equity: Vec<(i64, f64)> = Vec::with_capacity(len);
    for i in 0..len {
        let t = results[0].equity[i].0;
        let mut val = initial;
        for j in 0..n {
            val += weights[j] * (results[j].equity[i].1 - initial);
        }
        if !val.is_finite(){return invalid_portfolio(format!("Combined equity is nonfinite at sample {i}"),initial,bar_count,periods_per_year);}
        equity.push((t, val));
    }

    // Weighted trades, re-time-sorted.
    let mut trades: Vec<Trade> = Vec::new();
    for j in 0..n {
        for tr in &results[j].trades {
            let mut t = tr.clone();
            t.pnl *= weights[j];
            t.commission *= weights[j];
            t.financing *= weights[j];
            trades.push(t);
        }
    }
    trades.sort_by_key(|t| t.close_time);

    let final_equity = equity.last().map(|(_, e)| *e).unwrap_or(initial);
    let combined = BacktestResult {
        errors: results.iter().flat_map(|r| r.errors.clone()).collect(),
        rejected_orders: results.iter().flat_map(|r| r.rejected_orders.clone()).collect(),
        ambiguous_bars: results.iter().flat_map(|r| r.ambiguous_bars.clone()).collect(),
        execution: sqx_backtest::ExecutionEvidence {account_currency:results[0].execution.account_currency.clone(),mode:"weighted_independent_portfolio".into(),..Default::default()},
        trades,
        equity: equity.clone(),
        initial_deposit: initial,
        final_equity,
    };
    let metrics = compute(&combined, bar_count, periods_per_year);

    // Include the initial-deposit -> first-sample return. A window can incur
    // spread/commission on its very first sample; dropping it hides exposure.
    let rets: Vec<Vec<f64>> = results.iter().map(|r| returns(&r.equity,r.initial_deposit)).collect();
    let mut correlation = vec![vec![f64::NAN; n]; n];
    for i in 0..n {
        correlation[i][i] = pearson(&rets[i],&rets[i]);
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

    PortfolioResult { account_currency:results[0].execution.account_currency.clone(),errors:Vec::new(), metrics, equity, correlation, components }
}

/// Serializable score conventions: zero-drawdown cases are explicit, not JSON null.
#[derive(Debug,Clone,PartialEq,serde::Serialize)]
pub enum SubsetScore { Ratio(f64), PositiveNoDrawdown, NonpositiveNoDrawdown, Unavailable }

#[derive(Debug,Clone,serde::Serialize)]
pub struct SubsetSearchReport {
    pub selected:Vec<usize>,
    pub considered_components:usize,
    pub attempted:usize,
    pub invalid:usize,
    pub errors:Vec<String>,
}

#[derive(Debug,serde::Serialize)]
#[serde(tag="kind",rename_all="snake_case")]
pub enum SubsetEvent<'a>{
    Started{index:usize,components:&'a [usize]},
    Outcome{index:usize,components:&'a [usize],net_profit:Option<f64>,max_drawdown:Option<f64>,score:SubsetScore,errors:&'a [String]},
}

/// Search for the subset (size min_k..=max_k) with the best return/drawdown.
/// Only the first 12 results are considered to bound the search.
pub fn best_subset(results:&[BacktestResult],min_k:usize,max_k:usize,initial:f64)->Vec<usize>{
    best_subset_observed(results,min_k,max_k,initial,&mut |_|Ok(())).expect("infallible subset observer").selected
}

/// Freeze each ordered subset before calculation and report its outcome before
/// selecting a winner. A failed observer stops the search immediately.
pub fn best_subset_observed(results:&[BacktestResult],min_k:usize,max_k:usize,initial:f64,
    observer:&mut impl FnMut(SubsetEvent<'_>)->Result<(),String>)->Result<SubsetSearchReport,String>{
    let n=results.len().min(12);
    let mut report=SubsetSearchReport{selected:Vec::new(),considered_components:n,attempted:0,invalid:0,errors:Vec::new()};
    if n==0 || min_k==0 || min_k>max_k || min_k>n{
        report.errors.push("Subset sizes require 1 <= minimum <= maximum and enough considered components".into());return Ok(report);
    }
    if let Err(error)=validate_components(results,initial){report.errors.push(error);return Ok(report);}
    let mut best_score=f64::NEG_INFINITY;
    for k in min_k..=max_k.min(n){
        for combo in gen_combos(n,k){
            let index=report.attempted;observer(SubsetEvent::Started{index,components:&combo})?;
            let (np,dd)=combo_stats(results,&combo,1.0/combo.len() as f64,initial);
            let mut errors=Vec::new();
            let score=if !np.is_finite() || !dd.is_finite() || dd<0.0 || (dd>0.0 && !(np/dd).is_finite()){
                errors.push(format!("Subset {index} has nonfinite profit, drawdown or ratio"));SubsetScore::Unavailable
            }else if dd>0.0{SubsetScore::Ratio(np/dd)}else if np>0.0{SubsetScore::PositiveNoDrawdown}else{SubsetScore::NonpositiveNoDrawdown};
            observer(SubsetEvent::Outcome{index,components:&combo,net_profit:np.is_finite().then_some(np),max_drawdown:dd.is_finite().then_some(dd),score:score.clone(),errors:&errors})?;
            report.attempted+=1;
            if !errors.is_empty(){report.invalid+=1;report.errors.extend(errors);continue;}
            let rank=match score{SubsetScore::Ratio(value)=>value,SubsetScore::PositiveNoDrawdown=>f64::INFINITY,SubsetScore::NonpositiveNoDrawdown=>f64::NEG_INFINITY,SubsetScore::Unavailable=>unreachable!()};
            if report.selected.is_empty() || rank>best_score{best_score=rank;report.selected=combo;}
        }
    }
    // Never silently select around an invalid combination.
    if !report.errors.is_empty(){report.selected.clear();}
    Ok(report)
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
        if !val.is_finite(){return (f64::NAN,f64::NAN);}
        if val > peak { peak = val; }
        let dd = peak - val;
        if dd > max_dd { max_dd = dd; }
        last = val;
    }
    (last - initial, max_dd)
}

fn returns(equity: &[(i64, f64)], initial:f64) -> Vec<f64> {
    let mut previous=initial;
    equity.iter().map(|(_,value)| {
        let change=if previous>0.0 {(value-previous)/previous} else {f64::NAN};
        previous=*value;
        change
    }).collect()
}

fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len();
    if n < 2 || b.len()!=n || a.iter().chain(b).any(|value|!value.is_finite()) { return f64::NAN; }
    let ma = a[..n].iter().sum::<f64>() / n as f64;
    let mb = b[..n].iter().sum::<f64>() / n as f64;
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for i in 0..n {
        cov += (a[i] - ma) * (b[i] - mb);
        va += (a[i] - ma).powi(2);
        vb += (b[i] - mb).powi(2);
    }
    if va == 0.0 || vb == 0.0 { return f64::NAN; }
    let correlation=cov / (va.sqrt() * vb.sqrt());
    if correlation.is_finite(){correlation.clamp(-1.0,1.0)}else{f64::NAN}
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
