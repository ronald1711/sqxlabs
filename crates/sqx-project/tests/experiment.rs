use serde_json::json;
use sqx_project::experiment::{Experiments,ExperimentData,ExperimentPlan};
use sqx_backtest::BacktestConfig;
use sqx_core::{Bar,Instrument};
use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Comparator,Operand,PriceField,Join,MoneyManagement,ExitType};
static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {fn new()->Self{Self(std::env::temp_dir().join(format!("sqx-experiment-{}-{}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed))))}}
impl Drop for Temp {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn strategy()->Strategy {
    let mut s=Strategy::new("precommitted");s.long=true;s.short=false;
    s.money_management=MoneyManagement::FixedSize{size:0.1};s.exits=vec![ExitType::ExitAfterBars(3)];
    s.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Price(PriceField::Close,0),Comparator::Gt,Operand::Const(0.0))],Join::And));s
}
fn data()->ExperimentData {ExperimentData{bars:(0..100).map(|i|Bar::new(i*60000,1.0,1.02,0.98,1.001,1.0)).collect(),instrument:Instrument::fx("EURUSD"),provenance:json!({"source":"fixture"})}}
fn plan()->ExperimentPlan {ExperimentPlan{schema:1,engine_id:"test-engine".into(),dataset_id:String::new(),validation_start:None,requirements:None,holdout_start:70,config:BacktestConfig::default(),min_development_trades:1,min_holdout_trades:1,periods_per_year:100.0,prior_exposure_disclosed:false}}
#[test]
fn development_is_independent_of_holdout_and_final_choice_survives_restart(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let original=data();
    let id=store.create(&original,plan()).unwrap();
    let mut changed=original.clone();for bar in &mut changed.bars[70..] {bar.open=1.5;bar.high=1.6;bar.low=1.4;bar.close=1.55;}
    let alternate=store.create(&changed,plan()).unwrap();
    let a=store.trial(&id,"test-engine",&strategy()).unwrap();let b=store.trial(&alternate,"test-engine",&strategy()).unwrap();
    assert_eq!(a,b,"development must receive no holdout candles");
    let report=store.reveal(&id,"test-engine",0).unwrap();
    assert!(report["execution"]["trades"].as_array().unwrap().iter().all(|t|t["open_time"].as_i64().unwrap()>=70*60000));
    assert!(report["execution"]["equity"].as_array().unwrap().iter().all(|p|p[0].as_i64().unwrap()>=70*60000));
    assert_eq!(report,Experiments::new(&temp.0).reveal(&id,"test-engine",0).unwrap());
    assert!(store.trial(&id,"test-engine",&strategy()).is_err());
    assert!(store.reveal(&id,"test-engine",1).is_err());
    assert!(store.reveal(&id,"other-engine",0).is_err());
    assert_eq!(store.status(&id).unwrap()["trials_attempted"],1);
}
#[test]
fn insufficient_evidence_is_recorded_and_does_not_unlock_holdout(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();
    let result=store.trial(&id,"test-engine",&Strategy::new("no trades")).unwrap();
    assert_eq!(result["report"]["evidence_sufficient"],false);
    assert!(store.reveal(&id,"test-engine",0).is_err());
    assert_eq!(store.status(&id).unwrap()["state"],"development");
    assert_eq!(store.status(&id).unwrap()["trials_attempted"],1);
}
#[test]
fn competing_commitments_choose_exactly_one_candidate(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();
    store.trial(&id,"test-engine",&strategy()).unwrap();store.trial(&id,"test-engine",&strategy()).unwrap();
    let threads:Vec<_>=(0..2).map(|trial|{let store=store.clone();let id=id.clone();std::thread::spawn(move||store.reveal(&id,"test-engine",trial))}).collect();
    let results:Vec<_>=threads.into_iter().map(|t|t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r|r.is_ok()).count(),1);
    let status=store.status(&id).unwrap();assert_eq!(status["trials_attempted"],2);assert_eq!(status["state"],"revealed");
}
#[test]
fn ledger_corruption_and_invalid_splits_fail_closed(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);
    let mut invalid=plan();invalid.holdout_start=100;assert!(store.create(&data(),invalid).is_err());
    let id=store.create(&data(),plan()).unwrap();store.trial(&id,"test-engine",&strategy()).unwrap();
    let path=std::fs::read_dir(temp.0.join("experiments").join(&id)).unwrap().map(|p|p.unwrap().path()).find(|p|p.extension().is_some_and(|e|e=="json")).unwrap();
    std::fs::write(path,b"{}").unwrap();assert!(store.status(&id).is_err());assert!(store.reveal(&id,"test-engine",0).is_err());
    assert!(store.status("../../outside").is_err());
}

#[test]
fn backup_restores_full_history_without_rewinding_a_final_commitment(){
    let source=Temp::new();let destination=Temp::new();let a=Experiments::new(&source.0);let b=Experiments::new(&destination.0);
    let id=a.create(&data(),plan()).unwrap();
    a.trial(&id,"test-engine",&strategy()).unwrap();
    let older=a.backup(&id).unwrap();
    let final_result=a.reveal(&id,"test-engine",0).unwrap();
    let backup=a.backup(&id).unwrap();
    // Exercise the actual JSON interchange rather than an in-memory clone.
    let backup=serde_json::from_str(&serde_json::to_string(&backup).unwrap()).unwrap();
    b.restore(&backup).unwrap();assert_eq!(a.status(&id).unwrap(),b.status(&id).unwrap());
    b.restore(&older).unwrap();assert_eq!(b.status(&id).unwrap()["state"],"revealed");
    assert!(b.trial(&id,"test-engine",&strategy()).is_err());
    assert_eq!(final_result,b.reveal(&id,"test-engine",0).unwrap());
    assert_eq!(b.backup(&id).unwrap().head,a.backup(&id).unwrap().head);
}
#[test]
fn restore_rejects_corruption_and_divergence_without_mutation(){
    let source=Temp::new();let destination=Temp::new();let a=Experiments::new(&source.0);let b=Experiments::new(&destination.0);
    let id=a.create(&data(),plan()).unwrap();a.trial(&id,"test-engine",&strategy()).unwrap();
    let backup=a.backup(&id).unwrap();let mut damaged=backup.clone();damaged.data.bars[0].close=1.002;
    assert!(b.restore(&damaged).is_err());assert!(b.list().unwrap().is_empty());
    let mut truncated=backup.clone();truncated.events.pop();assert!(b.restore(&truncated).is_err());assert!(b.list().unwrap().is_empty());
    b.create(&data(),plan()).unwrap();let mut other=strategy();other.name="divergent".into();b.trial(&id,"test-engine",&other).unwrap();
    let before=b.status(&id).unwrap();assert!(b.restore(&backup).unwrap_err().contains("diverge"));assert_eq!(before,b.status(&id).unwrap());
}
#[test]
fn concurrent_restores_preserve_a_single_history(){
    let source=Temp::new();let destination=Temp::new();let a=Experiments::new(&source.0);let b=Experiments::new(&destination.0);
    let id=a.create(&data(),plan()).unwrap();a.trial(&id,"test-engine",&strategy()).unwrap();a.reveal(&id,"test-engine",0).unwrap();let backup=a.backup(&id).unwrap();
    let threads:Vec<_>=(0..4).map(|_|{let store=b.clone();let backup=backup.clone();std::thread::spawn(move||store.restore(&backup))}).collect();
    for thread in threads {assert_eq!(thread.join().unwrap().unwrap(),id);}
    assert_eq!(a.status(&id).unwrap(),b.status(&id).unwrap());
    assert!(b.trial(&id,"test-engine",&strategy()).is_err());
}

#[test]
fn backup_hashes_do_not_override_state_transition_validation(){
    use sqx_project::archive::content_id;
    let source=Temp::new();let destination=Temp::new();let a=Experiments::new(&source.0);let b=Experiments::new(&destination.0);
    let id=a.create(&data(),plan()).unwrap();a.trial(&id,"test-engine",&strategy()).unwrap();
    let mut backup=a.backup(&id).unwrap();
    // All hashes are self-consistent, but a commitment without a completed
    // eligible trial is not a legitimate protocol history.
    backup.events.clear();
    let record=json!({"index":0,"previous":null,"payload":{"kind":"holdout_committed","trial":0}});
    // Event serialization has declaration order, so construct its bytes explicitly.
    let raw=format!("{{\"index\":0,\"previous\":null,\"payload\":{}}}",record["payload"]);
    let hash=content_id(raw.as_bytes());
    backup.events.push(json!({"id":hash,"record":record}));backup.head=Some(hash);
    backup.checksum=backup.integrity_id().unwrap();
    assert!(b.restore(&backup).unwrap_err().contains("state transition"));
    assert!(b.list().unwrap().is_empty());
}

#[test]
fn overlapping_development_is_detected_across_snapshot_and_setting_changes(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();
    let mut other_plan=plan();other_plan.holdout_start=90;other_plan.config.spread_points=2.0;
    let mut imported_again=data();imported_again.provenance=json!({"source":"different import"});imported_again.bars[0].close=1.002;
    let other=store.create(&imported_again,other_plan).unwrap();
    assert_eq!(store.status(&id).unwrap()["current_exposure_audit"]["classification"],"no_known_registry_overlap","reserving a plan alone is not an evaluation");
    store.trial(&other,"test-engine",&strategy()).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["same_market_experiment_attempts"],1);
    assert_eq!(audit["known_overlaps"][0]["source"],"experiment_development");
    assert_eq!(audit["known_overlaps"][0]["id"],other);
}
#[test]
fn concurrent_final_tests_record_the_first_commitment_as_reuse_for_the_second(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let first=store.create(&data(),plan()).unwrap();
    let mut changed=plan();changed.config.spread_points=2.0;let second=store.create(&data(),changed).unwrap();
    store.trial(&first,"test-engine",&strategy()).unwrap();store.trial(&second,"test-engine",&strategy()).unwrap();
    let threads:Vec<_>=[first,second].into_iter().map(|id|{let store=store.clone();std::thread::spawn(move||store.reveal(&id,"test-engine",0).unwrap())}).collect();
    let reports:Vec<_>=threads.into_iter().map(|t|t.join().unwrap()).collect();
    assert_eq!(reports.iter().filter(|r|r["exposure_at_commitment"]["classification"]=="known_reuse_or_disclosed_exposure").count(),1);
    assert_eq!(reports.iter().filter(|r|r["exposure_at_commitment"]["classification"]=="no_known_registry_overlap").count(),1);
}
#[test]
fn archived_backtests_count_as_exposure_but_unrelated_markets_do_not(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();
    let mut unrelated=data();unrelated.instrument=Instrument::fx("GBPUSD");let other=store.create(&unrelated,plan()).unwrap();
    store.trial(&other,"test-engine",&strategy()).unwrap();store.reveal(&other,"test-engine",0).unwrap();
    assert_eq!(store.status(&id).unwrap()["current_exposure_audit"]["classification"],"no_known_registry_overlap");
    let archive=sqx_project::archive::Archive::new(&temp.0);
    let data=data();let run=archive.save("test-engine".into(),json!({"instrument":data.instrument,"bars":data.bars}),json!(strategy()),json!({}),json!({"net_profit":-10.0})).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["known_overlaps"][0]["id"],run);
    assert_eq!(audit["same_market_archived_backtest_records"],1);
    assert_eq!(audit["same_market_experiment_attempts"],0);
}
#[test]
fn failed_pipeline_reports_also_disclose_holdout_exposure(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();
    let fixture=data();let archive=sqx_project::archive::Archive::new(&temp.0);
    archive.save("test-engine".into(),json!({"instrument":fixture.instrument,"bars":fixture.bars}),json!({"name":"Pipeline"}),json!({"research_kind":"pipeline"}),json!({"error":"candidate failed after research"})).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["same_market_archived_backtest_records"],0);
    assert_eq!(audit["same_market_archived_pipeline_records"],1);
    assert_eq!(audit["known_overlaps"][0]["source"],"archived_pipeline");
}
#[test]
fn failed_robustness_reports_also_disclose_holdout_exposure(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();
    let fixture=data();let archive=sqx_project::archive::Archive::new(&temp.0);
    archive.save("test-engine".into(),json!({"instrument":fixture.instrument,"bars":fixture.bars}),json!({"name":"Stress test"}),json!({"research_kind":"robustness"}),json!({"error":"candidate failed after research"})).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["same_market_archived_backtest_records"],0);
    assert_eq!(audit["same_market_archived_robustness_records"],1);
    assert_eq!(audit["known_overlaps"][0]["source"],"archived_robustness");
}
#[test]
fn nonoverlapping_observation_periods_do_not_trigger_reuse(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();
    let mut earlier=data();for bar in &mut earlier.bars {bar.time-=100*60000;}
    let other=store.create(&earlier,plan()).unwrap();store.trial(&other,"test-engine",&strategy()).unwrap();store.reveal(&other,"test-engine",0).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"no_known_registry_overlap");assert_eq!(audit["same_market_experiment_attempts"],1);
}

#[test]
fn restore_preserves_known_reuse_even_without_the_other_experiment(){
    let source=Temp::new();let destination=Temp::new();let a=Experiments::new(&source.0);let b=Experiments::new(&destination.0);
    let id=a.create(&data(),plan()).unwrap();let mut other_plan=plan();other_plan.holdout_start=90;
    let other=a.create(&data(),other_plan).unwrap();a.trial(&other,"test-engine",&strategy()).unwrap();
    a.trial(&id,"test-engine",&strategy()).unwrap();
    assert_eq!(a.reveal(&id,"test-engine",0).unwrap()["exposure_at_commitment"]["classification"],"known_reuse_or_disclosed_exposure");
    b.restore(&a.backup(&id).unwrap()).unwrap();
    let audit=b.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["carried_commitment_evidence"]["known_overlaps"][0]["id"],other);
}

fn search_spec()->sqx_project::experiment::SearchSpec {
    sqx_project::experiment::SearchSpec{seed:42,generator:json!({"fixture":true}),candidates:(0..4).map(|i|{let mut s=strategy();s.name=format!("candidate-{i}");s}).collect()}
}
#[test]
fn search_jobs_resume_from_saved_candidates_without_duplicate_trials(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();let spec=search_spec();
    let job=store.start_search(&id,"test-engine","request-1",&spec).unwrap();let job_id=job["job_id"].as_u64().unwrap() as usize;
    assert_eq!(job,store.start_search(&id,"test-engine","request-1",&spec).unwrap());
    assert_eq!(store.advance_search(&id,"test-engine",job_id).unwrap()["completed"],1);
    let reopened=Experiments::new(&temp.0);assert_eq!(reopened.status(&id).unwrap()["jobs"][0]["completed"],1);
    assert!(reopened.advance_search(&id,"changed-engine",job_id).is_err());
    for expected in 2..=4 {
        let progress = reopened.advance_search(&id, "test-engine", job_id).unwrap();
        assert_eq!(progress["completed"], expected);
        let counted: u64 = ["passed", "rejected", "incomplete_assessments"].iter()
            .map(|key| progress[key].as_u64().unwrap()).sum();
        assert_eq!(counted, expected);
    }
    assert_eq!(reopened.advance_search(&id,"test-engine",job_id).unwrap()["state"],"complete");
    assert_eq!(reopened.status(&id).unwrap()["trials_attempted"],4);
    let mut changed=spec.clone();changed.candidates[0].name="changed".into();assert!(reopened.start_search(&id,"test-engine","request-1",&changed).is_err());
    let fresh=reopened.start_search(&id,"test-engine","request-2",&spec).unwrap();assert_ne!(fresh["job_id"],job["job_id"]);
}
#[test]
fn search_backup_can_resume_on_another_store(){
    let source=Temp::new();let destination=Temp::new();let a=Experiments::new(&source.0);let b=Experiments::new(&destination.0);
    let id=a.create(&data(),plan()).unwrap();let job=a.start_search(&id,"test-engine","frozen",&search_spec()).unwrap();let job_id=job["job_id"].as_u64().unwrap() as usize;
    a.advance_search(&id,"test-engine",job_id).unwrap();let backup=a.backup(&id).unwrap();assert_eq!(backup.search_specs.len(),1);
    b.restore(&serde_json::from_str(&serde_json::to_string(&backup).unwrap()).unwrap()).unwrap();
    for _ in 1..4 {assert_eq!(a.advance_search(&id,"test-engine",job_id).unwrap(),b.advance_search(&id,"test-engine",job_id).unwrap());}
    assert_eq!(a.status(&id).unwrap(),b.status(&id).unwrap());
    let mut corrupted=backup.clone();corrupted.search_specs.values_mut().next().unwrap().candidates[0].name="mutated".into();corrupted.checksum=corrupted.integrity_id().unwrap();
    assert!(b.restore(&corrupted).is_err());
}
#[test]
fn concurrent_search_workers_checkpoint_each_candidate_once(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();
    let job=store.start_search(&id,"test-engine","parallel",&search_spec()).unwrap();let job_id=job["job_id"].as_u64().unwrap() as usize;
    let workers:Vec<_>=(0..8).map(|_|{let store=store.clone();let id=id.clone();std::thread::spawn(move||store.advance_search(&id,"test-engine",job_id).unwrap())}).collect();
    for worker in workers{worker.join().unwrap();}
    let status=store.status(&id).unwrap();assert_eq!(status["trials_attempted"],4);assert_eq!(status["jobs"][0]["completed"],4);
    let starts:Vec<_>=status["events"].as_array().unwrap().iter().filter(|e|e["event"]["kind"]=="trial_started").collect();
    for (i,start) in starts.iter().enumerate(){assert_eq!(start["event"]["context"]["candidate_index"],i);}
}
#[test]
fn legacy_experiment_backups_still_restore_and_final_commitment_freezes_jobs(){
    let source=Temp::new();let destination=Temp::new();let a=Experiments::new(&source.0);let b=Experiments::new(&destination.0);
    let id=a.create(&data(),plan()).unwrap();a.trial(&id,"test-engine",&strategy()).unwrap();
    let mut legacy=a.backup(&id).unwrap();legacy.schema=1;legacy.checksum=legacy.integrity_id().unwrap();
    let mut wire=serde_json::to_value(legacy).unwrap();wire.as_object_mut().unwrap().remove("search_specs");
    b.restore(&serde_json::from_value(wire).unwrap()).unwrap();
    let job=b.start_search(&id,"test-engine","pending",&search_spec()).unwrap();b.reveal(&id,"test-engine",0).unwrap();
    assert!(b.advance_search(&id,"test-engine",job["job_id"].as_u64().unwrap() as usize).is_err());
    assert_eq!(b.status(&id).unwrap()["jobs"][0]["state"],"frozen");
}

#[test]
fn restored_comparison_preserves_embedded_backtest_exposure(){
    use sqx_project::{archive::Archive,reconciliation::*};
    let original=Temp::new();let archive=Archive::new(&original.0);let fixture=data();
    let run=archive.save("test-engine".into(),json!({"instrument":fixture.instrument,"bars":fixture.bars,"timeframe":sqx_core::Timeframe::M1}),json!(Strategy::new("empty exposure fixture")),json!({}),json!({"trades":0,"trade_details":[],"run_context":{"account_currency":"USD"}})).unwrap();
    let source=json!({"schema":1,"source_label":"synthetic fixture","scope":{"market":"EURUSD","quote_currency":"USD","account_currency":"USD","contract_size":100000,"start_ms":0,"end_ms":5999999},"trades":[]}).to_string();
    let comparison=save_comparison(&archive,&run,&source,&Tolerances{time_ms:0,price:0.0,size:0.0,money:0.0}).unwrap();
    let record=load_comparison(&archive,&comparison).unwrap();
    let restored=Temp::new();let restored_archive=Archive::new(&restored.0);restore_comparison(&restored_archive,&comparison,&record).unwrap();
    assert!(restored_archive.list().unwrap().is_empty());
    let store=Experiments::new(&restored.0);let id=store.create(&data(),plan()).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["same_market_archived_comparison_records"],1);
    assert_eq!(audit["known_overlaps"][0]["source"],"comparison_reference_backtest");
    assert_eq!(audit["known_overlaps"][0]["run_id"],run);
    let mut other=data();other.instrument=Instrument::fx("GBPUSD");let unrelated=store.create(&other,plan()).unwrap();
    assert_eq!(store.status(&unrelated).unwrap()["current_exposure_audit"]["classification"],"no_known_registry_overlap");
    let mut later=data();for bar in &mut later.bars {bar.time+=200*60000;}
    let later_id=store.create(&later,plan()).unwrap();
    assert_eq!(store.status(&later_id).unwrap()["current_exposure_audit"]["classification"],"no_known_registry_overlap");
}

#[test]
fn assistant_summaries_disclose_holdout_exposure_without_claiming_backtest_counts(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();
    let fixture=data();let archive=sqx_project::archive::Archive::new(&temp.0);
    archive.save("test-engine".into(),json!({"instrument":fixture.instrument,"bars":fixture.bars}),json!({"name":"AI research"}),json!({"research_kind":"assistant_candidates"}),json!({"session_completed":false,"candidate_evaluations":[]})).unwrap();
    for kind in ["assistant_trial_started","assistant_trial"] {
        archive.save("test-engine".into(),json!({"instrument":fixture.instrument,"bars":fixture.bars}),json!({"name":"AI trial"}),json!({"research_kind":kind}),json!({"fixture":true})).unwrap();
    }
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["same_market_archived_backtest_records"],0);
    assert_eq!(audit["same_market_archived_assistant_records"],3);
    assert_eq!(audit["known_overlaps"][0]["source"],"archived_assistant_candidates");
}

#[test]
fn cli_backtest_archives_disclose_exposure_even_for_failed_execution(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();
    let fixture=data();let archive=sqx_project::archive::Archive::new(&temp.0);
    for errors in [json!([]),json!(["execution failed"])] {
        archive.save("test-engine".into(),json!({"instrument":fixture.instrument,"bars":fixture.bars,"ticks":null}),json!({"name":"CLI research"}),json!({"research_kind":"cli_backtest"}),json!({"errors":errors})).unwrap();
    }
    archive.save("test-engine".into(),json!({"instrument":fixture.instrument,"bars":fixture.bars}),json!({"name":"Pending CLI research"}),json!({"research_kind":"cli_backtest_started"}),json!({"status":"execution_not_recorded"})).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["same_market_archived_backtest_records"],3);
    assert_eq!(audit["same_market_archived_cli_backtest_records"],3);
    assert_eq!(audit["known_overlaps"].as_array().unwrap().len(),3);
    assert!(audit["known_overlaps"].as_array().unwrap().iter().all(|r|r["source"]=="archived_cli_backtest"));
    let mut other=data();other.instrument=Instrument::fx("GBPUSD");let other_id=store.create(&other,plan()).unwrap();
    assert_eq!(store.status(&other_id).unwrap()["current_exposure_audit"]["same_market_archived_cli_backtest_records"],0);
    let mut later=data();for bar in &mut later.bars{bar.time+=200*60000;}
    let later_id=store.create(&later,plan()).unwrap();
    assert_eq!(store.status(&later_id).unwrap()["current_exposure_audit"]["classification"],"no_known_registry_overlap");
}

#[test]
fn crosscheck_inputs_include_later_markets_and_unfinished_execution_in_exposure(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let fixture=data();let id=store.create(&fixture,plan()).unwrap();
    let archive=sqx_project::archive::Archive::new(&temp.0);
    let dataset=json!({"markets":[
        {"instrument":Instrument::fx("USDJPY"),"bars":fixture.bars},
        {"instrument":fixture.instrument,"bars":fixture.bars},
        {"instrument":fixture.instrument,"bars":fixture.bars}
    ]});
    let checkpoint=archive.save("test-engine".into(),dataset.clone(),json!(strategy()),json!({"research_kind":"cli_crosscheck_started"}),json!({"status":"execution_not_recorded"})).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["same_market_archived_crosscheck_records"],1,"one archive record despite multiple matching datasets");
    assert_eq!(audit["same_market_archived_backtest_records"],0);
    assert_eq!(audit["known_overlaps"].as_array().unwrap().len(),2);
    for (i,overlap) in audit["known_overlaps"].as_array().unwrap().iter().enumerate(){
        assert_eq!(overlap["source"],"archived_cli_crosscheck");assert_eq!(overlap["id"],checkpoint);
        assert_eq!(overlap["market_index"],i+1);assert_eq!(overlap["record_kind"],"cli_crosscheck_started");
    }
    archive.save("test-engine".into(),dataset,json!(strategy()),json!({"research_kind":"cli_crosscheck","started_run_id":checkpoint}),json!({"report":{"invalid_markets":1}})).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["same_market_archived_crosscheck_records"],2,"record count is not an independent trial count");
    store.trial(&id,"test-engine",&strategy()).unwrap();
    let result=store.reveal(&id,"test-engine",0).unwrap();
    assert_eq!(result["exposure_at_commitment"]["classification"],"known_reuse_or_disclosed_exposure");
    let restored_root=Temp::new();let restored=Experiments::new(&restored_root.0);
    restored.restore(&store.backup(&id).unwrap()).unwrap();
    let restored_audit=restored.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(restored_audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(restored_audit["carried_commitment_evidence"]["same_market_archived_crosscheck_records"],2);
}

#[test]
fn crosscheck_market_and_time_boundaries_do_not_fabricate_overlap(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let fixture=data();let id=store.create(&fixture,plan()).unwrap();
    let archive=sqx_project::archive::Archive::new(&temp.0);
    archive.save("test-engine".into(),json!({"markets":[
        {"instrument":Instrument::fx("USDJPY"),"bars":fixture.bars},
        {"instrument":fixture.instrument,"bars":&fixture.bars[..70]}
    ]}),json!(strategy()),json!({"research_kind":"cli_crosscheck"}),json!({})).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"no_known_registry_overlap");
    assert_eq!(audit["same_market_archived_crosscheck_records"],1);
    assert!(audit["known_overlaps"].as_array().unwrap().is_empty());
    archive.save("test-engine".into(),json!({"markets":[]}),json!(strategy()),json!({"research_kind":"cli_crosscheck_started"}),json!({})).unwrap();
    assert!(store.status(&id).unwrap_err().contains("nonempty market inputs"),"malformed evidence cannot become a clean audit");
}

#[test]
fn candle_contents_overlap_holdout_even_when_the_open_is_before_it(){
    for crosscheck in [false,true] {
        let temp=Temp::new();let store=Experiments::new(&temp.0);let mut current=data();current.provenance=json!({"timeframe":"M1"});let mut config=plan();config.config.bar_duration_ms=Some(60000);
        let id=store.create(&current,config).unwrap();let archive=sqx_project::archive::Archive::new(&temp.0);
        let market=json!({"instrument":current.instrument,"bars":[Bar::new(3600000,1.0,1.0,1.0,1.0,1.0)],"timeframe":"H1","source_context":{"timeframe":"H1"},"config":{"bar_duration_ms":3600000}});
        let (dataset,request)=if crosscheck{(json!({"markets":[market]}),json!({"research_kind":"cli_crosscheck_started"}))}else{(market,json!({"research_kind":"cli_backtest_started","config":{"bar_duration_ms":3600000}}))};
        archive.save("test-engine".into(),dataset,json!(strategy()),request,json!({})).unwrap();
        let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
        assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure","H1 [60min,120min) overlaps M1 holdout [70min,100min)");
        assert_eq!(audit["observation_extent_complete"],true);
        assert_eq!(audit["known_overlaps"][0]["end"],7199999);
    }
}

#[test]
fn final_holdout_candle_extent_is_included_but_exclusive_end_is_not(){
    for (source_start,overlaps) in [(9000000,true),(10800000,false)] {
        let temp=Temp::new();let store=Experiments::new(&temp.0);
        let current=ExperimentData{bars:(0..3).map(|i|Bar::new(i*3600000,1.0,1.0,1.0,1.0,1.0)).collect(),instrument:Instrument::fx("EURUSD"),provenance:json!({"timeframe":"H1"})};
        let mut config=plan();config.holdout_start=2;config.config.bar_duration_ms=Some(3600000);
        let id=store.create(&current,config).unwrap();
        sqx_project::archive::Archive::new(&temp.0).save("test-engine".into(),json!({"instrument":current.instrument,"timeframe":"M1","bars":[Bar::new(source_start,1.0,1.0,1.0,1.0,1.0)]}),json!(strategy()),json!({}),json!({})).unwrap();
        let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
        assert_eq!(audit["classification"]=="known_reuse_or_disclosed_exposure",overlaps);
        assert_eq!(audit["observation_extent_complete"],true);
    }
}

#[test]
fn other_experiment_training_candle_extents_disclose_data_reuse(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let id=store.create(&data(),plan()).unwrap();
    let other=ExperimentData{bars:(0..3).map(|i|Bar::new(i*3600000,1.0,1.0,1.0,1.0,1.0)).collect(),instrument:Instrument::fx("EURUSD"),provenance:json!({"timeframe":"H1"})};
    let mut config=plan();config.holdout_start=2;config.config.bar_duration_ms=Some(3600000);
    let other_id=store.create(&other,config).unwrap();store.trial(&other_id,"test-engine",&strategy()).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["known_overlaps"][0]["source"],"experiment_development");
    assert_eq!(audit["known_overlaps"][0]["end"],7199999);
}

#[test]
fn missing_duration_is_explicit_uncertainty_and_survives_restored_commitment(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let mut current=data();current.provenance=json!({"timeframe":"M1"});let mut config=plan();config.config.bar_duration_ms=Some(60000);
    let id=store.create(&current,config).unwrap();
    sqx_project::archive::Archive::new(&temp.0).save("test-engine".into(),json!({"instrument":current.instrument,"bars":[Bar::new(0,1.0,1.0,1.0,1.0,1.0)]}),json!(strategy()),json!({}),json!({})).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"no_known_registry_overlap");assert_eq!(audit["observation_extent_complete"],false);
    assert_eq!(audit["unknown_interval_extents"].as_array().unwrap().len(),1);
    store.trial(&id,"test-engine",&strategy()).unwrap();let revealed=store.reveal(&id,"test-engine",0).unwrap();
    assert_eq!(revealed["exposure_at_commitment"]["observation_extent_complete"],false);
    let target=Temp::new();let restored=Experiments::new(&target.0);restored.restore(&store.backup(&id).unwrap()).unwrap();
    let audit=restored.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["observation_extent_complete"],false);assert_eq!(audit["carried_commitment_evidence"]["unknown_interval_extents"].as_array().unwrap().len(),1);
}

#[test]
fn restored_comparison_candle_extent_discloses_overlap_without_its_source_run(){
    use sqx_project::{archive::Archive,reconciliation::*};
    let original=Temp::new();let archive=Archive::new(&original.0);
    let run=archive.save("test-engine".into(),json!({"instrument":Instrument::fx("EURUSD"),"bars":[Bar::new(3600000,1.0,1.0,1.0,1.0,1.0)],"timeframe":"H1"}),json!(Strategy::new("empty comparison fixture")),json!({}),json!({"trades":0,"trade_details":[],"run_context":{"account_currency":"USD"}})).unwrap();
    let source=json!({"schema":1,"source_label":"synthetic H1 fixture","scope":{"market":"EURUSD","quote_currency":"USD","account_currency":"USD","contract_size":100000,"start_ms":3600000,"end_ms":7199999},"trades":[]}).to_string();
    let comparison=save_comparison(&archive,&run,&source,&Tolerances{time_ms:0,price:0.0,size:0.0,money:0.0}).unwrap();
    let record=load_comparison(&archive,&comparison).unwrap();let restored=Temp::new();let target=Archive::new(&restored.0);
    restore_comparison(&target,&comparison,&record).unwrap();assert!(target.list().unwrap().is_empty());
    let store=Experiments::new(&restored.0);let id=store.create(&data(),plan()).unwrap();
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["known_overlaps"][0]["source"],"comparison_reference_backtest");assert_eq!(audit["known_overlaps"][0]["end"],7199999);
}

#[test]
fn contradictory_timeframe_metadata_is_rejected_before_creating_experiment(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let mut fixture=data();fixture.provenance=json!({"timeframe":"H1"});let mut config=plan();config.config.bar_duration_ms=Some(60000);
    assert!(store.create(&fixture,config).unwrap_err().contains("conflicts with timeframe"));
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn normalization_sources_survive_restore_and_rehashed_candle_edits_are_rejected(){
    use sqx_project::archive::content_id;
    let mut utc=String::from("timestamp,open,high,low,close,volume\n");
    let mut raw=String::from("timestamp_broker,open,high,low,close,volume\n");
    for i in 0..100 {
        utc.push_str(&format!("{},1,1.02,0.98,1.001,1\n",1700000040+i*60));
        raw.push_str(&format!("{},1,1.02,0.98,1.001,1\n",1700007240+i*60));
    }
    let policy=json!({"schema":1,"server":"synthetic","intervals":[{"raw_start_ms":1700007240000i64,"raw_end_ms":1700013240000i64,"offset_seconds":7200,"source":"explicit synthetic offset"}]}).to_string();
    let manifest=json!({"schema":1,"server":"synthetic","raw_sha256":content_id(raw.as_bytes()),"policy_sha256":content_id(policy.as_bytes()),"normalized_sha256":content_id(utc.as_bytes()),"rows":100,"bar_duration_ms":60000,"clock":"broker to UTC","scope":"synthetic transformation only"}).to_string();
    let bundle=sqx_data::normalization::Bundle{manifest_json:manifest,policy_json:policy,raw_csv:raw,utc_csv:utc.clone()};
    let original=ExperimentData{bars:sqx_data::read_csv_bars_str(&utc).unwrap(),instrument:Instrument::fx("EURUSD"),provenance:json!({"timeframe":"M1","normalization":bundle})};
    let mut planned=plan();planned.config.bar_duration_ms=Some(60000);
    let source=Temp::new();let archive=Experiments::new(&source.0);
    let id=archive.create(&original,planned.clone()).unwrap();let backup=archive.backup(&id).unwrap();
    let destination=Temp::new();let restored=Experiments::new(&destination.0);
    restored.restore(&backup).unwrap();
    assert_eq!(restored.backup(&id).unwrap().data.provenance,original.provenance);
    let mut forged=backup.clone();forged.data.bars[0].close=1.002;
    forged.plan.dataset_id=content_id(&serde_json::to_vec(&forged.data).unwrap());
    forged.experiment_id=content_id(&serde_json::to_vec(&forged.plan).unwrap());
    forged.checksum=forged.integrity_id().unwrap();
    assert!(restored.restore(&forged).unwrap_err().contains("supplied candles"));
    assert_eq!(restored.list().unwrap().len(),1);
    let mut corrupt=original.clone();corrupt.provenance["normalization"]["policy_json"]=json!("{}");
    assert!(archive.create(&corrupt,planned.clone()).is_err());
    planned.config.bar_duration_ms=Some(120000);
    assert!(archive.create(&original,planned).unwrap_err().contains("timeframe"));
}

#[test]
fn mcp_generation_phases_disclose_exposure_without_inventing_backtest_or_trial_counts(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);let mut contract=plan();contract.config.bar_duration_ms=Some(60000);
    let id=store.create(&data(),contract.clone()).unwrap();let fixture=data();let archive=sqx_project::archive::Archive::new(&temp.0);
    for kind in ["mcp_generation_started","mcp_generation_trial_started","mcp_generation_trial","mcp_generation"] {
        archive.save("test-engine".into(),json!({"instrument":fixture.instrument,"bars":fixture.bars,"timeframe":"M1"}),json!({"name":"MCP fixture"}),json!({"research_kind":kind,"config":{"bar_duration_ms":60000}}),json!({"status":"fixture","errors":["incomplete or failed execution"]})).unwrap();
    }
    let audit=store.status(&id).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["same_market_archived_mcp_generation_records"],4);
    assert_eq!(audit["same_market_archived_backtest_records"],0);assert_eq!(audit["same_market_experiment_attempts"],0);
    assert_eq!(audit["known_overlaps"].as_array().unwrap().len(),4);
    for row in audit["known_overlaps"].as_array().unwrap(){assert_eq!(row["source"],"archived_mcp_generation");assert_eq!(row["end"],5999999,"inclusive end includes the final millisecond of the last candle");}
    let mut other=data();other.instrument=Instrument::fx("GBPUSD");let other_id=store.create(&other,contract.clone()).unwrap();
    assert_eq!(store.status(&other_id).unwrap()["current_exposure_audit"]["same_market_archived_mcp_generation_records"],0);
    let mut later=data();for bar in &mut later.bars{bar.time+=200*60000;}
    let later_id=store.create(&later,contract).unwrap();assert_eq!(store.status(&later_id).unwrap()["current_exposure_audit"]["classification"],"no_known_registry_overlap");
    store.trial(&id,"test-engine",&strategy()).unwrap();store.reveal(&id,"test-engine",0).unwrap();
    let restored_root=Temp::new();let restored=Experiments::new(&restored_root.0);restored.restore(&store.backup(&id).unwrap()).unwrap();
    let audit=restored.status(&id).unwrap()["current_exposure_audit"].clone();assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["carried_commitment_evidence"]["same_market_archived_mcp_generation_records"],4);
}

// --- Walk-forward gate inside the development/holdout protocol -------------
//
// Synthetic 160-bar oracle: two eligible in-sample runs, two out-of-sample
// trades per window, window nets -20 and +70 (joint total 50, threshold 50).
// Holdout data starts at bar 160 and must never influence the development
// selection or its walk-forward archive.

fn oracle_candidate() -> Strategy {
    let mut s=Strategy::new("frozen walk-forward candidate");s.long=true;s.short=false;
    s.money_management=MoneyManagement::FixedSize{size:0.1};s.exits=vec![ExitType::ExitAfterBars(12)];
    s.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));s
}

fn alternate_candidate() -> Strategy {
    let mut s=oracle_candidate();s.name="alternate frozen candidate".into();s.exits=vec![ExitType::ExitAfterBars(1)];s
}

fn oracle_bars() -> Vec<Bar> {
    let steps: &[(usize,f64)]=&[
        (7,1.1010),(20,1.0990),(33,1.1000),(46,1.0980),(54,1.1000),
        (56,1.1013),(63,1.1003),(76,1.0993),
        (87,1.1003),(100,1.0983),(113,1.0993),(126,1.0973),(134,1.0999),
        (143,1.1049),(154,1.1069),
        // Validation region [160,240): an oscillating path so the basic checks
        // see both winners and losers instead of a non-finite profit factor.
        (168,1.1072),(176,1.1062),(184,1.1074),(192,1.1064),
        (200,1.1076),(208,1.1066),(216,1.1078),(224,1.1068),(232,1.1080),
    ];
    let mut bars=Vec::with_capacity(320);let mut price=1.1000f64;let mut next=0;
    for index in 0..320 {
        while next<steps.len()&&steps[next].0<=index{price=steps[next].1;next+=1;}
        bars.push(Bar::new(index as i64*60_000,price,price,price,price,1.0));
    }
    bars
}

fn oracle_data()->ExperimentData {
    ExperimentData{bars:oracle_bars(),instrument:Instrument::fx("EURUSD"),provenance:json!({"source":"synthetic walk-forward oracle","timeframe":"M1"})}
}

fn walk_forward_requirements()->sqx_project::search_requirements::Requirements {
    sqx_project::search_requirements::Requirements{
        min_trades:1,min_net_profit:-1_000_000_000.0,min_profit_factor:0.0,max_drawdown_pct:1e12,
        spread_stress:None,monte_carlo:None,
        walk_forward:Some(sqx_project::search_requirements::WalkForward{
            windows:2,oos_fraction:0.30,min_is_trades:2,min_oos_trades_per_window:2,
            min_total_oos_net_profit:50.0,
        }),
    }
}

fn walk_forward_plan()->ExperimentPlan {
    let mut plan=plan();
    // Development [0,160) carries the walk-forward gate; validation [160,240)
    // and the reserved final test [240,320) follow it.
    plan.validation_start=Some(160);
    plan.holdout_start=240;
    plan.requirements=Some(walk_forward_requirements());
    plan.config.initial_deposit=1_000.0;
    plan.config.bar_duration_ms=Some(60_000);
    plan.periods_per_year=525_600.0;
    plan
}

fn walk_forward_spec()->sqx_project::experiment::SearchSpec {
    sqx_project::experiment::SearchSpec{seed:11,generator:json!({"fixture":"walk_forward"}),
        candidates:vec![oracle_candidate(),alternate_candidate()]}
}

#[test]
fn walk_forward_search_pauses_resumes_and_reopens_with_identical_evidence(){
    let temp=Temp::new();let store=Experiments::new(&temp.0);
    let id=store.create(&oracle_data(),walk_forward_plan()).unwrap();
    let spec=walk_forward_spec();
    let job=store.start_search(&id,"test-engine","wf-request",&spec).unwrap();
    let job_id=job["job_id"].as_u64().unwrap() as usize;
    // Pause after the first candidate.
    let first=store.advance_search(&id,"test-engine",job_id).unwrap();
    assert_eq!(first["state"],"resumable");
    let frozen_report=first["last_trial"]["report"].clone();
    let measured=frozen_report["assessment"]["walk_forward"]["oos_total_net_profit"].as_f64().unwrap();
    assert!((measured-50.0).abs()<1e-6,"joint total {measured} must equal the 50 threshold's realized value");
    assert_eq!(frozen_report["assessment"]["walk_forward"]["min_total_oos_net_profit"].as_f64().unwrap(),50.0);
    assert_eq!(frozen_report["assessment"]["checks"].as_array().unwrap()
        .iter().find(|check|check["metric"]=="walk_forward_total_oos_net_profit").unwrap()["decision"],"passed");
    // A reopened store resumes the same frozen job without duplicate trials.
    let resumed=Experiments::new(&temp.0);
    let second=resumed.advance_search(&id,"test-engine",job_id).unwrap();
    assert_eq!(second["state"],"complete");
    assert_eq!(second["completed"],2);
    assert_eq!(resumed.status(&id).unwrap()["trials_attempted"],2);
    assert_eq!(resumed.status(&id).unwrap()["jobs"][0]["state"],"complete");
    // The same run without the pause produces byte-identical evidence.
    let uninterrupted_root=Temp::new();let uninterrupted=Experiments::new(&uninterrupted_root.0);
    let expected_id=uninterrupted.create(&oracle_data(),walk_forward_plan()).unwrap();
    let expected_job=uninterrupted.start_search(&expected_id,"test-engine","wf-request",&spec).unwrap();
    let expected_job_id=expected_job["job_id"].as_u64().unwrap() as usize;
    let expected_first=uninterrupted.advance_search(&expected_id,"test-engine",expected_job_id).unwrap();
    let expected_second=uninterrupted.advance_search(&expected_id,"test-engine",expected_job_id).unwrap();
    assert_eq!(first["last_trial"],expected_first["last_trial"]);
    assert_eq!(second["last_trial"],expected_second["last_trial"]);
    assert_eq!(resumed.status(&id).unwrap()["events"],uninterrupted.status(&expected_id).unwrap()["events"]);
    // Every archived candidate keeps the per-window archive for later re-reading.
    let events=resumed.status(&id).unwrap()["events"].clone();
    let finished:Vec<_>=events.as_array().unwrap().iter()
        .filter(|event|event["event"]["kind"]=="trial_finished").collect();
    assert_eq!(finished.len(),2);
    for event in finished {
        let assessment=&event["event"]["report"]["assessment"];
        let walk_forward=&assessment["walk_forward"];
        assert_eq!(walk_forward["method"],"fixed_candidate_segmented_v1");
        assert_eq!(walk_forward["windows"].as_array().unwrap().len(),2);
        let check=assessment["checks"].as_array().unwrap().iter()
            .find(|check|check["metric"]=="walk_forward_total_oos_net_profit").unwrap();
        match check["decision"].as_str().unwrap() {
            "passed" => {
                let total=walk_forward["oos_total_net_profit"].as_f64().unwrap();
                assert!((total-50.0).abs()<1e-6,"archived joint total {total} must be 50");
            }
            "insufficient_evidence" => assert!(walk_forward["oos_total_net_profit"].is_null(),
                "missing evidence must never be archived as a fabricated total"),
            other => panic!("unexpected walk-forward decision {other}"),
        }
        for window in walk_forward["windows"].as_array().unwrap(){
            assert!(window["reason"].as_str().is_some());
            assert_eq!(window["thresholds"]["min_oos_trades_per_window"],2);
        }
    }
}

#[test]
fn mutating_only_the_reserved_final_test_leaves_selection_and_dev_results_intact(){
    let original=oracle_data();
    let mut mutated=original.clone();
    // Only the reserved holdout candles change (a rising path instead of flat).
    for (offset,bar) in mutated.bars[240..].iter_mut().enumerate(){
        let price=1.2000+offset as f64*0.0010;
        bar.open=price;bar.high=price+0.0005;bar.low=price-0.0005;bar.close=price+0.0005;
    }
    let root_a=Temp::new();let root_b=Temp::new();
    let a=Experiments::new(&root_a.0);let b=Experiments::new(&root_b.0);
    let id_a=a.create(&original,walk_forward_plan()).unwrap();
    let id_b=b.create(&mutated,walk_forward_plan()).unwrap();
    assert_ne!(id_a,id_b,"mutated holdout data is a distinct dataset");
    for (store,id) in [(&a,&id_a),(&b,&id_b)] {
        let job=store.start_search(id,"test-engine","wf-request",&walk_forward_spec()).unwrap();
        let job_id=job["job_id"].as_u64().unwrap() as usize;
        store.advance_search(id,"test-engine",job_id).unwrap();
        store.advance_search(id,"test-engine",job_id).unwrap();
    }
    // Candidate selection and every pre-final-test result are identical.
    assert_eq!(a.status(&id_a).unwrap()["jobs"],b.status(&id_b).unwrap()["jobs"]);
    let dev_a=a.status(&id_a).unwrap()["events"].clone();
    let dev_b=b.status(&id_b).unwrap()["events"].clone();
    let dev_only=|events:&serde_json::Value|events.as_array().unwrap().iter()
        .filter(|event|event["event"]["kind"]=="trial_finished")
        .map(|event|event["event"]["report"].clone()).collect::<Vec<_>>();
    let reports_a=dev_only(&dev_a);let reports_b=dev_only(&dev_b);
    assert_eq!(reports_a.len(),2);
    assert_eq!(reports_a,reports_b,"development and walk-forward results must not see the holdout");
    for report in &reports_a {
        assert_eq!(report["assessment"]["walk_forward"]["method"],"fixed_candidate_segmented_v1");
        assert_eq!(report["assessment"]["walk_forward"]["windows"].as_array().unwrap().len(),2);
    }
    let total=reports_a[0]["assessment"]["walk_forward"]["oos_total_net_profit"].as_f64().unwrap();
    assert!((total-50.0).abs()<1e-6,"development walk-forward total {total} must be 50");
    // The mutation is effective: it changes the reserved final test itself.
    let final_a=a.reveal(&id_a,"test-engine",0).unwrap();
    let final_b=b.reveal(&id_b,"test-engine",0).unwrap();
    assert_eq!(final_a["trial"],final_b["trial"]);
    assert_ne!(final_a["metrics"]["net_profit"],final_b["metrics"]["net_profit"]);
    assert_ne!(final_a["execution"]["trades"],final_b["execution"]["trades"]);
    assert!(final_b["metrics"]["net_profit"].as_f64().unwrap()>0.0);
    // Pre-final-test history stays unchanged after the reveal in both studies.
    assert_eq!(dev_only(&a.status(&id_a).unwrap()["events"].clone()),reports_a);
    assert_eq!(dev_only(&b.status(&id_b).unwrap()["events"].clone()),reports_b);
}
