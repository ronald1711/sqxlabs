//! # sqx-pipeline
//!
//! Custom projects: an ordered pipeline of tasks that chain Build, Retest and
//! Optimize (with GoToTask loops), mirroring StrategyQuant's Custom Projects.

use rand::rngs::StdRng;
use rand::SeedableRng;

use sqx_backtest::{backtest, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_optimizer::{backtest_batch_detailed, rank, random_strategy, Fitness, GeneratorConfig};
use sqx_strategy::Strategy;

/// One pipeline step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TaskKind {
    EvaluateDatabank,
    WalkForwardStudy { studies:Vec<WalkForwardSpec> },
    PortfolioCombination { names:Vec<String>, strategies:Vec<Strategy> },
    Portfolio { strategies:Vec<Strategy>, selection:sqx_portfolio::SearchConfig },
    Backtest { strategy: Strategy },
    Build { count: usize, keep: usize },
    Generate { count: usize, min_trades: usize },
    Retest { mc_runs: usize, min_positive_pct: f64 },
    Optimize { windows: usize, candidates: usize },
    /// Standalone generated search: a fresh seeded family in each training window.
    DiscoverWalkForward { windows: usize, candidates: usize, oos_fraction: f64 },
    Robustness { strategy: Strategy, scenarios: Vec<(f64,f64,f64)>, mc_runs: usize,
        mc_seed:u64, skip_probability:f64, retest_runs:usize, retest_seed:u64,
        windows:usize, candidates:usize, oos_fraction:f64 },
    /// Cost sensitivity and trade resampling, without unrelated generated searches.
    Sensitivity { strategy: Strategy, scenarios: Vec<(f64,f64,f64)>, mc_runs:usize,
        mc_seed:u64, skip_probability:f64 },
    GoToTask { index: usize },
}

/// Explicit matrix cells; each retains its own full report and trial journal.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WalkForwardSpec {
    pub windows:usize,
    pub candidates:usize,
    pub oos_fraction:f64,
    pub criteria:sqx_optimizer::WalkForwardCriteria,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct WalkForwardStudyResult {
    pub specification:WalkForwardSpec,
    pub report:sqx_optimizer::WalkForwardReport,
}

/// Final full-data executions for presentation; these are not untouched holdouts.
#[derive(Debug,Clone,serde::Serialize)]
pub struct FinalEvaluation {
    pub errors:Vec<String>,
    pub step:usize,
    pub candidate_index:usize,
    pub strategy:Strategy,
    pub execution:sqx_backtest::BacktestResult,
    pub metrics:sqx_metrics::Metrics,
}

/// A named task.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Task {
    pub kind: TaskKind,
}

/// A custom project pipeline.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Pipeline {
    pub name: String,
    pub tasks: Vec<Task>,
}

/// The outcome of running a pipeline.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PipelineReport {
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub final_evaluations:Vec<FinalEvaluation>,
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub walk_forward_studies:Vec<WalkForwardStudyResult>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub portfolio_combination: Option<sqx_portfolio::CombinationReport>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub portfolio: Option<sqx_portfolio::SearchReport>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub backtest: Option<sqx_backtest::BacktestResult>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub robustness: Option<RobustnessStudy>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub discovery: Option<sqx_optimizer::WalkForwardReport>,
    pub retests: Vec<RetestEvaluation>,
    pub builds: Vec<BuildEvaluation>,
    pub optimizations: Vec<CandidateOptimization>,
    pub errors: Vec<String>,
    pub databank: Vec<Strategy>,
    pub log: Vec<String>,
}

#[derive(Debug,Clone,serde::Serialize)]
pub struct RobustnessStudy {
    pub base:sqx_backtest::BacktestResult,
    pub what_if:Vec<sqx_robustness::WhatIfRow>,
    pub monte_carlo:Option<sqx_robustness::MonteCarloReport>,
    pub retest:Option<sqx_robustness::RetestReport>,
    pub walk_forward:Option<sqx_optimizer::WalkForwardReport>,
}

#[derive(Debug,Clone,serde::Serialize)]
pub struct BuildEvaluation {
    pub step: usize,
    pub candidate_index: usize,
    pub evaluation: sqx_optimizer::BatchEvaluation,
    pub selected: bool,
}

#[derive(Debug,Clone,serde::Serialize)]
pub struct RetestEvaluation {
    pub step:usize,
    pub strategy:Strategy,
    pub requested_runs:usize,
    pub seed:u64,
    pub minimum_positive_pct:f64,
    pub errors:Vec<String>,
    pub report:Option<sqx_robustness::MonteCarloReport>,
    pub selected:bool,
}

/// Recorded family evaluation, including rejected/insufficient candidates.
#[derive(Debug,Clone,serde::Serialize)]
pub struct CandidateOptimization {
    pub original: Strategy,
    pub family: Vec<Strategy>,
    pub report: sqx_optimizer::WalkForwardReport,
}

/// Run a pipeline over the given data.
pub fn run(
    pipeline: &Pipeline,
    bars: &[Bar],
    instrument: &Instrument,
    config: &BacktestConfig,
    periods_per_year: f64,
    gen: &GeneratorConfig,
    seed: u64,
) -> PipelineReport {
    run_observed(pipeline,bars,instrument,config,periods_per_year,gen,seed,&mut |_|Ok(()))
        .unwrap_or_else(|error|PipelineReport{errors:vec![error],..Default::default()})
}

/// Events precede execution/selection. Observer errors stop work immediately.
/// Optimize events include each walk-forward trial and its full execution result.
#[derive(Debug,serde::Serialize)]
#[serde(tag="kind",rename_all="snake_case")]
pub enum PipelineEvent<'a> {
    FinalEvaluationStarted { step:usize, candidate_index:usize, strategy:&'a Strategy },
    FinalEvaluationOutcome { evaluation:&'a FinalEvaluation },
    WalkForwardStudyStarted { step:usize, study_index:usize, specification:&'a WalkForwardSpec },
    WalkForwardStudyOutcome { step:usize, study_index:usize, result:&'a WalkForwardStudyResult },
    PortfolioCombination { step:usize, event:sqx_portfolio::SearchEvent<'a> },
    Portfolio { step:usize, event:sqx_portfolio::SearchEvent<'a> },
    BacktestStarted { step:usize, strategy:&'a Strategy },
    BacktestOutcome { step:usize, result:&'a sqx_backtest::BacktestResult },
    RobustnessBaseStarted { step:usize, strategy:&'a Strategy },
    RobustnessBaseOutcome { step:usize, result:&'a sqx_backtest::BacktestResult },
    RobustnessScenario { step:usize, event:sqx_robustness::RobustnessEvent<'a> },
    StepStarted { step:usize, task_index:usize, task:&'a Task, inputs:&'a [Strategy] },
    BuildFamily { step:usize, candidates:&'a [Strategy] },
    BuildStarted { step:usize, candidate_index:usize, strategy:&'a Strategy },
    BuildOutcome { step:usize, candidate_index:usize, evaluation:&'a sqx_optimizer::BatchEvaluation },
    RetestStarted { step:usize, candidate_index:usize, strategy:&'a Strategy },
    RetestBaseOutcome { step:usize, candidate_index:usize, result:&'a sqx_backtest::BacktestResult },
    RetestTrial { step:usize, candidate_index:usize, event:sqx_robustness::RobustnessEvent<'a> },
    RetestOutcome { step:usize, candidate_index:usize, evaluation:&'a RetestEvaluation },
    OptimizeFamily { step:usize, candidate_index:usize, original:&'a Strategy, family:&'a [Strategy] },
    OptimizeFailure { step:usize, candidate_index:usize, original:&'a Strategy, error:&'a str },
    WalkForward { step:usize, candidate_index:usize, event:sqx_optimizer::WalkForwardEvent<'a> },
    OptimizeOutcome { step:usize, candidate_index:usize, evaluation:&'a CandidateOptimization },
    StepSelected { step:usize, strategies:&'a [Strategy] },
}

#[allow(clippy::too_many_arguments)]
pub fn run_observed(
    pipeline:&Pipeline,bars:&[Bar],instrument:&Instrument,config:&BacktestConfig,
    periods_per_year:f64,gen:&GeneratorConfig,seed:u64,
    observer:&mut impl FnMut(PipelineEvent<'_>)->Result<(),String>,
)->Result<PipelineReport,String> {
    let failure=|error:String| Ok(PipelineReport{errors:vec![error],..Default::default()});
    if pipeline.tasks.iter().any(|t|matches!(t.kind,TaskKind::Portfolio{..}|TaskKind::PortfolioCombination{..})) && pipeline.tasks.len()!=1{
        return failure("Portfolio studies must be standalone; they do not populate a strategy databank".into());
    }
    if pipeline.tasks.iter().any(|t|matches!(t.kind,TaskKind::Backtest{..})) && pipeline.tasks.len()!=1 {
        return failure("A single backtest must be a standalone task; it does not select candidates".into());
    }
    if pipeline.tasks.iter().any(|t|matches!(t.kind,TaskKind::Robustness{..}|TaskKind::Sensitivity{..})) && pipeline.tasks.len()!=1 {
        return failure("Robustness studies must be standalone tasks; they do not select a deployable strategy".into());
    }
    if pipeline.tasks.iter().any(|t|matches!(t.kind,TaskKind::DiscoverWalkForward{..})) && pipeline.tasks.len()!=1 {
        return failure("Generated walk-forward must be a standalone task; it does not select a single deployable strategy".into());
    }
    if pipeline.tasks.iter().any(|t|matches!(t.kind,TaskKind::WalkForwardStudy{..})) && pipeline.tasks.len()!=1 {
        return failure("A walk-forward study must be standalone; matrix cells do not select a deployable strategy".into());
    }
    for task in &pipeline.tasks {
        if let TaskKind::WalkForwardStudy{studies}=&task.kind {
            if studies.is_empty() || studies.len()>12 || studies.iter().any(|s|
                !(2..=12).contains(&s.windows) || !(1..=2000).contains(&s.candidates) ||
                !s.oos_fraction.is_finite() || !(0.05..=0.5).contains(&s.oos_fraction) ||
                s.criteria.min_is_trades==0 || s.criteria.min_oos_trades_per_window==0) {
                return failure("Walk-forward study requires 1..12 valid cells, 2..12 windows, 1..2000 candidates, 0.05..0.5 OOS fraction and positive evidence thresholds".into());
            }
        }
    }
    if let Err(error)=gen.validate(){return failure(error);}
    if let Err(error)=sqx_backtest::validate_backtest_inputs(bars,instrument,config){return failure(error);}
    for task in &pipeline.tasks {
        match task.kind {
            TaskKind::Optimize{windows,candidates} if windows==0 || windows>12 || candidates==0 || candidates>2000=>return failure("Optimize requires 1..12 windows and 1..2000 variants".into()),
            TaskKind::Build{count,keep} if count==0 || count>2000 || keep==0 || keep>count=>return failure("Build requires 1 <= keep <= count <= 2000".into()),
            TaskKind::Generate{count,..} if count==0=>return failure("Generate requires at least one candidate".into()),
            TaskKind::Retest{mc_runs,min_positive_pct} if mc_runs==0 || mc_runs>10000 || !min_positive_pct.is_finite() || !(0.0..=100.0).contains(&min_positive_pct)=>return failure("Invalid retest run count or percentage".into()),
            TaskKind::GoToTask{index} if index>=pipeline.tasks.len()=>return failure("Pipeline jump target is out of range".into()),
            _=>{}
        }
    }
    let mut final_evaluations=Vec::new();
    let mut walk_forward_studies=Vec::new();
    let mut retests=Vec::new();let mut builds=Vec::new();let mut optimizations=Vec::new();let mut errors=Vec::new();let mut discovery=None;let mut robustness=None;let mut backtest_result=None;let mut portfolio=None;let mut portfolio_combination=None;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut databank: Vec<Strategy> = Vec::new();
    let mut log = Vec::new();
    let mut pc = 0usize;
    let mut steps = 0usize;
    while pc < pipeline.tasks.len() && steps < 200 {
        steps += 1;
        observer(PipelineEvent::StepStarted{step:steps,task_index:pc,task:&pipeline.tasks[pc],inputs:&databank})?;
        match &pipeline.tasks[pc].kind {
            TaskKind::PortfolioCombination{names,strategies}=>{
                let report=sqx_portfolio::combine_observed(names,strategies,bars,instrument,config,periods_per_year,&mut |event|observer(PipelineEvent::PortfolioCombination{step:steps,event}))?;
                errors.extend(report.errors.iter().cloned());portfolio_combination=Some(report);pc+=1;
            },
            TaskKind::Portfolio{strategies,selection}=>{
                let report=sqx_portfolio::search_observed(strategies,bars,instrument,config,periods_per_year,selection,&mut |event|observer(PipelineEvent::Portfolio{step:steps,event}))?;
                errors.extend(report.errors.iter().cloned());portfolio=Some(report);pc+=1;
            },
            TaskKind::Backtest{strategy}=>{
                observer(PipelineEvent::BacktestStarted{step:steps,strategy})?;
                let result=backtest(strategy,bars,instrument,config);
                observer(PipelineEvent::BacktestOutcome{step:steps,result:&result})?;
                errors.extend(result.errors.iter().cloned());backtest_result=Some(result);pc+=1;
            },
            TaskKind::Robustness{strategy,scenarios,mc_runs,mc_seed,skip_probability,..} |
            TaskKind::Sensitivity{strategy,scenarios,mc_runs,mc_seed,skip_probability}=>{
                observer(PipelineEvent::RobustnessBaseStarted{step:steps,strategy})?;
                let base=backtest(strategy,bars,instrument,config);
                observer(PipelineEvent::RobustnessBaseOutcome{step:steps,result:&base})?;
                if !base.errors.is_empty(){
                    errors.extend(base.errors.iter().cloned());
                    robustness=Some(RobustnessStudy{base,what_if:vec![],monte_carlo:None,retest:None,walk_forward:None});
                }else{
                    let what_if=sqx_robustness::what_if_observed(strategy,bars,instrument,config,scenarios,&mut |event|observer(PipelineEvent::RobustnessScenario{step:steps,event}))?;
                    let mc=sqx_robustness::monte_carlo_trade_manipulation_observed(&base.trades,*mc_runs,*skip_probability,*mc_seed,&mut |event|observer(PipelineEvent::RobustnessScenario{step:steps,event}))?;
                    let (retest,wf)=if let TaskKind::Robustness{retest_runs,retest_seed,windows,candidates,oos_fraction,..}=&pipeline.tasks[pc].kind {
                        let retest=sqx_robustness::monte_carlo_retest_observed(strategy,bars,instrument,config,*retest_runs,*retest_seed,&mut |event|observer(PipelineEvent::RobustnessScenario{step:steps,event}))?;
                        let family=match sqx_optimizer::parameter_family(strategy,*candidates,seed,&gen.param_ranges){
                            Ok(family)=>family,
                            Err(error)=>{
                                observer(PipelineEvent::OptimizeFailure{step:steps,candidate_index:0,original:strategy,error:&error})?;
                                return Err(format!("Robustness walk-forward family could not be prepared: {error}"));
                            }
                        };
                        observer(PipelineEvent::OptimizeFamily{step:steps,candidate_index:0,original:strategy,family:&family})?;
                        let wf=sqx_optimizer::walk_forward_candidates_observed(bars,instrument,config,periods_per_year,&family,*windows,*oos_fraction,seed,&sqx_optimizer::WalkForwardCriteria::default(),&mut |event|observer(PipelineEvent::WalkForward{step:steps,candidate_index:0,event}))?;
                        (Some(retest),Some(wf))
                    }else{(None,None)};
                    for (index,row) in what_if.iter().enumerate(){errors.extend(row.errors.iter().map(|e|format!("What-if scenario {index}: {e}")));}
                    errors.extend(mc.errors.iter().map(|e|format!("Monte Carlo: {e}")));
                    if let Some(report)=&retest{errors.extend(report.errors.iter().map(|e|format!("MC retest: {e}")));}
                    if let Some(report)=&wf{errors.extend(report.errors.iter().map(|e|format!("Walk-forward: {e}")));}
                    robustness=Some(RobustnessStudy{base,what_if,monte_carlo:Some(mc),retest,walk_forward:wf});
                }
                log.push("Robustness study completed; inspect errors and scenario evidence before use".into());
                pc+=1;
            },
            TaskKind::EvaluateDatabank=>{
                for (candidate_index,strategy) in databank.iter().enumerate(){
                    observer(PipelineEvent::FinalEvaluationStarted{step:steps,candidate_index,strategy})?;
                    let execution=backtest(strategy,bars,instrument,config);
                    let metrics=sqx_metrics::compute(&execution,bars.len(),periods_per_year);
                    let mut evaluation_errors=execution.errors.clone();
                    if let Err(error)=sqx_metrics::validate_observations(&execution){evaluation_errors.push(error);}
                    errors.extend(evaluation_errors.iter().map(|e|format!("Final evaluation {}: {e}",strategy.name)));
                    final_evaluations.push(FinalEvaluation{errors:evaluation_errors,step:steps,candidate_index,strategy:strategy.clone(),execution,metrics});
                    observer(PipelineEvent::FinalEvaluationOutcome{evaluation:final_evaluations.last().unwrap()})?;
                }
                log.push(format!("Recorded {} final full-data executions; exploratory, not untouched holdout results",databank.len()));
                pc+=1;
            },
            TaskKind::WalkForwardStudy{studies}=>{
                for (study_index,specification) in studies.iter().enumerate() {
                    observer(PipelineEvent::WalkForwardStudyStarted{step:steps,study_index,specification})?;
                    let report=sqx_optimizer::walk_forward_generated_observed(bars,instrument,config,periods_per_year,gen,
                        specification.windows,specification.oos_fraction,specification.candidates,seed,&specification.criteria,
                        &mut |event|observer(PipelineEvent::WalkForward{step:steps,candidate_index:study_index,event}))?;
                    errors.extend(report.errors.iter().map(|e|format!("Walk-forward cell {study_index}: {e}")));
                    walk_forward_studies.push(WalkForwardStudyResult{specification:specification.clone(),report});
                    observer(PipelineEvent::WalkForwardStudyOutcome{step:steps,study_index,result:walk_forward_studies.last().unwrap()})?;
                }
                log.push(format!("{} exploratory walk-forward cells retained; no best OOS cell selected",studies.len()));
                pc+=1;
            },
            TaskKind::DiscoverWalkForward{windows,candidates,oos_fraction}=>{
                let report=sqx_optimizer::walk_forward_generated_observed(bars,instrument,config,periods_per_year,gen,*windows,*oos_fraction,*candidates,seed,&sqx_optimizer::WalkForwardCriteria::default(),&mut |event|observer(PipelineEvent::WalkForward{step:steps,candidate_index:0,event}))?;
                errors.extend(report.errors.iter().cloned());
                log.push(format!("Generated walk-forward: {} candidates across {} windows",report.candidates_attempted,report.windows));
                discovery=Some(report);
                pc+=1;
            },
            TaskKind::Build { count, .. } | TaskKind::Generate { count, .. } => {
                let (keep,min_trades,standalone)=match &pipeline.tasks[pc].kind {
                    TaskKind::Build{keep,..}=>(*keep,5,false),
                    TaskKind::Generate{min_trades,..}=>(*count,*min_trades,true),
                    _=>unreachable!(),
                };
                let cands: Vec<Strategy> = (0..*count)
                    .map(|i| random_strategy(&mut rng, &if standalone{format!("s{i:04}")}else{format!("b{steps}-{i}")}, gen))
                    .collect();
                observer(PipelineEvent::BuildFamily{step:steps,candidates:&cands})?;
                let mut evaluations=Vec::with_capacity(cands.len());
                for (candidate_index,strategy) in cands.iter().enumerate() {
                    observer(PipelineEvent::BuildStarted{step:steps,candidate_index,strategy})?;
                    let evaluation=backtest_batch_detailed(std::slice::from_ref(strategy),bars,instrument,config,periods_per_year).remove(0);
                    observer(PipelineEvent::BuildOutcome{step:steps,candidate_index,evaluation:&evaluation})?;
                    evaluations.push(evaluation);
                }
                let mut scored=Vec::new();
                for (index,evaluation) in evaluations.iter().enumerate(){
                    if !evaluation.errors.is_empty(){
                        errors.push(format!("Build step {steps}, candidate {index} ({}): {}",evaluation.strategy.name,evaluation.errors.join("; ")));
                    }else{scored.push((evaluation.strategy.clone(),evaluation.metrics));}
                }
                let ranked = rank(scored, Fitness::ProfitFactor, min_trades);
                databank = ranked.into_iter().take(keep).map(|(s, _)| s).collect();
                for (candidate_index,evaluation) in evaluations.into_iter().enumerate(){
                    let selected=databank.contains(&evaluation.strategy);
                    builds.push(BuildEvaluation{step:steps,candidate_index,evaluation,selected});
                }
                log.push(format!("Build: {} candidates -> {} kept", count, databank.len()));
                pc += 1;
            }
            TaskKind::Retest { mc_runs, min_positive_pct } => {
                let mut kept = Vec::new();
                for (candidate_index,s) in databank.drain(..).enumerate() {
                    observer(PipelineEvent::RetestStarted{step:steps,candidate_index,strategy:&s})?;
                    let r = backtest(&s, bars, instrument, config);
                    observer(PipelineEvent::RetestBaseOutcome{step:steps,candidate_index,result:&r})?;
                    if !r.errors.is_empty(){
                        retests.push(RetestEvaluation{step:steps,strategy:s,requested_runs:*mc_runs,seed,minimum_positive_pct:*min_positive_pct,errors:r.errors.clone(),report:None,selected:false});
                        observer(PipelineEvent::RetestOutcome{step:steps,candidate_index,evaluation:retests.last().unwrap()})?;
                        errors.extend(r.errors);continue;
                    }
                    let mc = sqx_robustness::monte_carlo_trade_manipulation_observed(&r.trades, *mc_runs, 0.1, seed,&mut |event|observer(PipelineEvent::RetestTrial{step:steps,candidate_index,event}))?;
                    if !mc.errors.is_empty(){let error=format!("Retest step {steps}, {} unavailable: {}",s.name,mc.errors.join("; "));log.push(error.clone());errors.push(error);}
                    let selected=mc.meets_positive_threshold(*min_positive_pct);
                    retests.push(RetestEvaluation{step:steps,strategy:s.clone(),requested_runs:*mc_runs,seed,minimum_positive_pct:*min_positive_pct,errors:mc.errors.clone(),report:Some(mc),selected});
                    observer(PipelineEvent::RetestOutcome{step:steps,candidate_index,evaluation:retests.last().unwrap()})?;
                    if selected {kept.push(s);}
                }
                log.push(format!("Retest: {} survivors", kept.len()));
                databank = kept;
                pc += 1;
            }
            TaskKind::Optimize { windows,candidates } => {
                let mut kept=Vec::new();
                for (candidate_index,original) in databank.drain(..).enumerate() {
                    let family=match sqx_optimizer::parameter_family(&original,*candidates,seed,&gen.param_ranges){Ok(family)=>family,Err(error)=>{observer(PipelineEvent::OptimizeFailure{step:steps,candidate_index,original:&original,error:&error})?;errors.push(format!("{}: {error}",original.name));continue;}};
                    observer(PipelineEvent::OptimizeFamily{step:steps,candidate_index,original:&original,family:&family})?;
                    let report=sqx_optimizer::walk_forward_candidates_observed(bars,instrument,config,periods_per_year,&family,*windows,0.25,seed,&sqx_optimizer::WalkForwardCriteria::default(),&mut |event|observer(PipelineEvent::WalkForward{step:steps,candidate_index,event}))?;
                    errors.extend(report.errors.iter().map(|error|format!("{}: {error}",original.name)));
                    if report.evidence_sufficient && report.oos_net_profit>0.0 {
                        // Keep the last training-selected variant, never retrospectively
                        // choose the variant with the best OOS outcome.
                        if let Some(selected)=report.window_results.last().and_then(|w|w.selected_strategy.clone()){kept.push(selected);}
                    }
                    log.push(format!("Optimize {}: {} frozen parameter/SL/PT variants; {} IS evaluations; OOS net {}; evidence sufficient {}. Full-data candidate discovery means this is exploratory, not an untouched holdout.",original.name,family.len(),report.candidates_attempted,report.oos_net_profit,report.evidence_sufficient));
                    optimizations.push(CandidateOptimization{original,family,report});
                    observer(PipelineEvent::OptimizeOutcome{step:steps,candidate_index,evaluation:optimizations.last().unwrap()})?;
                }
                databank=kept;pc+=1;
            },
            TaskKind::GoToTask { index } => {
                log.push(format!("GoToTask {} (databank {})", index, databank.len()));
                pc = *index;
            }
        }
        observer(PipelineEvent::StepSelected{step:steps,strategies:&databank})?;
    }
    log.push(format!("Pipeline {} after {} steps; {} strategies in databank",if pc<pipeline.tasks.len(){"stopped at its limit"}else{"finished"}, steps, databank.len()));
    if pc<pipeline.tasks.len(){errors.push("Pipeline step limit reached before completion".into());}
    Ok(PipelineReport {final_evaluations,walk_forward_studies,portfolio_combination,portfolio,backtest:backtest_result,errors,retests,builds,optimizations,discovery,robustness,databank, log })
}

/// A default demo pipeline: Build -> Retest -> Optimize.
pub fn default_pipeline(count: usize, keep: usize) -> Pipeline {
    Pipeline {
        name: "Build -> Retest -> Optimize".to_string(),
        tasks: vec![
            Task { kind: TaskKind::Build { count, keep } },
            Task { kind: TaskKind::Retest { mc_runs: 500, min_positive_pct: 60.0 } },
            Task { kind: TaskKind::Optimize { windows: 4, candidates: 20 } },
        ],
    }
}
