use std::{fs,io::{Read,Write},net::TcpListener,process::Command,time::{Duration,Instant}};
use serde_json::{json,Value};

#[test]
fn assistant_archives_candidates_after_success_and_provider_failure_and_reports_storage_failure(){
    for (provider_ok,storage_ok) in [(true,true),(false,true),(true,false)] {
        let root=std::env::temp_dir().join(format!("sqx-assistant-archive-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("bars.csv"),"timestamp,open,high,low,close,volume\n1700000000,1,1,1,1,1\n1700000060,1,1,1,1,1\n").unwrap();
        if !storage_ok {fs::write(root.join("data"),"block archive directory creation").unwrap();}
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
        let endpoint=format!("http://{}/v1",listener.local_addr().unwrap());
        let server=std::thread::spawn(move||{
            let mut requests=Vec::new();
            for round in 0..if storage_ok {3}else{1} {
                let deadline=Instant::now()+Duration::from_secs(15);
                let mut stream=loop {match listener.accept(){Ok((s,_))=>break s,Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>{assert!(Instant::now()<deadline,"provider request missing");std::thread::sleep(Duration::from_millis(10));},Err(e)=>panic!("{e}")}};
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut bytes=Vec::new();let mut buf=[0;4096];
                let body=loop {
                    let n=stream.read(&mut buf).unwrap();assert!(n>0);bytes.extend_from_slice(&buf[..n]);
                    if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {
                        let headers=String::from_utf8_lossy(&bytes[..end]);
                        let length:usize=headers.lines().find_map(|line|line.split_once(':').filter(|(k,_)|k.eq_ignore_ascii_case("content-length")).map(|(_,v)|v.trim().parse().unwrap())).unwrap();
                        if bytes.len()>=end+4+length {break bytes[end+4..end+4+length].to_vec();}
                    }
                };
                requests.push(serde_json::from_slice::<Value>(&body).unwrap());
                let response=if round==0 {json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"generation-1","type":"function","function":{"name":"generate","arguments":"{\"count\":8}"}}]}}]})}
                    else if round==1 {json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"details-1","type":"function","function":{"name":"details","arguments":"{\"index\":0}"}}]}}]})}
                    else if provider_ok {json!({"choices":[{"message":{"role":"assistant","content":"Research done"}}]})}
                    else {json!({"error":{"message":"fixture provider stopped"}})};
                let body=response.to_string();write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
            requests
        });
        let output=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).current_dir(&root)
            .args(["assistant","--data","bars.csv","--symbol","EURUSD","--timeframe","M1","--base-url",&endpoint,"--api-key","fixture-secret-key","--model","fixture","--ask","private-fixture-question"])
            .output().unwrap();
        let requests=server.join().unwrap();assert_eq!(requests.len(),if storage_ok {3}else{1});
        if storage_ok {assert!(requests[1]["messages"].as_array().unwrap().iter().any(|m|m["role"]=="tool"));}
        for request in &requests {
            assert!(!request.to_string().contains("cash_settlements"),"full execution evidence must stay out of provider requests");
        }
        let stdout=String::from_utf8(output.stdout).unwrap();let stderr=String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(),provider_ok&&storage_ok,"{stdout}\n{stderr}");
        let receipt=stdout.lines().find_map(|line|line.strip_prefix("Assistant archive: "));
        if storage_ok {
            let receipt:Value=serde_json::from_str(receipt.expect("archive receipt")).unwrap();
            let archive=sqx_project::archive::Archive::new(root.join("data/research"));
            let record=archive.load(receipt["run_id"].as_str().unwrap()).unwrap();
            assert_eq!(record.run.result["session_completed"],provider_ok);
            assert_eq!(record.run.result["candidate_evaluations"].as_array().unwrap().len(),8);
            assert_eq!(record.run.result["detail_evaluations"].as_array().unwrap().len(),1);
            assert_eq!(record.run.result["detail_evaluations"][0]["outcome"]["index"],0);
            let trials=record.run.result["trial_executions"].as_array().unwrap();
            assert_eq!(trials.len(),9);
            assert_eq!(trials[8]["tool"],"details");
            assert_eq!(archive.list().unwrap().len(),19,"nine checkpoints, nine results and the session summary");
            for trial in trials {
                let saved=archive.load(trial["run_id"].as_str().unwrap()).unwrap();
                assert_eq!(saved.run.result,trial["result"]);
                assert_eq!(saved.run.request["started_run_id"],trial["started_run_id"]);
                let checkpoint=archive.load(trial["started_run_id"].as_str().unwrap()).unwrap();
                assert_eq!(checkpoint.run.request["research_kind"],"assistant_trial_started");
                assert!(trial["result"]["trades"].is_array());
                assert!(trial["result"]["equity"].is_array());
                assert_eq!(trial["result"]["execution"]["account_evidence_schema"],1);
            }
            assert_eq!(record.dataset["bars"].as_array().unwrap().len(),2);
            assert_eq!(record.dataset["timeframe"],"M1");
            assert_eq!(record.run.request["research_context"]["config"]["bar_duration_ms"],60000);
            for run_id in archive.list().unwrap() {
                let encoded=serde_json::to_string(&archive.load(&run_id).unwrap()).unwrap();
                for excluded in ["fixture-secret-key","private-fixture-question","Research done","fixture provider stopped"] {assert!(!encoded.contains(excluded));}
            }
            if !provider_ok {assert!(stderr.contains("fixture provider stopped"));}
        }else{assert!(receipt.is_none());assert!(stderr.contains("could not be archived"));}
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn assistant_trials_survive_process_kill_before_session_return() {
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
    let root=std::env::temp_dir().join(format!("sqx-assistant-kill-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("bars.csv"),"timestamp,open,high,low,close,volume\n1700000000,1,1,1,1,1\n1700000060,1,1,1,1,1\n").unwrap();
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
    let endpoint=format!("http://{}/v1",listener.local_addr().unwrap());
    let mut child=ChildGuard(Command::new(env!("CARGO_BIN_EXE_sqx-cli")).current_dir(&root)
        .args(["assistant","--data","bars.csv","--symbol","EURUSD","--timeframe","M1","--base-url",&endpoint,"--model","fixture","--ask","generate"])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap());
    for round in 0..2 {
        let deadline=Instant::now()+Duration::from_secs(30);
        let mut stream=loop {match listener.accept(){Ok((s,_))=>break s,Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>{assert!(Instant::now()<deadline,"provider request missing");assert!(child.0.try_wait().unwrap().is_none());std::thread::sleep(Duration::from_millis(10));},Err(e)=>panic!("{e}")}};
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut bytes=Vec::new();let mut buf=[0;4096];
        loop {
            let n=stream.read(&mut buf).unwrap();assert!(n>0);bytes.extend_from_slice(&buf[..n]);
            if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {
                let headers=String::from_utf8_lossy(&bytes[..end]);
                let length:usize=headers.lines().find_map(|line|line.split_once(':').filter(|(k,_)|k.eq_ignore_ascii_case("content-length")).map(|(_,v)|v.trim().parse().unwrap())).unwrap();
                if bytes.len()>=end+4+length {break;}
            }
        }
        if round==0 {
            let body=json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"g","type":"function","function":{"name":"generate","arguments":"{\"count\":2}"}}]}}]}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        } else {
            // The second provider request is pending: run() has not returned.
            let archive=sqx_project::archive::Archive::new(root.join("data/research"));
            assert_eq!(archive.list().unwrap().len(),4,"both completed trials must already be durable");
            assert!(child.0.try_wait().unwrap().is_none());
            child.0.kill().unwrap();assert!(!child.0.wait().unwrap().success());
        }
    }
    fs::remove_file(root.join("bars.csv")).unwrap();
    let archive=sqx_project::archive::Archive::new(root.join("data/research"));
    let restored=sqx_project::archive::Archive::new(root.join("restored"));
    let mut completed=0;
    for id in archive.list().unwrap() {
        let bundle=archive.load(&id).unwrap();
        restored.restore(&bundle).unwrap();
        if bundle.run.request["research_kind"]!="assistant_trial" {continue;}
        completed+=1;
        let strategy=serde_json::from_value(bundle.strategy.clone()).unwrap();
        let bars:Vec<sqx_core::Bar>=serde_json::from_value(bundle.dataset["bars"].clone()).unwrap();
        let instrument=serde_json::from_value(bundle.dataset["instrument"].clone()).unwrap();
        let config=serde_json::from_value(bundle.run.request["config"].clone()).unwrap();
        let result=sqx_backtest::backtest(&strategy,&bars,&instrument,&config);
        assert_eq!(json!(result),bundle.run.result);
        let checkpoint=archive.load(bundle.run.request["started_run_id"].as_str().unwrap()).unwrap();
        assert_eq!(checkpoint.run.dataset_id,bundle.run.dataset_id);
        assert_eq!(checkpoint.run.strategy_id,bundle.run.strategy_id);
        assert_eq!(checkpoint.run.request["session_id"],bundle.run.request["session_id"]);
    }
    assert_eq!(completed,2);
    assert_eq!(restored.list().unwrap().len(),4);
    let recovery=sqx_project::archive::Archive::new(root.join("recovery"));
    let mut pending=Vec::new();
    for id in archive.list().unwrap() {
        let record=archive.load(&id).unwrap();
        if record.run.request["research_kind"]=="assistant_trial_started" {
            recovery.restore(&record).unwrap();pending.push(id);
        }
    }
    let call=|args:&[&str]|Command::new(env!("CARGO_BIN_EXE_sqx-cli")).current_dir(&root).args(args).output().unwrap();
    let listed=call(&["list-assistant-trials","--archive-dir","recovery"]);
    assert!(listed.status.success());
    let listed:Value=serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(listed["checkpoints"].as_array().unwrap().len(),2);
    assert!(listed["checkpoints"].as_array().unwrap().iter().all(|r|r["state"]=="result_not_recorded"));
    for id in pending {
        let resumed=call(&["replay-assistant-trial","--archive-dir","recovery","--run-id",&id,"--resume"]);
        assert!(resumed.status.success(),"{}",String::from_utf8_lossy(&resumed.stderr));
        let stdout=String::from_utf8(resumed.stdout).unwrap();
        let receipt:Value=serde_json::from_str(stdout.trim().strip_prefix("Assistant trial: ").unwrap()).unwrap();
        let result_id=receipt["run_id"].as_str().unwrap();
        assert_eq!(recovery.load(result_id).unwrap().run.result,archive.load(result_id).unwrap().run.result);
        let replay=call(&["replay-assistant-trial","--archive-dir","recovery","--run-id",result_id]);
        assert!(replay.status.success(),"{}",String::from_utf8_lossy(&replay.stderr));
        assert!(String::from_utf8_lossy(&replay.stdout).contains("\"matches\":true"));
    }
    assert_eq!(recovery.list().unwrap().len(),4);
    fs::remove_dir_all(root).unwrap();
}
