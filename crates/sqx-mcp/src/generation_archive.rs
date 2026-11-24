//! Durable MCP generation evidence; never loads or executes a stored binary.
use serde_json::{json,Value};
use sqx_project::archive::Archive;
use sqx_core::{Bar,Instrument};
use sqx_strategy::Strategy;
use sqx_backtest::{backtest,BacktestConfig};
use sqx_optimizer::{BatchEvaluation,Fitness};

pub fn configured_archive()->Archive {
    Archive::new(std::env::var_os("SQX_MCP_RESEARCH_DIR").map(std::path::PathBuf::from)
        .unwrap_or_else(||std::path::PathBuf::from("data/research")))
}

#[allow(clippy::too_many_arguments)]
pub fn execute(archive:&Archive,strategies:&[Strategy],bars:&[Bar],instrument:&Instrument,config:&BacktestConfig,pyp:f64,seed:u64,context:&Value)
 ->Result<(Vec<BatchEvaluation>,String,String),String> {
    execute_using(archive,strategies,bars,instrument,config,pyp,seed,context,backtest)
}

#[allow(clippy::too_many_arguments)]
fn execute_using<F>(archive:&Archive,strategies:&[Strategy],bars:&[Bar],instrument:&Instrument,config:&BacktestConfig,pyp:f64,seed:u64,context:&Value,mut evaluate:F)
 ->Result<(Vec<BatchEvaluation>,String,String),String>
where F:FnMut(&Strategy,&[Bar],&Instrument,&BacktestConfig)->sqx_backtest::BacktestResult {
    let engine=archive.retain_current_engine()?;
    // Distinguish repeated requests, while frozen prices/rules/seed remain reusable.
    let attempt_id=format!("{:032x}",rand::random::<u128>());
    let dataset=json!({"symbol":instrument.name,"instrument":instrument,"bars":bars,"timeframe":context["timeframe"],"calendar":context["calendar"]});
    let family=json!({"name":"MCP generation candidate family","candidates":strategies});
    let request=json!({"research_kind":"mcp_generation_started","attempt_id":attempt_id,"seed":seed,"config":config,"periods_per_year":pyp,"context":context,"fitness":"ProfitFactor","min_trades":5});
    let search_id=archive.save(engine.clone(),dataset.clone(),family.clone(),request.clone(),json!({"status":"execution_not_recorded"}))?;
    continue_search(archive,&search_id,&engine,&mut evaluate)
        .map(|(evaluations,completion,_,_)|(evaluations,search_id.clone(),completion))
        .map_err(|error|format!("search {search_id}: {error}; inspect generation_history before retrying"))
}

fn search_lock(archive:&Archive,id:&str)->Result<std::fs::File,String> {
    archive.load(id)?; // Validate the content-addressed identifier before using it as a path.
    search_lock_verified_id(archive,id)
}

fn search_lock_verified_id(archive:&Archive,id:&str)->Result<std::fs::File,String> {
    let directory=archive.root().join("mcp-generation-locks");std::fs::create_dir_all(&directory).map_err(|e|e.to_string())?;
    let file=std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(directory.join(id)).map_err(|e|e.to_string())?;
    file.try_lock().map_err(|e|format!("Search is active or its process lock is unavailable: {e}"))?;
    Ok(file) // OS releases the lock on process death; file existence is not liveness.
}

fn continue_search<F>(archive:&Archive,search_id:&str,engine:&str,evaluate:&mut F)
 ->Result<(Vec<BatchEvaluation>,String,usize,usize),String>
where F:FnMut(&Strategy,&[Bar],&Instrument,&BacktestConfig)->sqx_backtest::BacktestResult {
    let _lock=search_lock(archive,search_id)?;
    let search=archive.load(search_id)?;
    if search.run.engine_id!=engine{return Err("Search recovery requires the original engine executable".into());}
    if search.run.request["research_kind"]!="mcp_generation_started" || search.run.result!=json!({"status":"execution_not_recorded"})
        || search.run.request["fitness"]!="ProfitFactor" || search.run.request["min_trades"]!=5 {
        return Err("Unsupported frozen generation search".into());
    }
    let strategies:Vec<Strategy>=serde_json::from_value(search.strategy["candidates"].clone()).map_err(|e|e.to_string())?;
    if strategies.is_empty() || strategies.len()>2000{return Err("Frozen search candidate count must be between 1 and 2000".into());}
    let bars:Vec<Bar>=serde_json::from_value(search.dataset["bars"].clone()).map_err(|e|e.to_string())?;
    let instrument:Instrument=serde_json::from_value(search.dataset["instrument"].clone()).map_err(|e|e.to_string())?;
    let config:BacktestConfig=serde_json::from_value(search.run.request["config"].clone()).map_err(|e|e.to_string())?;
    let pyp=search.run.request["periods_per_year"].as_f64().filter(|p|p.is_finite()&&*p>0.0).ok_or("Invalid frozen annualization")?;
    sqx_backtest::validate_backtest_inputs(&bars,&instrument,&config)?;
    let mut existing=std::collections::BTreeMap::<usize,Vec<String>>::new();
    let mut completions=Vec::new();
    for id in archive.list()? {
        let record=archive.load(&id)?;
        if record.run.request["search_id"]==search_id {
            if !matches!(record.run.request["research_kind"].as_str(),Some("mcp_generation_trial"|"mcp_generation_trial_started")) {return Err("Unknown record refers to generation search".into());}
            let index=record.run.request["candidate_index"].as_u64().and_then(|n|usize::try_from(n).ok()).filter(|&n|n<strategies.len()).ok_or("Invalid recorded candidate index")?;
            let expected=json!({"research_kind":"mcp_generation_trial_started","search_id":search_id,"candidate_index":index,"config":config,"periods_per_year":pyp});
            let checkpoint=if record.run.request["research_kind"]=="mcp_generation_trial" {
                archive.load(record.run.request["started_run_id"].as_str().ok_or("Result has no checkpoint")?)?
            }else{record.clone()};
            if checkpoint.run.request!=expected || checkpoint.run.result!=json!({"status":"execution_not_recorded"})
                || checkpoint.run.engine_id!=engine || checkpoint.run.dataset_id!=search.run.dataset_id
                || checkpoint.run.strategy_id!=sqx_project::archive::content_id(&serde_json::to_vec(&search.strategy["candidates"][index]).map_err(|e|e.to_string())?) {return Err("Trial checkpoint differs from frozen search".into());}
            if record.run.request["research_kind"]=="mcp_generation_trial" {
                if record.run.request!=completed_request(&checkpoint) || record.run.engine_id!=engine
                    || record.run.dataset_id!=checkpoint.run.dataset_id || record.run.strategy_id!=checkpoint.run.strategy_id {return Err("Trial result differs from its checkpoint".into());}
                existing.entry(index).or_default().push(id);
            }
        } else if record.run.request["research_kind"]=="mcp_generation" && record.run.request["started_run_id"]==search_id {
            completions.push(record);
        }
    }
    if !completions.is_empty() && existing.len()!=strategies.len(){return Err("Completed search has missing candidate results; recovery cannot silently replace them".into());}
    let mut evaluations=Vec::with_capacity(strategies.len());let mut trials=Vec::with_capacity(strategies.len());
    let mut verified=0;let mut new_results=0;
    for (candidate_index,strategy) in strategies.iter().enumerate() {
        let strategy_json=serde_json::to_value(strategy).map_err(|e|e.to_string())?;
        let request=json!({"research_kind":"mcp_generation_trial_started","search_id":search_id,"candidate_index":candidate_index,"config":config,"periods_per_year":pyp});
        let checkpoint_id=archive.save(engine.into(),search.dataset.clone(),strategy_json.clone(),request,json!({"status":"execution_not_recorded"}))?;
        let checkpoint=archive.load(&checkpoint_id)?;
        let (evaluation,outcome)=trial_outcome(strategy,evaluate(strategy,&bars,&instrument,&config),bars.len(),pyp);
        let outcome_bytes=serde_json::to_vec(&outcome).map_err(|e|e.to_string())?;
        let result_id=if let Some(ids)=existing.get(&candidate_index) {
            for id in ids {
                let recorded=archive.load(id)?;
                if recorded.run.request!=completed_request(&checkpoint) || serde_json::to_vec(&recorded.run.result).map_err(|e|e.to_string())?!=outcome_bytes {
                    return Err(format!("Replay differs at candidate {candidate_index}; no replacement outcome or search completion was published"));
                }
            }
            verified+=1;ids[0].clone()
        }else{
            let id=archive.save(engine.into(),search.dataset.clone(),strategy_json,completed_request(&checkpoint),outcome)?;
            new_results+=1;id
        };
        trials.push(json!({"candidate_index":candidate_index,"checkpoint_id":checkpoint_id,"result_id":result_id}));
        evaluations.push(evaluation);
    }
    let mut request=search.run.request.clone();request["research_kind"]=json!("mcp_generation");request["started_run_id"]=json!(search_id);
    let outcome=json!({"status":"evaluations_recorded","trials":trials});
    for completed in completions {
        if completed.run.engine_id!=engine || completed.run.dataset_id!=search.run.dataset_id || completed.run.strategy_id!=search.run.strategy_id
            || completed.run.request!=request || serde_json::to_vec(&completed.run.result).map_err(|e|e.to_string())?!=serde_json::to_vec(&outcome).map_err(|e|e.to_string())? {
            return Err("Recorded search completion conflicts with verified trials".into());
        }
    }
    let completion=archive.save(engine.into(),search.dataset,search.strategy,request,outcome)?;
    Ok((evaluations,completion,verified,new_results))
}

/// Replay saved outcomes and finish missing candidates under an exclusive OS lock.
pub fn resume_search(archive:&Archive,id:&str)->Result<Value,String> {
    let engine=archive.retain_current_engine()?;
    let (evaluations,completion,verified,new_results)=continue_search(archive,id,&engine,&mut backtest)?;
    let failed:Vec<_>=evaluations.iter().enumerate().filter(|(_,row)|!row.errors.is_empty()).map(|(index,row)|json!({"candidate_index":index,"strategy":row.strategy,"errors":row.errors})).collect();
    let ranked=sqx_optimizer::rank(evaluations.iter().filter(|row|row.errors.is_empty()).map(|row|(row.strategy.clone(),row.metrics)).collect(),Fitness::ProfitFactor,5);
    Ok(json!({"search_id":id,"completion_id":completion,"evaluations_recorded":evaluations.len(),"previous_results_replayed":verified,"new_results_recorded":new_results,"invalid":failed.len(),"failures":failed,"eligible":ranked.len(),"top_candidates":ranked.iter().take(10).map(|(strategy,metrics)|json!({"strategy":strategy,"metrics":metrics})).collect::<Vec<_>>(),"scope":"Saved outcomes were replayed before completing missing candidates. Completion means evaluation coverage, not valid execution, independent trials, untouched holdout or independent correctness. Repeated recovery replays recorded work without creating a new search."}))
}

fn trial_outcome(strategy:&Strategy,result:sqx_backtest::BacktestResult,bar_count:usize,pyp:f64)->(BatchEvaluation,Value) {
    let metrics=sqx_metrics::compute(&result,bar_count,pyp);
    let mut errors=result.errors.clone();
    if let Err(error)=sqx_metrics::validate_observations(&result){errors.push(error);}
    let eligible=errors.is_empty() && sqx_optimizer::fitness(&metrics,Fitness::ProfitFactor,5).is_finite();
    let outcome=json!({"backtest":result,"metrics":metrics,"errors":errors,"eligible":eligible});
    (BatchEvaluation{strategy:strategy.clone(),metrics,errors},outcome)
}

fn completed_request(checkpoint:&sqx_project::archive::ResearchBundle)->Value {
    let mut request=checkpoint.run.request.clone();
    request["research_kind"]=json!("mcp_generation_trial");
    request["started_run_id"]=json!(checkpoint.run_id);
    request
}

/// Recompute a result or explicitly restart one frozen candidate. Neither mode
/// advances other candidates or publishes a whole-search completion record.
pub fn replay_trial(archive:&Archive,id:&str,restart:bool)->Result<Value,String> {
    let bytes=std::fs::read(std::env::current_exe().map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let engine=sqx_project::archive::content_id(&bytes);
    let record=archive.load(id)?;
    let kind=if restart{"mcp_generation_trial_started"}else{"mcp_generation_trial"};
    if record.run.request["research_kind"]!=kind{return Err("Record type does not match replay/restart mode".into());}
    if record.run.engine_id!=engine{return Err("Trial replay/restart requires the original engine executable".into());}
    let checkpoint=if restart {record.clone()}else{
        archive.load(record.run.request["started_run_id"].as_str().ok_or("Result has no checkpoint identity")?)?
    };
    let search=archive.load(checkpoint.run.request["search_id"].as_str().ok_or("Trial has no search identity")?)?;
    let _lock=if restart {Some(search_lock(archive,&search.run_id)?)}else{None};
    if checkpoint.run.request["research_kind"]!="mcp_generation_trial_started" || search.run.request["research_kind"]!="mcp_generation_started" {
        return Err("Invalid trial/search checkpoint kinds".into());
    }
    if checkpoint.run.result!=json!({"status":"execution_not_recorded"}) || search.run.result!=json!({"status":"execution_not_recorded"}) {
        return Err("Invalid checkpoint state".into());
    }
    let index=checkpoint.run.request["candidate_index"].as_u64().and_then(|n|usize::try_from(n).ok()).ok_or("Invalid candidate index")?;
    let family=search.strategy["candidates"].as_array().ok_or("Search has no candidate family")?;
    if family.get(index)!=Some(&checkpoint.strategy) || checkpoint.run.dataset_id!=search.run.dataset_id
        || checkpoint.run.engine_id!=engine || search.run.engine_id!=engine {
        return Err("Candidate, dataset or engine differs from frozen search".into());
    }
    let expected=json!({"research_kind":"mcp_generation_trial_started","search_id":search.run_id,"candidate_index":index,"config":search.run.request["config"],"periods_per_year":search.run.request["periods_per_year"]});
    if checkpoint.run.request!=expected || search.run.request["fitness"]!="ProfitFactor" || search.run.request["min_trades"]!=5 {
        return Err("Trial settings differ from supported frozen search".into());
    }
    let request=completed_request(&checkpoint);
    if !restart && (record.run.request!=request || record.run.dataset_id!=checkpoint.run.dataset_id || record.run.strategy_id!=checkpoint.run.strategy_id) {
        return Err("Result differs from its trial checkpoint".into());
    }
    let strategy:Strategy=serde_json::from_value(checkpoint.strategy.clone()).map_err(|e|e.to_string())?;
    let bars:Vec<Bar>=serde_json::from_value(checkpoint.dataset["bars"].clone()).map_err(|e|e.to_string())?;
    let instrument:Instrument=serde_json::from_value(checkpoint.dataset["instrument"].clone()).map_err(|e|e.to_string())?;
    let config:BacktestConfig=serde_json::from_value(checkpoint.run.request["config"].clone()).map_err(|e|e.to_string())?;
    let pyp=checkpoint.run.request["periods_per_year"].as_f64().filter(|p|p.is_finite()&&*p>0.0).ok_or("Invalid frozen annualization")?;
    sqx_backtest::validate_backtest_inputs(&bars,&instrument,&config)?;
    if restart {archive.retain_engine(&bytes,&engine)?;}
    let (_,actual)=trial_outcome(&strategy,backtest(&strategy,&bars,&instrument,&config),bars.len(),pyp);
    let valid=actual["errors"].as_array().is_some_and(|errors|errors.is_empty());
    let actual_bytes=serde_json::to_vec(&actual).map_err(|e|e.to_string())?;
    if !restart {
        return Ok(json!({"run_id":id,"matches":actual_bytes==serde_json::to_vec(&record.run.result).map_err(|e|e.to_string())?,"execution_valid":valid,"scope":"Byte-for-byte comparison of the serialized frozen trial outcome, not independent validation. No stored executable was run or result overwritten."}));
    }
    // An already recorded different outcome is a discrepancy, not a new success.
    for existing_id in archive.list()? {
        let existing=archive.load(&existing_id)?;
        if existing.run.request["research_kind"]=="mcp_generation_trial" && existing.run.request["started_run_id"]==checkpoint.run_id {
            if existing.run.request!=request || existing.run.dataset_id!=checkpoint.run.dataset_id || existing.run.strategy_id!=checkpoint.run.strategy_id
                || existing.run.engine_id!=engine || serde_json::to_vec(&existing.run.result).map_err(|e|e.to_string())?!=actual_bytes {
                return Err("Recorded trial outcome conflicts with restart; no replacement result was published".into());
            }
        }
    }
    let result_id=archive.save(engine,checkpoint.dataset,checkpoint.strategy,request,actual)?;
    Ok(json!({"checkpoint_id":id,"run_id":result_id,"restarted":true,"execution_valid":valid,"scope":"Restarted one frozen candidate from the beginning. No other candidate was executed and the search was not marked complete. Repeated restarts are not independent trials."}))
}

/// Inspect content-verified records without retaining all datasets in memory.
/// This is recorded progress, not an assertion that an interrupted worker stopped.
/// Export one complete recorded search graph, without running any strategy.
/// The search writer lock freezes publication by cooperating MCP processes.
pub fn export_search(archive:&Archive,id:&str)->Result<Value,String> {
    let _lock=search_lock(archive,id)?;
    let search=archive.load(id)?;
    if search.run.request["research_kind"]!="mcp_generation_started"
        || search.run.result!=json!({"status":"execution_not_recorded"}) {
        return Err("Expected a frozen generation search".into());
    }
    let candidates=search.strategy["candidates"].as_array().ok_or("Missing candidate family")?;
    if candidates.is_empty() || candidates.len()>2000{return Err("Invalid candidate count".into());}
    let mut records=std::collections::BTreeMap::new();
    records.insert(id.to_owned(),search.clone());
    for run_id in archive.list()? {
        let record=archive.load(&run_id)?;
        if record.run.request["search_id"]==id || record.run.request["started_run_id"]==id {
            records.insert(run_id,record);
        }
    }
    let mut checkpoints=std::collections::BTreeMap::new();
    let mut results=std::collections::BTreeMap::new();
    let mut completions=Vec::new();
    for record in records.values().filter(|record|record.run_id!=id) {
        if record.run.engine_id!=search.run.engine_id || record.run.dataset_id!=search.run.dataset_id {
            return Err("Linked record has different engine or dataset".into());
        }
        match record.run.request["research_kind"].as_str() {
            Some("mcp_generation_trial_started"|"mcp_generation_trial")=>{
                let index=record.run.request["candidate_index"].as_u64().and_then(|n|usize::try_from(n).ok()).filter(|&n|n<candidates.len()).ok_or("Invalid candidate index")?;
                let expected=json!({"research_kind":"mcp_generation_trial_started","search_id":id,"candidate_index":index,"config":search.run.request["config"],"periods_per_year":search.run.request["periods_per_year"]});
                let checkpoint=if record.run.request["research_kind"]=="mcp_generation_trial" {
                    records.get(record.run.request["started_run_id"].as_str().ok_or("Missing checkpoint identity")?).ok_or("Missing linked checkpoint")?
                }else{record};
                if checkpoint.run.request!=expected || checkpoint.run.result!=json!({"status":"execution_not_recorded"})
                    || record.run.strategy_id!=sqx_project::archive::content_id(&serde_json::to_vec(&candidates[index]).map_err(|e|e.to_string())?)
                    || checkpoint.run.strategy_id!=record.run.strategy_id {
                    return Err("Candidate record differs from frozen search".into());
                }
                if record.run.request["research_kind"]=="mcp_generation_trial" {
                    if record.run.request!=completed_request(checkpoint){return Err("Result differs from checkpoint".into());}
                    if results.insert(index,record.run_id.clone()).is_some(){return Err("Conflicting candidate outcomes".into());}
                }else if checkpoints.insert(index,record.run_id.clone()).is_some(){return Err("Conflicting candidate checkpoints".into());}
            },
            Some("mcp_generation")=>completions.push(record),
            _=>return Err("Unknown record refers to search".into()),
        }
    }
    let mut expected_request=search.run.request.clone();expected_request["research_kind"]=json!("mcp_generation");expected_request["started_run_id"]=json!(id);
    let trials:Vec<_>=results.iter().map(|(index,result)|json!({"candidate_index":index,"checkpoint_id":checkpoints.get(index),"result_id":result})).collect();
    let outcome=json!({"status":"evaluations_recorded","trials":trials});
    for record in &completions {
        if results.len()!=candidates.len() || record.run.strategy_id!=search.run.strategy_id
            || record.run.request!=expected_request || record.run.result!=outcome {
            return Err("Completion differs from linked candidate records".into());
        }
    }
    let complete=!completions.is_empty();
    Ok(json!({"schema":1,"kind":"mcp_generation_backup","search_id":id,
        "recorded_completion":complete,"records":records.values().collect::<Vec<_>>(),
        "engine_bytes_included":false,
        "scope":"Content and record relationships checked; no strategy execution, outcome replay, origin authentication or independent validation. Includes all records linked to this search at export time. Executable bytes and unrelated holdout/experiment records are not included."}))
}

/// Version 2 transport stores shared objects once; run identities are unchanged.
pub fn export_compact(archive:&Archive,id:&str)->Result<Value,String> {
    compact_backup(export_search(archive,id)?)
}
fn compact_backup(mut backup:Value)->Result<Value,String> {
    let rows=backup["records"].as_array().ok_or("Missing backup records")?;
    let mut datasets=serde_json::Map::new();let mut strategies=serde_json::Map::new();let mut records=Vec::new();
    for row in rows {
        let record:sqx_project::archive::ResearchBundle=serde_json::from_value(row.clone()).map_err(|e|e.to_string())?;
        datasets.insert(record.run.dataset_id.clone(),record.dataset);
        strategies.insert(record.run.strategy_id.clone(),record.strategy);
        records.push(json!({"run_id":record.run_id,"run":record.run}));
    }
    backup["schema"]=json!(2);backup["records"]=json!(records);
    backup["datasets"]=json!(datasets);backup["strategies"]=json!(strategies);
    Ok(backup)
}
fn expand_compact(backup:&Value)->Result<Value,String> {
    let object=backup.as_object().ok_or("Backup must be an object")?;
    if object.len()!=9 || object.keys().any(|key|!matches!(key.as_str(),"schema"|"kind"|"search_id"|"recorded_completion"|"records"|"engine_bytes_included"|"scope"|"datasets"|"strategies")) {
        return Err("Unsupported compact backup envelope".into());
    }
    let datasets=backup["datasets"].as_object().ok_or("Missing shared datasets")?;
    let strategies=backup["strategies"].as_object().ok_or("Missing shared strategies")?;
    let rows=backup["records"].as_array().ok_or("Missing backup records")?;
    if rows.is_empty() || rows.len()>4002{return Err("Invalid backup record count".into());}
    let mut used_data=std::collections::BTreeSet::new();let mut used_rules=std::collections::BTreeSet::new();let mut records=Vec::new();
    for row in rows {
        let fields=row.as_object().ok_or("Invalid compact record")?;
        if fields.len()!=2 || !fields.contains_key("run_id") || !fields.contains_key("run"){return Err("Invalid compact record fields".into());}
        let run:sqx_project::archive::RunRecord=serde_json::from_value(row["run"].clone()).map_err(|e|e.to_string())?;
        let dataset=datasets.get(&run.dataset_id).ok_or("Missing referenced dataset")?;
        let strategy=strategies.get(&run.strategy_id).ok_or("Missing referenced strategy")?;
        used_data.insert(run.dataset_id.clone());used_rules.insert(run.strategy_id.clone());
        records.push(json!({"run_id":row["run_id"],"run":run,"dataset":dataset,"strategy":strategy}));
    }
    if used_data.len()!=datasets.len() || used_rules.len()!=strategies.len(){return Err("Unreferenced shared objects in backup".into());}
    let mut expanded=backup.clone();let fields=expanded.as_object_mut().unwrap();fields.remove("datasets");fields.remove("strategies");
    expanded["schema"]=json!(1);expanded["records"]=json!(records);Ok(expanded)
}

/// Validate in a private staging archive; publish only after checking the merged
/// destination graph under its writer lock. No executable bytes are accepted.
pub fn import_search(archive:&Archive,backup:&Value)->Result<Value,String> {
    if backup["schema"]==2 {return import_search(archive,&expand_compact(backup)?);}
    let object=backup.as_object().ok_or("Backup must be an object")?;
    if object.len()!=7 || object.keys().any(|key|!matches!(key.as_str(),"schema"|"kind"|"search_id"|"recorded_completion"|"records"|"engine_bytes_included"|"scope"))
        || backup["schema"]!=1 || backup["kind"]!="mcp_generation_backup" || backup["engine_bytes_included"]!=false
        || !backup["recorded_completion"].is_boolean() || !backup["scope"].is_string() {
        return Err("Unsupported generation backup envelope".into());
    }
    let id=backup["search_id"].as_str().ok_or("Missing search identity")?;
    let rows=backup["records"].as_array().ok_or("Missing backup records")?;
    if rows.is_empty() || rows.len()>4002{return Err("Invalid backup record count".into());}
    struct Stage(std::path::PathBuf);
    impl Drop for Stage {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
    let path=std::env::temp_dir().join(format!("sqx-generation-import-{:032x}",rand::random::<u128>()));
    let mut directory=std::fs::DirBuilder::new();
    #[cfg(unix)] {use std::os::unix::fs::DirBuilderExt;directory.mode(0o700);}
    directory.create(&path).map_err(|e|e.to_string())?;
    let stage=Stage(path);let staging=Archive::new(&stage.0);
    let mut ids=std::collections::BTreeSet::new();
    for row in rows {
        let record:sqx_project::archive::ResearchBundle=serde_json::from_value(row.clone()).map_err(|e|e.to_string())?;
        if !ids.insert(record.run_id.clone()){return Err("Duplicate backup record".into());}
        staging.restore(&record)?;
    }
    let checked=export_search(&staging,id)?; // Also validates id before destination path construction.
    if checked["records"].as_array().unwrap().len()!=rows.len()
        || checked["recorded_completion"]!=backup["recorded_completion"] {
        return Err("Backup contains unrelated records or a false completion status".into());
    }
    let _lock=search_lock_verified_id(archive,id)?;
    // Validate both histories together. Existing conflicting results must never be
    // hidden by importing a preferred outcome or an incomplete backup.
    for existing in archive.list()? {
        let record=archive.load(&existing)?;
        if existing==id || record.run.request["search_id"]==id || record.run.request["started_run_id"]==id {
            staging.restore(&record)?;
        }
    }
    let merged=export_search(&staging,id)?;
    let mut records:Vec<sqx_project::archive::ResearchBundle>=serde_json::from_value(merged["records"].clone()).map_err(|e|e.to_string())?;
    // Checkpoints precede results, and completion is published last. A storage
    // failure may leave an incomplete import; repeating the import is idempotent.
    records.sort_by_key(|record|match record.run.request["research_kind"].as_str(){
        Some("mcp_generation_started")=>0,Some("mcp_generation_trial_started")=>1,
        Some("mcp_generation_trial")=>2,_=>3,
    });
    for record in &records {
        archive.restore(record).map_err(|error|format!("Import publication incomplete; retry the same backup after resolving storage error: {error}"))?;
    }
    Ok(json!({"search_id":id,"records_retained":records.len(),"recorded_completion":merged["recorded_completion"],
        "engine_retention":archive.engine_retention(&records[0].run.engine_id),
        "scope":"Record identities and merged relationships verified. No strategy replay or execution. Storage failure can leave a partial import; retries are idempotent. Engine binaries and unrelated experiment/holdout evidence are not restored."}))
}

pub fn history(archive:&Archive)->Result<Value,String> {
    let mut searches=std::collections::BTreeMap::new();
    let mut trial_starts=Vec::new();let mut trial_results=Vec::new();let mut completions=Vec::new();
    for id in archive.list()? {
        let bundle=archive.load(&id)?;
        match bundle.run.request["research_kind"].as_str() {
            Some("mcp_generation_started")=>{
                let count=bundle.strategy["candidates"].as_array().ok_or("missing generation candidates")?.len();
                searches.insert(id,json!({"planned_candidates":count,"engine_id":bundle.run.engine_id,"seed":bundle.run.request["seed"],"trial_checkpoints":[],"trial_results":[],"completion_records":[]}));
            },
            Some("mcp_generation_trial_started")=>trial_starts.push((bundle.run.request["search_id"].as_str().ok_or("missing search identity")?.to_owned(),json!({"candidate_index":bundle.run.request["candidate_index"],"checkpoint_id":id}))),
            Some("mcp_generation_trial")=>trial_results.push((bundle.run.request["search_id"].as_str().ok_or("missing search identity")?.to_owned(),json!({"candidate_index":bundle.run.request["candidate_index"],"checkpoint_id":bundle.run.request["started_run_id"],"result_id":id,"execution_valid":bundle.run.result["errors"].as_array().is_some_and(|errors|errors.is_empty()),"eligible":bundle.run.result["eligible"]}))),
            Some("mcp_generation")=>completions.push((bundle.run.request["started_run_id"].as_str().ok_or("missing search identity")?.to_owned(),json!(id))),
            _=>{},
        }
    }
    let mut orphaned=Vec::new();
    for (field,records) in [("trial_checkpoints",trial_starts),("trial_results",trial_results),("completion_records",completions)] {
        for (search_id,record) in records {
            match searches.get_mut(&search_id) {
                Some(search)=>search[field].as_array_mut().unwrap().push(record),
                None=>orphaned.push(json!({"search_id":search_id,"kind":field,"record":record})),
            }
        }
    }
    Ok(json!({"searches":searches,"orphaned_records":orphaned,"scope":"Content-verified recorded progress, not authenticated origin, process liveness, full relationship/replay verification or independent validation. Planned candidates are not completed or independent trials. No automatic resume is performed."}))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(std::path::PathBuf);
    impl Temp {fn new()->Self{Self(std::env::temp_dir().join(format!("sqx-mcp-generation-{:032x}",rand::random::<u128>())))} }
    impl Drop for Temp {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
    fn bars()->Vec<Bar>{vec![Bar::new(0,1.0,1.1,0.9,1.0,1.0),Bar::new(60000,1.0,1.1,0.9,1.0,1.0)]}
    #[test]
    fn compact_transport_preserves_ids_and_rejects_missing_altered_or_extra_objects(){
        let source=Temp::new();let archive=Archive::new(&source.0);
        let (_,id,_)=execute(&archive,&[Strategy::new("one"),Strategy::new("two")],&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,7,&json!({"timeframe":"M1"})).unwrap();
        let legacy=export_search(&archive,&id).unwrap();let compact=export_compact(&archive,&id).unwrap();
        assert_eq!(compact["schema"],2);assert_eq!(compact["datasets"].as_object().unwrap().len(),1);
        assert_eq!(compact["strategies"].as_object().unwrap().len(),3);
        assert_eq!(expand_compact(&compact).unwrap(),legacy);
        let target=Temp::new();let destination=Archive::new(&target.0);
        for mutation in 0..5 {
            let mut damaged=compact.clone();let key=damaged["datasets"].as_object().unwrap().keys().next().unwrap().clone();
            match mutation {
                0=>{damaged["datasets"].as_object_mut().unwrap().remove(&key);},
                1=>{damaged["datasets"][&key]["bars"]=json!([]);},
                2=>{damaged["datasets"]["extra"]=json!({});},
                3=>{damaged["records"][0]["dataset"]=json!({});},
                _=>{damaged["strategies"]=json!({});},
            }
            assert!(import_search(&destination,&damaged).is_err());assert!(destination.list().unwrap().is_empty());
        }
        import_search(&destination,&compact).unwrap();assert_eq!(export_compact(&destination,&id).unwrap(),compact);
        import_search(&destination,&legacy).unwrap();assert_eq!(export_compact(&destination,&id).unwrap(),compact);
    }
    #[test]
    fn import_validates_before_publication_and_survives_source_deletion(){
        let source=Temp::new();let archive=Archive::new(&source.0);
        let (_,id,completion)=execute(&archive,&[Strategy::new("one"),Strategy::new("two")],&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,7,&json!({"timeframe":"M1"})).unwrap();
        let backup=export_search(&archive,&id).unwrap();
        std::fs::remove_dir_all(&source.0).unwrap();
        let interrupted=Temp::new();let interrupted_archive=Archive::new(&interrupted.0);
        let candidate=backup["records"].as_array().unwrap().iter().find(|row|row["run"]["request"]["research_kind"]=="mcp_generation_trial_started").unwrap();
        let broken=interrupted.0.join("strategies").join(format!("{}.json",candidate["run"]["strategy_id"].as_str().unwrap()));
        std::fs::create_dir_all(broken.parent().unwrap()).unwrap();std::fs::write(&broken,b"corrupted object").unwrap();
        assert!(import_search(&interrupted_archive,&backup).unwrap_err().contains("publication incomplete"));
        assert!(!interrupted.0.join("runs").join(format!("{completion}.json")).exists());
        assert!(!interrupted_archive.list().unwrap().is_empty(),"failure should exercise partial publication");
        std::fs::remove_file(broken).unwrap();
        assert_eq!(import_search(&interrupted_archive,&backup).unwrap()["recorded_completion"],true);
        assert_eq!(export_search(&interrupted_archive,&id).unwrap(),backup);
        let target=Temp::new();let destination=Archive::new(&target.0);
        for mutation in 0..5 {
            let mut damaged=backup.clone();
            match mutation {
                0=>{damaged["records"][0]["dataset"]["bars"]=json!([]);},
                1=>{let duplicate=damaged["records"][0].clone();damaged["records"].as_array_mut().unwrap().push(duplicate);},
                2=>{damaged["recorded_completion"]=json!(false);},
                3=>{damaged["engine_bytes_included"]=json!(true);},
                _=>{damaged["records"].as_array_mut().unwrap().retain(|row|row["run"]["request"]["research_kind"]!="mcp_generation_trial_started");},
            }
            assert!(import_search(&destination,&damaged).is_err());assert!(destination.list().unwrap().is_empty());
        }
        let report=import_search(&destination,&backup).unwrap();assert_eq!(report["recorded_completion"],true);
        assert_eq!(report["engine_retention"]["status"],"missing");
        assert_eq!(export_search(&destination,&id).unwrap(),backup);
        let before=destination.list().unwrap();import_search(&destination,&backup).unwrap();assert_eq!(destination.list().unwrap(),before);
        let replayed=resume_search(&destination,&id).unwrap();assert_eq!(replayed["previous_results_replayed"],2);assert_eq!(replayed["new_results_recorded"],0);
        assert_eq!(destination.list().unwrap(),before);
        let guard=search_lock(&destination,&id).unwrap();assert!(import_search(&destination,&backup).unwrap_err().contains("active"));drop(guard);
        let done=destination.load(&completion).unwrap();let trial=destination.load(done.run.result["trials"][0]["result_id"].as_str().unwrap()).unwrap();
        let mut conflict=trial.run.result.clone();conflict["eligible"]=json!(true);
        destination.save(trial.run.engine_id,trial.dataset,trial.strategy,trial.run.request,conflict).unwrap();
        let before=destination.list().unwrap();assert!(import_search(&destination,&backup).unwrap_err().contains("Conflicting"));assert_eq!(destination.list().unwrap(),before);
        let partial=Temp::new();let partial_archive=Archive::new(&partial.0);
        let mut incomplete=backup.clone();incomplete["records"].as_array_mut().unwrap().retain(|row|row["run_id"]!=completion);incomplete["recorded_completion"]=json!(false);
        assert_eq!(import_search(&partial_archive,&incomplete).unwrap()["recorded_completion"],false);
        assert_eq!(export_search(&partial_archive,&id).unwrap(),incomplete);
    }
    #[test]
    fn export_preserves_entire_search_and_rejects_missing_conflicting_or_active_records(){
        let temp=Temp::new();let archive=Archive::new(&temp.0);
        let (_,id,completion)=execute_using(&archive,&[Strategy::new("rejected"),Strategy::new("failed")],&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,42,&json!({"timeframe":"M1"}),|s,b,i,c|{
            let mut result=backtest(s,b,i,c);if s.name=="failed"{result.errors.push("injected failure".into());}result
        }).unwrap();
        let backup=export_search(&archive,&id).unwrap();
        assert_eq!(backup["recorded_completion"],true);assert_eq!(backup["engine_bytes_included"],false);
        assert_eq!(backup["records"].as_array().unwrap().len(),6);
        assert!(backup["records"].as_array().unwrap().iter().any(|r|r["run"]["result"]["errors"].as_array().is_some_and(|errors|errors.contains(&json!("injected failure")))));
        let restored=Temp::new();let destination=Archive::new(&restored.0);
        for row in backup["records"].as_array().unwrap(){destination.restore(&serde_json::from_value(row.clone()).unwrap()).unwrap();}
        assert_eq!(export_search(&destination,&id).unwrap(),backup);
        let guard=search_lock(&archive,&id).unwrap();assert!(export_search(&archive,&id).unwrap_err().contains("active"));drop(guard);
        let done=archive.load(&completion).unwrap();let result_id=done.run.result["trials"][0]["result_id"].as_str().unwrap();
        let result=archive.load(result_id).unwrap();let mut altered=result.run.result.clone();altered["eligible"]=json!(true);
        archive.save(result.run.engine_id,result.dataset,result.strategy,result.run.request,altered).unwrap();
        assert!(export_search(&archive,&id).unwrap_err().contains("Conflicting"));
        std::fs::remove_file(restored.0.join("runs").join(format!("{result_id}.json"))).unwrap();
        assert!(export_search(&destination,&id).unwrap_err().contains("Completion"));
        std::fs::remove_file(restored.0.join("runs").join(format!("{completion}.json"))).unwrap();
        let partial=export_search(&destination,&id).unwrap();assert_eq!(partial["recorded_completion"],false);
        assert_eq!(partial["records"].as_array().unwrap().len(),4);
    }
    #[test]
    fn whole_search_recovery_replays_prefix_and_finishes_once_under_an_exclusive_lock(){
        let temp=Temp::new();let archive=Archive::new(&temp.0);
        let strategies=vec![Strategy::new("first"),Strategy::new("interrupted"),Strategy::new("remaining")];
        assert!(std::panic::catch_unwind(||execute_using(&archive,&strategies,&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,42,&json!({"timeframe":"M1"}),|s,b,i,c|{
            assert_ne!(s.name,"interrupted","injected interruption");backtest(s,b,i,c)
        })).is_err());
        let report=history(&archive).unwrap();let id=report["searches"].as_object().unwrap().keys().next().unwrap();
        let guard=search_lock(&archive,id).unwrap();
        assert!(resume_search(&archive,id).unwrap_err().contains("active"));
        let checkpoint=report["searches"][id]["trial_checkpoints"][0]["checkpoint_id"].as_str().unwrap();
        assert!(replay_trial(&archive,checkpoint,true).unwrap_err().contains("active"));
        drop(guard);
        let recovered=resume_search(&archive,id).unwrap();
        assert_eq!(recovered["evaluations_recorded"],3);assert_eq!(recovered["previous_results_replayed"],1);assert_eq!(recovered["new_results_recorded"],2);
        assert_eq!(recovered["invalid"],0);assert_eq!(recovered["eligible"],0);
        let before=archive.list().unwrap();let repeated=resume_search(&archive,id).unwrap();
        assert_eq!(repeated["completion_id"],recovered["completion_id"]);assert_eq!(repeated["previous_results_replayed"],3);assert_eq!(repeated["new_results_recorded"],0);
        assert_eq!(archive.list().unwrap(),before);
        let completion=archive.load(recovered["completion_id"].as_str().unwrap()).unwrap();let mut broken=completion.run.result.clone();broken["trials"]=json!([]);
        archive.save(completion.run.engine_id,completion.dataset,completion.strategy,completion.run.request,broken).unwrap();
        let before=archive.list().unwrap();assert!(resume_search(&archive,id).unwrap_err().contains("completion conflicts"));assert_eq!(archive.list().unwrap(),before);
    }
    #[test]
    fn replay_checks_relationships_and_restart_never_overwrites_a_conflicting_result(){
        let temp=Temp::new();let archive=Archive::new(&temp.0);
        let (_,search_id,completion)=execute(&archive,&[Strategy::new("frozen")],&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,42,&json!({"timeframe":"M1"})).unwrap();
        let completed=archive.load(&completion).unwrap();let trial=&completed.run.result["trials"][0];
        let checkpoint_id=trial["checkpoint_id"].as_str().unwrap();let result_id=trial["result_id"].as_str().unwrap();
        assert_eq!(replay_trial(&archive,result_id,false).unwrap()["matches"],true);
        assert_eq!(replay_trial(&archive,checkpoint_id,true).unwrap()["run_id"],result_id);
        assert!(replay_trial(&archive,checkpoint_id,false).is_err());assert!(replay_trial(&archive,result_id,true).is_err());
        let checkpoint=archive.load(checkpoint_id).unwrap();
        for changed in [json!({"candidate_index":1}),json!({"config":{"initial_deposit":123.0}})] {
            let mut request=checkpoint.run.request.clone();for (key,value) in changed.as_object().unwrap(){request[key]=value.clone();}
            let id=archive.save(checkpoint.run.engine_id.clone(),checkpoint.dataset.clone(),checkpoint.strategy.clone(),request,checkpoint.run.result.clone()).unwrap();
            assert!(replay_trial(&archive,&id,true).is_err());
        }
        let wrong_engine=archive.save("0".repeat(64),checkpoint.dataset.clone(),checkpoint.strategy.clone(),checkpoint.run.request.clone(),checkpoint.run.result.clone()).unwrap();
        assert!(replay_trial(&archive,&wrong_engine,true).unwrap_err().contains("original engine"));
        let wrong_strategy=archive.save(checkpoint.run.engine_id.clone(),checkpoint.dataset.clone(),json!({"name":"other"}),checkpoint.run.request.clone(),checkpoint.run.result.clone()).unwrap();
        assert!(replay_trial(&archive,&wrong_strategy,true).unwrap_err().contains("differs from frozen search"));
        let original=archive.load(result_id).unwrap();
        let mut signed_zero=original.run.result.clone();let net=signed_zero["metrics"]["net_profit"].as_f64().unwrap();assert_eq!(net,0.0);
        signed_zero["metrics"]["net_profit"]=json!(-net);
        let signed_zero_id=archive.save(original.run.engine_id.clone(),original.dataset.clone(),original.strategy.clone(),original.run.request.clone(),signed_zero).unwrap();
        assert_eq!(replay_trial(&archive,&signed_zero_id,false).unwrap()["matches"],false,"numeric equality must not replace serialized-byte equality");
        let mut changed_result=original.run.result.clone();changed_result["backtest"]["final_equity"]=json!(123.0);
        let altered=archive.save(original.run.engine_id.clone(),original.dataset.clone(),original.strategy.clone(),original.run.request.clone(),changed_result).unwrap();
        assert_eq!(replay_trial(&archive,&altered,false).unwrap()["matches"],false);
        let before=archive.list().unwrap();assert!(replay_trial(&archive,checkpoint_id,true).unwrap_err().contains("conflicts with restart"));assert_eq!(archive.list().unwrap(),before);
        assert_eq!(archive.load(result_id).unwrap().run.result,original.run.result);
        assert_eq!(history(&archive).unwrap()["searches"][&search_id]["completion_records"],json!([completion]));
    }
    #[test]
    fn full_results_and_failed_candidates_survive_reopen_and_repeated_searches_stay_distinct(){
        let temp=Temp::new();let archive=Archive::new(&temp.0);
        let strategies=vec![Strategy::new("valid"),Strategy::new("failed")];
        let (rows,id,completion)=execute_using(&archive,&strategies,&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,42,&json!({"timeframe":"M1"}),|s,b,i,c|{
            let mut r=backtest(s,b,i,c);if s.name=="failed"{r.errors.push("injected execution failure".into());}r
        }).unwrap();
        assert_eq!(rows.len(),2);assert!(!rows[1].errors.is_empty());
        let reopened=Archive::new(&temp.0);let completed=reopened.load(&completion).unwrap();
        let trials=completed.run.result["trials"].as_array().unwrap();assert_eq!(trials.len(),2);
        for trial in trials {
            let result=reopened.load(trial["result_id"].as_str().unwrap()).unwrap();
            let checkpoint=reopened.load(trial["checkpoint_id"].as_str().unwrap()).unwrap();
            assert_eq!(result.run.dataset_id,checkpoint.run.dataset_id);assert_eq!(result.run.strategy_id,checkpoint.run.strategy_id);
            assert!(result.run.result.get("backtest").is_some());assert!(result.run.result.get("metrics").is_some());
            assert_eq!(result.run.request["search_id"],id);
        }
        let report=history(&reopened).unwrap();assert_eq!(report["searches"][&id]["trial_results"].as_array().unwrap().len(),2);
        assert_eq!(report["searches"][&id]["completion_records"],json!([completion]));
        let (_,second,_)=execute(&archive,&strategies,&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,42,&json!({"timeframe":"M1"})).unwrap();
        assert_ne!(id,second);assert_eq!(history(&archive).unwrap()["searches"].as_object().unwrap().len(),2);
    }
    #[test]
    fn interrupted_candidate_keeps_prior_result_and_pending_checkpoint_without_completion(){
        let temp=Temp::new();let archive=Archive::new(&temp.0);
        let strategies=vec![Strategy::new("first"),Strategy::new("interrupted"),Strategy::new("not started")];
        let interrupted=std::panic::catch_unwind(||{
            execute_using(&archive,&strategies,&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,42,&json!({"timeframe":"M1"}),|s,b,i,c|{
                assert_ne!(s.name,"interrupted","simulated interruption after checkpoint");backtest(s,b,i,c)
            })
        });assert!(interrupted.is_err());
        let report=history(&Archive::new(&temp.0)).unwrap();let searches=report["searches"].as_object().unwrap();assert_eq!(searches.len(),1);
        let search=searches.values().next().unwrap();assert_eq!(search["planned_candidates"],3);
        assert_eq!(search["trial_checkpoints"].as_array().unwrap().len(),2);
        assert_eq!(search["trial_results"].as_array().unwrap().len(),1);
        assert!(search["completion_records"].as_array().unwrap().is_empty());
        let pending=search["trial_checkpoints"].as_array().unwrap().iter().find(|trial|trial["candidate_index"]==1).unwrap()["checkpoint_id"].as_str().unwrap();
        let restarted=replay_trial(&archive,pending,true).unwrap();assert_eq!(restarted["restarted"],true);assert_eq!(restarted["execution_valid"],true);
        assert_eq!(replay_trial(&archive,restarted["run_id"].as_str().unwrap(),false).unwrap()["matches"],true);
        let after=history(&archive).unwrap();let search=after["searches"].as_object().unwrap().values().next().unwrap();
        assert_eq!(search["trial_checkpoints"].as_array().unwrap().len(),2);assert_eq!(search["trial_results"].as_array().unwrap().len(),2);
        assert!(search["completion_records"].as_array().unwrap().is_empty());
    }
    #[test]
    fn result_storage_failure_stops_before_the_next_candidate(){
        let temp=Temp::new();let archive=Archive::new(&temp.0);let calls=std::cell::Cell::new(0);
        let result=execute_using(&archive,&[Strategy::new("first"),Strategy::new("must not run")],&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,42,&json!({}),|s,b,i,c|{
            calls.set(calls.get()+1);
            let result=backtest(s,b,i,c);
            std::fs::rename(temp.0.join("runs"),temp.0.join("saved-runs")).unwrap();
            std::fs::write(temp.0.join("runs"),b"simulate unavailable record directory").unwrap();
            result
        });
        assert!(result.is_err());assert_eq!(calls.get(),1);
        std::fs::remove_file(temp.0.join("runs")).unwrap();std::fs::rename(temp.0.join("saved-runs"),temp.0.join("runs")).unwrap();
        let report=history(&archive).unwrap();let search=report["searches"].as_object().unwrap().values().next().unwrap();
        assert_eq!(search["trial_checkpoints"].as_array().unwrap().len(),1);
        assert!(search["trial_results"].as_array().unwrap().is_empty());
        assert!(search["completion_records"].as_array().unwrap().is_empty());
    }
    #[test]
    fn checkpoint_storage_failure_prevents_any_evaluation(){
        let temp=Temp::new();std::fs::create_dir_all(&temp.0).unwrap();
        std::fs::write(temp.0.join("runs"),b"block record storage").unwrap();
        let calls=std::cell::Cell::new(0);
        let result=execute_using(&Archive::new(&temp.0),&[Strategy::new("blocked")],&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,42,&json!({}),|s,b,i,c|{calls.set(calls.get()+1);backtest(s,b,i,c)});
        assert!(result.is_err());assert_eq!(calls.get(),0);
    }
}
