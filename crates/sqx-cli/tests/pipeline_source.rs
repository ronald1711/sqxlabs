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
    let events=saved.run.result["event_ids"].as_array().unwrap();
    let mut previous=Value::Null;
    let mut outcomes=0;
    for (sequence,id) in events.iter().enumerate() {
        let event=archive.load(id.as_str().unwrap()).unwrap();
        assert_eq!(event.run.request["sequence"],sequence);
        assert_eq!(event.run.request["previous_event_id"],previous);
        assert_eq!(event.run.request["started_run_id"],started.run_id);
        if event.run.result["kind"]=="build_outcome" {outcomes+=1;}
        previous=id.clone();
    }
    assert_eq!(outcomes,2);
    portable_restore(&root,&started.run_id);
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
    interrupted_generation_recovery(false);
}

#[test]
fn interrupted_standalone_generation_preserves_all_trials_and_recovers() {
    interrupted_generation_recovery(true);
}

fn interrupted_generation_recovery(standalone:bool) {
    use std::process::Stdio;
    // Parallel cases can observe the same clock tick in the same test process.
    // Include the route and a random identity instead of using time as uniqueness.
    let root=std::env::temp_dir().join(format!("sqx-pipeline-kill-{}-{:032x}",if standalone{"generate"}else{"pipeline"},rand::random::<u128>()));
    fs::create_dir_all(&root).unwrap();
    let csv=root.join("bars.csv");
    let mut data="timestamp,open,high,low,close,volume\n".to_string();
    for i in 0..500 { data.push_str(&format!("{},1.1,1.101,1.099,1.1,1\n",1700000000+i*60)); }
    fs::write(&csv,&data).unwrap();
    struct Child(std::process::Child);
    impl Drop for Child {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
    let generate=|directory:&str|{
        let mut command=Command::new(env!("CARGO_BIN_EXE_sqx-cli"));
        command.args([if standalone{"generate"}else{"pipeline"},"--data"]).arg(&csv)
            .args(["--symbol","EURUSD","--timeframe","M1","--count","64","--archive-dir"]).arg(root.join(directory));
        if !standalone{command.args(["--keep","1"]);}
        command
    };
    let mut child=Child(generate("archive").stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
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
    let start_id=&records.iter().find(|r|r.run.request["research_kind"]=="cli_pipeline_started").unwrap().run_id;
    portable_restore(&root,start_id);
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
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        let uid=Command::new("id").arg("-u").output().unwrap();assert!(uid.status.success());
        // Root bypasses ordinary permission bits; this OS-level fault fixture
        // applies to the non-root Linux/macOS accounts used in CI and locally.
        if String::from_utf8_lossy(&uid.stdout).trim()!="0" {
            let directory=root.join("restored/runs");
            struct RestorePermissions(std::path::PathBuf,std::fs::Permissions);
            impl Drop for RestorePermissions{fn drop(&mut self){let _=fs::set_permissions(&self.0,self.1.clone());}}
            let permissions=RestorePermissions(directory.clone(),fs::metadata(&directory).unwrap().permissions());
            let before=restored.list().unwrap();
            fs::set_permissions(&directory,fs::Permissions::from_mode(0o500)).unwrap();
            let denied=command("replay-pipeline",true);
            assert!(!denied.status.success(),"unwritable archive must not report completion");
            assert!(String::from_utf8_lossy(&denied.stderr).contains("os error 13"),"{}",String::from_utf8_lossy(&denied.stderr));
            assert_eq!(restored.list().unwrap(),before,"no execution event may be published after the write failure");
            drop(permissions);
            let after=command("pipeline-history",false);assert!(after.status.success());
            assert_eq!(serde_json::from_slice::<Value>(&after.stdout).unwrap()["status"],"incomplete");
        }
    }
    let resumed=command("replay-pipeline",true);assert!(resumed.status.success(),"{}",String::from_utf8_lossy(&resumed.stderr));
    let summary:Value=serde_json::from_slice(&resumed.stdout).unwrap();
    assert!(summary["replayed_events"].as_u64().unwrap()>0);assert!(summary["appended_events"].as_u64().unwrap()>0);
    let completed=restored.load(summary["completion_id"].as_str().unwrap()).unwrap();
    let count=restored.list().unwrap().len();
    let replay=command("replay-pipeline",false);assert!(replay.status.success(),"{}",String::from_utf8_lossy(&replay.stderr));
    assert_eq!(serde_json::from_slice::<Value>(&replay.stdout).unwrap()["appended_events"],0);
    assert_eq!(restored.list().unwrap().len(),count);
    fs::write(&csv,&data).unwrap();
    let reference=generate("reference").output().unwrap();
    assert!(reference.status.success(),"{}",String::from_utf8_lossy(&reference.stderr));
    let receipt:Value=serde_json::from_str(String::from_utf8_lossy(&reference.stdout).lines().find_map(|l|l.strip_prefix(if standalone{"Generation archive: "}else{"Pipeline archive: "})).unwrap()).unwrap();
    let expected=sqx_project::archive::Archive::new(root.join("reference")).load(receipt["run_id"].as_str().unwrap()).unwrap();
    assert_eq!(completed.run.result["report"],expected.run.result["report"]);
    assert_eq!(completed.run.result["report"]["builds"].as_array().unwrap().len(),64);
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

// Exercise the public transport boundary, with the original archive unavailable
// during import. No test-only record restoration can hide missing backup inputs.
fn portable_restore(root:&std::path::Path, start_id:&str) {
    let exported=Command::new(env!("CARGO_BIN_EXE_sqx-cli"))
        .args(["export-pipeline","--archive-dir"]).arg(root.join("archive"))
        .args(["--run-id",start_id]).output().unwrap();
    assert!(exported.status.success(),"{}",String::from_utf8_lossy(&exported.stderr));
    let file=root.join("backup.json");fs::write(&file,&exported.stdout).unwrap();
    fs::remove_dir_all(root.join("archive")).unwrap();
    let import=||Command::new(env!("CARGO_BIN_EXE_sqx-cli"))
        .args(["import-pipeline","--archive-dir"]).arg(root.join("restored"))
        .arg("--input").arg(&file).output().unwrap();
    let imported=import();
    assert!(imported.status.success(),"{}",String::from_utf8_lossy(&imported.stderr));
    let receipt:Value=serde_json::from_slice(&imported.stdout).unwrap();
    assert_eq!(receipt["started_run_id"],start_id);
    assert_eq!(receipt["execution_replayed"],false);
    assert_eq!(receipt["engine_bytes_included"],false);
    let repeated=import();assert!(repeated.status.success());
    assert_eq!(serde_json::from_slice::<Value>(&repeated.stdout).unwrap()["published_records"],0);
}

#[test]
fn killed_walkforward_restores_full_trial_evidence_and_matches_uninterrupted_run() {
    use sqx_project::archive::{Archive,content_id};
    let root=std::env::temp_dir().join(format!("sqx-wf-kill-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
    let _cleanup=Cleanup(root.clone());
    let archive=Archive::new(root.join("archive"));
    let bars:Vec<_>=(0..1500).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;sqx_core::Bar::new(i*60000,p,p+0.003,p-0.003,p+0.0002,10.0)}).collect();
    let pipeline=sqx_pipeline::Pipeline{name:"walk-forward recovery".into(),tasks:vec![
        sqx_pipeline::Task{kind:sqx_pipeline::TaskKind::Build{count:100,keep:1}},
        sqx_pipeline::Task{kind:sqx_pipeline::TaskKind::Optimize{windows:3,candidates:16}}]};
    let executable=env!("CARGO_BIN_EXE_sqx-cli");
    let engine=content_id(&fs::read(executable).unwrap());
    let id=archive.save(engine,json!({"bars":bars,"instrument":sqx_core::Instrument::fx("EURUSD"),"timeframe":"M1"}),
        json!({"pipeline":pipeline,"generator":sqx_optimizer::GeneratorConfig::default()}),
        json!({"research_kind":"cli_pipeline_started","attempt_id":"killed-walkforward-fixture","config":sqx_backtest::BacktestConfig::default(),"seed":42,"periods_per_year":525600.0}),
        json!({"status":"execution_not_recorded"})).unwrap();
    let start=archive.load(&id).unwrap();
    let command=|folder:&str|{let mut command=Command::new(executable);command.args(["replay-pipeline","--archive-dir"]).arg(root.join(folder)).args(["--run-id",&id,"--resume"]);command};
    struct Child(std::process::Child);
    impl Drop for Child{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
    let mut child=Child(command("archive").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap());
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(120);
    let mut seen=std::collections::BTreeSet::new();
    let retained=loop {
        let mut found=None;
        for entry in fs::read_dir(archive.root().join("runs")).unwrap(){
            let path=entry.unwrap().path();if path.extension().and_then(|v|v.to_str())!=Some("json"){continue;}
            let key=path.file_stem().unwrap().to_str().unwrap().to_string();
            if !seen.insert(key.clone()){continue;}
            let record=archive.load(&key).unwrap();
            if record.run.result["kind"]=="walk_forward" && record.run.result["event"]["kind"]=="candidate_outcome" {found=Some(record);break;}
        }
        if let Some(record)=found{break record;}
        assert!(child.0.try_wait().unwrap().is_none(),"pipeline ended before a walk-forward outcome was persisted");
        assert!(std::time::Instant::now()<deadline,"walk-forward checkpoint deadline exceeded");
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    child.0.kill().unwrap();child.0.wait().unwrap();
    assert!(!retained.run.result["event"]["execution"]["trades"].as_array().unwrap().is_empty());
    assert!(!archive.list().unwrap().iter().any(|key|archive.load(key).unwrap().run.request["research_kind"]=="cli_pipeline"));
    portable_restore(&root,&id);
    let restored=Archive::new(root.join("restored"));
    assert_eq!(restored.load(&retained.run_id).unwrap().run.result,retained.run.result);
    let resumed=command("restored").output().unwrap();
    assert!(resumed.status.success(),"{}",String::from_utf8_lossy(&resumed.stderr));
    let receipt:Value=serde_json::from_slice(&resumed.stdout).unwrap();
    assert!(receipt["replayed_events"].as_u64().unwrap()>0);assert!(receipt["appended_events"].as_u64().unwrap()>0);
    let completion=restored.load(receipt["completion_id"].as_str().unwrap()).unwrap();
    let uninterrupted=Archive::new(root.join("uninterrupted"));uninterrupted.restore(&start).unwrap();
    let fresh=command("uninterrupted").output().unwrap();assert!(fresh.status.success(),"{}",String::from_utf8_lossy(&fresh.stderr));
    let fresh_receipt:Value=serde_json::from_slice(&fresh.stdout).unwrap();
    let expected=uninterrupted.load(fresh_receipt["completion_id"].as_str().unwrap()).unwrap();
    // Exact event IDs compare all archived full executions, not only summary metrics.
    assert_eq!(completion.run.result,expected.run.result);
    assert_eq!(completion.run_id,expected.run_id);
    assert_eq!(completion.run.result["report"]["optimizations"].as_array().unwrap().len(),1);
    let outcomes=completion.run.result["event_ids"].as_array().unwrap().iter().filter(|key|{
        let event=restored.load(key.as_str().unwrap()).unwrap();
        event.run.result["kind"]=="walk_forward" && event.run.result["event"]["kind"]=="candidate_outcome"
    }).count();
    assert_eq!(outcomes,48);
}

#[test]
fn standalone_optimization_survives_kill_backup_and_source_deletion(){
    use sqx_project::archive::Archive;
    use std::process::Stdio;
    let root=std::env::temp_dir().join(format!("sqx-optimize-recovery-{:032x}",rand::random::<u128>()));
    struct Cleanup(std::path::PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
    let _cleanup=Cleanup(root.clone());fs::create_dir_all(&root).unwrap();
    let data=root.join("bars.csv");let mut csv="timestamp,open,high,low,close,volume\n".to_owned();
    let bars:Vec<_>=(0..600).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;csv.push_str(&format!("{},{},{},{},{},1\n",1700000000+i*60,p,p+0.002,p-0.002,p));sqx_core::Bar::new((1700000000+i*60)*1000,p,p+0.002,p-0.002,p,1.0)}).collect();
    fs::write(&data,csv).unwrap();
    struct Child(std::process::Child);impl Drop for Child{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
    let mut child=Child(Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["optimize","--data"]).arg(&data).args(["--symbol","EURUSD","--timeframe","M1","--windows","2","--oos","0.3","--candidates","16","--seed","731","--archive-dir"]).arg(root.join("archive")).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    let archive=Archive::new(root.join("archive"));let deadline=std::time::Instant::now()+std::time::Duration::from_secs(60);
    let start_id=loop{
        let found=archive.list().unwrap_or_default().into_iter().find_map(|id|{let b=archive.load(&id).ok()?;(b.run.result["kind"]=="walk_forward" && b.run.result["event"]["kind"]=="candidate_outcome").then(||b.run.request["started_run_id"].as_str().unwrap().to_owned())});
        if let Some(id)=found{break id;}
        assert!(child.0.try_wait().unwrap().is_none(),"optimization finished before interruption");
        assert!(std::time::Instant::now()<deadline,"no optimization checkpoint");std::thread::sleep(std::time::Duration::from_millis(10));
    };
    child.0.kill().unwrap();child.0.wait().unwrap();
    let start=archive.load(&start_id).unwrap();
    fs::remove_file(data).unwrap();portable_restore(&root,&start_id);
    let restored=Archive::new(root.join("restored"));
    let replay=|directory:&str|Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["replay-pipeline","--run-id",&start_id,"--resume","--archive-dir"]).arg(root.join(directory)).output().unwrap();
    let out=replay("restored");assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let receipt:Value=serde_json::from_slice(&out.stdout).unwrap();assert!(receipt["appended_events"].as_u64().unwrap()>0);
    let done=restored.load(receipt["completion_id"].as_str().unwrap()).unwrap();
    let plain=sqx_optimizer::walk_forward(&bars,&serde_json::from_value(start.dataset["instrument"].clone()).unwrap(),&serde_json::from_value(start.run.request["config"].clone()).unwrap(),start.run.request["periods_per_year"].as_f64().unwrap(),&sqx_optimizer::GeneratorConfig::default(),2,0.3,16,731);
    assert_eq!(done.run.result["report"]["discovery"],serde_json::to_value(plain).unwrap());
    let fresh=Archive::new(root.join("fresh"));fresh.restore(&start).unwrap();let out=replay("fresh");assert!(out.status.success());
    let receipt:Value=serde_json::from_slice(&out.stdout).unwrap();assert_eq!(receipt["completion_id"],done.run_id);
    let before=restored.list().unwrap();let out=replay("restored");assert!(out.status.success());assert_eq!(restored.list().unwrap(),before);
    let count=done.run.result["event_ids"].as_array().unwrap().iter().filter(|id|restored.load(id.as_str().unwrap()).unwrap().run.result["event"]["kind"]=="candidate_outcome").count();assert_eq!(count,32);
}
