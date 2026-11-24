//! MCP backtests and robustness studies use the shared frozen pipeline journal and recovery protocol.
use serde_json::{json,Value};
use sqx_project::{archive::Archive,pipeline_archive};

#[allow(clippy::too_many_arguments)]
pub fn execute_robustness(archive:&Archive,strategy:&sqx_strategy::Strategy,bars:&[sqx_core::Bar],instrument:&sqx_core::Instrument,timeframe:sqx_core::Timeframe,config:&sqx_backtest::BacktestConfig,pyp:f64,context:&Value)
 ->Result<(sqx_pipeline::PipelineReport,String,String),String>{
 let pipeline=sqx_pipeline::Pipeline{name:"MCP robustness".into(),tasks:vec![sqx_pipeline::Task{kind:sqx_pipeline::TaskKind::Robustness{
  strategy:strategy.clone(),scenarios:vec![(1.0,0.0,0.0),(3.0,1.0,5.0),(5.0,2.0,10.0)],mc_runs:500,mc_seed:42,skip_probability:0.1,retest_runs:200,retest_seed:7,windows:4,candidates:20,oos_fraction:0.25,
 }}]};
 execute_study(archive,pipeline,"mcp_robustness",bars,instrument,timeframe,config,pyp,42,context)
}

#[allow(clippy::too_many_arguments)]
pub fn execute_backtest(archive:&Archive,strategy:&sqx_strategy::Strategy,bars:&[sqx_core::Bar],instrument:&sqx_core::Instrument,timeframe:sqx_core::Timeframe,config:&sqx_backtest::BacktestConfig,pyp:f64,context:&Value)
 ->Result<(sqx_pipeline::PipelineReport,String,String),String>{
 let pipeline=sqx_pipeline::Pipeline{name:format!("{} — MCP backtest",strategy.name),tasks:vec![sqx_pipeline::Task{kind:sqx_pipeline::TaskKind::Backtest{strategy:strategy.clone()}}]};
 execute_study(archive,pipeline,"mcp_backtest",bars,instrument,timeframe,config,pyp,42,context)
}

#[allow(clippy::too_many_arguments)]
pub fn execute_portfolio(archive:&Archive,strategies:&[sqx_strategy::Strategy],seed:u64,bars:&[sqx_core::Bar],instrument:&sqx_core::Instrument,timeframe:sqx_core::Timeframe,config:&sqx_backtest::BacktestConfig,pyp:f64,context:&Value)
 ->Result<(sqx_pipeline::PipelineReport,String,String),String>{
 let pipeline=sqx_pipeline::Pipeline{name:"MCP portfolio search".into(),tasks:vec![sqx_pipeline::Task{kind:sqx_pipeline::TaskKind::Portfolio{strategies:strategies.to_vec(),selection:sqx_portfolio::SearchConfig::default()}}]};
 execute_study(archive,pipeline,"mcp_portfolio",bars,instrument,timeframe,config,pyp,seed,context)
}

#[allow(clippy::too_many_arguments)]
fn execute_study(archive:&Archive,pipeline:sqx_pipeline::Pipeline,origin:&str,bars:&[sqx_core::Bar],instrument:&sqx_core::Instrument,timeframe:sqx_core::Timeframe,config:&sqx_backtest::BacktestConfig,pyp:f64,seed:u64,context:&Value)
 ->Result<(sqx_pipeline::PipelineReport,String,String),String>{
 let engine=archive.retain_current_engine()?;
 let id=archive.save(engine,json!({"bars":bars,"instrument":instrument,"symbol":instrument.name,"timeframe":timeframe.to_string(),"source_context":context,"calendar":context["calendar"]}),
  json!({"pipeline":pipeline,"generator":sqx_optimizer::GeneratorConfig::default()}),
  json!({"research_kind":"cli_pipeline_started","origin":origin,"attempt_id":format!("{:032x}",rand::random::<u128>()),"config":config,"seed":seed,"periods_per_year":pyp,"context":context}),json!({"status":"execution_not_recorded"}))?;
 let recovered=pipeline_archive::recover(archive,&id,true).map_err(|e|format!("Study {id}: {e}; inspect research_history before retrying"))?;
 Ok((recovered.report,id,recovered.completion_id))
}

pub fn tools()->Vec<Value>{
 [
  ("research_history","Inspect saved research studies without executing them. Incomplete records do not establish process liveness.",json!({})),
  ("research_resume","Replay and resume one frozen research study using the exact running engine. Never executes archived binaries. Completion is reproducibility evidence, not independent validation.",json!({"run_id":{"type":"string"}})),
  ("research_export","Export a complete or partial research study and its ordered checkpoints. No execution or engine binaries are included.",json!({"run_id":{"type":"string"}})),
  ("research_import","Validate and restore a research backup without executing it. Conflicts are rejected. Retry the same backup after partial storage failure.",json!({"backup":{"type":"object"}})),
 ].into_iter().map(|(name,description,properties)|{
  let required:Vec<_>=properties.as_object().unwrap().keys().cloned().collect();
  json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
 }).collect()
}

pub fn call(name:&str,args:&Value,archive:&Archive)->Result<Value,String>{
 let fields=args.as_object().ok_or("Arguments must be an object")?;
 if name=="research_history"{
  if !fields.is_empty(){return Err("History accepts no input overrides".into());}
  let mut studies=Vec::new();
  for id in archive.list()?{let record=archive.load(&id)?;if record.run.request["research_kind"]=="cli_pipeline_started"{
   match pipeline_archive::summary(archive,&id){Ok(summary)=>studies.push(summary),Err(error)=>studies.push(json!({"started_run_id":id,"inspection_error":error}))}
  }}
  return Ok(json!({"studies":studies,"execution_replayed":false}));
 }
 let key=if name=="research_import"{"backup"}else{"run_id"};
 if fields.len()!=1||!fields.contains_key(key){return Err(format!("Only {key} is accepted; frozen inputs cannot be overridden"));}
 if name=="research_import"{return pipeline_archive::import(archive,args["backup"].clone());}
 let id=args["run_id"].as_str().ok_or("run_id must be a string")?;
 match name{
  "research_export"=>pipeline_archive::export(archive,id),
  "research_resume"=>{let result=pipeline_archive::recover(archive,id,true)?;Ok(json!({"started_run_id":id,"completion_id":result.completion_id,"replayed_events":result.replayed,"appended_events":result.appended,"report":result.report}))},
  _=>Err("Unknown research archive tool".into()),
 }
}
