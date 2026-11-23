//! Segmented walk-forward: select on IS only, warm from past data, trade OOS.
use rand::{rngs::StdRng, SeedableRng};
use sqx_backtest::{backtest_window, BacktestConfig, BacktestResult};
use sqx_core::{Bar, Instrument};
use sqx_metrics::compute as compute_metrics;
use sqx_strategy::Strategy;
use crate::{random_strategy, rank, Fitness, GeneratorConfig};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WalkForwardCriteria {
    pub min_is_trades: usize,
    pub min_oos_trades_per_window: usize,
}
impl Default for WalkForwardCriteria {
    fn default()->Self{Self{min_is_trades:10,min_oos_trades_per_window:10}}
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct CandidateFailure {
    pub candidate_index: usize,
    pub strategy: Strategy,
    pub errors: Vec<String>,
}
/// Every in-sample candidate, before eligibility filtering or selection.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WalkForwardCandidate {
    pub candidate_index: usize,
    pub strategy: Strategy,
    pub metrics: Option<sqx_metrics::Metrics>,
    pub errors: Vec<String>,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub selected: bool,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct WalkForwardWindow {
    pub index: usize,
    pub train_start: i64,
    pub test_start: i64,
    pub test_end: i64,
    pub candidates_attempted: usize,
    pub candidate_failures: Vec<CandidateFailure>,
    pub candidate_evaluations: Vec<WalkForwardCandidate>,
    pub selected_candidate_index: Option<usize>,
    pub selected_strategy: Option<Strategy>,
    pub starting_equity: f64,
    pub final_equity: f64,
    pub oos_trades: usize,
    pub evidence_sufficient: bool,
    pub reason: String,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct WalkForwardReport {
    pub windows: usize,
    pub oos_trades: usize,
    pub oos_net_profit: f64,
    pub oos_profit_factor: f64,
    pub oos_winning_pct: f64,
    pub oos_max_dd_pct: f64,
    pub criteria: WalkForwardCriteria,
    pub window_results: Vec<WalkForwardWindow>,
    pub candidates_attempted: usize,
    pub evidence_sufficient: bool,
    pub errors: Vec<String>,
    pub seed: u64,
}
#[allow(clippy::too_many_arguments)]
pub fn walk_forward(bars:&[Bar], instrument:&Instrument, config:&BacktestConfig, periods_per_year:f64,
    gen_config:&GeneratorConfig, windows:usize, oos_fraction:f64, candidates_per_window:usize, seed:u64)->WalkForwardReport {
    walk_forward_with_criteria(bars,instrument,config,periods_per_year,gen_config,windows,oos_fraction,candidates_per_window,seed,&WalkForwardCriteria::default())
}

#[allow(clippy::too_many_arguments)]
pub fn walk_forward_with_criteria(bars:&[Bar], instrument:&Instrument, config:&BacktestConfig, periods_per_year:f64,
    gen_config:&GeneratorConfig, windows:usize, oos_fraction:f64, candidates_per_window:usize, seed:u64, criteria:&WalkForwardCriteria)->WalkForwardReport {
    walk_forward_impl(bars,instrument,config,periods_per_year,gen_config,windows,oos_fraction,candidates_per_window,seed,criteria,None)
}

/// Select only among the supplied, already-frozen candidate family. No unrelated
/// rule trees are generated. The same family is evaluated separately in every IS window.
#[allow(clippy::too_many_arguments)]
pub fn walk_forward_candidates(bars:&[Bar],instrument:&Instrument,config:&BacktestConfig,periods_per_year:f64,
    candidates:&[Strategy],windows:usize,oos_fraction:f64,seed:u64,criteria:&WalkForwardCriteria)->WalkForwardReport {
    walk_forward_impl(bars,instrument,config,periods_per_year,&GeneratorConfig::default(),windows,oos_fraction,candidates.len(),seed,criteria,Some(candidates))
}

#[allow(clippy::too_many_arguments)]
fn walk_forward_impl(bars:&[Bar],instrument:&Instrument,config:&BacktestConfig,periods_per_year:f64,
    gen_config:&GeneratorConfig,windows:usize,oos_fraction:f64,candidates_per_window:usize,seed:u64,criteria:&WalkForwardCriteria,
    frozen_candidates:Option<&[Strategy]>)->WalkForwardReport {
    let mut report=WalkForwardReport{windows:0,oos_trades:0,oos_net_profit:0.0,oos_profit_factor:f64::NAN,
        oos_winning_pct:f64::NAN,oos_max_dd_pct:f64::NAN,criteria:criteria.clone(),window_results:Vec::new(),
        candidates_attempted:0,evidence_sufficient:false,errors:Vec::new(),seed};
    if windows==0 || bars.len()<100 || candidates_per_window==0 || !oos_fraction.is_finite() || !(0.05..=0.5).contains(&oos_fraction)
        || criteria.min_is_trades==0 || criteria.min_oos_trades_per_window==0 {
        report.errors.push("invalid walk-forward sizes, OOS fraction or evidence thresholds".into());return report;
    }
    if let Err(e)=sqx_backtest::validate_backtest_inputs(bars,instrument,config){report.errors.push(e);return report;}
    if let Err(error)=gen_config.validate(){report.errors.push(error);return report;}
    let mut rng=StdRng::seed_from_u64(seed);
    let mut balance=config.initial_deposit;
    let mut all=BacktestResult{errors:Vec::new(),rejected_orders:Vec::new(),ambiguous_bars:Vec::new(),execution:Default::default(),
        trades:Vec::new(),equity:Vec::new(),initial_deposit:balance,final_equity:balance};
    let window_len=bars.len()/windows;
    let mut oos_bar_count=0;
    for w in 0..windows {
        let start=w*window_len;
        let end=if w+1==windows{bars.len()}else{(w+1)*window_len};
        let split=start+((end-start) as f64*(1.0-oos_fraction)).round() as usize;
        if split-start<50 || end-split<10 {report.errors.push(format!("window {w} is too short"));continue;}
        if balance<=0.0 {report.errors.push("account exhausted; remaining windows not traded".into());break;}
        let mut window=WalkForwardWindow{index:w,train_start:bars[start].time,test_start:bars[split].time,test_end:bars[end-1].time,
            candidates_attempted:candidates_per_window,candidate_failures:Vec::new(),candidate_evaluations:Vec::new(),selected_candidate_index:None,selected_strategy:None,starting_equity:balance,final_equity:balance,
            oos_trades:0,evidence_sufficient:false,reason:"no eligible in-sample strategy".into()};
        let candidates:Vec<Strategy>=match frozen_candidates {
            Some(candidates)=>candidates.to_vec(),
            None=>(0..candidates_per_window).map(|i|random_strategy(&mut rng,&format!("w{w}-c{i}"),gen_config)).collect(),
        };
        report.candidates_attempted+=candidates.len();
        // The preceding OOS windows are already known at this selection boundary.
        // Train and test with the same available capital and account constraints.
        let mut window_config=config.clone();window_config.initial_deposit=balance;
        // Only past data is provided; the prefix warms indicators without training trades.
        let mut scored=Vec::new();
        for (candidate_index,s) in candidates.into_iter().enumerate(){
            let r=backtest_window(&s,&bars[..split],start,instrument,&window_config);
            if !r.errors.is_empty(){
                report.errors.push(format!("window {w}, training candidate {candidate_index} ({}): {}",s.name,r.errors.join("; ")));
                window.candidate_evaluations.push(WalkForwardCandidate{candidate_index,strategy:s.clone(),metrics:None,errors:r.errors.clone(),eligible:false,exclusion_reason:Some("execution_failed".into()),selected:false});
                window.candidate_failures.push(CandidateFailure{candidate_index,strategy:s,errors:r.errors});
                continue;
            }
            let m=compute_metrics(&r,split-start,periods_per_year);
            let exclusion_reason=if m.number_of_trades<criteria.min_is_trades as f64 {Some("insufficient_in_sample_trades".into())}
                else if !crate::fitness(&m,Fitness::ProfitFactor,criteria.min_is_trades).is_finite(){Some("nonfinite_profit_factor".into())}else{None};
            window.candidate_evaluations.push(WalkForwardCandidate{candidate_index,strategy:s.clone(),metrics:Some(m),errors:Vec::new(),eligible:exclusion_reason.is_none(),exclusion_reason,selected:false});
            scored.push((s,m));
        }
        let ranked=rank(scored,Fitness::ProfitFactor,criteria.min_is_trades);
        if let Some((best,_))=ranked.first(){
            // Stable ranking keeps the first occurrence for identical strategies.
            // Resolve the original index before recording the selection.
            let selected=window.candidate_evaluations.iter_mut().find(|candidate|candidate.eligible && candidate.strategy==*best).expect("ranked candidate must have evaluation evidence");
            selected.selected=true;window.selected_candidate_index=Some(selected.candidate_index);
            window.selected_strategy=Some(best.clone());
            let r=backtest_window(best,&bars[..end],split,instrument,&window_config);
            if !r.errors.is_empty(){report.errors.extend(r.errors.into_iter().map(|e|format!("window {w}, out-of-sample ({}): {e}",best.name)));window.reason="out-of-sample execution failed".into();}
            else {
                window.oos_trades=r.trades.len();
                window.final_equity=r.final_equity;balance=r.final_equity;
                window.evidence_sufficient=window.candidate_failures.is_empty() && window.oos_trades>=criteria.min_oos_trades_per_window;
                window.reason=if !window.candidate_failures.is_empty(){"training candidate execution failed; selection evidence incomplete"}else if window.evidence_sufficient{"trade-count evidence met; not a robustness certificate"}else{"insufficient out-of-sample trades"}.into();
                all.equity.extend(r.equity);all.trades.extend(r.trades);
                all.rejected_orders.extend(r.rejected_orders);all.ambiguous_bars.extend(r.ambiguous_bars);
                oos_bar_count+=end-split;report.windows+=1;
            }
        }
        report.window_results.push(window);
    }
    all.final_equity=balance;
    for (id,trade) in all.trades.iter_mut().enumerate(){trade.id=id;}
    let m=compute_metrics(&all,oos_bar_count,periods_per_year);
    report.oos_trades=all.trades.len();report.oos_net_profit=m.net_profit;
    report.oos_profit_factor=m.profit_factor;report.oos_winning_pct=m.winning_pct;report.oos_max_dd_pct=m.max_drawdown_pct;
    report.evidence_sufficient=report.errors.is_empty() && report.window_results.len()==windows && report.window_results.iter().all(|w|w.evidence_sufficient);
    report
}
