use std::{fs,process::Command};
use serde_json::{json,Value};
#[test]
fn pipeline_binds_seed_and_settings_and_assistant_checks_data_before_provider_access(){
    let root=std::env::temp_dir().join(format!("sqx-cli-pipeline-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();
    let data=root.join("bars.csv");let config=root.join("config.json");
    fs::write(&config,json!({"account":{"currency":"JPY"},"spread_points":7}).to_string()).unwrap();
    fs::write(&data,"timestamp,open,high,low,close,volume\n1700000000,150,151,149,150,1\n1700000060,150,151,149,150,1\n").unwrap();
    let run=|extra:&[&str]|Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["pipeline","--data"]).arg(&data).args(["--count","2","--keep","1","--archive-dir"]).arg(root.join("archive")).args(extra).output().unwrap();
    assert!(!run(&[]).status.success());
    let out=run(&["--symbol","USDJPY","--timeframe","M1","--config",config.to_str().unwrap(),"--seed","731"]);assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let text=String::from_utf8(out.stdout).unwrap();let context:Value=serde_json::from_str(text.lines().find_map(|s|s.strip_prefix("Research context: ")).unwrap()).unwrap();
    assert_eq!(context["seed"],731);assert_eq!(context["symbol"],"USDJPY");assert_eq!(context["config"]["account"]["currency"],"JPY");assert_eq!(context["pipeline"]["build_evaluations"],2);
    let archive=sqx_project::archive::Archive::new(root.join("archive"));
    let receipt:Value=serde_json::from_str(text.lines().find_map(|s|s.strip_prefix("Pipeline archive: ")).unwrap()).unwrap();
    let saved=archive.load(receipt["run_id"].as_str().unwrap()).unwrap();
    let started=archive.load(saved.run.request["started_run_id"].as_str().unwrap()).unwrap();
    assert_eq!(started.run.result,json!({"status":"execution_not_recorded"}));
    assert_eq!(started.dataset,saved.dataset);
    assert_eq!(saved.run.result["report"]["builds"].as_array().unwrap().len(),2);
    assert!(saved.run.result["report"]["builds"].as_array().unwrap().iter().all(|r|r["selected"]==false));
    assert_eq!(saved.run.request["seed"],731);
    assert_eq!(saved.strategy["pipeline"]["tasks"].as_array().unwrap().len(),3);
    let restored=sqx_project::archive::Archive::new(root.join("restored"));
    restored.restore(&started).unwrap();restored.restore(&saved).unwrap();
    let events=saved.run.result["event_ids"].as_array().unwrap();
    let mut previous=Value::Null;
    let mut outcomes=0;
    for (sequence,id) in events.iter().enumerate() {
        let event=archive.load(id.as_str().unwrap()).unwrap();
        assert_eq!(event.run.request["sequence"],sequence);
        assert_eq!(event.run.request["previous_event_id"],previous);
        assert_eq!(event.run.request["started_run_id"],started.run_id);
        if event.run.result["kind"]=="build_outcome" {outcomes+=1;}
        restored.restore(&event).unwrap();
        previous=id.clone();
    }
    assert_eq!(outcomes,2);
    fs::remove_dir_all(root.join("archive")).unwrap();
    assert_eq!(restored.load(&saved.run_id).unwrap().run.result,saved.run.result);
    let experiments=sqx_project::experiment::Experiments::new(root.join("restored"));
    let mut bars:Vec<sqx_core::Bar>=serde_json::from_value(saved.dataset["bars"].clone()).unwrap();
    let mut prior=bars[0];prior.time-=60000;bars.insert(0,prior);
    let input=sqx_project::experiment::ExperimentData{bars,instrument:sqx_core::Instrument::fx("USDJPY"),provenance:json!({"timeframe":"M1"})};
    let plan=sqx_project::experiment::ExperimentPlan{schema:1,engine_id:saved.run.engine_id.clone(),dataset_id:String::new(),holdout_start:2,config:sqx_backtest::BacktestConfig::default(),min_development_trades:1,min_holdout_trades:1,periods_per_year:525600.0,prior_exposure_disclosed:false};
    let experiment=experiments.create(&input,plan).unwrap();
    let audit=experiments.status(&experiment).unwrap()["current_exposure_audit"].clone();
    assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");
    assert_eq!(audit["same_market_archived_pipeline_records"],events.len()+2);
    let failed=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["pipeline","--data"]).arg(&data)
        .args(["--symbol","USDJPY","--timeframe","M1","--count","0","--keep","1","--archive-dir"]).arg(root.join("failed-archive")).output().unwrap();
    assert!(!failed.status.success());
    let failure_text=String::from_utf8(failed.stdout).unwrap();
    let failure_receipt:Value=serde_json::from_str(failure_text.lines().find_map(|s|s.strip_prefix("Pipeline archive: ")).unwrap()).unwrap();
    let failure=sqx_project::archive::Archive::new(root.join("failed-archive")).load(failure_receipt["run_id"].as_str().unwrap()).unwrap();
    assert!(!failure.run.result["report"]["errors"].as_array().unwrap().is_empty());
    fs::write(&data,"timestamp,open,high,low,close,volume\n1700000000,150,151,149,150,1\n1700000120,150,151,149,150,1\n").unwrap();
    let gap=run(&["--symbol","USDJPY","--timeframe","M1"]);assert!(!gap.status.success());assert!(String::from_utf8_lossy(&gap.stderr).contains("Unresolved data gaps"));
    let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
    let endpoint=format!("http://{}/v1",listener.local_addr().unwrap());
    let assistant=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["assistant","--data"]).arg(&data).args(["--symbol","USDJPY","--timeframe","M1","--base-url",&endpoint,"--api-key","fixture-key","--ask","backtest this data"]).output().unwrap();
    assert!(!assistant.status.success());assert!(String::from_utf8_lossy(&assistant.stderr).contains("Unresolved data gaps"));
    assert_eq!(listener.accept().unwrap_err().kind(),std::io::ErrorKind::WouldBlock,"invalid research input must not contact the provider");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn interrupted_pipeline_retains_frozen_family_and_completed_candidate() {
    use std::process::Stdio;
    let root=std::env::temp_dir().join(format!("sqx-pipeline-kill-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&root).unwrap();
    let csv=root.join("bars.csv");
    let mut data="timestamp,open,high,low,close,volume\n".to_string();
    for i in 0..500 { data.push_str(&format!("{},1.1,1.101,1.099,1.1,1\n",1700000000+i*60)); }
    fs::write(&csv,&data).unwrap();
    struct Child(std::process::Child);
    impl Drop for Child {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
    let mut child=Child(Command::new(env!("CARGO_BIN_EXE_sqx-cli"))
        .args(["pipeline","--data"]).arg(&csv)
        .args(["--symbol","EURUSD","--timeframe","M1","--count","64","--keep","1","--archive-dir"])
        .arg(root.join("archive")).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    let archive=sqx_project::archive::Archive::new(root.join("archive"));
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(60);
    loop {
        let found=archive.list().unwrap_or_default().into_iter().any(|id|archive.load(&id).map(|b|b.run.result["kind"]=="build_outcome").unwrap_or(false));
        if found {break;}
        assert!(child.0.try_wait().unwrap().is_none(),"pipeline ended before interruption point");
        assert!(std::time::Instant::now()<deadline,"no durable candidate before deadline");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    child.0.kill().unwrap();child.0.wait().unwrap();
    fs::remove_file(&csv).unwrap();
    let records:Vec<_>=archive.list().unwrap().iter().map(|id|archive.load(id).unwrap()).collect();
    assert!(records.iter().any(|r|r.run.request["research_kind"]=="cli_pipeline_started"));
    assert!(!records.iter().any(|r|r.run.request["research_kind"]=="cli_pipeline"),"interrupted run must not claim completion");
    assert!(records.iter().any(|r|r.run.result["kind"]=="build_family" && r.run.result["candidates"].as_array().unwrap().len()==64));
    let outcomes:Vec<_>=records.iter().filter(|r|r.run.result["kind"]=="build_outcome").collect();
    assert!(!outcomes.is_empty());
    assert_eq!(outcomes[0].dataset["bars"].as_array().unwrap().len(),500);
    let restored=sqx_project::archive::Archive::new(root.join("restored"));
    for record in &records {restored.restore(record).unwrap();}
    fs::remove_dir_all(root.join("archive")).unwrap();
    for record in &records {assert_eq!(restored.load(&record.run_id).unwrap().run.result,record.run.result);}
    let started=records.iter().find(|r|r.run.request["research_kind"]=="cli_pipeline_started").unwrap();
    let command=|subcommand:&str,resume:bool|{
        let mut c=Command::new(env!("CARGO_BIN_EXE_sqx-cli"));
        c.args([subcommand,"--archive-dir"]).arg(root.join("restored")).args(["--run-id",&started.run_id]);
        if resume{c.arg("--resume");}c.output().unwrap()
    };
    let inspection=command("pipeline-history",false);assert!(inspection.status.success());
    assert_eq!(serde_json::from_slice::<Value>(&inspection.stdout).unwrap()["status"],"incomplete");
    assert!(!command("replay-pipeline",false).status.success());
    let lock_path=root.join("restored/pipeline-locks").join(&started.run_id);
    let lock=fs::OpenOptions::new().read(true).write(true).open(&lock_path).unwrap();lock.try_lock().unwrap();
    let busy=command("replay-pipeline",true);assert!(!busy.status.success());
    assert!(String::from_utf8_lossy(&busy.stderr).contains("process lock"));drop(lock);
    let resumed=command("replay-pipeline",true);assert!(resumed.status.success(),"{}",String::from_utf8_lossy(&resumed.stderr));
    let summary:Value=serde_json::from_slice(&resumed.stdout).unwrap();
    assert!(summary["replayed_events"].as_u64().unwrap()>0);assert!(summary["appended_events"].as_u64().unwrap()>0);
    let completed=restored.load(summary["completion_id"].as_str().unwrap()).unwrap();
    let count=restored.list().unwrap().len();
    let replay=command("replay-pipeline",false);assert!(replay.status.success(),"{}",String::from_utf8_lossy(&replay.stderr));
    assert_eq!(serde_json::from_slice::<Value>(&replay.stdout).unwrap()["appended_events"],0);
    assert_eq!(restored.list().unwrap().len(),count);
    fs::write(&csv,&data).unwrap();
    let reference=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["pipeline","--data"]).arg(&csv)
        .args(["--symbol","EURUSD","--timeframe","M1","--count","64","--keep","1","--archive-dir"])
        .arg(root.join("reference")).output().unwrap();
    let receipt:Value=serde_json::from_str(String::from_utf8_lossy(&reference.stdout).lines().find_map(|l|l.strip_prefix("Pipeline archive: ")).unwrap()).unwrap();
    let expected=sqx_project::archive::Archive::new(root.join("reference")).load(receipt["run_id"].as_str().unwrap()).unwrap();
    assert_eq!(completed.run.result["report"],expected.run.result["report"]);
    assert_ne!(completed.run.request["started_run_id"],expected.run.request["started_run_id"],"fresh attempts need distinct identities");
    let altered=sqx_project::archive::Archive::new(root.join("altered"));
    altered.restore(started).unwrap();
    let mut prefix:Vec<_>=records.iter().filter(|r|r.run.request["research_kind"]=="cli_pipeline_event").collect();
    prefix.sort_by_key(|r|r.run.request["sequence"].as_u64().unwrap());
    for event in prefix {
        if event.run.result["kind"]=="build_outcome" {
            let mut changed=event.run.result.clone();changed["candidate_index"]=json!(999);
            altered.save(event.run.engine_id.clone(),event.dataset.clone(),event.strategy.clone(),event.run.request.clone(),changed).unwrap();break;
        }
        altered.restore(event).unwrap();
    }
    let before=altered.list().unwrap().len();
    let mismatch=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["replay-pipeline","--archive-dir"]).arg(root.join("altered"))
        .args(["--run-id",&started.run_id,"--resume"]).output().unwrap();
    assert!(!mismatch.status.success());assert!(String::from_utf8_lossy(&mismatch.stderr).contains("replay differs"));
    assert_eq!(altered.list().unwrap().len(),before,"mismatched prefix must not append work");
    let wrong=altered.save("different engine".into(),started.dataset.clone(),started.strategy.clone(),started.run.request.clone(),started.run.result.clone()).unwrap();
    let mismatch=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["replay-pipeline","--archive-dir"]).arg(root.join("altered"))
        .args(["--run-id",&wrong,"--resume"]).output().unwrap();
    assert!(!mismatch.status.success());assert!(String::from_utf8_lossy(&mismatch.stderr).contains("engine executable differs"));
    let event=records.iter().find(|r|r.run.result["kind"]=="build_outcome").unwrap();
    let mut corrupt=event.run.result.clone();corrupt["candidate_index"]=json!(999);
    restored.save(event.run.engine_id.clone(),event.dataset.clone(),event.strategy.clone(),event.run.request.clone(),corrupt).unwrap();
    let conflict=command("pipeline-history",false);assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("Conflicting pipeline events"));
    assert!(!command("replay-pipeline",true).status.success());
    fs::remove_dir_all(root).unwrap();
}
