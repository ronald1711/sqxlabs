//! Recovery of frozen assistant backtests, without contacting a language model.
use crate::archive::{Archive, ResearchBundle};
use serde::Deserialize;
use serde_json::{json, Value};
use sqx_backtest::{backtest, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_strategy::Strategy;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Dataset {
    symbol: String,
    bars: Vec<Bar>,
    instrument: Instrument,
}

fn completed_request(checkpoint: &ResearchBundle) -> Value {
    let mut request = checkpoint.run.request.clone();
    request["research_kind"] = json!("assistant_trial");
    request["started_run_id"] = json!(checkpoint.run_id);
    request
}

fn same_checkpoint(result: &ResearchBundle, checkpoint: &ResearchBundle) -> bool {
    result.run.request == completed_request(checkpoint)
        && result.run.dataset_id == checkpoint.run.dataset_id
        && result.run.strategy_id == checkpoint.run.strategy_id
        && result.run.engine_id == checkpoint.run.engine_id
}

/// Inspect recorded state only. A pending record does not prove a process stopped.
pub fn list(archive: &Archive, engine: &str) -> Result<Value, String> {
    // Listing must not retain every archived market series and full result at once.
    let records = archive.list()?.into_iter().map(|id| {
        let mut record=archive.load(&id)?;
        record.dataset=json!({"symbol":record.dataset["symbol"]});
        record.strategy=json!({"name":record.strategy["name"]});
        record.run.result=json!({"errors":record.run.result["errors"]});
        Ok(record)
    }).collect::<Result<Vec<_>, String>>()?;
    let mut checkpoints = Vec::new();
    let mut standalone_results = Vec::new();
    for result in records.iter().filter(|r| r.run.request["research_kind"] == "assistant_trial") {
        if !records.iter().any(|r| r.run_id == result.run.request["started_run_id"] && r.run.request["research_kind"] == "assistant_trial_started") {
            standalone_results.push(json!({"run_id":result.run_id,"matching_engine":result.run.engine_id==engine}));
        }
    }
    for checkpoint in records.iter().filter(|r| r.run.request["research_kind"] == "assistant_trial_started") {
        let mut results = Vec::new();
        for result in records.iter().filter(|r| r.run.request["research_kind"] == "assistant_trial" && r.run.request["started_run_id"] == checkpoint.run_id) {
            if !same_checkpoint(result, checkpoint) {
                return Err("assistant result does not match its checkpoint identity".into());
            }
            results.push(json!({"run_id":result.run_id,"execution_valid":result.run.result["errors"].as_array().is_some_and(|errors|errors.is_empty())}));
        }
        checkpoints.push(json!({"checkpoint_id":checkpoint.run_id,"session_id":checkpoint.run.request["session_id"],"tool":checkpoint.run.request["tool"],"tool_call":checkpoint.run.request["tool_call"],"index":checkpoint.run.request["index"],"name":checkpoint.strategy["name"],"symbol":checkpoint.dataset["symbol"],"matching_engine":checkpoint.run.engine_id==engine,"state":if results.is_empty(){"result_not_recorded"}else{"result_recorded"},"results":results}));
    }
    Ok(json!({"checkpoints":checkpoints,"standalone_results":standalone_results,"scope":"Recorded trial state, not process liveness or independent validation. Resume restarts a single frozen backtest, not a provider conversation. Record counts are not independent trial counts."}))
}

/// Replay a standalone result, or restart a checkpoint and publish its result.
/// Only the caller's exact executable identity is accepted; no stored code runs.
pub fn execute(archive: &Archive, id: &str, engine: &str, resume: bool) -> Result<Value, String> {
    let bundle = archive.load(id)?;
    let kind = if resume {"assistant_trial_started"} else {"assistant_trial"};
    if bundle.run.request["research_kind"] != kind {
        return Err("assistant record type does not match replay/resume mode".into());
    }
    if bundle.run.engine_id != engine {
        return Err("engine executable differs from the recorded trial; use the matching build".into());
    }
    let dataset: Dataset = serde_json::from_value(bundle.dataset.clone()).map_err(|e|e.to_string())?;
    if dataset.symbol.trim().is_empty() || dataset.bars.len() > 100_000 {
        return Err("invalid archived assistant dataset or bar budget".into());
    }
    let strategy: Strategy = serde_json::from_value(bundle.strategy.clone()).map_err(|e|e.to_string())?;
    let config: BacktestConfig = serde_json::from_value(bundle.run.request["config"].clone()).map_err(|e|e.to_string())?;
    let mut previous = Vec::new();
    if !resume {
        // Portable standalone results are valid, but an available parent must agree.
        if let Some(parent)=bundle.run.request["started_run_id"].as_str() {
            if archive.list()?.iter().any(|saved|saved==parent) {
                let checkpoint=archive.load(parent)?;
                if checkpoint.run.request["research_kind"]!="assistant_trial_started" || !same_checkpoint(&bundle,&checkpoint) {
                    return Err("assistant result does not match its checkpoint identity".into());
                }
            }
        }
    }
    if resume {
        // Detect contradictory linked inputs before running or publishing anything.
        for other in archive.list()? {
            let record = archive.load(&other)?;
            if record.run.request["research_kind"] == "assistant_trial" && record.run.request["started_run_id"] == id {
                if !same_checkpoint(&record, &bundle) {
                    return Err("assistant result does not match its checkpoint identity".into());
                }
                previous.push(crate::archive::content_id(&serde_json::to_vec(&record.run.result).map_err(|e|e.to_string())?));
            }
        }
    }
    let result = backtest(&strategy, &dataset.bars, &dataset.instrument, &config);
    let actual = serde_json::to_value(&result).map_err(|e|e.to_string())?;
    if resume {
        let actual_id=crate::archive::content_id(&serde_json::to_vec(&actual).map_err(|e|e.to_string())?);
        if previous.iter().any(|saved| saved != &actual_id) {
            return Err("restarted assistant execution differs from an existing linked result; no new result published".into());
        }
        let request = completed_request(&bundle);
        let run_id = archive.save(engine.into(), bundle.dataset, bundle.strategy, request, actual)?;
        Ok(json!({"run_id":run_id,"checkpoint_id":id,"resumed":true,"execution_valid":result.errors.is_empty(),"errors":result.errors,"scope":"Restarted one frozen backtest; provider conversation was not resumed."}))
    } else {
        Ok(json!({"run_id":id,"matches":actual==bundle.run.result,"execution_valid":result.errors.is_empty(),"errors":result.errors,"scope":"Exact engine-result comparison; not independent broker validation."}))
    }
}
