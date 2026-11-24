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
    let mut run=||->Result<(Vec<BatchEvaluation>,String),String>{
        let mut evaluations=Vec::with_capacity(strategies.len());
        let mut trials=Vec::with_capacity(strategies.len());
        for (candidate_index,strategy) in strategies.iter().enumerate() {
            let strategy_json=serde_json::to_value(strategy).map_err(|e|e.to_string())?;
            let mut trial_request=json!({"research_kind":"mcp_generation_trial_started","search_id":search_id,"candidate_index":candidate_index,"config":config,"periods_per_year":pyp});
            let checkpoint=archive.save(engine.clone(),dataset.clone(),strategy_json.clone(),trial_request.clone(),json!({"status":"execution_not_recorded"}))?;
            let result=evaluate(strategy,bars,instrument,config);
            let metrics=sqx_metrics::compute(&result,bars.len(),pyp);
            let mut errors=result.errors.clone();
            if let Err(error)=sqx_metrics::validate_observations(&result){errors.push(error);}
            let eligible=errors.is_empty() && sqx_optimizer::fitness(&metrics,Fitness::ProfitFactor,5).is_finite();
            trial_request["research_kind"]=json!("mcp_generation_trial");
            trial_request["started_run_id"]=json!(checkpoint);
            let result_id=archive.save(engine.clone(),dataset.clone(),strategy_json,trial_request,json!({"backtest":result,"metrics":metrics,"errors":errors,"eligible":eligible}))?;
            trials.push(json!({"candidate_index":candidate_index,"checkpoint_id":checkpoint,"result_id":result_id}));
            evaluations.push(BatchEvaluation{strategy:strategy.clone(),metrics,errors});
        }
        // No selection is returned unless every candidate outcome is durable.
        let mut completed_request=request.clone();
        completed_request["research_kind"]=json!("mcp_generation");
        completed_request["started_run_id"]=json!(search_id);
        let completion=archive.save(engine.clone(),dataset.clone(),family.clone(),completed_request,json!({"status":"evaluations_recorded","trials":trials}))?;
        Ok((evaluations,completion))
    };
    run().map(|(evaluations,completion)|(evaluations,search_id.clone(),completion))
        .map_err(|error|format!("search {search_id}: {error}; inspect generation_history before retrying"))
}

/// Inspect content-verified records without retaining all datasets in memory.
/// This is recorded progress, not an assertion that an interrupted worker stopped.
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
