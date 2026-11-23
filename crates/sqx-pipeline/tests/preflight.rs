use sqx_pipeline::{run,Pipeline,Task,TaskKind};
use sqx_core::{Bar,Instrument};
use sqx_backtest::BacktestConfig;
use sqx_optimizer::GeneratorConfig;
fn execute(pipeline:&Pipeline)->sqx_pipeline::PipelineReport {
 let bars=vec![Bar::new(0,1.0,1.1,0.9,1.0,1.0),Bar::new(60000,1.0,1.1,0.9,1.0,1.0)];
 run(pipeline,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,&GeneratorConfig::default(),42)
}
#[test]
fn optimization_evaluates_only_each_original_candidates_exit_family(){
 let bars:Vec<_>=(0..1500).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;Bar::new(i*60000,p,p+0.003,p-0.003,p+0.0002,10.0)}).collect();
 let pipeline=Pipeline{name:"candidate-linked".into(),tasks:vec![Task{kind:TaskKind::Build{count:100,keep:2}},Task{kind:TaskKind::Optimize{windows:3,candidates:4}}]};
 let report=run(&pipeline,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),525600.0,&GeneratorConfig::default(),42);
 assert!(report.errors.is_empty(),"{:?}",report.errors);assert!(!report.optimizations.is_empty());
 for evaluation in &report.optimizations {
  assert_eq!(evaluation.family[0],evaluation.original);
  for variant in &evaluation.family {assert_eq!(variant.signals,evaluation.original.signals);assert_eq!(variant.money_management,evaluation.original.money_management);assert_eq!(variant.trading_options,evaluation.original.trading_options);}
  for window in &evaluation.report.window_results {if let Some(selected)=&window.selected_strategy{assert!(evaluation.family.contains(selected));}}
 }
 for survivor in &report.databank {assert!(report.optimizations.iter().any(|evaluation|evaluation.report.evidence_sufficient && evaluation.report.oos_net_profit>0.0 && evaluation.report.window_results.last().and_then(|window|window.selected_strategy.as_ref())==Some(survivor)));}
}
#[test]
fn invalid_steps_and_unfinished_loops_report_errors(){
 for kind in [TaskKind::Build{count:0,keep:0},TaskKind::Retest{mc_runs:1,min_positive_pct:f64::NAN},TaskKind::GoToTask{index:5}] {
  let report=execute(&Pipeline{name:"invalid".into(),tasks:vec![Task{kind}]});assert!(!report.errors.is_empty());assert!(report.log.is_empty());
 }
 let report=execute(&Pipeline{name:"loop".into(),tasks:vec![Task{kind:TaskKind::GoToTask{index:0}}]});
 assert!(report.errors[0].contains("step limit"));
}
#[test]
fn build_report_preserves_every_candidate_and_selection_even_without_survivors(){
 let pipeline=Pipeline{name:"all attempts".into(),tasks:vec![Task{kind:TaskKind::Build{count:12,keep:2}}]};
 let report=execute(&pipeline);
 assert_eq!(report.builds.len(),12);assert!(report.databank.is_empty());
 for (index,row) in report.builds.iter().enumerate(){
  assert_eq!(row.step,1);assert_eq!(row.candidate_index,index);assert_eq!(row.evaluation.strategy.name,format!("b1-{index}"));
  assert!(!row.selected);assert!(row.evaluation.errors.is_empty());
 }
 let repeat=execute(&pipeline);
 for (a,b) in report.builds.iter().zip(&repeat.builds){assert_eq!(a.evaluation.strategy,b.evaluation.strategy);assert_eq!(a.selected,b.selected);}
}
#[test]
fn retest_records_preserve_inputs_and_match_independent_recalculation(){
 let bars:Vec<_>=(0..1500).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;Bar::new(i*60000,p,p+0.003,p-0.003,p+0.0002,10.0)}).collect();
 let instrument=Instrument::fx("EURUSD");let config=BacktestConfig::default();
 for threshold in [0.0,100.0] {
  let pipeline=Pipeline{name:"retest evidence".into(),tasks:vec![Task{kind:TaskKind::Build{count:100,keep:2}},Task{kind:TaskKind::Retest{mc_runs:20,min_positive_pct:threshold}}]};
  let result=run(&pipeline,&bars,&instrument,&config,525600.0,&GeneratorConfig::default(),42);
  assert!(result.errors.is_empty(),"{:?}",result.errors);
  let selected:Vec<_>=result.builds.iter().filter(|row|row.selected).collect();
  assert!(!selected.is_empty());assert_eq!(result.retests.len(),selected.len());
  for row in &result.retests {
   assert_eq!(row.step,2);assert_eq!(row.seed,42);assert_eq!(row.requested_runs,20);assert_eq!(row.minimum_positive_pct,threshold);
   assert!(selected.iter().any(|build|build.evaluation.strategy==row.strategy));assert!(row.errors.is_empty());
   let backtest=sqx_backtest::backtest(&row.strategy,&bars,&instrument,&config);
   let expected=sqx_robustness::monte_carlo_trade_manipulation(&backtest.trades,20,0.1,42);
   let actual=row.report.as_ref().expect("completed retest evidence");
   assert_eq!(actual.samples.len(),20);assert_eq!(actual.samples,expected.samples);
   assert_eq!(actual.net_profit_5pct.to_bits(),expected.net_profit_5pct.to_bits());
   assert_eq!(actual.positive_runs_pct.to_bits(),expected.positive_runs_pct.to_bits());
   assert_eq!(row.selected,expected.meets_positive_threshold(threshold));
   assert_eq!(result.databank.contains(&row.strategy),row.selected);
  }
 }
}
