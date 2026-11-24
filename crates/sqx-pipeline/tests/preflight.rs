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
 let mut trial_events=Vec::new();
 let report=sqx_pipeline::run_observed(&pipeline,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),525600.0,&GeneratorConfig::default(),42,&mut |event|{
  if let sqx_pipeline::PipelineEvent::WalkForward{step,candidate_index,event}=event {
   assert_eq!(step,2);trial_events.push((candidate_index,serde_json::to_value(event).unwrap()));
  }
  Ok(())
 }).unwrap();
 assert_eq!(trial_events.iter().filter(|(_,e)|e["kind"]=="candidate_outcome").count(),report.optimizations.iter().map(|r|r.report.candidates_attempted).sum::<usize>());
 assert!(!trial_events.is_empty());
 for (_,event) in &trial_events {if event["kind"]=="candidate_outcome"||event["kind"]=="out_of_sample_outcome"{assert!(event["execution"]["trades"].is_array());}}
 let interrupted=sqx_pipeline::run_observed(&pipeline,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),525600.0,&GeneratorConfig::default(),42,&mut |event|{
  if matches!(event,sqx_pipeline::PipelineEvent::WalkForward{event:sqx_optimizer::WalkForwardEvent::CandidateOutcome{..},..}) {return Err("optimization checkpoint unavailable".into());}
  Ok(())
 });
 assert_eq!(interrupted.unwrap_err(),"optimization checkpoint unavailable");
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

#[test]
fn observer_freezes_family_and_stops_immediately_on_storage_failure() {
 use sqx_pipeline::{run_observed,PipelineEvent};
 let bars=vec![Bar::new(0,1.0,1.1,0.9,1.0,1.0),Bar::new(60000,1.0,1.1,0.9,1.0,1.0)];
 let pipeline=Pipeline{name:"journal".into(),tasks:vec![Task{kind:TaskKind::Build{count:3,keep:1}}]};
 let mut events=Vec::new();
 let result=run_observed(&pipeline,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,&GeneratorConfig::default(),42,&mut |event| {
  match event {
   PipelineEvent::StepStarted{..}=>events.push("step"),
   PipelineEvent::BuildFamily{candidates,..}=>{assert_eq!(candidates.len(),3);events.push("family");},
   PipelineEvent::BuildStarted{candidate_index,..}=>{assert_eq!(candidate_index,0);events.push("start");},
   PipelineEvent::BuildOutcome{candidate_index,..}=>{assert_eq!(candidate_index,0);events.push("outcome");return Err("simulated disk failure".into());},
   _=>panic!("selection or later execution after failed write"),
  }
  Ok(())
 });
 assert_eq!(result.unwrap_err(),"simulated disk failure");
 assert_eq!(events,vec!["step","family","start","outcome"]);
}

#[test]
fn standalone_generation_preserves_seed_names_ranking_and_every_outcome(){
 use rand::SeedableRng;
 use sqx_optimizer::{random_strategy,backtest_batch,rank,Fitness};
 let bars:Vec<_>=(0..180).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;Bar::new(i*60000,p,p+0.002,p-0.002,p,1.0)}).collect();
 let instrument=Instrument::fx("EURUSD");let config=BacktestConfig::default();let generator=GeneratorConfig::default();
 let mut rng=rand::rngs::StdRng::seed_from_u64(731);
 let family:Vec<_>=(0..4).map(|i|random_strategy(&mut rng,&format!("s{i:04}"),&generator)).collect();
 let expected=rank(backtest_batch(&family,&bars,&instrument,&config,525600.0),Fitness::ProfitFactor,1);
 let pipeline=Pipeline{name:"generation".into(),tasks:vec![Task{kind:TaskKind::Generate{count:4,min_trades:1}}]};
 let report=sqx_pipeline::run(&pipeline,&bars,&instrument,&config,525600.0,&generator,731);
 assert!(report.errors.is_empty());assert_eq!(report.builds.len(),4);
 assert_eq!(report.databank,expected.into_iter().map(|(strategy,_)|strategy).collect::<Vec<_>>());
 for (index,row) in report.builds.iter().enumerate(){assert_eq!(row.evaluation.strategy,family[index]);assert_eq!(row.candidate_index,index);}
 assert_eq!(report.log.len(),2,"the generation task must advance exactly once");
}

#[test]
fn retest_journal_records_each_trial_and_stops_before_selection_on_write_failure(){
 let bars:Vec<_>=(0..1500).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;Bar::new(i*60000,p,p+0.003,p-0.003,p+0.0002,10.0)}).collect();
 let instrument=Instrument::fx("EURUSD");let config=BacktestConfig::default();let generator=GeneratorConfig::default();
 let pipeline=Pipeline{name:"retest checkpoints".into(),tasks:vec![Task{kind:TaskKind::Build{count:100,keep:2}},Task{kind:TaskKind::Retest{mc_runs:3,min_positive_pct:0.0}}]};
 let mut events=Vec::new();
 let report=sqx_pipeline::run_observed(&pipeline,&bars,&instrument,&config,525600.0,&generator,42,&mut |event|{events.push(serde_json::to_value(event).unwrap());Ok(())}).unwrap();
 assert!(!report.retests.is_empty());
 let trial_positions:Vec<_>=events.iter().enumerate().filter(|(_,event)|event["kind"]=="retest_trial").map(|(i,_)|i).collect();
 assert_eq!(trial_positions.len(),report.retests.len()*6,"every sampled path needs a start and outcome");
 for (candidate,row) in report.retests.iter().enumerate(){
  let base=events.iter().find(|event|event["kind"]=="retest_base_outcome"&&event["candidate_index"]==candidate).expect("full retest trade input");
  let expected=sqx_backtest::backtest(&row.strategy,&bars,&instrument,&config);
  assert_eq!(base["result"],serde_json::to_value(expected).unwrap());
  assert_eq!(events.iter().filter(|event|event["kind"]=="retest_trial"&&event["candidate_index"]==candidate).count(),6);
 }
 let failure_positions:Vec<_>=events.iter().enumerate().filter(|(_,event)|matches!(event["kind"].as_str(),Some("retest_started"|"retest_base_outcome"|"retest_trial"|"retest_outcome"))).map(|(i,_)|i).collect();
 for fail_at in failure_positions {
  let mut seen=0;
  let result=sqx_pipeline::run_observed(&pipeline,&bars,&instrument,&config,525600.0,&generator,42,&mut |event|{
   assert_eq!(serde_json::to_value(event).unwrap(),events[seen]);let position=seen;seen+=1;
   if position==fail_at{Err("simulated trial journal failure".into())}else{Ok(())}
  });
  assert_eq!(result.unwrap_err(),"simulated trial journal failure");assert_eq!(seen,fail_at+1);
 }
}

#[test]
fn standalone_backtest_retains_full_execution_and_stops_at_every_failed_checkpoint(){
 let bars:Vec<_>=(0..20).map(|i|Bar::new(i*60000,1.1,1.101,1.099,1.1,1.0)).collect();
 let instrument=Instrument::fx("EURUSD");let config=BacktestConfig::default();let generator=GeneratorConfig::default();
 for invalid in [false,true]{
  let mut strategy=sqx_strategy::Strategy::new("single control");strategy.long=true;strategy.short=false;
  for kind in [sqx_strategy::SignalKind::LongEntry,sqx_strategy::SignalKind::LongExit]{strategy.signals.push(sqx_strategy::Signal::new(kind,vec![sqx_strategy::Condition::new(sqx_strategy::Operand::Const(1.0),sqx_strategy::Comparator::Gt,sqx_strategy::Operand::Const(0.0))],sqx_strategy::Join::And));}
  if invalid{strategy.stop_loss=Some(sqx_strategy::StopLoss::Points(-1.0));}
  let pipeline=Pipeline{name:"single".into(),tasks:vec![Task{kind:TaskKind::Backtest{strategy:strategy.clone()}}]};
  let expected=sqx_backtest::backtest(&strategy,&bars,&instrument,&config);
  if !invalid{assert!(!expected.trades.is_empty());}
  let mut events=Vec::new();
  let result=sqx_pipeline::run_observed(&pipeline,&bars,&instrument,&config,525600.0,&generator,42,&mut |event|{events.push(serde_json::to_value(event).unwrap());Ok(())}).unwrap();
  assert_eq!(serde_json::to_value(result.backtest.as_ref().unwrap()).unwrap(),serde_json::to_value(&expected).unwrap());
  assert_eq!(result.errors,expected.errors);assert!(result.databank.is_empty());assert!(result.builds.is_empty());assert!(result.robustness.is_none());
  assert_eq!(events[1]["kind"],"backtest_started");assert_eq!(events[2]["kind"],"backtest_outcome");
  assert_eq!(events[2]["result"],serde_json::to_value(expected).unwrap());
  for failure in 0..events.len(){
   let mut seen=0;
   let interrupted=sqx_pipeline::run_observed(&pipeline,&bars,&instrument,&config,525600.0,&generator,42,&mut |event|{
    assert_eq!(serde_json::to_value(event).unwrap(),events[seen]);let index=seen;seen+=1;if index==failure{Err("checkpoint failure".into())}else{Ok(())}
   });
   assert_eq!(interrupted.unwrap_err(),"checkpoint failure");assert_eq!(seen,failure+1);
  }
  let mixed=Pipeline{name:"invalid chain".into(),tasks:vec![pipeline.tasks[0].clone(),Task{kind:TaskKind::Build{count:1,keep:1}}]};
  let report=run(&mixed,&bars,&instrument,&config,525600.0,&generator,42);assert!(!report.errors.is_empty());assert!(report.backtest.is_none());
 }
}

#[test]
fn portfolio_rejections_remain_evidence_and_mixed_tasks_never_start(){
 let bars:Vec<_>=(0..20).map(|i|Bar::new(i*60000,1.1,1.101,1.099,1.1,1.0)).collect();
 let instrument=Instrument::fx("EURUSD");let config=BacktestConfig::default();let generator=GeneratorConfig::default();
 let task=Task{kind:TaskKind::Portfolio{strategies:vec![sqx_strategy::Strategy::new("no signals")],selection:Default::default()}};
 let pipeline=Pipeline{name:"insufficient portfolio".into(),tasks:vec![task.clone()]};let mut events=Vec::new();
 let report=sqx_pipeline::run_observed(&pipeline,&bars,&instrument,&config,252.0,&generator,42,&mut |event|{events.push(serde_json::to_value(event).unwrap());Ok(())}).unwrap();
 assert!(!report.errors.is_empty());assert!(report.databank.is_empty());
 let study=report.portfolio.unwrap();assert_eq!(study.candidates.len(),1);assert!(study.selected.is_empty());assert!(study.combined.is_none());
 assert_eq!(events[1]["event"]["kind"],"candidate_started");assert_eq!(events[2]["event"]["kind"],"candidate_outcome");assert_eq!(events[3]["event"]["kind"],"shortlist");
 for failure in 0..events.len(){let mut seen=0;let result=sqx_pipeline::run_observed(&pipeline,&bars,&instrument,&config,252.0,&generator,42,&mut |_|{seen+=1;if seen==failure+1{Err("disk failure".into())}else{Ok(())}});assert_eq!(result.unwrap_err(),"disk failure");assert_eq!(seen,failure+1);}
 let mixed=Pipeline{name:"mixed".into(),tasks:vec![task,Task{kind:TaskKind::Build{count:1,keep:1}}]};
 let result=sqx_pipeline::run_observed(&mixed,&bars,&instrument,&config,252.0,&generator,42,&mut |_|panic!("invalid workflow must not start")).unwrap();assert!(!result.errors.is_empty());assert!(result.portfolio.is_none());
}

#[test]
fn mixed_defined_and_undefined_portfolio_scores_have_a_valid_ranking_order(){
 use rand::SeedableRng;
 let bars:Vec<_>=(0..1500).map(|i|{let p=1.1+(i as f64*0.07).sin()*0.01;Bar::new((1700000000+i*60)*1000,p,p+0.001,p-0.001,p,1.0)}).collect();
 let generator=GeneratorConfig::default();let mut rng=rand::rngs::StdRng::seed_from_u64(123);
 let strategies=(0..40).map(|i|sqx_optimizer::random_strategy(&mut rng,&format!("p{i:04}"),&generator)).collect();
 let pipeline=Pipeline{name:"mixed ranking scores".into(),tasks:vec![Task{kind:TaskKind::Portfolio{strategies,selection:sqx_portfolio::SearchConfig{min_trades:5,shortlist_limit:4,min_subset:2,max_subset:3}}}]};
 let report=run(&pipeline,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),525600.0,&generator,123);
 assert!(report.errors.is_empty(),"{:?}",report.errors);let study=report.portfolio.unwrap();assert_eq!(study.candidates.len(),40);
 assert!(study.candidates.iter().any(|row|!row.metrics.profit_factor.is_finite()),"fixture must exercise undefined scores");
 assert_eq!(study.shortlist.len(),4);assert_eq!(study.subset.unwrap().attempted,10);
 for &i in &study.shortlist{assert!(study.candidates[i].metrics.profit_factor.is_finite());assert!(study.candidates[i].metrics.number_of_trades>=5.0);}
 for pair in study.shortlist.windows(2){assert!(study.candidates[pair[0]].metrics.profit_factor>=study.candidates[pair[1]].metrics.profit_factor);}
}
