use sqx_pipeline::{Pipeline,Task,TaskKind,PipelineEvent,run_observed};
use sqx_core::{Bar,Instrument};
use sqx_backtest::BacktestConfig;
use sqx_optimizer::GeneratorConfig;

#[test]
fn final_databank_execution_is_full_reproducible_and_write_failures_abort(){
 let bars:Vec<_>=(0..1500).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;Bar::new(i*60000,p,p+0.003,p-0.003,p+0.0002,10.0)}).collect();
 let instrument=Instrument::fx("EURUSD");let config=BacktestConfig::default();let generator=GeneratorConfig::default();
 let pipeline=Pipeline{name:"final records".into(),tasks:vec![Task{kind:TaskKind::Build{count:100,keep:2}},Task{kind:TaskKind::EvaluateDatabank}]};
 let mut events=Vec::new();
 let report=run_observed(&pipeline,&bars,&instrument,&config,525600.0,&generator,42,&mut |event|{events.push(serde_json::to_value(event).unwrap());Ok(())}).unwrap();
 assert!(report.errors.is_empty(),"{:?}",report.errors);assert_eq!(report.final_evaluations.len(),2);
 for (index,row) in report.final_evaluations.iter().enumerate(){
  assert_eq!(row.strategy,report.databank[index]);assert_eq!(row.candidate_index,index);assert_eq!(row.step,2);assert!(row.errors.is_empty());
  let direct=sqx_backtest::backtest(&row.strategy,&bars,&instrument,&config);
  assert_eq!(serde_json::to_value(&row.execution).unwrap(),serde_json::to_value(direct).unwrap());
 }
 assert_eq!(events.iter().filter(|e|e["kind"]=="final_evaluation_outcome").count(),2);
 for fail_on_outcome in [false,true]{
  let mut seen=0;
  let error=run_observed(&pipeline,&bars,&instrument,&config,525600.0,&generator,42,&mut |event|{
   match event{
    PipelineEvent::FinalEvaluationStarted{candidate_index,..}=>{assert_eq!(candidate_index,0);seen+=1;if !fail_on_outcome{return Err("storage unavailable".into());}},
    PipelineEvent::FinalEvaluationOutcome{evaluation}=>{assert_eq!(evaluation.candidate_index,0);seen+=1;return Err("storage unavailable".into());},
    _=>{}
   }Ok(())
  }).unwrap_err();assert_eq!(error,"storage unavailable");assert_eq!(seen,if fail_on_outcome{2}else{1});
 }
}
