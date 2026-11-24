//! Frozen development/holdout protocol with an append-only, hash-linked ledger.
//! Isolation applies to this protocol, not to access outside this research store.
use std::{fs::{self, File, OpenOptions}, path::PathBuf};
use serde::{Serialize, Deserialize};
use serde_json::{json, Value};
use sqx_backtest::{backtest, backtest_window, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_strategy::Strategy;
use crate::archive::{Archive, content_id};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentData {
    pub bars: Vec<Bar>,
    pub instrument: Instrument,
    pub provenance: Value,
}
impl ExperimentData {
    fn verify_normalization(&self,config:&BacktestConfig)->Result<(),String>{
        if let Some(value)=self.provenance.get("normalization").filter(|v|!v.is_null()) {
            let bundle:sqx_data::normalization::Bundle=serde_json::from_value(value.clone()).map_err(|e|e.to_string())?;
            bundle.verify_bars(&self.bars,config.bar_duration_ms)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentPlan {
    pub schema: u32,
    pub engine_id: String,
    pub dataset_id: String,
    pub holdout_start: usize,
    pub config: BacktestConfig,
    pub min_development_trades: usize,
    pub min_holdout_trades: usize,
    pub periods_per_year: f64,
    /// User disclosure only; the protocol cannot prove data was never seen elsewhere.
    pub prior_exposure_disclosed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    index: usize,
    previous: Option<String>,
    payload: Value,
}
/// Candidate generation is frozen before execution, so resumption never depends
/// on regenerating RNG state or on subsequently edited generator defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchSpec {
    pub seed: u64,
    pub generator: Value,
    pub candidates: Vec<Strategy>,
}
/// Portable snapshot. Hashes verify integrity, not the identity of its author.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentBackup {
    pub checksum: String,
    pub schema: u32,
    pub experiment_id: String,
    pub plan: ExperimentPlan,
    pub data: ExperimentData,
    pub events: Vec<Value>,
    pub head: Option<String>,
    #[serde(default)]
    pub search_specs: std::collections::BTreeMap<String,SearchSpec>,
}
impl ExperimentBackup {
    /// Content checksum binding plan, data, and history together; not a signature.
    pub fn integrity_id(&self)->Result<String,String>{
        let manifest=if self.schema==1 {json!([self.schema,self.experiment_id,self.plan,self.data,self.events,self.head])}
            else {json!([self.schema,self.experiment_id,self.plan,self.data,self.events,self.head,self.search_specs])};
        Ok(content_id(&bytes(&manifest)?))
    }
}
#[derive(Debug, Clone)]
pub struct Experiments { archive: Archive }
struct Ledger {
    archive: Archive,
    dir: PathBuf,
    _lock: File,
    events: Vec<(String, Event)>,
}
impl Experiments {
    pub fn new(root: impl Into<PathBuf>) -> Self {Self{archive:Archive::new(root)}}
    fn registry_lock(&self)->Result<File,String>{
        fs::create_dir_all(self.archive.root()).map_err(err)?;
        let file=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(self.archive.root().join(".experiment-commit.lock")).map_err(err)?;
        file.lock().map_err(err)?;Ok(file)
    }
    pub fn create(&self, data: &ExperimentData, mut plan: ExperimentPlan) -> Result<String,String> {
        data.verify_normalization(&plan.config)?;
        sqx_backtest::validate_backtest_inputs(&data.bars,&data.instrument,&plan.config)?;
        crate::observation::range(&data.bars,plan.config.bar_duration_ms,&data.provenance["timeframe"])?;
        if plan.schema != 1 || plan.engine_id.is_empty() || plan.holdout_start < 2 || plan.holdout_start >= data.bars.len()
            || plan.min_development_trades == 0 || plan.min_holdout_trades == 0
            || !plan.periods_per_year.is_finite() || plan.periods_per_year <= 0.0 {
            return Err("invalid experiment split, engine identity or evidence thresholds".into());
        }
        let _registry_lock=self.registry_lock()?;
        plan.dataset_id = self.archive.write_blob("experiment_data",&bytes(data)?)?;
        let encoded=bytes(&plan)?;
        let id=content_id(&encoded);
        let _ledger=self.open_ledger(&id,false)?;
        self.archive.write_blob("experiment_plans",&encoded)
    }
    fn plan(&self,id:&str)->Result<ExperimentPlan,String>{
        let plan:ExperimentPlan=serde_json::from_slice(&self.archive.read_blob("experiment_plans",id)?).map_err(err)?;
        if plan.schema!=1{return Err("unsupported experiment schema".into());} Ok(plan)
    }
    fn data(&self,plan:&ExperimentPlan)->Result<ExperimentData,String>{
        serde_json::from_slice(&self.archive.read_blob("experiment_data",&plan.dataset_id)?).map_err(err)
    }
    fn ledger(&self,id:&str)->Result<Ledger,String>{
        self.open_ledger(id,true)
    }
    fn open_ledger(&self,id:&str,require_plan:bool)->Result<Ledger,String>{
        if id.len()!=64 || !id.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)) {return Err("invalid experiment identifier".into());}
        if require_plan || self.archive.root().join("experiment_plans").join(format!("{id}.json")).exists() {self.plan(id)?;}
        let dir=self.archive.root().join("experiments").join(id);
        fs::create_dir_all(&dir).map_err(err)?;
        let lock=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(dir.join(".lock")).map_err(err)?;
        lock.lock().map_err(err)?; // OS releases on process exit; no stale lock-file guessing.
        let mut paths:Vec<_>=fs::read_dir(&dir).map_err(err)?.map(|e|e.map(|e|e.path())).collect::<Result<_,_>>().map_err(err)?;
        paths.retain(|p|p.extension().is_some_and(|e|e=="json"));paths.sort();
        let mut events:Vec<(String,Event)>=Vec::new();
        for path in paths {
            let raw=fs::read(&path).map_err(err)?;let hash=content_id(&raw);
            let event:Event=serde_json::from_slice(&raw).map_err(err)?;
            if event.index!=events.len() || event.previous!=events.last().map(|e|e.0.clone())
                || path.file_name().and_then(|s|s.to_str())!=Some(&format!("{:020}-{hash}.json",event.index)) {
                return Err("experiment ledger integrity/sequence failure".into());
            }
            events.push((hash,event));
        }
        validate_history(&events)?;
        Ok(Ledger{archive:self.archive.clone(),dir,_lock:lock,events})
    }
    /// Evaluates only the development prefix. Every attempt is recorded before execution.
    pub fn trial(&self,id:&str,engine:&str,strategy:&Strategy)->Result<Value,String>{
        self.trial_with_context(id,engine,strategy,json!({"source":"manual"}))
    }
    pub fn trial_with_context(&self,id:&str,engine:&str,strategy:&Strategy,context:Value)->Result<Value,String>{
        if context.get("job_id").is_some(){return Err("job context is reserved for checkpointed search execution".into());}
        let plan=self.plan(id)?;check_engine(&plan,engine)?;
        let mut ledger=self.ledger(id)?;
        if ledger.has("holdout_committed"){return Err("experiment is frozen after holdout commitment; no further development trials allowed".into());}
        let data=self.data(&plan)?;
        let trial=ledger.events.iter().filter(|e|e.1.payload["kind"]=="trial_started").count();
        ledger.append(json!({"kind":"trial_started","trial":trial,"strategy":strategy,"context":context}))?;
        let result=backtest(strategy,&data.bars[..plan.holdout_start],&data.instrument,&plan.config);
        let report=report(&result,plan.holdout_start,plan.periods_per_year,plan.min_development_trades);
        ledger.append(json!({"kind":"trial_finished","trial":trial,"report":report}))?;
        Ok(json!({"trial":trial,"report":report}))
    }
    pub fn start_search(&self,id:&str,engine:&str,request_key:&str,spec:&SearchSpec)->Result<Value,String>{
        let plan=self.plan(id)?;check_engine(&plan,engine)?;
        if request_key.is_empty()||request_key.len()>128||!request_key.is_ascii()||spec.candidates.is_empty()||spec.candidates.len()>2000 {
            return Err("invalid search request key or candidate count (1..2000 required)".into());
        }
        let mut ledger=self.ledger(id)?;
        let spec_bytes=bytes(spec)?;let spec_id=content_id(&spec_bytes);
        if let Some(job)=ledger.events.iter().find(|e|e.1.payload["kind"]=="job_started"&&e.1.payload["request_key"]==request_key) {
            if job.1.payload["spec_id"]!=spec_id{return Err("search request key already belongs to different parameters".into());}
            return job_summary(&ledger,job.1.index);
        }
        if ledger.has("holdout_committed"){return Err("experiment is frozen after holdout commitment".into());}
        self.archive.write_blob("experiment_searches",&spec_bytes)?;
        let job_id=ledger.events.len();
        ledger.append(json!({"kind":"job_started","job_id":job_id,"request_key":request_key,"spec_id":spec_id,"count":spec.candidates.len(),"seed":spec.seed}))?;
        job_summary(&ledger,job_id)
    }
    /// One durable candidate checkpoint per call. OS locking prevents duplicate
    /// execution records across concurrent workers; unfinished claims resume exactly.
    pub fn advance_search(&self,id:&str,engine:&str,job_id:usize)->Result<Value,String>{
        let plan=self.plan(id)?;check_engine(&plan,engine)?;
        let mut ledger=self.ledger(id)?;
        let job=ledger.events.get(job_id).filter(|e|e.1.payload["kind"]=="job_started").ok_or("unknown search job")?.1.payload.clone();
        let summary=job_summary(&ledger,job_id)?;
        if summary["state"]=="complete" {return Ok(summary);}
        if ledger.has("holdout_committed"){return Err("experiment is frozen; unfinished search jobs cannot continue".into());}
        let spec:SearchSpec=serde_json::from_slice(&self.archive.read_blob("experiment_searches",job["spec_id"].as_str().ok_or("invalid search spec")?)?).map_err(err)?;
        if spec.candidates.len()!=job["count"].as_u64().unwrap_or(0) as usize{return Err("search candidate count mismatch".into());}
        let candidate=summary["completed"].as_u64().unwrap() as usize;
        let strategy=&spec.candidates[candidate];
        let existing=ledger.events.iter().find(|e|e.1.payload["kind"]=="trial_started"&&e.1.payload["context"]["job_id"]==job_id&&e.1.payload["context"]["candidate_index"]==candidate);
        let trial=if let Some(start)=existing {
            if start.1.payload["strategy"]!=json!(strategy){return Err("interrupted candidate differs from frozen search specification".into());}
            start.1.payload["trial"].as_u64().ok_or("invalid trial")? as usize
        } else {
            let trial=ledger.events.iter().filter(|e|e.1.payload["kind"]=="trial_started").count();
            ledger.append(json!({"kind":"trial_started","trial":trial,"strategy":strategy,
                "context":{"job_id":job_id,"seed":spec.seed,"candidate_index":candidate,"generator":spec.generator}}))?;
            trial
        };
        let data=self.data(&plan)?;
        let result=backtest(strategy,&data.bars[..plan.holdout_start],&data.instrument,&plan.config);
        let report=report(&result,plan.holdout_start,plan.periods_per_year,plan.min_development_trades);
        ledger.append(json!({"kind":"trial_finished","trial":trial,"report":report}))?;
        let mut result=job_summary(&ledger,job_id)?;result["last_trial"]=json!({"trial":trial,"report":report});Ok(result)
    }
    /// Commits exactly one development candidate before reading the holdout bars.
    /// Repeating the same commitment resumes an interrupted execution or returns
    /// the saved result. A different selection is rejected even after a restart.
    pub fn reveal(&self,id:&str,engine:&str,trial:usize)->Result<Value,String>{
        // Serialize commitments across experiments so two concurrent reveals cannot
        // both miss the other's commitment. Trial execution needs no global lock.
        let _registry_lock=self.registry_lock()?;
        let plan=self.plan(id)?;check_engine(&plan,engine)?;
        let mut ledger=self.ledger(id)?;
        if let Some(commit)=ledger.find("holdout_committed") {
            if commit["trial"].as_u64()!=Some(trial as u64){return Err("holdout already committed to another candidate".into());}
            if let Some(done)=ledger.find("holdout_finished"){return Ok(done["report"].clone());}
        } else {
            let finished=ledger.events.iter().find(|e|e.1.payload["kind"]=="trial_finished" && e.1.payload["trial"]==trial)
                .ok_or("selected development trial is missing/incomplete")?;
            if finished.1.payload["report"]["evidence_sufficient"]!=true {return Err("selected candidate does not meet the frozen development evidence threshold".into());}
            let data=self.data(&plan)?;
            let own_attempts=ledger.events.iter().filter(|e|e.1.payload["kind"]=="trial_started").count();
            let exposure=self.exposure_audit(id,&plan,&data,own_attempts)?;
            ledger.append(json!({"kind":"holdout_committed","trial":trial,"exposure_at_commitment":exposure}))?;
        }
        let start=ledger.events.iter().find(|e|e.1.payload["kind"]=="trial_started" && e.1.payload["trial"]==trial).ok_or("missing committed strategy")?;
        let strategy:Strategy=serde_json::from_value(start.1.payload["strategy"].clone()).map_err(err)?;
        let data=self.data(&plan)?;
        let result=backtest_window(&strategy,&data.bars,plan.holdout_start,&data.instrument,&plan.config);
        let mut report=report(&result,data.bars.len()-plan.holdout_start,plan.periods_per_year,plan.min_holdout_trades);
        report["trial"]=json!(trial);
        report["holdout_start_time"]=json!(data.bars[plan.holdout_start].time);
        report["holdout_end_time"]=json!(crate::observation::range(&data.bars[plan.holdout_start..],plan.config.bar_duration_ms,&data.provenance["timeframe"])?.end);
        report["prior_exposure_disclosed"]=json!(plan.prior_exposure_disclosed);
        report["exposure_at_commitment"]=ledger.find("holdout_committed").and_then(|v|v.get("exposure_at_commitment")).cloned()
            .unwrap_or_else(||json!({"classification":"not_recorded","scope":"Legacy commitment; no exposure audit was recorded."}));
        report["interpretation"]=json!("One frozen candidate; trade-count threshold is not a statistical robustness certificate. Prior exposure outside this protocol cannot be excluded.");
        ledger.append(json!({"kind":"holdout_finished","report":report}))?;
        Ok(report)
    }
    pub fn status(&self,id:&str)->Result<Value,String>{
        let plan=self.plan(id)?;let ledger=self.ledger(id)?;
        let data=self.data(&plan)?;
        let events:Vec<_>=ledger.events.iter().map(|(hash,e)|json!({"id":hash,"index":e.index,"event":e.payload})).collect();
        let state=if ledger.has("holdout_finished"){"revealed"}else if ledger.has("holdout_committed"){"committed"}else{"development"};
        let own_attempts=ledger.events.iter().filter(|e|e.1.payload["kind"]=="trial_started").count();
        let jobs:Vec<Value>=ledger.events.iter().filter(|e|e.1.payload["kind"]=="job_started").map(|e|job_summary(&ledger,e.1.index)).collect::<Result<_,_>>()?;
        let remembered_exposure=ledger.find("holdout_committed").and_then(|e|e.get("exposure_at_commitment")).cloned();
        drop(ledger); // never acquire another experiment's lock while holding ours
        let mut exposure=self.exposure_audit(id,&plan,&data,own_attempts)?;
        if let Some(remembered)=remembered_exposure {
            if remembered["observation_extent_complete"]==false {
                exposure["observation_extent_complete"]=json!(false);
                exposure["carried_commitment_evidence"]=remembered.clone();
            }
            if remembered["classification"]=="known_reuse_or_disclosed_exposure" {
                exposure["classification"]=json!("known_reuse_or_disclosed_exposure");
                exposure["carried_commitment_evidence"]=remembered;
            }
        }
        Ok(json!({"experiment_id":id,"plan":plan,"state":state,"current_exposure_audit":exposure,"jobs":jobs,
            "development_start_time":data.bars[0].time,"holdout_start_time":data.bars[plan.holdout_start].time,"holdout_end_time":exposure["holdout_observation_range"]["end"],
            "scope":"Reserved within this experiment only. Data access and testing outside this protocol are not prevented or certified.",
            "trials_attempted":events.iter().filter(|e|e["event"]["kind"]=="trial_started").count(),"events":events}))
    }
    fn exposure_audit(&self,id:&str,plan:&ExperimentPlan,data:&ExperimentData,own_attempts:usize)->Result<Value,String>{
        let own_range=crate::observation::range(&data.bars[plan.holdout_start..],plan.config.bar_duration_ms,&data.provenance["timeframe"])?;
        let (start,end)=(own_range.start,own_range.end);
        let mut unknown_extents=Vec::new();
        if !own_range.extent_known{unknown_extents.push(json!({"source":"current_holdout","id":id}));}
        let mut overlaps=Vec::new();let mut attempts=own_attempts;let mut archived_runs=0usize;let mut archived_pipelines=0usize;let mut archived_robustness=0usize;let mut archived_assistant=0usize;let mut archived_cli=0usize;let mut archived_crosschecks=0usize;let mut archived_mcp=0usize;
        for other_id in self.list()? {
            if other_id==id{continue;}
            let other_plan=self.plan(&other_id)?;let other_data=self.data(&other_plan)?;
            if !same_market(&data.instrument,&other_data.instrument){continue;}
            let ledger=self.ledger(&other_id)?;
            let count=ledger.events.iter().filter(|e|e.1.payload["kind"]=="trial_started").count();attempts+=count;
            let development=crate::observation::range(&other_data.bars[..other_plan.holdout_start],other_plan.config.bar_duration_ms,&other_data.provenance["timeframe"])?;
            let (development_start,development_end)=(development.start,development.end);
            if count>0 && !development.extent_known{unknown_extents.push(json!({"source":"experiment_development","id":other_id}));}
            if count>0 && intersects(start,end,development_start,development_end) {
                overlaps.push(json!({"source":"experiment_development","id":other_id,"attempts":count,"start":development_start,"end":development_end}));
            }
            if ledger.has("holdout_committed") {
                let final_range=crate::observation::range(&other_data.bars[other_plan.holdout_start..],other_plan.config.bar_duration_ms,&other_data.provenance["timeframe"])?;
                let (final_start,final_end)=(final_range.start,final_range.end);
                if !final_range.extent_known{unknown_extents.push(json!({"source":"experiment_final_commitment","id":other_id}));}
                if intersects(start,end,final_start,final_end) {
                    overlaps.push(json!({"source":"experiment_final_commitment","id":other_id,"start":final_start,"end":final_end}));
                }
            }
        }
        // Immutable dashboard and explicitly archived CLI backtests disclose exposure.
        // A single archive record is not a count of all optimizer trials or reruns.
        for run_id in self.archive.list()? {
            let bundle=self.archive.load(&run_id)?;
            if matches!(bundle.run.request["research_kind"].as_str(),Some("cli_crosscheck"|"cli_crosscheck_started")) {
                let markets=bundle.dataset["markets"].as_array().filter(|m|!m.is_empty()).ok_or("crosscheck exposure requires nonempty market inputs")?;
                let mut same_market_record=false;
                for (market_index,market) in markets.iter().enumerate() {
                    let instrument:Instrument=serde_json::from_value(market["instrument"].clone()).map_err(err)?;
                    let bars:Vec<Bar>=serde_json::from_value(market["bars"].clone()).map_err(err)?;
                    if bars.is_empty(){return Err("crosscheck exposure requires market candles".into());}
                    if !same_market(&data.instrument,&instrument){continue;}
                    let range=crate::observation::range(&bars,crate::observation::duration(&market["config"])?,&market["source_context"]["timeframe"])?;
                    if !range.extent_known{unknown_extents.push(json!({"source":"archived_cli_crosscheck","id":run_id,"market_index":market_index}));}
                    same_market_record=true;
                    // A checkpoint claims the full input window before execution.
                    // A crash cannot establish which inputs were already inspected.
                    if intersects(start,end,range.start,range.end) {
                        overlaps.push(json!({"source":"archived_cli_crosscheck","id":run_id,"market_index":market_index,"record_kind":bundle.run.request["research_kind"],"start":range.start,"end":range.end}));
                    }
                }
                archived_crosschecks+=usize::from(same_market_record);
                continue;
            }
            let instrument:Instrument=serde_json::from_value(bundle.dataset["instrument"].clone()).map_err(err)?;
            if !same_market(&data.instrument,&instrument){continue;}
            let bars:Vec<Bar>=serde_json::from_value(bundle.dataset["bars"].clone()).map_err(err)?;
            if !bars.is_empty() {
                let config=if !bundle.run.request["config"].is_null(){&bundle.run.request["config"]}else{&bundle.run.request["_resolved_config"]};
                let range=crate::observation::range(&bars,crate::observation::duration(config)?,&bundle.dataset["timeframe"])?;
                if !range.extent_known{unknown_extents.push(json!({"source":"archived_research","id":run_id}));}
                let pipeline=matches!(bundle.run.request["research_kind"].as_str(),Some("pipeline"|"cli_pipeline_started"|"cli_pipeline"));
                let robustness=bundle.run.request["research_kind"]=="robustness";
                let assistant=matches!(bundle.run.request["research_kind"].as_str(),Some("assistant_candidates"|"assistant_trial"|"assistant_trial_started"));
                let cli=matches!(bundle.run.request["research_kind"].as_str(),Some("cli_backtest"|"cli_backtest_started"));
                let mcp=matches!(bundle.run.request["research_kind"].as_str(),Some("mcp_generation_started"|"mcp_generation_trial_started"|"mcp_generation_trial"|"mcp_generation"));
                if cli {archived_cli+=1;}
                if mcp {archived_mcp+=1;}else if assistant {archived_assistant+=1;}else if pipeline {archived_pipelines+=1;}else if robustness {archived_robustness+=1;}else{archived_runs+=1;}
                if intersects(start,end,range.start,range.end) {
                    overlaps.push(json!({"source":if mcp {"archived_mcp_generation"}else if cli {"archived_cli_backtest"}else if assistant {"archived_assistant_candidates"}else if pipeline {"archived_pipeline"}else if robustness {"archived_robustness"}else{"archived_backtest"},"id":run_id,"start":range.start,"end":range.end}));
                }
            }
        }
        // A portable comparison embeds its reference run even when that run was not
        // separately restored. Its observations remain known prior exposure.
        let mut archived_comparisons=0usize;
        for comparison_id in crate::reconciliation::list_comparisons(&self.archive)? {
            let record=crate::reconciliation::load_comparison(&self.archive,&comparison_id)?;
            let instrument:Instrument=serde_json::from_value(record.reference.dataset["instrument"].clone()).map_err(err)?;
            if !same_market(&data.instrument,&instrument){continue;}
            archived_comparisons+=1;
            let bars:Vec<Bar>=serde_json::from_value(record.reference.dataset["bars"].clone()).map_err(err)?;
            if !bars.is_empty() {
                let request=&record.reference.run.request;
                let config=if !request["config"].is_null(){&request["config"]}else{&request["_resolved_config"]};
                let range=crate::observation::range(&bars,crate::observation::duration(config)?,&record.reference.dataset["timeframe"])?;
                if !range.extent_known{unknown_extents.push(json!({"source":"comparison_reference_backtest","id":comparison_id}));}
                if intersects(start,end,range.start,range.end){overlaps.push(json!({"source":"comparison_reference_backtest","id":comparison_id,"run_id":record.reference.run_id,"start":range.start,"end":range.end}));}
            }
        }
        let mut evolution_claims=0usize;
        for job in crate::evolution::EvolutionJobs::new(self.archive.root()).exposure()? {
            if !same_market(&data.instrument,&job.instrument){continue;}
            evolution_claims+=job.claimed_generations;
            if !job.extent_known{unknown_extents.push(json!({"source":"evolution_generation_claim","id":job.job_id}));}
            if intersects(start,end,job.start,job.end){overlaps.push(json!({"source":"evolution_generation_claim","id":job.job_id,"claimed_generations":job.claimed_generations,"start":job.start,"end":job.end}));}
        }
        Ok(json!({"classification":if plan.prior_exposure_disclosed||!overlaps.is_empty(){"known_reuse_or_disclosed_exposure"}else{"no_known_registry_overlap"},
            "holdout_observation_range":{"start":start,"end":end,"extent_known":own_range.extent_known},"observation_extent_complete":unknown_extents.is_empty(),"unknown_interval_extents":unknown_extents,
            "known_overlaps":overlaps,"same_market_experiment_attempts":attempts,"same_market_archived_backtest_records":archived_runs,"same_market_archived_cli_backtest_records":archived_cli,"same_market_archived_crosscheck_records":archived_crosschecks,"same_market_archived_assistant_records":archived_assistant,"same_market_archived_mcp_generation_records":archived_mcp,"same_market_archived_pipeline_records":archived_pipelines,"same_market_archived_robustness_records":archived_robustness,"same_market_archived_comparison_records":archived_comparisons,"same_market_evolution_generation_claims":evolution_claims,
            "scope":"Candle extents use declared timeframe/duration, including true UTC calendar months. Missing extents are explicitly unverified, not inferred from gaps. Overlap of recorded observation-time ranges on the same named market or FX pair. Includes other experiment trials/commitments and archived multi-market CLI crosscheck inputs (including checkpoints and failures) and dashboard and CLI backtests (including recorded failures) and completed pipeline/robustness reports and assistant session summaries and trial checkpoints/results (including pending work and reported failures), plus backtests embedded in saved paper comparisons and claimed durable evolution generations (including interrupted work). CLI backtest counts are a subset of backtest record counts. Record counts may reference the same run and are not independent trial counts. Only records in this archive are visible; unarchived searches, data viewing, other CLI/MCP and external work are not covered. No known overlap does not prove independence."}))
    }
    pub fn backup(&self,id:&str)->Result<ExperimentBackup,String>{
        let plan=self.plan(id)?;let ledger=self.ledger(id)?;let data=self.data(&plan)?;
        let mut search_specs=std::collections::BTreeMap::new();
        for (_,event) in &ledger.events {if event.payload["kind"]=="job_started" {
            let spec_id=event.payload["spec_id"].as_str().ok_or("missing search specification")?;
            let spec=serde_json::from_slice(&self.archive.read_blob("experiment_searches",spec_id)?).map_err(err)?;
            search_specs.insert(spec_id.to_string(),spec);
        }}
        let mut backup=ExperimentBackup{checksum:String::new(),schema:2,experiment_id:id.into(),plan,data,search_specs,
            events:ledger.events.iter().map(|(hash,e)|json!({"id":hash,"record":e})).collect(),
            head:ledger.events.last().map(|e|e.0.clone())};
        backup.checksum=backup.integrity_id()?;Ok(backup)
    }
    /// Validate the complete backup before publication. Existing history must be a
    /// matching prefix (or a superset); restore can never rewind a final commitment.
    pub fn restore(&self,backup:&ExperimentBackup)->Result<String,String>{
        if backup.checksum!=backup.integrity_id()? || !(1..=2).contains(&backup.schema) || content_id(&bytes(&backup.plan)?)!=backup.experiment_id
            || content_id(&bytes(&backup.data)?)!=backup.plan.dataset_id {
            return Err("experiment backup identity/integrity failure".into());
        }
        backup.data.verify_normalization(&backup.plan.config)?;
        sqx_backtest::validate_backtest_inputs(&backup.data.bars,&backup.data.instrument,&backup.plan.config)?;
        crate::observation::range(&backup.data.bars,backup.plan.config.bar_duration_ms,&backup.data.provenance["timeframe"])?;
        if backup.plan.schema!=1 || backup.plan.holdout_start<2 || backup.plan.holdout_start>=backup.data.bars.len()
            || backup.plan.min_development_trades==0 || backup.plan.min_holdout_trades==0
            || !backup.plan.periods_per_year.is_finite() || backup.plan.periods_per_year<=0.0 || backup.plan.engine_id.is_empty() {
            return Err("invalid experiment backup plan".into());
        }
        let mut records:Vec<(String,Event)>=Vec::new();
        for value in &backup.events {
            let event:Event=serde_json::from_value(value["record"].clone()).map_err(err)?;
            let hash=content_id(&bytes(&event)?);
            if value["id"]!=hash || event.index!=records.len() || event.previous!=records.last().map(|e|e.0.clone()) {
                return Err("experiment backup event integrity/sequence failure".into());
            }
            records.push((hash,event));
        }
        if backup.head!=records.last().map(|e|e.0.clone()){return Err("experiment backup head does not match history".into());}
        validate_history(&records)?;
        let mut needed=std::collections::BTreeSet::new();
        for (_,event) in &records {if event.payload["kind"]=="job_started" {
            let spec_id=event.payload["spec_id"].as_str().ok_or("missing search specification")?;
            let spec=backup.search_specs.get(spec_id).ok_or("backup missing frozen search candidates")?;
            if content_id(&bytes(spec)?)!=spec_id || spec.candidates.len()!=event.payload["count"].as_u64().unwrap_or(0) as usize {
                return Err("search specification integrity/count failure".into());
            }
            needed.insert(spec_id.to_string());
        }}
        for (_,event) in &records {if event.payload["kind"]=="trial_started" {
            if let Some(job)=event.payload["context"]["job_id"].as_u64() {
                let definition=&records[job as usize].1.payload;
                let spec=backup.search_specs.get(definition["spec_id"].as_str().ok_or("missing search spec")?).ok_or("missing search spec")?;
                let index=event.payload["context"]["candidate_index"].as_u64().ok_or("missing candidate index")? as usize;
                if json!(spec.candidates.get(index).ok_or("candidate index out of bounds")?)!=event.payload["strategy"]
                    || event.payload["context"]["seed"]!=spec.seed || event.payload["context"]["generator"]!=spec.generator
                    || definition["seed"]!=spec.seed {return Err("recorded candidate differs from frozen search specification".into());}
            }
        }}
        if needed.len()!=backup.search_specs.len() || (backup.schema==1&&!needed.is_empty()) {return Err("unexpected search specifications in backup".into());}
        let _registry_lock=self.registry_lock()?;
        let mut ledger=self.open_ledger(&backup.experiment_id,false)?;
        for ((local,_),(incoming,_)) in ledger.events.iter().zip(&records) {
            if local!=incoming {return Err("experiment histories diverge; restore will not replace local evidence".into());}
        }
        // Archive objects may be deduplicated. The plan is the discovery marker,
        // published only after the complete journal while holding its OS lock.
        self.archive.write_blob("experiment_data",&bytes(&backup.data)?)?;
        for spec in backup.search_specs.values(){self.archive.write_blob("experiment_searches",&bytes(spec)?)?;}
        for (_,event) in records.into_iter().skip(ledger.events.len()) {ledger.append(event.payload)?;}
        self.archive.write_blob("experiment_plans",&bytes(&backup.plan)?)?;
        Ok(backup.experiment_id.clone())
    }
    pub fn list(&self)->Result<Vec<String>,String>{
        let dir=self.archive.root().join("experiment_plans");if !dir.exists(){return Ok(vec![]);}
        let mut ids=Vec::new();for file in fs::read_dir(dir).map_err(err)? {
            let path=file.map_err(err)?.path();if path.extension().is_some_and(|e|e=="json") {
                let id=path.file_stem().and_then(|s|s.to_str()).ok_or("invalid experiment filename")?;
                self.plan(id)?;ids.push(id.to_string());
            }
        }ids.sort();Ok(ids)
    }
}
impl Ledger {
    fn has(&self,kind:&str)->bool{self.find(kind).is_some()}
    fn find(&self,kind:&str)->Option<&Value>{self.events.iter().find(|e|e.1.payload["kind"]==kind).map(|e|&e.1.payload)}
    fn append(&mut self,payload:Value)->Result<(),String>{
        let event=Event{index:self.events.len(),previous:self.events.last().map(|e|e.0.clone()),payload};
        let hash=self.archive.write_blob("experiment_events",&bytes(&event)?)?;
        let src=self.archive.root().join("experiment_events").join(format!("{hash}.json"));
        fs::hard_link(src,self.dir.join(format!("{:020}-{hash}.json",event.index))).map_err(err)?;
        File::open(&self.dir).and_then(|f|f.sync_all()).map_err(err)?;
        self.events.push((hash,event));Ok(())
    }
}
fn same_market(a:&Instrument,b:&Instrument)->bool{
    a.name.eq_ignore_ascii_case(&b.name) || (!a.base_currency.is_empty()&&!a.quote_currency.is_empty()
        &&a.base_currency.eq_ignore_ascii_case(&b.base_currency)&&a.quote_currency.eq_ignore_ascii_case(&b.quote_currency))
}
fn intersects(a:i64,b:i64,c:i64,d:i64)->bool{a<=d&&c<=b}
fn job_summary(ledger:&Ledger,job_id:usize)->Result<Value,String>{
    let job=&ledger.events.get(job_id).filter(|e|e.1.payload["kind"]=="job_started").ok_or("unknown search job")?.1.payload;
    let starts:Vec<_>=ledger.events.iter().filter(|e|e.1.payload["kind"]=="trial_started"&&e.1.payload["context"]["job_id"]==job_id).collect();
    let finished:std::collections::BTreeSet<_>=ledger.events.iter().filter(|e|e.1.payload["kind"]=="trial_finished").filter_map(|e|e.1.payload["trial"].as_u64()).collect();
    let completed=starts.iter().filter(|start|start.1.payload["trial"].as_u64().is_some_and(|id|finished.contains(&id))).count();
    let total=job["count"].as_u64().ok_or("invalid job count")? as usize;
    Ok(json!({"job_id":job_id,"seed":job["seed"],"total":total,"completed":completed,"started":starts.len(),
        "state":if completed==total{"complete"}else if ledger.has("holdout_committed"){"frozen"}else{"resumable"}}))
}
fn validate_history(events:&[(String,Event)])->Result<(),String>{
    let mut started=Vec::new();let mut finished=std::collections::BTreeMap::new();
    let mut committed=None;let mut final_done=false;
    let mut jobs=std::collections::BTreeMap::<usize,(usize,usize,usize)>::new();
    let mut requests=std::collections::BTreeSet::new();
    let mut trial_jobs=std::collections::BTreeMap::new();
    for (_,event) in events {
        let p=&event.payload;let trial=p["trial"].as_u64().map(|n|n as usize);
        match p["kind"].as_str() {
            Some("job_started") if committed.is_none() && p["job_id"].as_u64()==Some(event.index as u64)
                && p["count"].as_u64().is_some_and(|n|(1..=2000).contains(&n))
                && p["seed"].as_u64().is_some() && p["spec_id"].as_str().is_some()
                && p["request_key"].as_str().is_some_and(|k|!k.is_empty()&&k.len()<=128&&k.is_ascii()&&requests.insert(k.to_string()))=>{
                jobs.insert(event.index,(p["count"].as_u64().unwrap() as usize,0,0));
            },
            Some("trial_started") if committed.is_none() && trial==Some(started.len())=>{
                serde_json::from_value::<Strategy>(p["strategy"].clone()).map_err(err)?;
                if let Some(job_id)=p["context"].get("job_id") {
                    let job_id=job_id.as_u64().ok_or("invalid trial job identifier")? as usize;
                    let job=jobs.get_mut(&job_id).ok_or("trial references missing search job")?;
                    if job.1!=job.2 || job.1>=job.0 || p["context"]["candidate_index"].as_u64()!=Some(job.1 as u64) {return Err("invalid/duplicate search candidate sequence".into());}
                    job.1+=1;trial_jobs.insert(started.len(),job_id);
                }
                started.push(());
            },
            Some("trial_finished") if committed.is_none() && trial.is_some_and(|n|n<started.len()&&!finished.contains_key(&n)) && p["report"].is_object()=>{
                if let Some(job)=trial_jobs.get(&trial.unwrap()){jobs.get_mut(job).unwrap().2+=1;}
                finished.insert(trial.unwrap(),p["report"]["evidence_sufficient"]==true);
            },
            Some("holdout_committed") if committed.is_none() && trial.is_some_and(|n|finished.get(&n)==Some(&true))=>{committed=trial;},
            Some("holdout_finished") if committed.is_some() && !final_done && p["report"]["trial"].as_u64().map(|n|n as usize)==committed=>{final_done=true;},
            _=>return Err("experiment backup contains an invalid state transition".into()),
        }
    }Ok(())
}
fn report(result:&sqx_backtest::BacktestResult,bars:usize,pyp:f64,minimum:usize)->Value{
    let metrics=sqx_metrics::compute(result,bars,pyp);
    json!({"execution":result,"metrics":{"net_profit":metrics.net_profit,"profit_factor":metrics.profit_factor,
        "max_drawdown_pct":metrics.max_drawdown_pct,"trades":metrics.number_of_trades},
        "minimum_trades":minimum,"evidence_sufficient":result.errors.is_empty()&&result.trades.len()>=minimum})
}
fn check_engine(plan:&ExperimentPlan,engine:&str)->Result<(),String>{if plan.engine_id!=engine{Err("experiment requires the original engine executable".into())}else{Ok(())}}
fn bytes<T:Serialize>(value:&T)->Result<Vec<u8>,String>{serde_json::to_vec(value).map_err(err)}
fn err(e:impl std::fmt::Display)->String{e.to_string()}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_search_claim_resumes_without_a_second_trial_number(){
        let root=std::env::temp_dir().join(format!("sqx-search-resume-{}",std::process::id()));
        let store=Experiments::new(&root);
        let data=ExperimentData{bars:(0..10).map(|i|Bar::new(i*60000,1.0,1.1,0.9,1.0,1.0)).collect(),instrument:Instrument::fx("EURUSD"),provenance:json!({})};
        let plan=ExperimentPlan{schema:1,engine_id:"fixture".into(),dataset_id:String::new(),holdout_start:5,config:BacktestConfig::default(),min_development_trades:1,min_holdout_trades:1,periods_per_year:100.0,prior_exposure_disclosed:true};
        let id=store.create(&data,plan).unwrap();let candidate=Strategy::new("frozen candidate");
        let spec=SearchSpec{seed:42,generator:json!({}),candidates:vec![candidate.clone()]};
        let job=store.start_search(&id,"fixture","interrupted",&spec).unwrap();let job_id=job["job_id"].as_u64().unwrap() as usize;
        {let mut ledger=store.ledger(&id).unwrap();ledger.append(json!({"kind":"trial_started","trial":0,"strategy":candidate,"context":{"job_id":job_id,"candidate_index":0,"seed":42,"generator":{}}})).unwrap();}
        let reopened=Experiments::new(&root);let resumed=reopened.advance_search(&id,"fixture",job_id).unwrap();
        assert_eq!(resumed["state"],"complete");assert_eq!(resumed["last_trial"]["trial"],0);assert_eq!(reopened.status(&id).unwrap()["trials_attempted"],1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn commitment_can_resume_after_process_loss_without_reselecting() {
        let root=std::env::temp_dir().join(format!("sqx-commit-resume-{}",std::process::id()));
        let store=Experiments::new(&root);
        let data=ExperimentData{bars:(0..10).map(|i|Bar::new(i*60000,1.0,1.1,0.9,1.0,1.0)).collect(),instrument:Instrument::fx("EURUSD"),provenance:json!({})};
        let plan=ExperimentPlan{schema:1,engine_id:"fixture".into(),dataset_id:String::new(),holdout_start:5,config:BacktestConfig::default(),min_development_trades:1,min_holdout_trades:1,periods_per_year:100.0,prior_exposure_disclosed:true};
        let id=store.create(&data,plan).unwrap();
        // Simulate interruption after the durable commitment but before execution.
        {let mut ledger=store.ledger(&id).unwrap();
            ledger.append(json!({"kind":"trial_started","trial":0,"strategy":Strategy::new("frozen")})).unwrap();
            ledger.append(json!({"kind":"trial_finished","trial":0,"report":{"evidence_sufficient":true}})).unwrap();
            ledger.append(json!({"kind":"holdout_committed","trial":0})).unwrap();}
        let reopened=Experiments::new(&root);
        assert!(reopened.reveal(&id,"fixture",1).is_err());
        let result=reopened.reveal(&id,"fixture",0).unwrap();
        assert_eq!(result["evidence_sufficient"],false,"a failed final evidence threshold must still be retained");
        assert_eq!(result,reopened.reveal(&id,"fixture",0).unwrap());
        assert_eq!(reopened.status(&id).unwrap()["state"],"revealed");
        fs::remove_dir_all(root).unwrap();
    }
}
