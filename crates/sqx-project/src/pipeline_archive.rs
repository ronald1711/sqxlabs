//! Pipeline prefix inspection and deterministic recovery. Stored binaries are never executed.
use serde_json::{json,Value};
use crate::archive::{Archive,ResearchBundle,RunRecord};
use std::collections::BTreeMap;

const SCOPE:&str="Ordered build/retest and walk-forward trial checkpoints, with full walk-forward execution outcomes. Recovery replays the saved prefix before appending missing events. Full-data discovery is exploratory, not an untouched holdout.";

/// Shared by web request creation and portable restore, including other processes.
pub fn stress_request_lock(archive:&Archive,key:&str)->Result<std::fs::File,String>{
    if key.is_empty() || key.len()>128 || !key.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-'){return Err("Invalid stress-test request key".into());}
    let directory=archive.root().join("stress-request-locks");std::fs::create_dir_all(&directory).map_err(|e|e.to_string())?;
    let file=std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true)
        .open(directory.join(crate::archive::content_id(key.as_bytes()))).map_err(|e|e.to_string())?;
    file.try_lock().map_err(|e|format!("Stress-test request creation or restore is busy or unavailable; retry the same request key: {e}"))?;
    Ok(file)
}

fn lock(archive:&Archive,id:&str)->Result<std::fs::File,String>{
    archive.load(id)?;
    lock_validated(archive,id)
}
fn lock_validated(archive:&Archive,id:&str)->Result<std::fs::File,String>{
    let directory=archive.root().join("pipeline-locks");
    std::fs::create_dir_all(&directory).map_err(|e|e.to_string())?;
    let file=std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(directory.join(id)).map_err(|e|e.to_string())?;
    file.try_lock().map_err(|e|format!("Pipeline is active or its process lock is unavailable: {e}"))?;
    Ok(file)
}

#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PortableRecord {run_id:String,run:crate::archive::RunRecord}
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    schema:u32,
    checksum:String,
    start:ResearchBundle,
    events:Vec<PortableRecord>,
    completion:Option<PortableRecord>,
}
fn checksum(backup:&Backup)->Result<String,String>{
    Ok(crate::archive::content_id(&serde_json::to_vec(backup).map_err(|e|e.to_string())?))
}

pub fn export(archive:&Archive,id:&str)->Result<Value,String>{
    let _guard=lock(archive,id)?;let state=load(archive,id)?;
    let mut events=Vec::new();let mut reader=archive.record_session();
    for event_id in &state.events {
        events.push(PortableRecord{run_id:event_id.clone(),run:reader.load_run(event_id)?});
    }
    let mut backup=Backup{schema:1,checksum:String::new(),start:state.root,events,
        completion:state.completion.map(|b|PortableRecord{run_id:b.run_id,run:b.run})};
    backup.checksum=checksum(&backup)?;
    serde_json::to_value(backup).map_err(|e|e.to_string())
}

/// Validate the complete incoming and merged graph before publishing any record.
/// Storage failure during publication can leave a prefix; importing again is safe.
pub fn import(archive:&Archive,value:Value)->Result<Value,String>{
    let mut backup:Backup=serde_json::from_value(value).map_err(|e|e.to_string())?;
    if backup.schema!=1 {return Err("Unsupported pipeline backup schema".into());}
    let expected=std::mem::take(&mut backup.checksum);
    if checksum(&backup)?!=expected {return Err("Pipeline backup checksum mismatch".into());}
    let path=std::env::temp_dir().join(format!("sqx-pipeline-import-{:032x}",rand::random::<u128>()));
    let mut builder=std::fs::DirBuilder::new();
    #[cfg(unix)] {use std::os::unix::fs::DirBuilderExt;builder.mode(0o700);}
    builder.create(&path).map_err(|e|e.to_string())?;
    struct Temporary(std::path::PathBuf);
    impl Drop for Temporary {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
    let _temporary=Temporary(path.clone());let staging=Archive::new(path);
    staging.restore(&backup.start)?;
    let mut incoming=vec![backup.start.run_id.clone()];let mut staging_reader=staging.record_session();
    for record in backup.events.iter().chain(backup.completion.iter()) {
        if incoming.contains(&record.run_id){return Err("Duplicate pipeline backup record".into());}
        staging_reader.restore_run(&record.run_id,&record.run)?;incoming.push(record.run_id.clone());
    }
    let id=&backup.start.run_id;
    let state=load(&staging,id)?;
    if state.events.len()!=backup.events.len() || state.completion.is_some()!=backup.completion.is_some(){
        return Err("Backup contains unrelated or incorrectly classified records".into());
    }
    // Validate container order as well as the stored sequence/previous links.
    if state.events.iter().ne(backup.events.iter().map(|e|&e.run_id)) {
        return Err("Backup events are not in recorded order".into());
    }
    let request_key=if matches!(state.root.run.request["origin"].as_str(),Some("web_robustness"|"web_portfolio"|"web_portfolio_combination"|"web_optimization")) {
        state.root.run.request["web_request"].get("request_key").map(|value|value.as_str().ok_or("Invalid stress-test request key")).transpose()?
    }else{None};
    let _request_guard=request_key.map(|key|stress_request_lock(archive,key)).transpose()?;
    let _guard=lock_validated(archive,id)?; // ID was verified by restore/load above.
    let existing=archive.list()?;
    let mut existing_reader=archive.record_session();
    for record_id in &existing {
        let record=PortableRecord{run_id:record_id.clone(),run:existing_reader.load_run(record_id)?};
        if let Some(key)=request_key {
            if record.run_id!=*id && record.run.request["research_kind"]=="cli_pipeline_started" && matches!(record.run.request["origin"].as_str(),Some("web_robustness"|"web_portfolio"|"web_portfolio_combination"|"web_optimization")) && record.run.request["web_request"]["request_key"]==key {
                return Err("Restored stress-test request key conflicts with an existing study".into());
            }
        }
        if record.run_id==*id || record.run.request["started_run_id"]==*id {staging_reader.restore_run(&record.run_id,&record.run)?;}
    }
    let merged=load(&staging,id)?;
    let mut published=0;
    // Publish shared inputs once, then every checked event; completion stays last.
    archive.restore(&backup.start)?;
    let mut publishing_reader=archive.record_session();
    for record_id in &incoming {
        publishing_reader.restore_run(record_id,&staging_reader.load_run(record_id)?)?;
        if !existing.contains(record_id){published+=1;}
    }
    Ok(json!({"started_run_id":id,"published_records":published,"event_count":merged.events.len(),
        "status":if merged.completion.is_some(){"completion_recorded"}else{"incomplete"},
        "engine_bytes_included":false,"execution_replayed":false,
        "scope":"Content identities and merged prefix links verified before publication. Storage failures may leave a recoverable prefix; no executable is installed or launched."}))
}

fn event_request(root:&ResearchBundle,sequence:usize,previous:Option<&String>)->Value{
    json!({"research_kind":"cli_pipeline_event","config":root.run.request["config"],
        "started_run_id":root.run_id,"sequence":sequence,"previous_event_id":previous})
}
fn completion_request(root:&ResearchBundle)->Value{
    let mut request=root.run.request.clone();
    request["research_kind"]=json!("cli_pipeline");request["started_run_id"]=json!(root.run_id);request
}

// Keep only ordered identities; full execution payloads can dominate archive size.
struct State {root:ResearchBundle,events:Vec<String>,completion:Option<ResearchBundle>}
fn load(archive:&Archive,id:&str)->Result<State,String>{
    let root=archive.load(id)?;
    if root.run.request["research_kind"]!="cli_pipeline_started" || root.run.result!=json!({"status":"execution_not_recorded"}){
        return Err("Expected a pipeline start record".into());
    }
    let mut events=BTreeMap::new();let mut completion=None;let mut reader=archive.record_session();
    for record_id in archive.list()?{
        let run=reader.load_run(&record_id)?;
        let record=PortableRecord{run_id:record_id,run};
        if record.run.request["started_run_id"]!=id {continue;}
        if record.run.engine_id!=root.run.engine_id || record.run.dataset_id!=root.run.dataset_id || record.run.strategy_id!=root.run.strategy_id{
            return Err("Pipeline record input/engine identities differ".into());
        }
        match record.run.request["research_kind"].as_str(){
            Some("cli_pipeline_event")=>{
                let sequence=record.run.request["sequence"].as_u64().and_then(|n|usize::try_from(n).ok()).ok_or("Invalid event sequence")?;
                if events.insert(sequence,(record.run_id,record.run.request)).is_some(){return Err("Conflicting pipeline events at the same sequence".into());}
            },
            Some("cli_pipeline")=>{
                if record.run.request!=completion_request(&root) || completion.replace(ResearchBundle{run_id:record.run_id,run:record.run,dataset:root.dataset.clone(),strategy:root.strategy.clone()}).is_some(){return Err("Conflicting pipeline completion records".into());}
            },
            _=>return Err("Unknown record refers to this pipeline".into()),
        }
    }
    let mut ordered=Vec::new();let mut previous=None;
    for (sequence,(record_id,request)) in events{
        if sequence!=ordered.len() || request!=event_request(&root,sequence,previous.as_ref()){
            return Err("Missing, reordered or incorrectly linked pipeline event".into());
        }
        previous=Some(record_id.clone());ordered.push(record_id);
    }
    if let Some(done)=&completion{
        if done.run.result["event_ids"]!=json!(ordered){
            return Err("Completion does not cover the exact recorded prefix".into());
        }
    }
    Ok(State{root,events:ordered,completion})
}

/// Compact structural status; does not run the engine or return full event payloads.
pub fn summary(archive:&Archive,id:&str)->Result<Value,String>{
    let _guard=lock(archive,id)?;let state=load(archive,id)?;
    Ok(json!({"started_run_id":id,"engine_id":state.root.run.engine_id,
        "status":if state.completion.is_some(){"completion_recorded"}else{"incomplete"},
        "completion_id":state.completion.as_ref().map(|r|&r.run_id),"event_count":state.events.len(),
        "execution_valid":state.completion.as_ref().map(|r|r.run.result["report"]["errors"].as_array().is_some_and(|e|e.is_empty())),
        "execution_replayed":false,"scope":"Ordered archive links checked; calculations have not been replayed and are not independently validated."}))
}

pub fn inspect(archive:&Archive,id:&str)->Result<Value,String>{
    let _guard=lock(archive,id)?;let state=load(archive,id)?;
    let mut reader=archive.record_session();
    let events:Vec<_>=state.events.iter().enumerate().map(|(sequence,id)|{
        reader.load_run(id).map(|record|json!({"sequence":sequence,"run_id":id,"event":record.result}))
    }).collect::<Result<_,_>>()?;
    Ok(json!({"started_run_id":id,"status":if state.completion.is_some(){"completion_recorded"}else{"incomplete"},
        "completion_id":state.completion.as_ref().map(|r|&r.run_id),"event_count":state.events.len(),
        "events":events,
        "execution_replayed":false,"scope":"Hash/input identities and ordered prefix links checked; inspection alone does not verify recorded calculations or correctness."}))
}

pub struct Recovery {pub report:sqx_pipeline::PipelineReport,pub completion_id:String,pub replayed:usize,pub appended:usize}

pub fn recover(archive:&Archive,id:&str,resume:bool)->Result<Recovery,String>{
    let _guard=lock(archive,id)?;
    let state=load(archive,id)?;
    let root=&state.root;
    let executable=std::fs::read(std::env::current_exe().map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if crate::archive::content_id(&executable)!=root.run.engine_id{return Err("Pipeline engine executable differs; use the exact recorded engine".into());}
    if !resume && state.completion.is_none(){return Err("Pipeline is incomplete; use --resume to replay its prefix and finish missing work".into());}
    let bars:Vec<sqx_core::Bar>=serde_json::from_value(root.dataset["bars"].clone()).map_err(|e|e.to_string())?;
    let instrument:sqx_core::Instrument=serde_json::from_value(root.dataset["instrument"].clone()).map_err(|e|e.to_string())?;
    let config:sqx_backtest::BacktestConfig=serde_json::from_value(root.run.request["config"].clone()).map_err(|e|e.to_string())?;
    let pipeline:sqx_pipeline::Pipeline=serde_json::from_value(root.strategy["pipeline"].clone()).map_err(|e|e.to_string())?;
    let generator:sqx_optimizer::GeneratorConfig=serde_json::from_value(root.strategy["generator"].clone()).map_err(|e|e.to_string())?;
    let seed=root.run.request["seed"].as_u64().ok_or("Missing pipeline seed")?;
    let pyp=root.run.request["periods_per_year"].as_f64().filter(|n|n.is_finite() && *n>0.0).ok_or("Invalid periods per year")?;
    let mut ids=Vec::new();let mut appended=0;let mut reader=archive.record_session();
    let report=sqx_pipeline::run_observed(&pipeline,&bars,&instrument,&config,pyp,&generator,seed,&mut |event|{
        let value=serde_json::to_value(event).map_err(|e|e.to_string())?;
        let index=ids.len();
        let event_id=if let Some(event_id)=state.events.get(index){
            let recorded=reader.load_run(event_id)?.result;
            if recorded!=value{return Err(format!("Pipeline replay differs at event {index}; no further work accepted"));}
            event_id.clone()
        }else{
            if !resume || state.completion.is_some(){return Err("Replay produced events beyond the saved completion".into());}
            let event_id=reader.save_run(&RunRecord{schema:1,engine_id:root.run.engine_id.clone(),dataset_id:root.run.dataset_id.clone(),strategy_id:root.run.strategy_id.clone(),request:event_request(root,index,ids.last()),result:value})?;
            appended+=1;event_id
        };
        ids.push(event_id);Ok(())
    })?;
    if ids.len()<state.events.len(){return Err("Replay ended before the saved prefix".into());}
    let result=json!({"report":report,"event_ids":ids,"scope":SCOPE});
    let completion_id=if let Some(done)=state.completion{
        // Scope prose may evolve; calculations and event references must match.
        if done.run.result["report"]!=result["report"] || done.run.result["event_ids"]!=result["event_ids"]{
            return Err("Pipeline final report differs on replay".into());
        }
        done.run_id
    }else{reader.save_run(&RunRecord{schema:1,engine_id:root.run.engine_id.clone(),dataset_id:root.run.dataset_id.clone(),strategy_id:root.run.strategy_id.clone(),request:completion_request(root),result})?};
    Ok(Recovery{report,completion_id,replayed:state.events.len(),appended})
}

#[cfg(test)]
mod backup_tests {
    use super::*;
    #[test]
    fn restored_stress_request_ids_are_unique_before_publication(){
        let dir=std::env::temp_dir().join(format!("sqx-request-restore-{:032x}",rand::random::<u128>()));
        let source=Archive::new(dir.join("source"));let target=Archive::new(dir.join("target"));
        let make=|attempt:&str|{
            let id=source.save("a".repeat(64),json!({"bars":[]}),json!({}),json!({
                "research_kind":"cli_pipeline_started","origin":"web_robustness","attempt_id":attempt,
                "web_request":{"request_key":"shared-request"},"config":{},"seed":42
            }),json!({"status":"execution_not_recorded"})).unwrap();
            export(&source,&id).unwrap()
        };
        let first=make("first");let second=make("second");
        let guard=stress_request_lock(&target,"shared-request").unwrap();
        assert!(import(&target,first.clone()).unwrap_err().contains("busy"));assert!(target.list().unwrap().is_empty());drop(guard);
        assert_eq!(import(&target,first.clone()).unwrap()["published_records"],1);
        assert_eq!(import(&target,first.clone()).unwrap()["published_records"],0);
        let before=target.list().unwrap();
        assert!(import(&target,second.clone()).unwrap_err().contains("conflicts with an existing study"));
        assert_eq!(target.list().unwrap(),before);
        for origin in ["web_portfolio","web_portfolio_combination","web_optimization"]{
        let other_id=source.save("a".repeat(64),json!({"bars":[]}),json!({}),json!({
            "research_kind":"cli_pipeline_started","origin":origin,"attempt_id":"another-workflow",
            "web_request":{"request_key":"shared-request"},"config":{},"seed":42
        }),json!({"status":"execution_not_recorded"})).unwrap();
        assert!(import(&target,export(&source,&other_id).unwrap()).unwrap_err().contains("conflicts with an existing study"));
        assert_eq!(target.list().unwrap(),before,"a request key cannot be rebound through a portfolio backup");
        }
        let concurrent=Archive::new(dir.join("concurrent"));
        let outcomes=std::thread::scope(|scope|{
            let a=scope.spawn(||import(&concurrent,first));let b=scope.spawn(||import(&concurrent,second));
            [a.join().unwrap(),b.join().unwrap()]
        });
        assert_eq!(outcomes.iter().filter(|result|result.is_ok()).count(),1);assert_eq!(concurrent.list().unwrap().len(),1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn portable_prefix_completion_and_conflict_validation() {
        let dir=std::env::temp_dir().join(format!("sqx-backup-test-{:032x}",rand::random::<u128>()));
        let source=Archive::new(dir.join("source"));let target=Archive::new(dir.join("target"));
        let id=source.save("a".repeat(64),json!({"bars":[]}),json!({}),
            json!({"research_kind":"cli_pipeline_started","config":{},"seed":42}),
            json!({"status":"execution_not_recorded"})).unwrap();
        let root=source.load(&id).unwrap();
        let event=source.save(root.run.engine_id.clone(),root.dataset.clone(),root.strategy.clone(),
            event_request(&root,0,None),json!({"kind":"step_started"})).unwrap();
        let prefix=export(&source,&id).unwrap();
        assert_eq!(import(&target,prefix.clone()).unwrap()["status"],"incomplete");
        let compact=summary(&target,&id).unwrap();
        assert_eq!(compact["event_count"],1);assert!(compact.get("events").is_none());
        assert_eq!(inspect(&target,&id).unwrap()["events"][0]["event"],json!({"kind":"step_started"}));
        assert_eq!(import(&target,prefix.clone()).unwrap()["published_records"],0);
        let done=source.save(root.run.engine_id.clone(),root.dataset.clone(),root.strategy.clone(),
            completion_request(&root),json!({"event_ids":[event],"report":{}})).unwrap();
        let complete=export(&source,&id).unwrap();
        std::fs::remove_dir_all(source.root()).unwrap();
        assert_eq!(import(&target,complete.clone()).unwrap()["published_records"],1);
        assert_eq!(inspect(&target,&id).unwrap()["completion_id"],done);
        // An older valid prefix can safely be imported over a completed archive.
        assert_eq!(import(&target,prefix).unwrap()["status"],"completion_recorded");
        let empty=Archive::new(dir.join("empty"));
        let mut damaged=complete.clone();damaged["events"][0]["run"]["result"]=json!({"changed":true});
        assert!(import(&empty,damaged).unwrap_err().contains("checksum"));
        assert!(empty.list().unwrap().is_empty());
        // Even with a fresh checksum, an incomplete completion graph is rejected.
        let mut missing:Backup=serde_json::from_value(complete.clone()).unwrap();
        missing.events.clear();missing.checksum.clear();missing.checksum=checksum(&missing).unwrap();
        assert!(import(&empty,serde_json::to_value(missing).unwrap()).is_err());
        assert!(empty.list().unwrap().is_empty());
        let conflict=Archive::new(dir.join("conflict"));conflict.restore(&root).unwrap();
        conflict.save(root.run.engine_id.clone(),root.dataset.clone(),root.strategy.clone(),
            event_request(&root,0,None),json!({"different":true})).unwrap();
        let before=conflict.list().unwrap();
        assert!(import(&conflict,complete).unwrap_err().contains("Conflicting"));
        assert_eq!(conflict.list().unwrap(),before);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
