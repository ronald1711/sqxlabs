//! # sqx-web
//!
//! A dependency-free embedded HTTP server that serves a single-page dashboard
//! over the trading engine, with multi-symbol data from the Data Manager.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, RwLock};

use rand::SeedableRng;
use serde_json::json;

use sqx_backtest::{backtest, BacktestConfig, BacktestResult};
use sqx_core::{Bar, Instrument, Timeframe};
use sqx_data::{DataStore, QuestdbClient, SymbolSeries};
use sqx_metrics::{compute as compute_metrics, Metrics};
use sqx_optimizer::{random_strategy, GeneratorConfig};
use sqx_robustness::{monte_carlo_trade_manipulation, what_if};
use sqx_strategy::{render as render_pseudocode, Strategy};

const INDEX: &str = include_str!("../assets/index.html");

struct State {
    assistant_gate: std::sync::Mutex<()>,
    archive: sqx_project::archive::Archive,
    store: RwLock<DataStore>,
    databank: RwLock<Vec<serde_json::Value>>,
    questdb: Option<QuestdbClient>,
}

pub fn serve(addr: &str, store: DataStore, questdb: Option<QuestdbClient>) -> std::io::Result<()> {
    let addresses:Vec<_>=addr.to_socket_addrs()?.collect();
    if addresses.is_empty() || addresses.iter().any(|a|!a.ip().is_loopback()) {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied,"The embedded research server only supports loopback addresses; remote authenticated serving is not implemented."));
    }
    engine_identity().map_err(std::io::Error::other)?;
    println!("SqxLabs web app: http://{addr} ({} symbols)", store.len());
    let databank = std::fs::read_to_string("data/databank.json")
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<serde_json::Value>>(&t).ok())
        .unwrap_or_default();
    let state = Arc::new(State { assistant_gate: std::sync::Mutex::new(()), archive: sqx_project::archive::Archive::new("data/research"), store: RwLock::new(store), databank: RwLock::new(databank), questdb });
    let listener = TcpListener::bind(addresses.as_slice())?;
    let connections=Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for stream in listener.incoming() {
        match stream {
            Ok(mut s) => {
                use std::sync::atomic::Ordering;
                if connections.fetch_update(Ordering::SeqCst,Ordering::SeqCst,|n|(n<16).then_some(n+1)).is_err(){
                    s.set_write_timeout(Some(std::time::Duration::from_secs(1)))?;
                    let _=http_response(&mut s,"503 Service Unavailable","text/plain","Concurrent request limit reached");
                    continue;
                }
                let permit=ConnectionPermit(Arc::clone(&connections));
                let st = Arc::clone(&state);
                std::thread::spawn(move || {
                    let _permit=permit;
                    let _ = handle(s, &st);
                });
            }
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
    Ok(())
}

struct ConnectionPermit(Arc<std::sync::atomic::AtomicUsize>);
impl Drop for ConnectionPermit {
    fn drop(&mut self){self.0.fetch_sub(1,std::sync::atomic::Ordering::SeqCst);}
}

const MAX_HTTP_BODY:usize=100*1024*1024;
const MAX_HTTP_HEADERS:usize=16*1024;

fn http_response(stream:&mut TcpStream,status:&str,ctype:&str,payload:&str)->std::io::Result<()> {
    write!(stream,"HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\nCache-Control: no-store\r\n\r\n{payload}",payload.len())?;
    stream.flush()
}

fn handle(mut stream: TcpStream, state: &State) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(30)))?;
    let mut buf=Vec::new();let mut tmp=[0u8;4096];
    let hdr_end=loop {
        let n=stream.read(&mut tmp)?;if n==0{return Ok(());}buf.extend_from_slice(&tmp[..n]);
        if let Some(end)=buf.windows(4).position(|w|w==b"\r\n\r\n") {
            if end>MAX_HTTP_HEADERS{return http_response(&mut stream,"431 Request Header Fields Too Large","text/plain","Header limit exceeded");}
            break end;
        }
        if buf.len()>MAX_HTTP_HEADERS{return http_response(&mut stream,"431 Request Header Fields Too Large","text/plain","Header limit exceeded");}
    };
    let head=match std::str::from_utf8(&buf[..hdr_end]){Ok(s)=>s.to_owned(),Err(_)=>return http_response(&mut stream,"400 Bad Request","text/plain","Invalid headers")};
    let mut lines=head.split("\r\n");let parts:Vec<_>=lines.next().unwrap_or("").split_whitespace().collect();
    if parts.len()!=3 || parts[2]!="HTTP/1.1" || !parts[1].starts_with('/') || parts[1].starts_with("//") {
        return http_response(&mut stream,"400 Bad Request","text/plain","Invalid request line");
    }
    let (method,path)=(parts[0],parts[1]);
    let mut headers=std::collections::BTreeMap::new();
    for line in lines {
        let Some((key,value))=line.split_once(':')else{return http_response(&mut stream,"400 Bad Request","text/plain","Malformed header");};
        let key=key.to_ascii_lowercase();
        if key.trim()!=key || headers.insert(key,value.trim().to_string()).is_some(){return http_response(&mut stream,"400 Bad Request","text/plain","Duplicate or malformed headers");}
    }
    let port=stream.local_addr()?.port();
    let host=headers.get("host").map(|s|s.to_ascii_lowercase()).unwrap_or_default();
    let allowed=[format!("localhost:{port}"),format!("127.0.0.1:{port}"),format!("[::1]:{port}")];
    if !allowed.contains(&host) && !(port==80 && ["localhost","127.0.0.1","[::1]"].contains(&host.as_str())) {
        return http_response(&mut stream,"403 Forbidden","text/plain","Unrecognized local Host");
    }
    if headers.get("origin").is_some_and(|origin|origin!=&format!("http://{host}"))
        || headers.get("sec-fetch-site").is_some_and(|site|site=="cross-site") {
        return http_response(&mut stream,"403 Forbidden","text/plain","Cross-origin requests are not allowed");
    }
    if headers.contains_key("transfer-encoding") {return http_response(&mut stream,"400 Bad Request","text/plain","Transfer encoding is not supported");}
    if method!="GET" && method!="POST" {return http_response(&mut stream,"405 Method Not Allowed","text/plain","Unsupported method");}
    if method=="POST" && !headers.get("content-type").is_some_and(|s|s.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("application/json")) {
        return http_response(&mut stream,"415 Unsupported Media Type","text/plain","POST requires application/json");
    }
    let length=match headers.get("content-length"){
        Some(s)=>match s.parse::<usize>(){Ok(n)=>n,Err(_)=>return http_response(&mut stream,"400 Bad Request","text/plain","Invalid content length")},
        None if method=="POST"=>return http_response(&mut stream,"411 Length Required","text/plain","Content length required"),
        None=>0,
    };
    if length>MAX_HTTP_BODY {return http_response(&mut stream,"413 Payload Too Large","text/plain","Request exceeds 100 MB");}
    let mut body=buf[hdr_end+4..].to_vec();
    if body.len()>length{return http_response(&mut stream,"400 Bad Request","text/plain","Unexpected trailing request bytes");}
    while body.len()<length {
        let remaining=(length-body.len()).min(tmp.len());let n=stream.read(&mut tmp[..remaining])?;
        if n==0{return http_response(&mut stream,"400 Bad Request","text/plain","Incomplete request body");}body.extend_from_slice(&tmp[..n]);
    }
    let body=match std::str::from_utf8(&body){Ok(s)=>s,Err(_)=>return http_response(&mut stream,"400 Bad Request","text/plain","Invalid UTF-8 body")};
    let (status,ctype,payload)=route(method,path,body,state);
    http_response(&mut stream,status,ctype,&payload)
}

fn query_symbol(path: &str) -> (&str, String) {
    match path.split_once('?') {
        Some((p, q)) => {
            let sym = q
                .split('&')
                .find_map(|kv| kv.strip_prefix("symbol="))
                .map(|s| s.to_string())
                .unwrap_or_default();
            (p, sym)
        }
        None => (path, String::new()),
    }
}

fn query_param(path: &str, key: &str) -> String {
    match path.split_once('?') {
        Some((_, q)) => q
            .split('&')
            .find_map(|kv| kv.strip_prefix(&format!("{key}=")))
            .map(|s| s.to_string())
            .unwrap_or_default(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn research_state() -> State {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut store = DataStore::new();
        store.add("EURUSD", Timeframe::M1, sqx_data::synthetic_bars("EURUSD", 200));
        State { assistant_gate: std::sync::Mutex::new(()), archive: sqx_project::archive::Archive::new(std::env::temp_dir().join(format!("sqx-web-research-{}-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)))), store: RwLock::new(store), databank: RwLock::new(Vec::new()), questdb: None }
    }

    #[test]
    fn mcp_generation_records_do_not_offer_dashboard_replay(){
        let state=research_state();
        for kind in ["mcp_generation","mcp_generation_started","mcp_generation_trial","mcp_generation_trial_started"] {
            let id=state.archive.save(engine_identity().unwrap(),json!({}),json!({}),json!({"research_kind":kind}),json!({})).unwrap();
            let response:serde_json::Value=serde_json::from_str(&research_replay_json(&json!({"run_id":id}).to_string(),&state)).unwrap();
            assert!(response["error"].as_str().unwrap().contains("dashboard replay and restart are not implemented"),"{response}");
        }
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn dashboard_pipeline_restores_and_recovers_without_live_data(){
        let state=research_state();
        let bars=vec![Bar::new(0,1.1,1.1,1.1,1.1,1.0),Bar::new(60000,1.1,1.1,1.1,1.1,1.0)];
        let pipeline=sqx_pipeline::Pipeline{name:"dashboard recovery".into(),tasks:vec![sqx_pipeline::Task{kind:sqx_pipeline::TaskKind::Build{count:2,keep:1}}]};
        let id=state.archive.save(engine_identity().unwrap(),json!({"bars":bars,"instrument":Instrument::fx("EURUSD"),"timeframe":"M1"}),
            json!({"pipeline":pipeline,"generator":sqx_optimizer::GeneratorConfig::default()}),
            json!({"research_kind":"cli_pipeline_started","config":BacktestConfig::default(),"seed":42,"periods_per_year":525600.0}),json!({"status":"execution_not_recorded"})).unwrap();
        let request=json!({"run_id":id}).to_string();
        let status:serde_json::Value=serde_json::from_str(&pipeline_action_json("status",&request,&state)).unwrap();
        assert_eq!(status["status"],"incomplete");assert_eq!(status["engine_matches"],true);
        let denied:serde_json::Value=serde_json::from_str(&pipeline_action_json("replay",&request,&state)).unwrap();assert!(denied["error"].as_str().unwrap().contains("incomplete"));
        let backup=pipeline_action_json("backup",&request,&state);
        std::fs::remove_dir_all(state.archive.root()).unwrap();
        let target=research_state();target.store.write().unwrap().series.clear();
        let restored:serde_json::Value=serde_json::from_str(&pipeline_action_json("restore",&backup,&target)).unwrap();assert_eq!(restored["started_run_id"],id);
        let resumed:serde_json::Value=serde_json::from_str(&pipeline_action_json("replay",&json!({"run_id":id,"resume":true}).to_string(),&target)).unwrap();
        assert_eq!(resumed["matches"],true,"{resumed}");assert_eq!(resumed["execution_valid"],true);assert!(resumed["appended_events"].as_u64().unwrap()>0);
        let again:serde_json::Value=serde_json::from_str(&pipeline_action_json("replay",&request,&target)).unwrap();assert_eq!(again["appended_events"],0);assert_eq!(again["completion_id"],resumed["completion_id"]);
        let root=target.archive.load(&id).unwrap();
        let wrong=target.archive.save("f".repeat(64),root.dataset,root.strategy,root.run.request,root.run.result).unwrap();
        let wrong_request=json!({"run_id":wrong,"resume":true}).to_string();
        let mismatch:serde_json::Value=serde_json::from_str(&pipeline_action_json("status",&wrong_request,&target)).unwrap();assert_eq!(mismatch["engine_matches"],false);
        let before=target.archive.list().unwrap();
        let rejected:serde_json::Value=serde_json::from_str(&pipeline_action_json("replay",&wrong_request,&target)).unwrap();assert!(rejected["error"].as_str().unwrap().contains("engine executable differs"));assert_eq!(target.archive.list().unwrap(),before);
        std::fs::remove_dir_all(target.archive.root()).unwrap();
    }

    #[test]
    fn cli_pipeline_records_are_labeled_and_cannot_replay_as_single_backtests(){
        let state=research_state();
        for kind in ["cli_pipeline_started","cli_pipeline_event","cli_pipeline"] {
            let id=state.archive.save(engine_identity().unwrap(),json!({"instrument":{"name":"EURUSD"}}),json!({"pipeline":{}}),json!({"research_kind":kind}),json!({})).unwrap();
            let response:serde_json::Value=serde_json::from_str(&research_replay_json(&json!({"run_id":id}).to_string(),&state)).unwrap();
            assert!(response["error"].as_str().unwrap().contains("complete ordered archive"),"{response}");
        }
        let listed:serde_json::Value=serde_json::from_str(&research_list_json(&state)).unwrap();
        assert_eq!(listed["runs"].as_array().unwrap().len(),3);
        for run in listed["runs"].as_array().unwrap(){assert_eq!(run["name"],"CLI pipeline");assert_eq!(run["symbol"],"EURUSD");}
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn assistant_candidate_archive_survives_restore_without_live_data(){
        let state=research_state();
        let series=state.store.read().unwrap().series[0].clone();
        let cfg=sqx_assistant::LlmConfig{base_url:"http://127.0.0.1:1".into(),model:"fixture".into(),api_key:"never-archive-this-key".into(),max_rounds:1};
        let mut assistant=sqx_assistant::Assistant::new(cfg,&series.symbol,series.bars.clone(),instrument_for(&series.symbol),pyp_for(&series.bars))
            .with_trial_archive(state.archive.clone(),engine_identity().unwrap(),json!({"fixture":true}));
        assistant.dispatch("generate",&json!({"count":8}));
        assert_eq!(assistant.candidate_evaluations().len(),8);
        assistant.dispatch("details",&json!({"index":0}));
        let listed:serde_json::Value=serde_json::from_str(&research_list_json(&state)).unwrap();
        assert_eq!(listed["runs"].as_array().unwrap().len(),18);
        for run in listed["runs"].as_array().unwrap() {
            assert!(matches!(run["kind"].as_str(),Some("assistant_trial"|"assistant_trial_started")));
            let resume=run["kind"]=="assistant_trial_started";
            let replay:serde_json::Value=serde_json::from_str(&research_replay_json(&json!({"run_id":run["run_id"],"resume":resume}).to_string(),&state)).unwrap();
            assert_eq!(replay[if resume {"resumed"}else{"matches"}],true,"{replay}");
        }
        for completed in [true,false] {
            let archive=archive_assistant_candidates(&state,&series,assistant.research_context(),json!({"fixture":true}),assistant.candidate_evaluations(),assistant.detail_evaluations(),assistant.trial_executions(),completed).unwrap();
            let id=archive["run_id"].as_str().unwrap();
            let bundle=state.archive.load(id).unwrap();
            assert_eq!(bundle.run.result["candidate_evaluations"],json!(assistant.candidate_evaluations()));
            assert_eq!(bundle.run.result["detail_evaluations"],json!(assistant.detail_evaluations()));
            assert_eq!(bundle.run.result["detail_evaluations"].as_array().unwrap().len(),1);
            assert_eq!(bundle.run.result["trial_executions"],json!(assistant.trial_executions()));
            assert_eq!(bundle.run.result["trial_executions"].as_array().unwrap().len(),9);
            assert_eq!(bundle.run.result["session_completed"],completed);
            let bytes=serde_json::to_vec(&bundle).unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("never-archive-this-key"));
            let restored=research_state();restored.archive.restore(&serde_json::from_slice(&bytes).unwrap()).unwrap();
            restored.store.write().unwrap().series.clear();
            let reloaded=sqx_project::archive::Archive::new(restored.archive.root()).load(id).unwrap();
            assert_eq!(json!(reloaded),json!(bundle));
            let replay:serde_json::Value=serde_json::from_str(&research_replay_json(&json!({"run_id":id}).to_string(),&restored)).unwrap();
            assert!(replay["error"].as_str().unwrap().contains("replay is not implemented"));
            std::fs::remove_dir_all(restored.archive.root()).unwrap();
        }
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn gap_allowance_does_not_authorize_overlapping_candles(){
        let state=research_state();state.store.write().unwrap().series[0].timeframe=Timeframe::M5;
        let ga=sqx_optimizer::GeneticConfig{population_size:6,elite_count:2,generations:1,min_trades:1,..Default::default()};
        for allow in [false,true]{
            let request=json!({"symbol":"EURUSD","request_key":"overlap","seed":42,"ga":ga,"allow_data_gaps":allow});
            let response:serde_json::Value=serde_json::from_str(&evolution_json("start",&request.to_string(),&state)).unwrap();
            assert!(response["error"].as_str().unwrap().contains("overlapping candle"),"{response}");
            assert!(sqx_project::evolution::EvolutionJobs::new(state.archive.root()).list().unwrap().is_empty());
        }
    }

    #[test]
    fn evolution_api_freezes_inputs_and_rejects_duplicate_advances() {
        let state=research_state();
        let call=|action:&str,value:serde_json::Value|->serde_json::Value {
            serde_json::from_str(&evolution_json(action,&value.to_string(),&state)).unwrap()
        };
        let ga=sqx_optimizer::GeneticConfig{population_size:6,elite_count:2,generations:1,min_trades:1,..Default::default()};
        let request=json!({"request_key":"evolution-api-retry","seed":42,"ga":ga,"symbol":"EURUSD","allow_data_gaps":true});
        let created=call("start",request.clone());
        assert!(created.get("error").is_none(),"{created}");
        let id=created["status"]["job_id"].as_str().unwrap();
        assert_eq!(created["status"]["committed_generations"],0);
        assert_eq!(call("start",request.clone())["status"],created["status"]);
        let mut different=request.clone();different["seed"]=json!(43);
        assert!(call("start",different)["error"].is_string());
        let mut invalid=request.clone();invalid["seed"]=json!("42");
        assert!(call("start",invalid)["error"].is_string());
        assert_eq!(call("list",json!({}))["jobs"].as_array().unwrap().len(),1);
        state.store.write().unwrap().series.clear();
        let advance=json!({"job_id":id,"expected_sequence":0});
        let result=call("advance",advance.clone());
        assert!(result.get("error").is_none(),"{result}");
        assert_eq!(result["status"]["committed_generations"],1);
        assert_eq!(result["evaluations"].as_array().unwrap().len(),6);
        assert!(call("advance",advance)["error"].is_string());
        let snapshot=call("status",json!({"job_id":id}));
        assert_eq!(snapshot["status"],result["status"]);
        let replay=call("replay",json!({"job_id":id}));assert_eq!(replay["evaluations"],6);assert_eq!(replay["checkpoint_id"],snapshot["status"]["checkpoint_id"]);
        assert_eq!(snapshot["latest"]["evaluations"],result["evaluations"]);
        assert_eq!(snapshot["metadata"]["population_size"],6);
        let backup=call("backup",json!({"job_id":id}));
        let backup:sqx_project::evolution::EvolutionBackup=serde_json::from_value(backup).unwrap();
        backup.verify().unwrap();assert_eq!(backup.generations.len(),1);assert_eq!(backup.claims.len(),1);
        let restored=research_state();restored.store.write().unwrap().series.clear();
        let restored_result:serde_json::Value=serde_json::from_str(&evolution_json("restore",&serde_json::to_string(&backup).unwrap(),&restored)).unwrap();
        assert_eq!(restored_result["status"],snapshot["status"]);
        assert_eq!(restored_result["metadata"],snapshot["metadata"]);
        std::fs::remove_dir_all(restored.archive.root()).unwrap();


        assert_eq!(snapshot["metadata"]["context"]["seed"],42);
        assert_eq!(snapshot["metadata"]["context"]["data_quality"],created["data_quality"]);
        assert_eq!(snapshot["metadata"]["context"]["allow_data_gaps"],true);
        assert!(state.databank.read().unwrap().is_empty());
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    fn attach_normalization(state:&State)->sqx_data::normalization::Bundle{
            let mut store=state.store.write().unwrap();let series=&mut store.series[0];
            let mut utc=String::from("timestamp,open,high,low,close,volume\n");
            for b in &series.bars {utc.push_str(&format!("{},{},{},{},{},{}\n",b.time/1000,b.open,b.high,b.low,b.close,b.volume));}
            let raw=utc.replacen("timestamp,","timestamp_broker,",1);
            let policy=json!({"schema":1,"server":"synthetic","intervals":[{"raw_start_ms":series.bars[0].time,"raw_end_ms":series.bars.last().unwrap().time+60000,"offset_seconds":0,"source":"synthetic UTC fixture"}]}).to_string();
            let hash=|s:&str|sqx_project::archive::content_id(s.as_bytes());
            let manifest=json!({"schema":1,"server":"synthetic","raw_sha256":hash(&raw),"policy_sha256":hash(&policy),"normalized_sha256":hash(&utc),"rows":series.bars.len(),"bar_duration_ms":60000,"clock":"explicit UTC fixture","scope":"synthetic transformation"}).to_string();
            let bundle=sqx_data::normalization::Bundle{manifest_json:manifest,policy_json:policy,raw_csv:raw,utc_csv:utc};
            series.normalization=Some(bundle.clone());bundle
    }

    #[test]
    fn evolution_creation_preserves_normalization_after_live_data_is_removed(){
        let state=research_state();let bundle=attach_normalization(&state);
        let ga=sqx_optimizer::GeneticConfig{population_size:2,generations:1,elite_count:1,..Default::default()};
        let result:serde_json::Value=serde_json::from_str(&evolution_json("start",&json!({"symbol":"EURUSD","request_key":"normalization","seed":42,"ga":ga}).to_string(),&state)).unwrap();
        let id=result["status"]["job_id"].as_str().expect("evolution job created");
        state.store.write().unwrap().series.clear();
        let jobs=sqx_project::evolution::EvolutionJobs::new(state.archive.root());
        let backup=jobs.backup(id).unwrap();backup.verify().unwrap();
        let job:sqx_project::evolution::EvolutionJob=serde_json::from_str(&backup.job_json).unwrap();
        assert_eq!(serde_json::to_value(job.normalization.unwrap()).unwrap(),json!(bundle));
        let destination=research_state();let restored=sqx_project::evolution::EvolutionJobs::new(destination.archive.root());
        restored.restore(&backup).unwrap();assert_eq!(restored.backup(id).unwrap().job_json,backup.job_json);
        std::fs::remove_dir_all(state.archive.root()).unwrap();std::fs::remove_dir_all(destination.archive.root()).unwrap();
    }

    #[test]
    fn frozen_experiment_uses_saved_data_costs_and_records_all_search_trials() {
        let state=research_state();
        let bundle=attach_normalization(&state);
        let split=state.store.read().unwrap().series[0].bars[140].time;
        let request=json!({"symbol":"EURUSD","holdout_start_time":split,"min_development_trades":1,"min_holdout_trades":1,"prior_exposure_disclosed":true,"deposit":4321.0});
        let created:serde_json::Value=serde_json::from_str(&experiment_json("create",&request.to_string(),&state)).unwrap();
        let id=created["experiment_id"].as_str().expect("experiment must be created");
        assert_eq!(created["plan"]["config"]["initial_deposit"],4321.0);
        state.store.write().unwrap().series.clear();
        let result:serde_json::Value=serde_json::from_str(&experiment_json("search",&json!({"experiment_id":id,"count":3,"seed":42,"deposit":99999}).to_string(),&state)).unwrap();
        assert_eq!(result["status"]["trials_attempted"],3);
        for trial in result["trials"].as_array().unwrap() {
            assert_eq!(trial["report"]["execution"]["initial_deposit"],4321.0);
            assert!(trial["report"]["execution"]["equity"].as_array().unwrap().iter().all(|p|p[0].as_i64().unwrap()<split));
        }
        let attempt=&result["status"]["events"].as_array().unwrap().iter().find(|e|e["event"]["kind"]=="trial_started").unwrap()["event"];
        assert_eq!(attempt["context"]["seed"],42);
        assert!(attempt["context"]["generator"].is_object());
        let again:serde_json::Value=serde_json::from_str(&experiment_json("search",&json!({"experiment_id":id,"count":3,"seed":42}).to_string(),&state)).unwrap();
        assert_eq!(again["status"]["trials_attempted"],6,"repeated searches count as additional attempts");
        assert_eq!(again["trials"][0]["report"],result["trials"][0]["report"]);
        let backup=experiment_json("backup",&json!({"experiment_id":id}).to_string(),&state);
        let saved:serde_json::Value=serde_json::from_str(&backup).unwrap();
        assert_eq!(saved["data"]["provenance"]["normalization"],json!(bundle));
        let restored=research_state();
        let response:serde_json::Value=serde_json::from_str(&experiment_json("restore",&backup,&restored)).unwrap();
        assert_eq!(response["trials_attempted"],6);
        assert_eq!(response,again["status"]);
        std::fs::remove_dir_all(restored.archive.root()).unwrap();
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn saved_search_api_resumes_restored_job_and_deduplicates_start() {
        let state=research_state();
        let split=state.store.read().unwrap().series[0].bars[140].time;
        let created:serde_json::Value=serde_json::from_str(&experiment_json("create",&json!({"symbol":"EURUSD","holdout_start_time":split,"min_development_trades":1,"min_holdout_trades":1,"prior_exposure_disclosed":true}).to_string(),&state)).unwrap();
        let id=created["experiment_id"].as_str().unwrap();
        let start=json!({"experiment_id":id,"count":2,"seed":42,"request_key":"browser-retry"}).to_string();
        let job:serde_json::Value=serde_json::from_str(&experiment_json("job_start",&start,&state)).unwrap();
        assert_eq!(job["completed"],0);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&experiment_json("job_start",&start,&state)).unwrap(),job);
        let step=json!({"experiment_id":id,"job_id":job["job_id"]}).to_string();
        let first:serde_json::Value=serde_json::from_str(&experiment_json("job_step",&step,&state)).unwrap();
        assert_eq!(first["completed"],1);
        let backup=experiment_json("backup",&json!({"experiment_id":id}).to_string(),&state);
        let restored=research_state();restored.store.write().unwrap().series.clear();
        let response:serde_json::Value=serde_json::from_str(&experiment_json("restore",&backup,&restored)).unwrap();
        assert_eq!(response["trials_attempted"],1);
        let last:serde_json::Value=serde_json::from_str(&experiment_json("job_step",&step,&restored)).unwrap();
        assert_eq!(last["completed"],2);assert_eq!(last["state"],"complete");
        let repeated:serde_json::Value=serde_json::from_str(&experiment_json("job_step",&step,&restored)).unwrap();
        assert_eq!(repeated["completed"],2);assert!(repeated.get("last_trial").is_none());
        std::fs::remove_dir_all(restored.archive.root()).unwrap();
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn simulation_parser_rejects_present_but_invalid_fields() {
        for (key,value) in [("deposit",json!("10000")),("spread",json!(null)),("slippage",json!("bad")),("commission",json!([])),("intrabar_policy",json!("optimistic-unknown")),("allow_tick_fallback",json!("false")),("bar_duration_ms",json!(1.5)),("account",json!({"currency":"INVALID"}))] {
            let mut input=json!({});input[key]=value;
            assert!(config_from(&input).is_err(),"{input}");
        }
        let defaults=config_from(&json!({})).unwrap();assert_eq!(defaults.initial_deposit,10000.0);assert_eq!(defaults.spread_points,0.0);
        let config=config_from(&json!({"deposit":4321.0,"spread":12.5,"slippage":3.0,"commission":7.0,"intrabar_policy":"adverse_first"})).unwrap();
        assert_eq!(config.initial_deposit,4321.0);assert_eq!(config.spread_points,12.5);
    }

    #[test]
    fn source_duration_binding_handles_calendar_and_fixed_timeframes(){
        let mut series=SymbolSeries{symbol:"EURUSD".into(),timeframe:Timeframe::M1,calendar:None,normalization:None,bars:vec![]};
        for (timeframe,width) in [(Timeframe::M1,60000),(Timeframe::M5,300000),(Timeframe::D1,86400000),(Timeframe::W1,604800000)] {
            series.timeframe=timeframe;
            for request in [json!({}),json!({"bar_duration_ms":null}),json!({"bar_duration_ms":width})] {
                assert_eq!(config_for_series(&request,&series).unwrap().bar_duration_ms,Some(width));
            }
            assert!(config_for_series(&json!({"bar_duration_ms":width/2}),&series).is_err());
        }
        series.timeframe=Timeframe::MN1;
        assert_eq!(config_for_series(&json!({}),&series).unwrap().bar_duration_ms,None);
        assert!(config_for_series(&json!({"bar_duration_ms":2592000000i64}),&series).is_err());
        series.timeframe=Timeframe::Tick;
        assert!(config_for_series(&json!({}),&series).is_err());
    }

    #[test]
    fn fresh_backtests_bind_source_duration_and_reject_conflicting_requests() {
        let state=research_state();let mut request=serde_json::to_value(Strategy::new("source duration")).unwrap();
        request["symbol"]=json!("EURUSD");
        let result:serde_json::Value=serde_json::from_str(&archived_backtest_json(&request.to_string(),&state)).unwrap();
        assert!(result.get("error").is_none(),"{result}");
        assert_eq!(result["run_context"]["execution"]["bar_duration_ms"],60000);
        request["bar_duration_ms"]=json!(30000);
        let invalid:serde_json::Value=serde_json::from_str(&archived_backtest_json(&request.to_string(),&state)).unwrap();
        assert!(invalid["error"].as_str().unwrap().contains("timeframe"),"{invalid}");
        assert!(invalid.get("archive").is_none());
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn invalid_cost_request_cannot_create_an_archived_backtest() {
        let state=research_state();let mut request=serde_json::to_value(Strategy::new("invalid costs")).unwrap();
        request["symbol"]=json!("EURUSD");request["spread"]=json!("not a number");
        let result:serde_json::Value=serde_json::from_str(&archived_backtest_json(&request.to_string(),&state)).unwrap();
        assert!(result["error"].as_str().unwrap().contains("spread"));assert!(result.get("archive").is_none());assert!(!state.archive.root().exists());
    }

    #[test]
    fn walk_forward_routes_honor_requested_candidate_counts() {
        let state=research_state();
        state.store.write().unwrap().series[0].bars=sqx_data::synthetic_bars("EURUSD",1000);
        for kind in ["walk-forward","wf-matrix"] {
            let request=json!({"kind":kind,"symbol":"EURUSD","count":3,"windows":2,"seed":123});
            let response:serde_json::Value=serde_json::from_str(&optimize_json(&request.to_string(),&state)).unwrap();
            assert!(response.get("error").is_none(),"{response}");
            if kind=="walk-forward" {assert_eq!(response["candidates_attempted"],6);assert_eq!(response["seed"],123);}
            else {let rows=response["rows"].as_array().unwrap();assert_eq!(rows.len(),12);
                for row in rows {assert_eq!(row["candidates_attempted"].as_u64().unwrap(),row["windows"].as_u64().unwrap()*3);}}
        }
        assert!(state.databank.read().unwrap().is_empty());
    }

    #[test]
    fn optimization_rejects_invalid_settings_before_any_research() {
        let state=research_state();
        for invalid in [json!({"kind":"typo"}),json!({"count":null}),json!({"count":1.5}),json!({"count":0}),json!({"kind":"walk-forward","count":201}),json!({"seed":-1}),json!({"seed":"42"}),json!({"windows":13}),json!({"oos":0.9}),json!({"min_is_trades":0}),json!({"min_oos_trades":null})] {
            let response:serde_json::Value=serde_json::from_str(&optimize_json(&invalid.to_string(),&state)).unwrap();
            assert!(response["error"].is_string(),"{invalid}: {response}");
            assert_ne!(response["error"],"no data");
        }
        assert!(state.databank.read().unwrap().is_empty());assert!(!state.archive.root().exists());
        let input=OptimizationInputs::parse(&json!({"kind":"wf-matrix","count":17,"seed":123})).unwrap();
        assert_eq!(input.count,17);assert_eq!(input.seed,123);
        assert_eq!(OptimizationInputs::parse(&json!({})).unwrap().count,200);
    }

    #[test]
    fn generator_parser_does_not_silently_drop_invalid_parameter_rules() {
        for value in [json!({"param_ranges":[{"indicator":"RSI","param":"period","min":5}]}),json!({"param_ranges":"RSI.period=5..10"}),json!({"min_conditions":-1}),json!({"sl_min":"wrong"}),json!({"long":"true"})] {assert!(gen_config_from(&value).is_err(),"{value}");}
        let config=gen_config_from(&json!({"param_ranges":[{"indicator":"RSI","param":"period","min":5,"max":10}]})).unwrap();
        assert_eq!(config.param_ranges,vec![("RSI".into(),"period".into(),5.0,10.0)]);
    }

    #[test]
    fn generation_rejects_reversed_ranges_before_saving_candidates() {
        let state=research_state();
        for extra in [json!({"sl_min":20,"sl_max":10}),json!({"min_conditions":4,"max_conditions":1}),json!({"min_conditions":0}),json!({"count":0})] {
            let mut request=json!({"symbol":"EURUSD","count":10,"seed":42});
            for (key,value) in extra.as_object().unwrap(){request[key]=value.clone();}
            let result:serde_json::Value=serde_json::from_str(&generate_json(&request.to_string(),&state)).unwrap();
            assert!(result["error"].is_string(),"{request}: {result}");assert!(state.databank.read().unwrap().is_empty());
        }
    }

    #[test]
    fn discovery_routes_reject_bad_data_before_mutating_databank() {
        let state=research_state();state.store.write().unwrap().series[0].bars.remove(50);
        let routes:[fn(&str,&State)->String;3]=[generate_json,optimize_json,pipeline_json];
        for route in routes {
            for kind in ["simple","walk-forward","wf-matrix"] {
                let request=json!({"symbol":"EURUSD","count":1,"keep":1,"seed":42,"kind":kind});
                let response:serde_json::Value=serde_json::from_str(&route(&request.to_string(),&state)).unwrap();
                assert!(response["error"].as_str().unwrap().contains("data gaps"));
                assert_eq!(response["data_quality"]["unclassified_intervals"],1);
                assert!(state.databank.read().unwrap().is_empty());
                let mut acknowledged=request;acknowledged["allow_data_gaps"]=json!(true);acknowledged["deposit"]=json!(-1.0);
                let invalid:serde_json::Value=serde_json::from_str(&route(&acknowledged.to_string(),&state)).unwrap();
                assert!(invalid["error"].is_string());assert!(!invalid["error"].as_str().unwrap().contains("data gaps"));
                assert!(state.databank.read().unwrap().is_empty());
            }
        }
    }

    #[test]
    fn robustness_data_gate_requires_disclosure_and_never_allows_incomplete_candles() {
        let state=research_state();state.store.write().unwrap().series[0].bars.remove(50);
        let mut request=serde_json::to_value(Strategy::new("quality fixture")).unwrap();request["symbol"]=json!("EURUSD");
        let rejected:serde_json::Value=serde_json::from_str(&robustness_json(&request.to_string(),&state)).unwrap();
        assert!(rejected["error"].as_str().unwrap().contains("data gaps"));assert!(rejected.get("what_if").is_none());
        request["allow_data_gaps"]=json!(true);
        let allowed:serde_json::Value=serde_json::from_str(&robustness_json(&request.to_string(),&state)).unwrap();
        assert!(allowed.get("error").is_none());assert_eq!(allowed["data_gaps_acknowledged"],true);
        assert_eq!(allowed["data_quality"]["unclassified_intervals"],1);
        assert!(allowed["interpretation"].as_str().unwrap().contains("Exploratory"));
        {
            let mut store=state.store.write().unwrap();let bars=&mut store.series[0].bars;
            let start=sqx_data::quality::now_ms()-(bars.len() as i64-1)*60000;
            for (index,bar) in bars.iter_mut().enumerate(){bar.time=start+index as i64*60000;}
        }
        let incomplete:serde_json::Value=serde_json::from_str(&robustness_json(&request.to_string(),&state)).unwrap();
        assert!(incomplete["error"].is_string());assert!(incomplete.get("what_if").is_none());
        assert!(!incomplete["data_quality"]["errors"].as_array().unwrap().is_empty());
    }

    #[test]
    fn robustness_rejects_invalid_base_and_reports_no_trade_resampling() {
        let state=research_state();
        let mut request=serde_json::to_value(Strategy::new("no trades")).unwrap();request["symbol"]=json!("EURUSD");request["spread"]=json!(-1.0);
        let invalid:serde_json::Value=serde_json::from_str(&robustness_json(&request.to_string(),&state)).unwrap();
        assert!(invalid["error"].is_string());assert!(invalid.get("mc").is_none());
        request["spread"]=json!(0.0);
        let empty:serde_json::Value=serde_json::from_str(&robustness_json(&request.to_string(),&state)).unwrap();
        assert_eq!(empty["mc"]["runs"],0);assert!(!empty["mc"]["errors"].as_array().unwrap().is_empty());assert!(empty["mc"]["net_p50"].is_null());
    }

    #[test]
    fn assistant_gate_rejects_overlap_and_releases_after_errors() {
        let state=research_state();
        let guard=state.assistant_gate.lock().unwrap();
        std::thread::scope(|scope| {
            let result=scope.spawn(|| assistant_json("{}",&state)).join().unwrap();
            let response:serde_json::Value=serde_json::from_str(&result).unwrap();
            assert!(response["error"].as_str().unwrap().contains("already running"));
        });
        drop(guard);
        assert!(assistant_json("{}",&state).contains("missing question"));
        assert!(state.assistant_gate.try_lock().is_ok(),"early errors must release the gate");
        assert!(assistant_json("{invalid",&state).contains("error"));
        assert!(state.assistant_gate.try_lock().is_ok());
    }

    #[test]
    fn assistant_refuses_unacknowledged_data_gaps_before_contacting_provider() {
        let state=research_state();state.store.write().unwrap().series[0].bars.remove(50);
        let response:serde_json::Value=serde_json::from_str(&assistant_json(&json!({"symbol":"EURUSD","question":"Generate a strategy"}).to_string(),&state)).unwrap();
        assert!(response["error"].as_str().unwrap().contains("data gaps"));
        assert_eq!(response["data_quality"]["unclassified_intervals"],1);
    }

    #[test]
    fn dataset_identity_never_falls_back_to_another_symbol_or_timeframe(){
        let state=research_state();
        state.store.write().unwrap().add("EURUSD",Timeframe::H1,vec![Bar::new(0,1.0,1.1,0.9,1.0,1.0)]);
        assert!(find_series(&state,"unknown").is_none());
        assert!(find_series(&state,"EURUSD").is_none());
        assert_eq!(find_series(&state,&series_key("EURUSD","H1")).unwrap().bars.len(),1);
        assert_eq!(find_series(&state,&series_key("EURUSD","M1")).unwrap().bars.len(),200);
    }

    #[test]
    fn saved_import_failures_preserve_loaded_data(){
        let state=research_state();let before=state.store.read().unwrap().series[0].bars.clone();
        std::fs::write(state.archive.root(),b"blocked storage").unwrap();
        let request=json!({"symbol":"EURUSD","timeframe":"M1","csv":"timestamp,open,high,low,close,volume\n1700000040,1,1.1,0.9,1,1\n"});
        let failed:serde_json::Value=serde_json::from_str(&import_json(&request.to_string(),&state)).unwrap();
        assert!(failed["error"].as_str().unwrap().contains("could not be saved"));
        assert_eq!(state.store.read().unwrap().series[0].bars,before);
        std::fs::remove_file(state.archive.root()).unwrap();
        let good:serde_json::Value=serde_json::from_str(&import_json(&request.to_string(),&state)).unwrap();
        let id=good["snapshot_id"].as_str().unwrap();let current=state.store.read().unwrap().series[0].bars.clone();
        std::fs::write(state.archive.root().join("imports").join(format!("{id}.json")),b"{}").unwrap();
        let failed:serde_json::Value=serde_json::from_str(&saved_datasets_json("load",&json!({"id":id}).to_string(),&state)).unwrap();
        assert!(failed["error"].as_str().unwrap().contains("integrity"));
        assert_eq!(state.store.read().unwrap().series[0].bars,current);
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn normalized_import_is_bound_to_candles_and_survives_archive_replay() {
        let state=research_state();
        let raw="timestamp_broker,open,high,low,close,volume\n1700000040,1,1.1,0.9,1,1\n1700000100,1,1.1,0.9,1,1\n";
        let utc=raw.replace("timestamp_broker","timestamp").replace("1700000040","1699992840").replace("1700000100","1699992900");
        let policy=json!({"schema":1,"server":"fixture","intervals":[{"raw_start_ms":1700000000000i64,"raw_end_ms":1700000600000i64,"offset_seconds":7200,"source":"synthetic fixture"}]}).to_string();
        let hash=|s:&str|sqx_project::archive::content_id(s.as_bytes());
        let manifest=json!({"schema":1,"server":"fixture","raw_sha256":hash(raw),"policy_sha256":hash(&policy),"normalized_sha256":hash(&utc),"rows":2,"bar_duration_ms":60000,"clock":"broker to UTC","scope":"synthetic fixture"}).to_string();
        let bundle=sqx_data::normalization::Bundle{manifest_json:manifest,policy_json:policy,raw_csv:raw.into(),utc_csv:utc.clone()};
        let request=json!({"symbol":"EURUSD","timeframe":"M1","csv":utc,"normalization_bundle":bundle});
        let import=|v:&serde_json::Value|serde_json::from_str::<serde_json::Value>(&import_json(&v.to_string(),&state)).unwrap();
        let imported=import(&request);assert_eq!(imported["ok"],true);
        let saved_id=imported["snapshot_id"].as_str().unwrap();
        let mut reopened=research_state();reopened.archive=state.archive.clone();reopened.store.write().unwrap().series.clear();
        let loaded:serde_json::Value=serde_json::from_str(&saved_datasets_json("load",&json!({"id":saved_id}).to_string(),&reopened)).unwrap();
        assert_eq!(loaded["ok"],true);assert_eq!(loaded["bars"],2);
        assert_eq!(serde_json::to_value(reopened.store.read().unwrap().series[0].normalization.as_ref().unwrap()).unwrap(),json!(bundle));

        let info:serde_json::Value=serde_json::from_str(&data_json(&state,"EURUSD")).unwrap();
        assert_eq!(info["normalization"]["status"],"verified_transformation");
        for altered in [
            {let mut v=request.clone();v["csv"]=json!(utc.replace("1.1","1.2"));v},
            {let mut v=request.clone();v["timeframe"]=json!("M5");v},
            {let mut v=request.clone();v["normalization_bundle"]["policy_json"]=json!("{}");v},
        ] {assert!(import(&altered)["error"].is_string());}
        let mut monthly=request.clone();monthly["timeframe"]=json!("MN1");
        let mut month_policy:serde_json::Value=serde_json::from_str(&bundle.policy_json).unwrap();
        month_policy["intervals"][0]["raw_end_ms"]=json!(1704000000000i64);
        let month_policy=month_policy.to_string();
        let mut month_manifest:serde_json::Value=serde_json::from_str(&bundle.manifest_json).unwrap();
        month_manifest["policy_sha256"]=json!(hash(&month_policy));month_manifest["bar_duration_ms"]=json!(30i64*86400000);
        monthly["normalization_bundle"]["policy_json"]=json!(month_policy);
        monthly["normalization_bundle"]["manifest_json"]=json!(month_manifest.to_string());
        assert!(import(&monthly)["error"].as_str().unwrap().contains("declared candle timeframe"));
        let after:serde_json::Value=serde_json::from_str(&data_json(&state,"EURUSD")).unwrap();
        assert_eq!(info,after,"failed imports must not replace the dataset");
        let strategy=json!({"symbol":"EURUSD","name":"source fixture","long":true,"short":false,"signals":[],"exits":[],"stop_loss":null,"profit_target":null,"money_management":{"FixedSize":{"size":0.1}}});
        let run:serde_json::Value=serde_json::from_str(&archived_backtest_json(&strategy.to_string(),&state)).unwrap();
        assert!(run["error"].is_null(),"{run}");
        assert_eq!(run["run_context"]["normalization"],info["normalization"]);
        let id=run["archive"]["run_id"].as_str().unwrap();
        let backup=state.archive.load(id).unwrap();
        assert_eq!(backup.dataset["normalization"],serde_json::to_value(&bundle).unwrap());
        state.store.write().unwrap().series.clear();
        let replay:serde_json::Value=serde_json::from_str(&research_replay_json(&json!({"run_id":id}).to_string(),&state)).unwrap();
        assert_eq!(replay["matches"],true,"{replay}");
        // A freshly rehashed archive with altered candles must still fail the source binding.
        let mut bad_dataset=backup.dataset.clone();bad_dataset["bars"][0]["high"]=json!(1.2);
        let bad_id=state.archive.save(backup.run.engine_id.clone(),bad_dataset,backup.strategy.clone(),backup.run.request.clone(),backup.run.result.clone()).unwrap();
        let bad:serde_json::Value=serde_json::from_str(&research_replay_json(&json!({"run_id":bad_id}).to_string(),&state)).unwrap();
        assert!(bad["error"].as_str().unwrap().contains("supplied candles"));
        let mut plain=request.clone();plain.as_object_mut().unwrap().remove("normalization_bundle");
        assert_eq!(import(&plain)["ok"],true);
        let info:serde_json::Value=serde_json::from_str(&data_json(&state,"EURUSD")).unwrap();
        assert_eq!(info["normalization"]["status"],"not_supplied");
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn incomplete_import_does_not_replace_existing_data(){
        let state=research_state();
        let time=sqx_data::quality::now_ms()/1000;
        let response:serde_json::Value=serde_json::from_str(&import_json(&json!({"symbol":"EURUSD","timeframe":"H1","csv":format!("{time},1,1.1,0.9,1,1")}).to_string(),&state)).unwrap();
        assert!(response["error"].is_string());
        assert_eq!(state.store.read().unwrap().series.len(),1);
        assert_eq!(state.store.read().unwrap().series[0].bars.len(),200);
    }

    #[test]
    fn data_gaps_require_explicit_acknowledgment_and_are_reported(){
        let state=research_state();state.store.write().unwrap().series[0].bars.remove(50);
        let mut rng=rand::rngs::StdRng::seed_from_u64(42);
        let mut request=serde_json::to_value(random_strategy(&mut rng,"gap check",&GeneratorConfig::default())).unwrap();
        request["symbol"]=json!("EURUSD");
        let response:serde_json::Value=serde_json::from_str(&backtest_json(&request.to_string(),&state)).unwrap();
        assert!(response["error"].is_string());assert_eq!(response["data_quality"]["unclassified_intervals"],1);
        request["allow_data_gaps"]=json!(true);
        let response:serde_json::Value=serde_json::from_str(&backtest_json(&request.to_string(),&state)).unwrap();
        assert!(response.get("error").is_none());assert_eq!(response["run_context"]["data_gaps_acknowledged"],true);
        assert_eq!(response["run_context"]["data_quality"]["unclassified_intervals"],1);
    }

    #[test]
    fn archived_run_replays_after_live_data_changes_and_survives_restore() {
        let state=research_state();
        let mut rng=rand::rngs::StdRng::seed_from_u64(42);
        let strategy=random_strategy(&mut rng,"Archived",&GeneratorConfig::default());
        let mut request=serde_json::to_value(strategy).unwrap();request["symbol"]=json!("EURUSD");
        let response:serde_json::Value=serde_json::from_str(&archived_backtest_json(&request.to_string(),&state)).unwrap();
        let id=response["archive"]["run_id"].as_str().expect("run must be archived");
        let backup=state.archive.load(id).unwrap();
        let retained=state.archive.root().join("engines").join(format!("{}.bin",backup.run.engine_id));
        assert_eq!(std::fs::read(&retained).unwrap(),std::fs::read(std::env::current_exe().unwrap()).unwrap());
        std::fs::write(&retained,b"corrupt engine copy").unwrap();
        let refused:serde_json::Value=serde_json::from_str(&archived_backtest_json(&request.to_string(),&state)).unwrap();
        assert!(refused["error"].as_str().unwrap().contains("integrity"));
        assert!(refused.get("archive").is_none());assert_eq!(state.archive.list().unwrap().len(),1);
        state.store.write().unwrap().series.clear();
        let replay:serde_json::Value=serde_json::from_str(&research_replay_json(&json!({"run_id":id}).to_string(),&state)).unwrap();
        assert_eq!(replay["matches"],true);
        assert_eq!(replay["replayed"],backup.run.result);
        let restored=research_state();restored.archive.restore(&backup).unwrap();
        assert_eq!(restored.archive.load(id).unwrap().run.result,backup.run.result);
        let mismatch=state.archive.save("different-engine".into(),backup.dataset,backup.strategy,backup.run.request,backup.run.result).unwrap();
        let replay:serde_json::Value=serde_json::from_str(&research_replay_json(&json!({"run_id":mismatch}).to_string(),&state)).unwrap();
        assert!(replay["error"].as_str().unwrap().contains("differs"));
        std::fs::remove_dir_all(state.archive.root()).unwrap();std::fs::remove_dir_all(restored.archive.root()).unwrap();
    }

    #[test]
    fn pipeline_settings_reject_silent_workload_changes() {
        let state=research_state();
        for invalid in [json!({"count":null}),json!({"count":0}),json!({"count":2001}),json!({"count":2.5}),json!({"keep":0}),json!({"keep":201}),json!({"count":5,"keep":6}),json!({"seed":-1}),json!({"seed":"42"})] {
            let response:serde_json::Value=serde_json::from_str(&pipeline_json(&invalid.to_string(),&state)).unwrap();
            assert!(response["error"].as_str().unwrap().contains("pipeline") || response["error"].as_str().unwrap().contains("Pipeline"),"{response}");
        }
        let input=PipelineInputs::parse(&json!({"count":3,"keep":2,"seed":123})).unwrap();
        assert_eq!((input.count,input.keep,input.seed),(3,2,123));
        assert!(state.databank.read().unwrap().is_empty());assert!(!state.archive.root().exists());
    }

    #[test]
    fn quick_search_retains_rejected_executions_and_recovers_without_data(){
        let state=research_state();state.store.write().unwrap().series[0].bars=(0..40).map(|i|Bar::new(i*60000,1.1,1.1,1.1,1.1,1.0)).collect();
        let request=json!({"symbol":"EURUSD","count":4,"seed":123,"allow_data_gaps":true,"request_key":"quick-search-recovery"});
        let response:serde_json::Value=serde_json::from_str(&generate_json(&request.to_string(),&state)).unwrap();assert!(response.get("error").is_none(),"{response}");assert!(response["results"].as_array().unwrap().is_empty());
        let id=response["archive"]["started_run_id"].as_str().unwrap();let completion=response["archive"]["run_id"].as_str().unwrap();let done=state.archive.load(completion).unwrap();let start=state.archive.load(id).unwrap();
        let builds=done.run.result["report"]["builds"].as_array().unwrap();assert_eq!(builds.len(),4);
        let mut rng=rand::rngs::StdRng::seed_from_u64(123);let generator:GeneratorConfig=serde_json::from_value(start.strategy["generator"].clone()).unwrap();
        for (i,row) in builds.iter().enumerate(){assert_eq!(row["evaluation"]["strategy"],json!(random_strategy(&mut rng,&format!("s{i:04}"),&generator)));assert_eq!(row["selected"],false);assert_eq!(row["execution"]["equity"].as_array().unwrap().len(),40);}
        // Even a search with no surviving candidates has exposed its input window.
        // It must not become an apparently untouched holdout for a later study.
        let experiment:serde_json::Value=serde_json::from_str(&experiment_json("create",&json!({"symbol":"EURUSD","holdout_start_time":1200000,"min_development_trades":1,"min_holdout_trades":1,"prior_exposure_disclosed":false,"allow_data_gaps":true}).to_string(),&state)).unwrap();
        assert!(experiment.get("error").is_none(),"{experiment}");
        let audit=&experiment["current_exposure_audit"];
        assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure","{experiment}");
        assert!(audit["known_overlaps"].as_array().unwrap().iter().any(|row|row["source"]=="archived_pipeline" && row["id"]==id));
        let backup=sqx_project::pipeline_archive::export(&state.archive,id).unwrap();let restored=research_state();restored.store.write().unwrap().series.clear();sqx_project::pipeline_archive::import(&restored.archive,backup).unwrap();std::fs::remove_dir_all(state.archive.root()).unwrap();
        let repeated:serde_json::Value=serde_json::from_str(&generate_json(&request.to_string(),&restored)).unwrap();assert_eq!(repeated["recovery_required"],true);assert_eq!(repeated["archive"]["started_run_id"],id);
        let replay=sqx_project::pipeline_archive::recover(&restored.archive,id,true).unwrap();assert_eq!(replay.completion_id,completion);assert_eq!(json!(replay.report),done.run.result["report"]);assert_eq!(replay.appended,0);
        assert!(reserve_web_research_request("web_optimization",&request,&restored).unwrap_err().contains("different inputs"));
        std::fs::remove_dir_all(restored.archive.root()).unwrap();
    }

    #[test]
    fn dashboard_pipeline_no_survivors_remains_a_complete_saved_search(){
        let state=research_state();state.store.write().unwrap().series[0].bars=(0..20).map(|i|Bar::new(i*60000,1.1,1.1,1.1,1.1,1.0)).collect();
        let request=json!({"symbol":"EURUSD","count":2,"keep":1,"seed":123,"allow_data_gaps":true,"request_key":"pipeline-no-survivors"});
        let response:serde_json::Value=serde_json::from_str(&pipeline_json(&request.to_string(),&state)).unwrap();assert!(response.get("error").is_none(),"{response}");
        assert_eq!(response["databank"],0);assert_eq!(response["builds"].as_array().unwrap().len(),2);
        let id=response["archive"]["started_run_id"].as_str().unwrap();let completion=response["archive"]["run_id"].as_str().unwrap();let done=state.archive.load(completion).unwrap();
        let root=state.archive.load(id).unwrap();assert_eq!(root.run.request["origin"],"web_pipeline");assert_eq!(root.strategy["pipeline"]["tasks"].as_array().unwrap().len(),4);
        let backup=sqx_project::pipeline_archive::export(&state.archive,id).unwrap();
        let restored=research_state();restored.store.write().unwrap().series.clear();sqx_project::pipeline_archive::import(&restored.archive,backup).unwrap();std::fs::remove_dir_all(state.archive.root()).unwrap();
        let repeated:serde_json::Value=serde_json::from_str(&pipeline_json(&request.to_string(),&restored)).unwrap();assert_eq!(repeated["recovery_required"],true);assert_eq!(repeated["archive"]["started_run_id"],id);
        let replay=sqx_project::pipeline_archive::recover(&restored.archive,id,true).unwrap();assert_eq!(replay.completion_id,completion);assert_eq!(json!(replay.report),done.run.result["report"]);assert_eq!(replay.appended,0);
        assert!(reserve_web_research_request("web_optimization",&request,&restored).unwrap_err().contains("different inputs"));
        std::fs::remove_dir_all(restored.archive.root()).unwrap();
    }

    #[test]
    fn pipeline_archive_replays_frozen_inputs_after_backup_restore() {
        let state=research_state();
        let series=state.store.read().unwrap().series[0].clone();
        let inst=instrument_for(&series.symbol);
        let cfg=BacktestConfig::default();let gen=GeneratorConfig::default();
        let pyp=pyp_for(&series.bars);let context=json!({"scope":"exploratory fixture"});
        // Exercise both a completed search and an explicitly failed report.
        for count in [10,0] {
            let result=compute_pipeline_report(&series,&inst,&cfg,pyp,&gen,count,2,123,&context);
            assert!(result["optimizations"].is_array());
            assert_eq!(result["builds"].as_array().unwrap().len(),count);
            if count==0 {assert!(result["error"].is_string());}
            let request=json!({"research_kind":"pipeline","config":cfg,"generator":gen,"count":count,"keep":2,"seed":123,"periods_per_year":pyp,"research_context":context});
            let dataset=json!(ArchivedDataset{symbol:series.symbol.clone(),timeframe:series.timeframe,instrument:inst.clone(),calendar:series.calendar.clone(),normalization:series.normalization.clone(),bars:series.bars.clone()});
            let id=state.archive.save(engine_identity().unwrap(),dataset,json!({"name":"Pipeline","kind":"pipeline"}),request,result.clone()).unwrap();
            let bytes=serde_json::to_vec(&state.archive.load(&id).unwrap()).unwrap();
            let restored=research_state();
            restored.archive.restore(&serde_json::from_slice(&bytes).unwrap()).unwrap();
            restored.store.write().unwrap().series.clear();
            let replay:serde_json::Value=serde_json::from_str(&research_replay_json(&json!({"run_id":id}).to_string(),&restored)).unwrap();
            assert_eq!(replay["matches"],true,"{replay}");assert_eq!(replay["replayed"],result);
            assert!(restored.databank.read().unwrap().is_empty());
            std::fs::remove_dir_all(restored.archive.root()).unwrap();
        }
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn robustness_archive_replays_after_restore_without_live_data() {
        let state=research_state();
        let mut strategy=Strategy::new("Archived stress fixture");strategy.long=true;strategy.short=false;
        strategy.money_management=sqx_strategy::MoneyManagement::FixedSize{size:0.1};
        strategy.signals.push(sqx_strategy::Signal::new(sqx_strategy::SignalKind::LongEntry,vec![sqx_strategy::Condition::new(sqx_strategy::Operand::Const(1.0),sqx_strategy::Comparator::Gt,sqx_strategy::Operand::Const(0.0))],sqx_strategy::Join::And));
        let mut request=json!(strategy);request["symbol"]=json!(state.store.read().unwrap().series[0].symbol);request["allow_data_gaps"]=json!(true);
        let response:serde_json::Value=serde_json::from_str(&robustness_json(&request.to_string(),&state)).unwrap();
        assert!(response.get("error").is_none(),"{response}");
        assert_eq!(response["mc"]["samples"].as_array().unwrap().len(),1000);
        let id=response["archive"]["run_id"].as_str().unwrap();
        let bundle=state.archive.load(id).unwrap();assert_eq!(bundle.run.request["research_kind"],"cli_pipeline");
        let started=response["archive"]["started_run_id"].as_str().unwrap();
        let backup:serde_json::Value=serde_json::from_str(&pipeline_action_json("backup",&json!({"run_id":started}).to_string(),&state)).unwrap();
        assert_eq!(backup["events"].as_array().unwrap().len(),2016); // step start, base pair, 6 cost pairs, 1000 MC pairs, selection.
        let restored=research_state();
        let imported:serde_json::Value=serde_json::from_str(&pipeline_action_json("restore",&backup.to_string(),&restored)).unwrap();assert!(imported.get("error").is_none(),"{imported}");
        std::fs::remove_dir_all(state.archive.root()).unwrap();
        restored.store.write().unwrap().series.clear();
        let replay:serde_json::Value=serde_json::from_str(&pipeline_action_json("replay",&json!({"run_id":started}).to_string(),&restored)).unwrap();
        assert_eq!(replay["matches"],true,"{replay}");assert_eq!(replay["completion_id"],id);
        assert_eq!(restored.archive.load(id).unwrap().run.result,bundle.run.result);
        let listing:serde_json::Value=serde_json::from_str(&research_list_json(&restored)).unwrap();
        assert!(listing["runs"].as_array().unwrap().iter().all(|r|r["workflow_kind"]=="robustness"));
        assert!(restored.databank.read().unwrap().is_empty());
        std::fs::remove_dir_all(restored.archive.root()).unwrap();
    }

    #[test]
    fn stress_request_identity_survives_missing_sources_and_restore(){
        let state=research_state();let mut request=json!(Strategy::new("deduplicated study"));
        request["symbol"]=json!("EURUSD");request["allow_data_gaps"]=json!(true);request["request_key"]=json!("request-1");
        let (guard,existing)=reserve_stress_request(&request,&state).unwrap();assert!(existing.is_none());
        let busy:serde_json::Value=serde_json::from_str(&robustness_json(&request.to_string(),&state)).unwrap();assert!(busy["error"].as_str().unwrap().contains("busy"));
        assert!(state.archive.list().unwrap().is_empty());drop(guard);
        let first:serde_json::Value=serde_json::from_str(&robustness_json(&request.to_string(),&state)).unwrap();
        let id=first["archive"]["started_run_id"].as_str().expect("study recorded");let count=state.archive.list().unwrap().len();
        let start=state.archive.load(id).unwrap();state.store.write().unwrap().series.clear();
        let repeated:serde_json::Value=serde_json::from_str(&robustness_json(&request.to_string(),&state)).unwrap();
        assert_eq!(repeated["recovery_required"],true);assert_eq!(repeated["archive"]["started_run_id"],id);assert_eq!(state.archive.list().unwrap().len(),count);
        let restored=research_state();restored.store.write().unwrap().series.clear();restored.archive.restore(&start).unwrap();
        let repeated:serde_json::Value=serde_json::from_str(&robustness_json(&request.to_string(),&restored)).unwrap();assert_eq!(repeated["archive"]["started_run_id"],id);
        request["name"]=json!("changed inputs");
        let conflict:serde_json::Value=serde_json::from_str(&robustness_json(&request.to_string(),&restored)).unwrap();assert!(conflict["error"].as_str().unwrap().contains("different inputs"));assert_eq!(restored.archive.list().unwrap().len(),1);
        request["request_key"]=json!("../invalid");assert!(reserve_stress_request(&request,&restored).is_err());
        // Two valid content-addressed starts can arrive from separate backups.
        // Keep both as evidence, but never pick one arbitrarily on a retry.
        let original=start.run.request["web_request"].clone();
        let mut conflicting_request=start.run.request.clone();conflicting_request["attempt_id"]=json!("another-archived-attempt");
        let conflicting_id=state.archive.save(start.run.engine_id.clone(),start.dataset.clone(),start.strategy.clone(),conflicting_request,start.run.result.clone()).unwrap();
        assert_ne!(conflicting_id,id);
        let conflicting_bundle=state.archive.load(&conflicting_id).unwrap();
        let rejected:serde_json::Value=serde_json::from_str(&research_restore_json(&serde_json::to_string(&conflicting_bundle).unwrap(),&restored)).unwrap();
        assert!(rejected["error"].as_str().unwrap().contains("conflicts with an existing study"));assert_eq!(restored.archive.list().unwrap().len(),1);
        // Simulate a pre-existing ambiguous archive through the low-level content store.
        restored.archive.restore(&conflicting_bundle).unwrap();
        let before=restored.archive.list().unwrap();
        let ambiguous:serde_json::Value=serde_json::from_str(&robustness_json(&original.to_string(),&restored)).unwrap();
        assert!(ambiguous["error"].as_str().unwrap().contains("Conflicting studies"));assert!(ambiguous.get("archive").is_none());
        assert_eq!(restored.archive.list().unwrap(),before,"conflict must not execute, delete or replace either study");
        std::fs::remove_dir_all(state.archive.root()).unwrap();std::fs::remove_dir_all(restored.archive.root()).unwrap();
    }

    #[test]
    fn legacy_robustness_report_remains_replayable() {
        let state=research_state();let series=state.store.read().unwrap().series[0].clone();
        let instrument=instrument_for(&series.symbol);let strategy=Strategy::new("legacy stress report");
        let spec=RobustnessSpec{config:BacktestConfig::default(),scenarios:vec![(0.0,0.0,0.0)],runs:2,skip_probability:0.1,seed:42,context:json!({})};
        let result=compute_robustness_report(&strategy,&series,&instrument,&spec);
        let dataset=json!(ArchivedDataset{symbol:series.symbol,timeframe:series.timeframe,instrument,calendar:series.calendar,normalization:series.normalization,bars:series.bars});
        let id=state.archive.save(engine_identity().unwrap(),dataset,json!(strategy),json!({"research_kind":"robustness","spec":spec}),result.clone()).unwrap();
        state.store.write().unwrap().series.clear();
        let replay:serde_json::Value=serde_json::from_str(&research_replay_json(&json!({"run_id":id}).to_string(),&state)).unwrap();
        assert_eq!(replay["matches"],true,"{replay}");assert_eq!(replay["replayed"],result);
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn comparison_endpoints_import_read_restore_without_live_state(){
        let state=research_state();let series=state.store.read().unwrap().series[0].clone();let inst=instrument_for(&series.symbol);
        let strategy=Strategy::new("No trades does not certify parity");let mut request=json!(strategy);request["allow_data_gaps"]=json!(true);
        let report:serde_json::Value=serde_json::from_str(&backtest_series_json(&request.to_string(),&series,&inst)).unwrap();assert!(report.get("error").is_none(),"{report}");
        let dataset=json!(ArchivedDataset{symbol:series.symbol.clone(),timeframe:series.timeframe,instrument:inst.clone(),calendar:series.calendar.clone(),normalization:series.normalization.clone(),bars:series.bars.clone()});
        let run_id=state.archive.save(engine_identity().unwrap(),dataset,json!(strategy),request,report).unwrap();
        let source=json!({"schema":1,"source_label":"Synthetic empty fixture","scope":{"market":inst.name,"quote_currency":inst.quote_currency,"account_currency":"USD","contract_size":inst.contract_size,"start_ms":series.bars.first().unwrap().time,"end_ms":series.timeframe.bucket_bounds(series.bars.last().unwrap().time).unwrap().1-1},"trades":[]}).to_string();
        let import=json!({"run_id":run_id,"source_json":source,"tolerances":{"time_ms":0,"price":0.0,"size":0.0,"money":0.0}});
        let result:serde_json::Value=serde_json::from_str(&comparison_import_json(&import.to_string(),&state)).unwrap();assert!(result.get("error").is_none(),"{result}");assert_eq!(result["deployment_ready"],false);
        let id=result["comparison_id"].as_str().unwrap();let backup=comparison_read_json(id,&state);
        let restored=research_state();restored.store.write().unwrap().series.clear();
        let response:serde_json::Value=serde_json::from_str(&comparison_restore_json(&backup,&restored)).unwrap();assert_eq!(response["comparison_id"],id);
        let rows:serde_json::Value=serde_json::from_str(&comparison_list_json(&restored)).unwrap();assert_eq!(rows["comparisons"].as_array().unwrap().len(),1);assert_eq!(rows["comparisons"][0]["matches_within_tolerance"],false);assert_eq!(rows["comparisons"][0]["source_label"],"Synthetic empty fixture");assert_eq!(rows["comparisons"][0]["differing_pairs"],0);
        assert_eq!(comparison_read_json(id,&restored),backup);assert!(restored.archive.list().unwrap().is_empty());
        let invalid=json!({"run_id":run_id,"source_json":source,"tolerances":{"time_ms":-1,"price":0,"size":0,"money":0}});
        assert!(serde_json::from_str::<serde_json::Value>(&comparison_import_json(&invalid.to_string(),&state)).unwrap()["error"].is_string());
        let mut tampered:serde_json::Value=serde_json::from_str(&backup).unwrap();tampered["record"]["source_json"]=json!("{}");
        assert!(serde_json::from_str::<serde_json::Value>(&comparison_restore_json(&tampered.to_string(),&restored)).unwrap()["error"].is_string());
        std::fs::remove_dir_all(state.archive.root()).unwrap();std::fs::remove_dir_all(restored.archive.root()).unwrap();
    }

    #[test]
    fn fingerprint_detects_content_and_order_changes() {
        let bars = sqx_data::synthetic_bars("EURUSD", 20);
        assert_eq!(bars_fingerprint(&bars), bars_fingerprint(&bars.clone()));
        let mut changed = bars.clone();
        changed[0].volume += 1.0;
        assert_ne!(bars_fingerprint(&bars), bars_fingerprint(&changed));
        changed = bars.clone();
        changed.swap(0, 1);
        assert_ne!(bars_fingerprint(&bars), bars_fingerprint(&changed));
    }

    #[test]
    fn run_evidence_records_actual_costs_and_data() {
        let state = research_state();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let strategy = random_strategy(&mut rng, "Evidence", &GeneratorConfig::default());
        let mut request = serde_json::to_value(strategy).unwrap();
        request["symbol"] = json!("EURUSD");
        request["spread"] = json!(3.0);
        request["deposit"] = json!(25000.0);
        request["compare_paths"] = json!(true);
        let response: serde_json::Value = serde_json::from_str(&backtest_json(&request.to_string(), &state)).unwrap();
        assert_eq!(response["run_context"]["spread_points"], 3.0);
        assert_eq!(response["run_context"]["initial_deposit"], 25000.0);
        assert_eq!(response["run_context"]["bars"], 200);
        assert_eq!(response["run_context"]["execution"]["mode"], "ohlc_bars");
        assert_eq!(response["path_sensitivity"].as_array().unwrap().len(), 4);
        assert_eq!(response["run_context"]["data_fingerprint"], bars_fingerprint(&state.store.read().unwrap().series[0].bars));
        assert_eq!(response["trade_details"].as_array().unwrap().len(), response["trades"].as_u64().unwrap() as usize);
        assert_eq!(backtest_json(&request.to_string(), &state), backtest_json(&request.to_string(), &state));
    }

    #[test]
    fn portfolio_and_analyzer_share_quality_and_component_error_gates() {
        let state=research_state();state.store.write().unwrap().series[0].bars.remove(50);
        let request=json!({"symbol":"EURUSD","count":10,"seed":42,"top":2,"min_k":2,"max_k":2}).to_string();
        for route in [portfolio_json as fn(&str,&State)->String,portfolio_databank_json,analyze_json] {
            let result:serde_json::Value=serde_json::from_str(&route(&request,&state)).unwrap();
            assert!(result["error"].as_str().unwrap().contains("data gaps"));
        }
        let state=research_state();let mut invalid=Strategy::new("missing close time");invalid.trading_options.exit_at_end_of_day=true;
        let request=json!({"symbol":"EURUSD","strategies":[invalid.clone(),Strategy::new("valid")]});
        let result:serde_json::Value=serde_json::from_str(&portfolio_databank_json(&request.to_string(),&state)).unwrap();
        assert!(result["error"].as_str().unwrap().contains("invalid component"));
        assert_eq!(result["components"][0]["strategy"],"missing close time");assert!(result.get("metrics").is_none());
        let mut request=serde_json::to_value(invalid).unwrap();request["symbol"]=json!("EURUSD");
        let result:serde_json::Value=serde_json::from_str(&analyze_json(&request.to_string(),&state)).unwrap();
        assert!(result["error"].is_string());assert!(result.get("markdown").is_none());
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn invalid_matrix_keeps_all_cells_and_replays_as_invalid(){
        let state=research_state();
        let request=json!({"symbol":"EURUSD","kind":"wf-matrix","count":1,"seed":123,"allow_data_gaps":true});
        let response:serde_json::Value=serde_json::from_str(&optimize_json(&request.to_string(),&state)).unwrap();
        assert!(response["error"].is_string(),"{response}");
        let id=response["archive"]["started_run_id"].as_str().unwrap();
        let completion=response["archive"]["run_id"].as_str().unwrap();
        let done=state.archive.load(completion).unwrap();
        assert_eq!(done.run.result["report"]["walk_forward_studies"].as_array().unwrap().len(),12);
        let backup=sqx_project::pipeline_archive::export(&state.archive,id).unwrap();
        let restored=research_state();restored.store.write().unwrap().series.clear();sqx_project::pipeline_archive::import(&restored.archive,backup).unwrap();std::fs::remove_dir_all(state.archive.root()).unwrap();
        let replay=sqx_project::pipeline_archive::recover(&restored.archive,id,true).unwrap();
        assert!(!replay.report.errors.is_empty());assert_eq!(replay.completion_id,completion);assert_eq!(json!(replay.report),done.run.result["report"]);
        std::fs::remove_dir_all(restored.archive.root()).unwrap();
    }

    #[test]
    fn optimization_retains_all_modes_and_restores_without_live_inputs(){
        for kind in ["simple","walk-forward","wf-matrix"] {
            let state=research_state();state.store.write().unwrap().series[0].bars=(0..1500).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;Bar::new(i*60000,p,p+0.003,p-0.003,p+0.0002,10.0)}).collect();
            let request=json!({"symbol":"EURUSD","kind":kind,"count":2,"seed":123,"windows":2,"oos":0.3,"min_is_trades":999,"min_oos_trades":777,"allow_data_gaps":true,"request_key":format!("optimization-{kind}")});
            let response:serde_json::Value=serde_json::from_str(&optimize_json(&request.to_string(),&state)).unwrap();
            assert!(response.get("error").is_none(),"{response}");
            let id=response["archive"]["started_run_id"].as_str().unwrap();let completion=response["archive"]["run_id"].as_str().unwrap();
            let done=state.archive.load(completion).unwrap();
            assert_eq!(state.archive.load(id).unwrap().run.request["origin"],"web_optimization");
            if kind=="simple"{assert_eq!(done.run.result["report"]["builds"].as_array().unwrap().len(),2);}else{
                let cells=done.run.result["report"]["walk_forward_studies"].as_array().unwrap();assert_eq!(cells.len(),if kind=="wf-matrix"{12}else{1});
                for cell in cells {assert_eq!(cell["specification"]["criteria"]["min_is_trades"],999);assert_eq!(cell["specification"]["criteria"]["min_oos_trades_per_window"],777);assert_eq!(cell["report"]["evidence_sufficient"],false);}
            }
            let backup=sqx_project::pipeline_archive::export(&state.archive,id).unwrap();
            let restored=research_state();restored.store.write().unwrap().series.clear();sqx_project::pipeline_archive::import(&restored.archive,backup).unwrap();std::fs::remove_dir_all(state.archive.root()).unwrap();
            let repeated:serde_json::Value=serde_json::from_str(&optimize_json(&request.to_string(),&restored)).unwrap();assert_eq!(repeated["recovery_required"],true);assert_eq!(repeated["archive"]["started_run_id"],id);
            let replay=sqx_project::pipeline_archive::recover(&restored.archive,id,true).unwrap();assert_eq!(replay.completion_id,completion);assert_eq!(json!(replay.report),done.run.result["report"]);assert_eq!(replay.appended,0);
            let mut changed=request.clone();changed["seed"]=json!(124);assert!(reserve_web_research_request("web_optimization",&changed,&restored).unwrap_err().contains("different inputs"));
            assert!(reserve_web_research_request("web_portfolio",&request,&restored).unwrap_err().contains("different inputs"));
            std::fs::remove_dir_all(restored.archive.root()).unwrap();
        }
    }

    #[test]
    fn generated_portfolio_freezes_all_trials_and_request_identity_through_restore(){
        let state=research_state();state.store.write().unwrap().series[0].bars=(0..1500).map(|i|{let p=1.1+(i as f64*0.07).sin()*0.01;Bar::new((1700000000+i*60)*1000,p,p+0.001,p-0.001,p,1.0)}).collect();
        let mut request=json!({"symbol":"EURUSD","count":40,"seed":123,"top":4,"min_k":2,"max_k":3,"allow_data_gaps":true,"request_key":"portfolio-request-1"});
        let response:serde_json::Value=serde_json::from_str(&portfolio_json(&request.to_string(),&state)).unwrap();assert!(response.get("error").is_none(),"{response}");
        let id=response["archive"]["started_run_id"].as_str().unwrap();let completion=response["archive"]["run_id"].as_str().unwrap();
        let done=state.archive.load(completion).unwrap();let study=&done.run.result["report"]["portfolio"];
        assert_eq!(study["candidates"].as_array().unwrap().len(),40);assert_eq!(study["subset"]["attempted"],10);
        assert_eq!(state.archive.load(id).unwrap().run.request["origin"],"web_portfolio");
        let backup=sqx_project::pipeline_archive::export(&state.archive,id).unwrap();
        state.store.write().unwrap().series.clear();let before=state.archive.list().unwrap();
        let repeated:serde_json::Value=serde_json::from_str(&portfolio_json(&request.to_string(),&state)).unwrap();assert_eq!(repeated["recovery_required"],true);assert_eq!(repeated["archive"]["started_run_id"],id);assert_eq!(state.archive.list().unwrap(),before);
        let restored=research_state();restored.store.write().unwrap().series.clear();sqx_project::pipeline_archive::import(&restored.archive,backup).unwrap();std::fs::remove_dir_all(state.archive.root()).unwrap();
        let repeated:serde_json::Value=serde_json::from_str(&portfolio_json(&request.to_string(),&restored)).unwrap();assert_eq!(repeated["archive"]["started_run_id"],id);
        let replay=sqx_project::pipeline_archive::recover(&restored.archive,id,true).unwrap();assert_eq!(replay.completion_id,completion);assert_eq!(json!(replay.report),done.run.result["report"]);assert_eq!(replay.appended,0);
        request["seed"]=json!(124);assert!(reserve_web_research_request("web_portfolio",&request,&restored).unwrap_err().contains("different inputs"));
        request["seed"]=json!(123);assert!(reserve_web_research_request("web_robustness",&request,&restored).unwrap_err().contains("different inputs"));
        std::fs::remove_dir_all(restored.archive.root()).unwrap();
    }

    #[test]
    fn selected_portfolio_uses_explicit_strategies_without_databank_rows() {
        let state = research_state();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let a = random_strategy(&mut rng, "Selected A", &GeneratorConfig::default());
        let b = random_strategy(&mut rng, "Selected B", &GeneratorConfig::default());
        let request = json!({"symbol":"EURUSD", "strategies":[a,b],"deposit":4321.0,"spread":12.0,"commission":7.0});
        let response: serde_json::Value = serde_json::from_str(&portfolio_databank_json(&request.to_string(), &state)).unwrap();
        assert_eq!(response["components"], json!(["Selected A", "Selected B"]));
        assert_eq!(response["research_context"]["config"]["initial_deposit"],4321.0);
        assert_eq!(response["research_context"]["config"]["spread_points"],12.0);
        assert_eq!(response["research_context"]["config"]["commission_per_lot"],7.0);
        let response: serde_json::Value = serde_json::from_str(&portfolio_databank_json(r#"{"symbol":"EURUSD","strategies":[]}"#, &state)).unwrap();
        assert!(response["error"].is_string());
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn selected_portfolio_keeps_twenty_components_and_recovers_without_databank_or_data(){
        let state=research_state();state.store.write().unwrap().series[0].bars=(0..20).map(|i|Bar::new(i*60000,1.1,1.1,1.1,1.1,1.0)).collect();
        let strategies:Vec<_>=(0..20).map(|i|Strategy::new(&format!("Selected {i}"))).collect();
        let request=json!({"symbol":"EURUSD","strategies":strategies,"request_key":"selected-portfolio-1","allow_data_gaps":true});
        let response:serde_json::Value=serde_json::from_str(&portfolio_databank_json(&request.to_string(),&state)).unwrap();assert!(response.get("error").is_none(),"{response}");assert_eq!(response["components"].as_array().unwrap().len(),20);assert_eq!(response["no_trade_components"].as_array().unwrap().len(),20);
        let id=response["archive"]["started_run_id"].as_str().unwrap();let done=state.archive.load(response["archive"]["run_id"].as_str().unwrap()).unwrap();let study=&done.run.result["report"]["portfolio_combination"];
        assert_eq!(study["components"].as_array().unwrap().len(),20);assert_eq!(study["weights"],json!(vec![0.05;20]));assert!(done.run.result["report"].get("portfolio").is_none());
        let backup=sqx_project::pipeline_archive::export(&state.archive,id).unwrap();let restored=research_state();restored.store.write().unwrap().series.clear();sqx_project::pipeline_archive::import(&restored.archive,backup).unwrap();std::fs::remove_dir_all(state.archive.root()).unwrap();
        let repeated:serde_json::Value=serde_json::from_str(&portfolio_databank_json(&request.to_string(),&restored)).unwrap();assert_eq!(repeated["recovery_required"],true);assert_eq!(repeated["archive"]["started_run_id"],id);
        let replay=sqx_project::pipeline_archive::recover(&restored.archive,id,true).unwrap();assert_eq!(replay.completion_id,done.run_id);assert_eq!(json!(replay.report),done.run.result["report"]);
        let listing:serde_json::Value=serde_json::from_str(&research_list_json(&restored)).unwrap();assert!(listing["runs"].as_array().unwrap().iter().all(|row|row["workflow_kind"]=="portfolio"));
        std::fs::remove_dir_all(restored.archive.root()).unwrap();
    }

    #[test]
    fn databank_composition_freezes_display_names_and_selection_rows(){
        let state=research_state();let rows=json!([{"name":"Alias A","strategy":Strategy::new("Rules A"),"source_run_id":"declared-unverified-origin"},{"name":"Alias B","strategy":Strategy::new("Rules B")}]);
        *state.databank.write().unwrap()=rows.as_array().unwrap().clone();
        let response:serde_json::Value=serde_json::from_str(&portfolio_databank_json(&json!({"symbol":"EURUSD","top":2,"allow_data_gaps":true}).to_string(),&state)).unwrap();assert!(response.get("error").is_none(),"{response}");assert_eq!(response["components"],json!(["Alias A","Alias B"]));
        let start=state.archive.load(response["archive"]["started_run_id"].as_str().unwrap()).unwrap();assert_eq!(start.dataset["source_context"]["databank_rows"],rows);assert_eq!(start.dataset["source_context"]["selection_origin"],"databank_order");
        assert_eq!(start.strategy["pipeline"]["tasks"][0]["kind"]["PortfolioCombination"]["strategies"][0]["name"],"Rules A");
        std::fs::remove_dir_all(state.archive.root()).unwrap();
    }

    #[test]
    fn safe_name_strips_separators() {
        assert_eq!(safe_name("my strategy/v1"), "my_strategy_v1");
        assert_eq!(safe_name("ok-name_1"), "ok-name_1");
    }

    #[test]
    fn query_param_reads_values() {
        assert_eq!(query_param("/x?name=foo&a=1", "name"), "foo");
        assert_eq!(query_param("/x", "name"), "");
        assert_eq!(query_param("/x?name=", "name"), "");
    }

    #[test]
    fn dedupe_push_keeps_unique_strategies() {
        let mut db = Vec::new();
        let row = json!({ "name": "a", "strategy": { "name": "a" } });
        let row2 = json!({ "name": "b", "strategy": { "name": "b" } });
        dedupe_push(&mut db, &[row.clone(), row.clone(), row2.clone()]);
        assert_eq!(db.len(), 2);
        dedupe_push(&mut db, &[row]);
        assert_eq!(db.len(), 2);
    }

    #[test]
    fn export_json_produces_code() {
        let body = r#"{"engine":"mql5","strategy":{"name":"t","long":true,"short":false,"signals":[],"exits":[],"stop_loss":null,"profit_target":null,"money_management":{"FixedSize":{"size":0.1}}}}"#;
        let out = export_json(body);
        let response: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(response["status"],"reference_draft");
        assert_eq!(response["deployment_ready"],false);
        assert!(response["code"].as_str().unwrap().contains("REFERENCE DRAFT ONLY"));
        let mut request: serde_json::Value = serde_json::from_str(body).unwrap();
        request["strategy"]["stop_loss"] = json!({"Points":100.0});
        let response: serde_json::Value = serde_json::from_str(&export_json(&request.to_string())).unwrap();
        assert_eq!(response["status"],"blocked");
        assert!(response["code"].is_null());
        assert!(response["error"].as_str().unwrap().contains("Stop/target"));
        request["engine"] = json!("typo");
        assert!(export_json(&request.to_string()).contains("Unknown export engine"));
    }
}

fn route(method: &str, full_path: &str, body: &str, state: &State) -> (&'static str, &'static str, String) {
    let (path, qsym) = query_symbol(full_path);
    if method == "POST" {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
            if value.get("intrabar_up_first").is_some() {
                return ("200 OK", "application/json", json!({"error":"Replace intrabar_up_first with explicit intrabar_policy: adverse_first, favorable_first, high_first or low_first"}).to_string());
            }
            if let Some(policy) = value.get("intrabar_policy") {
                if let Err(error) = serde_json::from_value::<sqx_backtest::IntrabarPolicy>(policy.clone()) {
                    return ("200 OK", "application/json", json!({"error":error.to_string()}).to_string());
                }
            }
            if let Some(account) = value.get("account") {
                let parsed = serde_json::from_value::<sqx_backtest::AccountConfig>(account.clone())
                    .map_err(|e| e.to_string()).and_then(|a| a.validate());
                if let Err(error) = parsed { return ("200 OK", "application/json", json!({"error":error}).to_string()); }
            }
            if matches!(path, "/api/backtest" | "/api/generate" | "/api/portfolio" | "/api/portfolio/databank" | "/api/robustness" | "/api/optimize" | "/api/pipeline" | "/api/analyze") {
                let symbol = value.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(series) = find_series(state, symbol) {
                    let quality=sqx_data::quality::audit(&series.bars,series.timeframe,sqx_data::quality::now_ms(),series.calendar.as_ref());
                    if !quality.errors.is_empty() {return ("200 OK","application/json",json!({"error":quality.errors.join("; "),"data_quality":quality}).to_string());}
                    if (quality.missing_open_intervals>0 || quality.unclassified_intervals>0 || quality.irregular_steps>0) && !value.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false) {
                        return ("200 OK","application/json",json!({"error":"Unresolved data gaps: inspect data quality/calendar or explicitly allow_data_gaps for exploration","data_quality":quality}).to_string());
                    }
                    let config=match config_for_series(&value,&series){Ok(config)=>config,Err(error)=>return ("200 OK","application/json",json!({"error":error}).to_string())};
                    if let Err(error) = sqx_backtest::validate_backtest_inputs(&series.bars, &instrument_for(&series.symbol), &config) {
                        return ("200 OK", "application/json", json!({"error":error}).to_string());
                    }
                }
            }
        }
    }
    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => ("200 OK", "text/html; charset=utf-8", INDEX.to_string()),
        ("GET", "/workspace.js") => ("200 OK", "text/javascript; charset=utf-8", include_str!("../assets/workspace.js").to_string()),
        ("GET", "/api/indicators") => ("200 OK", "application/json", indicators_json()),
        ("GET", "/api/catalog") => ("200 OK", "application/json", catalog_json()),
        ("GET", "/api/datasources") => ("200 OK", "application/json", datasources_json(state)),
        ("GET", "/api/questdb/symbols") => ("200 OK", "application/json", questdb_symbols_json(state)),
        ("POST", "/api/questdb/load") => ("200 OK", "application/json", questdb_load_json(body, state)),
        ("GET", "/api/data") => ("200 OK", "application/json", data_json(state, &qsym)),
        ("POST", "/api/backtest") => ("200 OK", "application/json", archived_backtest_json(body, state)),
        ("GET", "/api/experiments/backup") => ("200 OK", "application/json", experiment_json("backup",&json!({"experiment_id":query_param(full_path,"id")}).to_string(),state)),
        ("POST", "/api/experiments/restore") => ("200 OK", "application/json", experiment_json("restore",body,state)),
        ("POST", "/api/experiments/create") => ("200 OK", "application/json", experiment_json("create",body,state)),
        ("POST", "/api/experiments/trial") => ("200 OK", "application/json", experiment_json("trial",body,state)),
        ("POST", "/api/experiments/jobs/start") => ("200 OK", "application/json", experiment_json("job_start",body,state)),
        ("POST", "/api/experiments/jobs/step") => ("200 OK", "application/json", experiment_json("job_step",body,state)),
        ("POST", "/api/experiments/search") => ("200 OK", "application/json", experiment_json("search",body,state)),
        ("POST", "/api/experiments/reveal") => ("200 OK", "application/json", experiment_json("reveal",body,state)),
        ("GET", "/api/experiments") => ("200 OK", "application/json", experiment_json("list","{}",state)),
        ("GET", "/api/experiments/status") => ("200 OK", "application/json", experiment_json("status",&json!({"experiment_id":query_param(full_path,"id")}).to_string(),state)),
        ("GET", "/api/evolution/jobs") => ("200 OK", "application/json", evolution_json("list","{}",state)),
        ("GET", "/api/evolution/backup") => ("200 OK", "application/json", evolution_json("backup",&json!({"job_id":query_param(full_path,"id")}).to_string(),state)),
        ("GET", "/api/evolution/status") => ("200 OK", "application/json", evolution_json("status",&json!({"job_id":query_param(full_path,"id")}).to_string(),state)),
        ("POST", "/api/evolution/replay") => ("200 OK", "application/json", evolution_json("replay",body,state)),
        ("POST", "/api/evolution/restore") => ("200 OK", "application/json", evolution_json("restore",body,state)),
        ("POST", "/api/evolution/start") => ("200 OK", "application/json", evolution_json("start",body,state)),
        ("POST", "/api/evolution/advance") => ("200 OK", "application/json", evolution_json("advance",body,state)),
        ("GET", "/api/comparisons") => ("200 OK", "application/json", comparison_list_json(state)),
        ("GET", "/api/comparison") => ("200 OK", "application/json", comparison_read_json(&query_param(full_path,"id"),state)),
        ("POST", "/api/comparison/import") => ("200 OK", "application/json", comparison_import_json(body,state)),
        ("POST", "/api/comparison/restore") => ("200 OK", "application/json", comparison_restore_json(body,state)),
        ("GET", "/api/pipeline/status") => ("200 OK", "application/json", pipeline_action_json("status",&json!({"run_id":query_param(full_path,"id")}).to_string(),state)),
        ("GET", "/api/pipeline/backup") => ("200 OK", "application/json", pipeline_action_json("backup",&json!({"run_id":query_param(full_path,"id")}).to_string(),state)),
        ("POST", "/api/pipeline/restore") => ("200 OK", "application/json", pipeline_action_json("restore",body,state)),
        ("POST", "/api/pipeline/replay") => ("200 OK", "application/json", pipeline_action_json("replay",body,state)),
        ("GET", "/api/research/runs") => ("200 OK", "application/json", research_list_json(state)),
        ("GET", "/api/research/run") => ("200 OK", "application/json", match state.archive.load(&query_param(full_path,"id")) {Ok(bundle)=>json!(bundle).to_string(),Err(e)=>json!({"error":e}).to_string()}),
        ("POST", "/api/research/replay") => ("200 OK", "application/json", research_replay_json(body,state)),
        ("POST", "/api/research/restore") => ("200 OK", "application/json", research_restore_json(body,state)),
        ("POST", "/api/generate") => ("200 OK", "application/json", generate_json(body, state)),
        ("POST", "/api/portfolio") => ("200 OK", "application/json", portfolio_json(body, state)),
        ("POST", "/api/pseudocode") => ("200 OK", "application/json", pseudocode_json(body)),
        ("POST", "/api/robustness") => ("200 OK", "application/json", robustness_json(body, state)),
        ("POST", "/api/optimize") => ("200 OK", "application/json", optimize_json(body, state)),
        ("POST", "/api/pipeline") => ("200 OK", "application/json", pipeline_json(body, state)),
        ("POST", "/api/export") => ("200 OK", "application/json", export_json(body)),
        ("POST", "/api/analyze") => ("200 OK", "application/json", analyze_json(body, state)),
        ("GET", "/api/data/saved") => ("200 OK", "application/json", saved_datasets_json("list","{}",state)),
        ("POST", "/api/data/load") => ("200 OK", "application/json", saved_datasets_json("load",body,state)),
        ("POST", "/api/data/import") => ("200 OK", "application/json", import_json(body, state)),
        ("POST", "/api/data/delete") => ("200 OK", "application/json", delete_json(body, state)),
        ("POST", "/api/assistant") => ("200 OK", "application/json", assistant_json(body, state)),
        ("GET", "/api/assistant/config") => ("200 OK", "application/json", assistant_config_json()),
        ("POST", "/api/assistant/config") => ("200 OK", "application/json", assistant_config_save_json(body)),
        ("GET", "/api/databank") => ("200 OK", "application/json", databank_json(state)),
        ("GET", "/api/databank.csv") => ("200 OK", "text/csv; charset=utf-8", databank_csv(state)),
        ("POST", "/api/databank/clear") => ("200 OK", "application/json", databank_clear(state)),
        ("GET", "/api/project/list") => ("200 OK", "application/json", project_list_json()),
        ("GET", "/api/project/load") => ("200 OK", "application/json", project_load_json(&query_param(full_path, "name"))),
        ("POST", "/api/project/save") => ("200 OK", "application/json", project_save_json(body)),
        ("POST", "/api/portfolio/databank") => ("200 OK", "application/json", portfolio_databank_json(body, state)),
        ("GET", "/api/profiles") => ("200 OK", "application/json", profiles_json()),
        ("POST", "/api/profiles/save") => ("200 OK", "application/json", profile_save_json(body)),
        ("POST", "/api/profiles/delete") => ("200 OK", "application/json", profile_delete_json(body)),
        _ => ("404 Not Found", "text/plain", "not found".to_string()),
    }
}

fn series_key(symbol:&str,timeframe:&str)->String {
    format!("series-{}",sqx_project::archive::content_id(json!([symbol,timeframe]).to_string().as_bytes()))
}
fn find_series(state: &State, symbol: &str) -> Option<SymbolSeries> {
    let st = state.store.read().unwrap();
    if symbol.is_empty() { return st.series.first().cloned(); }
    if let Some(s)=st.series.iter().find(|s|series_key(&s.symbol,s.timeframe.label())==symbol){return Some(s.clone());}
    let mut matches=st.series.iter().filter(|s|s.symbol==symbol);
    let first=matches.next()?;
    if matches.next().is_some(){None}else{Some(first.clone())}
}

fn instrument_for(symbol: &str) -> Instrument {
    Instrument::fx(symbol)
}

fn pyp_for(bars: &[Bar]) -> f64 {
    if bars.len() < 2 {
        return 252.0;
    }
    let mut diffs: Vec<i64> = bars.windows(2).map(|w| (w[1].time - w[0].time) / 1000).collect();
    diffs.sort_unstable();
    let secs = diffs[diffs.len() / 2] as f64;
    if secs > 0.0 {
        (365.25 * 24.0 * 3600.0 / secs).min(bars.len() as f64).max(1.0)
    } else {
        252.0
    }
}

fn indicators_json() -> String {
    let list: Vec<_> = sqx_strategy::indicator_catalog()
        .iter()
        .map(|s| json!({ "name": s.name, "label": s.label, "oscillator": s.is_oscillator }))
        .collect();
    json!({ "indicators": list }).to_string()
}

fn catalog_json() -> String {
    let indicators: Vec<_> = sqx_strategy::indicator_catalog()
        .iter()
        .map(|s| {
            let params: Vec<_> = s.params.iter().map(|(n, v)| json!({ "name": n, "value": v })).collect();
            json!({ "name": s.name, "label": s.label, "oscillator": s.is_oscillator, "params": params })
        })
        .collect();
    let mm = json!([
        {"type": "FixedSize", "label": "Fixed size", "params": [{"name": "size", "value": 0.1}]},
        {"type": "CryptoFixedBalancePct", "label": "Crypto fixed % balance", "params": [{"name": "risk", "value": 5}]},
        {"type": "RiskFixedBalancePct", "label": "Risk fixed % balance", "params": [{"name": "risk", "value": 5}]},
        {"type": "AtrRiskBased", "label": "ATR Risk-Based Sizing", "params": [{"name": "risk", "value": 1}, {"name": "atr_period", "value": 14}, {"name": "atr_mult", "value": 2}]},
        {"type": "RiskFixedPctOfAccount", "label": "Risk fixed % of account", "params": [{"name": "risk", "value": 5}, {"name": "stop_loss_points", "value": 100}]},
        {"type": "CryptoFixedAmount", "label": "Crypto fixed amount", "params": [{"name": "amount", "value": 500}]},
        {"type": "FixedAmount", "label": "Fixed amount", "params": [{"name": "amount", "value": 500}]},
        {"type": "CryptoSizeByPrice", "label": "Crypto size by price", "params": [{"name": "risk", "value": 5}]},
        {"type": "StocksSizeByPrice", "label": "Stocks size by price", "params": [{"name": "risk", "value": 5}]},
        {"type": "SimpleMartingale", "label": "Simple Martingale MM", "params": [{"name": "lots_start", "value": 0.1}, {"name": "lots_multiplier", "value": 2}, {"name": "lots_reset", "value": 1}]}
    ]);
    json!({
        "indicators": indicators,
        "comparators": ["Gt", "Gte", "Lt", "Lte", "Eq", "Neq", "CrossAbove", "CrossBelow", "Rising", "Falling"],
        "price_fields": ["Open", "High", "Low", "Close", "Typical", "Volume"],
        "signal_kinds": ["LongEntry", "ShortEntry", "LongExit", "ShortExit"],
        "money_management": mm,
        "trading_options": {"exit_at_end_of_day": false, "exit_on_friday": false, "max_trades_per_day": null, "limit_hours": null}
    })
    .to_string()
}

fn datasources_json(state: &State) -> String {
    let st = state.store.read().unwrap();
    let list: Vec<_> = st
        .list()
        .iter()
        .map(|s| json!({ "key":series_key(&s.symbol,&s.timeframe), "symbol": s.symbol, "timeframe": s.timeframe, "bars": s.bars, "first": s.first, "last": s.last }))
        .collect();
    let default = st.series.first().map(|s| series_key(&s.symbol,s.timeframe.label())).unwrap_or_default();
    json!({ "symbols": list, "default": default }).to_string()
}

fn questdb_symbols_json(state: &State) -> String {
    let client = match &state.questdb {
        None => return json!({ "error": "QuestDB not configured (start with --questdb)" }).to_string(),
        Some(c) => c,
    };
    match client.list_symbols() {
        Ok(syms) => json!({ "symbols": syms }).to_string(),
        Err(e) => json!({ "error": e }).to_string(),
    }
}

fn questdb_load_json(body: &str, state: &State) -> String {
    let client = match &state.questdb {
        None => return json!({ "error": "QuestDB not configured (start with --questdb)" }).to_string(),
        Some(c) => c,
    };
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if symbol.is_empty() {
        return json!({ "error": "symbol required" }).to_string();
    }
    let timeframe: Timeframe = match val.get("timeframe").and_then(|v| v.as_str()).unwrap_or("M5").parse() {
        Ok(t) => t,
        Err(e) => return json!({ "error": e }).to_string(),
    };
    let limit = val.get("limit").and_then(|v| v.as_u64()).unwrap_or(5000).clamp(100, 100_000) as usize;
    match client.fetch_bars(&symbol, timeframe, None, None, Some(limit)) {
        Ok(bars) => {
            let n = bars.len();
            state.store.write().unwrap().add(&symbol, timeframe, bars);
            json!({ "ok": true, "symbol": symbol, "timeframe": timeframe.label(), "bars": n }).to_string()
        }
        Err(e) => json!({ "error": e }).to_string(),
    }
}

fn strategy_key(r: &serde_json::Value) -> Option<String> {
    r.get("strategy").and_then(|s| serde_json::to_string(s).ok())
}

fn dedupe_push(db: &mut Vec<serde_json::Value>, rows: &[serde_json::Value]) {
    let mut seen: std::collections::HashSet<String> = db.iter().filter_map(strategy_key).collect();
    for r in rows {
        if let Some(key) = strategy_key(r) {
            if seen.insert(key) {
                db.push(r.clone());
            }
        }
    }
}

fn databank_push(state: &State, rows: &[serde_json::Value]) {
    let mut db = state.databank.write().unwrap();
    dedupe_push(&mut db, rows);
    drop(db);
    persist_databank(state);
}

fn persist_databank(state: &State) {
    let rows = state.databank.read().unwrap();
    let _ = std::fs::create_dir_all("data");
    let _ = std::fs::write(
        "data/databank.json",
        serde_json::to_string_pretty(&*rows).unwrap_or_default(),
    );
}

fn portfolio_databank_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let (creation_guard,existing)=match reserve_web_research_request("web_portfolio_combination",&val,state){Ok(result)=>result,Err(error)=>return json!({"error":error}).to_string()};
    if let Some(id)=existing{return json!({"recovery_required":true,"message":"This selection already has a saved portfolio study. Inspect Saved research before retrying.","archive":{"started_run_id":id}}).to_string();}
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let quality=match research_data_quality(&series,&val,sqx_data::quality::now_ms()) {Ok(quality)=>quality,Err(error)=>return error.to_string()};
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let cfg = match config_for_series(&val,&series){Ok(config)=>config,Err(error)=>return json!({"error":error}).to_string()};
    if let Err(error)=sqx_backtest::validate_backtest_inputs(&series.bars,&inst,&cfg){return json!({"error":error}).to_string();}
    let mut research_context=json!({"data_quality":quality,"config":cfg,"data_gaps_acknowledged":val.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false),"scope":"Exploratory analysis; portfolio combinations are weighted independent simulations, not shared-account execution or independent validation."});
    let top = val.get("top").and_then(|v| v.as_u64()).unwrap_or(5).clamp(2, 20) as usize;
    let rows = state.databank.read().unwrap().clone();
    let mut strategies: Vec<(String, Strategy)> = Vec::new();let mut selected_rows=Vec::new();
    if let Some(selected) = val.get("strategies") {
        match serde_json::from_value::<Vec<Strategy>>(selected.clone()) {
            Ok(selected) if (2..=20).contains(&selected.len()) => {
                strategies = selected.into_iter().map(|s| (s.name.clone(), s)).collect();
            }
            _ => return json!({"error": "select between 2 and 20 valid strategies"}).to_string(),
        }
    } else {
        for r in rows.iter() {
            let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if let Ok(s) = serde_json::from_value::<Strategy>(r.get("strategy").cloned().unwrap_or(json!({}))) {
                selected_rows.push(r.clone());strategies.push((name, s));
            }
            if strategies.len() >= top {
                break;
            }
        }
    }
    if strategies.len() < 2 {
        return json!({ "error": "need at least 2 databank strategies" }).to_string();
    }
    let names: Vec<String> = strategies.iter().map(|(n, _)| n.clone()).collect();
    research_context["selection_origin"]=json!(if val.get("strategies").is_some(){"explicit_client_selection"}else{"databank_order"});
    research_context["databank_rows"]=json!(selected_rows);
    let pipeline=sqx_pipeline::Pipeline{name:"Selected portfolio composition".into(),tasks:vec![sqx_pipeline::Task{kind:sqx_pipeline::TaskKind::PortfolioCombination{
        names:names.clone(),strategies:strategies.into_iter().map(|(_,strategy)|strategy).collect(),
    }}]};
    let request=json!({"research_kind":"cli_pipeline_started","origin":"web_portfolio_combination","attempt_id":format!("{:032x}",rand::random::<u128>()),"web_request":val,"config":cfg,"seed":0,"periods_per_year":pyp});
    let dataset=json!({"symbol":series.symbol,"timeframe":series.timeframe,"instrument":inst,"calendar":series.calendar,"normalization":series.normalization,"bars":series.bars,"source_context":research_context});
    let specification=json!({"pipeline":pipeline,"generator":GeneratorConfig::default()});
    let started=match retained_engine_identity(state).and_then(|engine|state.archive.save(engine,dataset,specification,request,json!({"status":"execution_not_recorded"}))){Ok(id)=>id,Err(error)=>return json!({"error":format!("Portfolio composition could not be archived: {error}")}).to_string()};
    drop(creation_guard);
    let recovered=match sqx_project::pipeline_archive::recover(&state.archive,&started,true){Ok(result)=>result,Err(error)=>return json!({"error":format!("Portfolio composition stopped: {error}. Inspect Saved research before retrying."),"archive":{"started_run_id":started}}).to_string()};
    let receipt=json!({"started_run_id":started,"run_id":recovered.completion_id});
    let study=match recovered.report.portfolio_combination{Some(study)=>study,None=>return json!({"error":recovered.report.errors.join("; "),"archive":receipt}).to_string()};
    if !study.errors.is_empty(){let failures:Vec<_>=study.components.iter().enumerate().filter(|(_,row)|!row.errors.is_empty()).map(|(index,row)|json!({"strategy":names[index],"errors":row.errors})).collect();return json!({"error":format!("Portfolio contains invalid component simulations: {}",study.errors.join("; ")),"components":failures,"archive":receipt,"research_context":research_context}).to_string();}
    let no_trade_components:Vec<_>=study.components.iter().enumerate().filter(|(_,row)|row.result.trades.is_empty()).map(|(i,_)|names[i].clone()).collect();
    let pf=match study.combined{Some(pf)=>pf,None=>return json!({"error":"Portfolio composition has no combined result","archive":receipt}).to_string()};
    json!({
        "archive":receipt,
        "no_trade_components":no_trade_components,
        "research_context":research_context,
        "account_currency":pf.account_currency,
        "symbol": series.symbol,
        "components": names,
        "metrics": metrics_json(&pf.metrics),
        "correlation": pf.correlation,
        "equity": pf.equity
    })
    .to_string()
}

fn profiles_load() -> serde_json::Value {
    std::fs::read_to_string("data/profiles.json")
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .unwrap_or_else(|| json!({ "brokers": [], "sessions": [] }))
}

fn profiles_store(v: &serde_json::Value) {
    let _ = std::fs::create_dir_all("data");
    let _ = std::fs::write(
        "data/profiles.json",
        serde_json::to_string_pretty(v).unwrap_or_default(),
    );
}

fn profiles_json() -> String {
    profiles_load().to_string()
}

fn profile_kind_key(kind: &str) -> &'static str {
    if kind == "session" { "sessions" } else { "brokers" }
}

fn profile_save_json(body: &str) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let kind = val.get("kind").and_then(|v| v.as_str()).unwrap_or("broker").to_string();
    let profile = match val.get("profile") {
        Some(p) => p.clone(),
        None => return json!({ "error": "profile required" }).to_string(),
    };
    let name = profile.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if name.is_empty() {
        return json!({ "error": "name required" }).to_string();
    }
    let key = profile_kind_key(&kind);
    let mut store = profiles_load();
    if !store.get(key).map(|v| v.is_array()).unwrap_or(false) {
        store[key] = json!([]);
    }
    if let Some(arr) = store.get_mut(key).and_then(|v| v.as_array_mut()) {
        arr.retain(|x| x.get("name").and_then(|v| v.as_str()) != Some(name.as_str()));
        arr.push(profile);
    }
    profiles_store(&store);
    json!({ "ok": true, "name": name }).to_string()
}

fn profile_delete_json(body: &str) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let kind = val.get("kind").and_then(|v| v.as_str()).unwrap_or("broker").to_string();
    let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let key = profile_kind_key(&kind);
    let mut store = profiles_load();
    if let Some(arr) = store.get_mut(key).and_then(|v| v.as_array_mut()) {
        arr.retain(|x| x.get("name").and_then(|v| v.as_str()) != Some(name.as_str()));
    }
    profiles_store(&store);
    json!({ "ok": true }).to_string()
}

fn databank_csv(state: &State) -> String {
    let rows = state.databank.read().unwrap();
    let mut out = String::from("name,profit_factor,net_profit,trades,dd_pct\n");
    for r in rows.iter() {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            r.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            r.get("profit_factor").and_then(|v| v.as_f64()).unwrap_or(0.0),
            r.get("net_profit").and_then(|v| v.as_f64()).unwrap_or(0.0),
            r.get("trades").and_then(|v| v.as_u64()).unwrap_or(0),
            r.get("dd_pct").and_then(|v| v.as_f64()).unwrap_or(0.0),
        ));
    }
    out
}

fn databank_json(state: &State) -> String {
    let rows = state.databank.read().unwrap();
    json!({ "rows": *rows, "count": rows.len() }).to_string()
}

fn databank_clear(state: &State) -> String {
    state.databank.write().unwrap().clear();
    persist_databank(state);
    json!({ "ok": true }).to_string()
}

fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn project_list_json() -> String {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir("data/projects") {
        for e in entries.flatten() {
            if let Some(stem) = e.path().file_stem().and_then(|s| s.to_str()) {
                out.push(stem.to_string());
            }
        }
    }
    out.sort();
    json!({ "projects": out }).to_string()
}

fn project_load_json(name: &str) -> String {
    let safe = safe_name(name);
    match std::fs::read_to_string(format!("data/projects/{safe}.json")) {
        Ok(text) => text,
        Err(e) => json!({ "error": e.to_string() }).to_string(),
    }
}

fn project_save_json(body: &str) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if name.is_empty() {
        return json!({ "error": "name required" }).to_string();
    }
    let strategy = match val.get("strategy") {
        Some(v) => v.clone(),
        None => return json!({ "error": "strategy required" }).to_string(),
    };
    let dir = std::path::Path::new("data/projects");
    if let Err(e) = std::fs::create_dir_all(dir) {
        return json!({ "error": e.to_string() }).to_string();
    }
    let path = dir.join(format!("{}.json", safe_name(&name)));
    match std::fs::write(&path, serde_json::to_string_pretty(&strategy).unwrap_or_default()) {
        Ok(_) => json!({ "ok": true, "name": name }).to_string(),
        Err(e) => json!({ "error": e.to_string() }).to_string(),
    }
}

fn analyze_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let quality=match research_data_quality(&series,&val,sqx_data::quality::now_ms()) {Ok(quality)=>quality,Err(error)=>return error.to_string()};
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let cfg = match config_for_series(&val,&series){Ok(config)=>config,Err(error)=>return json!({"error":error}).to_string()};
    if let Err(error)=sqx_backtest::validate_backtest_inputs(&series.bars,&inst,&cfg){return json!({"error":error}).to_string();}
    let research_context=json!({"data_quality":quality,"config":cfg,"data_gaps_acknowledged":val.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false),"scope":"Exploratory single-strategy analysis on the supplied dataset; no independent validation or live execution parity."});
    let s: Strategy = match serde_json::from_str(body) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let r = backtest(&s, &series.bars, &inst, &cfg);
    if !r.errors.is_empty(){return json!({"error":r.errors.join("; ")}).to_string();}
    let report = sqx_analyzer::analyze(&r, &series.bars, pyp);
    if !report.errors.is_empty(){return json!({"error":report.errors.join("; "),"research_context":research_context}).to_string();}
    json!({"research_context":research_context, "markdown": sqx_analyzer::render_markdown(&report) }).to_string()
}

fn export_json(body: &str) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let label = val.get("engine").and_then(|v| v.as_str()).unwrap_or("mql5");
    let Some(engine) = sqx_export::Engine::from_label(label) else { return json!({"error":"Unknown export engine"}).to_string(); };
    let strategy_val = match val.get("strategy") {
        Some(v) => v.clone(),
        None => return json!({ "error": "strategy required" }).to_string(),
    };
    let s: Strategy = match serde_json::from_value(strategy_val) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let report = sqx_export::export_report(&s, engine);
    json!({ "engine": engine.label(), "code": report.code, "status": report.status, "deployment_ready": false,
        "error": if report.blockers.is_empty() {None} else {Some(report.blockers.join(" "))},
        "blockers": report.blockers, "limitations": report.limitations }).to_string()
}

#[derive(serde::Deserialize)]
#[serde(default)]
struct PipelineInputs {count:usize,keep:usize,seed:u64}
impl Default for PipelineInputs {fn default()->Self{Self{count:300,keep:20,seed:42}}}
impl PipelineInputs {
    fn parse(value:&serde_json::Value)->Result<Self,String>{
        let input:Self=serde_json::from_value(value.clone()).map_err(|e|format!("Invalid pipeline settings: {e}"))?;
        if !(1..=2000).contains(&input.count){return Err("Pipeline count must be between 1 and 2000".into());}
        if !(1..=200).contains(&input.keep) || input.keep>input.count{return Err("Pipeline keep must be between 1 and 200 and cannot exceed count".into());}
        Ok(input)
    }
}

fn pipeline_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let input=match PipelineInputs::parse(&val){Ok(input)=>input,Err(error)=>return json!({"error":error}).to_string()};
    let (creation_guard,existing)=match reserve_web_research_request("web_pipeline",&val,state){Ok(result)=>result,Err(error)=>return json!({"error":error}).to_string()};
    if let Some(id)=existing{return json!({"recovery_required":true,"message":"This request already has a saved pipeline. Inspect Saved research before retrying.","archive":{"started_run_id":id}}).to_string();}
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let quality=match research_data_quality(&series,&val,sqx_data::quality::now_ms()) {Ok(quality)=>quality,Err(error)=>return error.to_string()};
    let research_context=json!({"data_quality":quality,"data_gaps_acknowledged":val.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false),"scope":"Exploratory research; candidate selection and data reuse require separate final-test validation."});
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let cfg = match config_for_series(&val,&series){Ok(config)=>config,Err(error)=>return json!({"error":error}).to_string()};
    if let Err(error)=sqx_backtest::validate_backtest_inputs(&series.bars,&inst,&cfg){return json!({"error":error}).to_string();}
    let count=input.count;
    let keep=input.keep;
    let gen=match gen_config_from(&val){Ok(gen)=>gen,Err(error)=>return json!({"error":error}).to_string()};
    if let Err(error)=gen.validate(){return json!({"error":error}).to_string();}
    let mut pipeline=sqx_pipeline::default_pipeline(count,keep);
    pipeline.tasks.push(sqx_pipeline::Task{kind:sqx_pipeline::TaskKind::EvaluateDatabank});
    let request=json!({"research_kind":"cli_pipeline_started","origin":"web_pipeline","attempt_id":format!("{:032x}",rand::random::<u128>()),"web_request":val,"config":cfg,"seed":input.seed,"periods_per_year":pyp});
    let dataset=json!({"symbol":series.symbol,"timeframe":series.timeframe,"instrument":inst,"calendar":series.calendar,"normalization":series.normalization,"bars":series.bars,"source_context":research_context});
    let started=match retained_engine_identity(state).and_then(|engine|state.archive.save(engine,dataset,json!({"pipeline":pipeline,"generator":gen}),request,json!({"status":"execution_not_recorded"}))){
        Ok(id)=>id,Err(error)=>return json!({"error":format!("Pipeline could not be archived: {error}")}).to_string()
    };
    drop(creation_guard);
    let recovered=match sqx_project::pipeline_archive::recover(&state.archive,&started,true){Ok(result)=>result,Err(error)=>return json!({"error":format!("Pipeline stopped: {error}. Inspect Saved research before retrying."),"archive":{"started_run_id":started}}).to_string()};
    let receipt=json!({"started_run_id":started,"run_id":recovered.completion_id});
    let mut response=render_pipeline_report(&recovered.report,keep,&research_context,||{
        if recovered.report.final_evaluations.len()!=recovered.report.databank.len(){return Err("Final execution evidence does not cover the selected databank".into());}
        recovered.report.final_evaluations.iter().map(|row|{
            if !row.errors.is_empty(){return Err(row.errors.join("; "));}
            let m=&row.metrics;Ok(json!({"name":row.strategy.name,"research_context":research_context,"profit_factor":m.profit_factor,"net_profit":m.net_profit,"trades":row.execution.trades.len(),"dd_pct":m.max_drawdown_pct,"strategy":row.strategy,"source_run_id":receipt["run_id"],"source_started_run_id":started,"source_candidate_index":row.candidate_index}))
        }).collect()
    });
    if response.get("error").is_none(){databank_push(state,response["results"].as_array().expect("computed pipeline rows"));}
    response["archive"]=receipt;response.to_string()
}

fn monte_carlo_samples_json(report:&sqx_robustness::MonteCarloReport)->Vec<serde_json::Value> {
    report.samples.iter().map(|sample|json!({"run_index":sample.run_index,"included_trades":sample.included_trades,"net_profit":sample.net_profit,"max_drawdown":sample.max_drawdown})).collect()
}

#[allow(clippy::too_many_arguments)]
fn compute_pipeline_report(series:&SymbolSeries,inst:&Instrument,cfg:&BacktestConfig,pyp:f64,gen:&GeneratorConfig,count:usize,keep:usize,seed:u64,research_context:&serde_json::Value)->serde_json::Value {
    let pipeline = sqx_pipeline::default_pipeline(count, keep);
    let report = sqx_pipeline::run(
        &pipeline,
        &series.bars,
        &inst,
        &cfg,
        pyp,
        &gen,
        seed,
    );
    render_pipeline_report(&report,keep,research_context,||{
    report
        .databank
        .iter()
        .map(|s| {
            let r = backtest(s, &series.bars, &inst, &cfg);
            if !r.errors.is_empty(){return Err(format!("{}: {}",s.name,r.errors.join("; ")));}
            let m = compute_metrics(&r, series.bars.len(), pyp);
            Ok(json!({
                "name": s.name,"research_context":research_context,
                "profit_factor": m.profit_factor,
                "net_profit": m.net_profit,
                "trades": r.trades.len(),
                "dd_pct": m.max_drawdown_pct,
                "strategy": s
            }))
        })
        .collect()
    })
}

fn render_pipeline_report(report:&sqx_pipeline::PipelineReport,keep:usize,research_context:&serde_json::Value,rows:impl FnOnce()->Result<Vec<serde_json::Value>,String>)->serde_json::Value{
    let retest_reports:Vec<_>=report.retests.iter().map(|row|json!({"step":row.step,"strategy":row.strategy,"requested_runs":row.requested_runs,"seed":row.seed,"skip_probability":0.1,"minimum_positive_pct":row.minimum_positive_pct,"errors":row.errors,"selected":row.selected,"report":row.report.as_ref().map(|mc|json!({"samples":monte_carlo_samples_json(mc),"runs":mc.runs,"net_p5":mc.net_profit_5pct,"net_p50":mc.net_profit_50pct,"net_p95":mc.net_profit_95pct,"worst_net":mc.worst_net_profit,"best_net":mc.best_net_profit,"losing_pct":mc.losing_runs_pct,"positive_pct":mc.positive_runs_pct,"flat_pct":mc.flat_runs_pct,"max_dd_95":mc.max_drawdown_95pct,"errors":mc.errors}))})).collect();
    let build_reports:Vec<_>=report.builds.iter().map(|row|json!({"step":row.step,"candidate_index":row.candidate_index,"strategy":row.evaluation.strategy,"metrics":metrics_json(&row.evaluation.metrics),"errors":row.evaluation.errors,"selected":row.selected,"selection":{"fitness":"profit_factor","min_trades":5,"keep":keep}})).collect();
    let optimization_reports:Vec<_>=report.optimizations.iter().map(|evaluation|json!({"original":evaluation.original,"family":evaluation.family,"walk_forward":evaluation.report,"scope":"Seeded explicit indicator-parameter and SL/PT distance optimization; exploratory because original discovery reused this dataset"})).collect();
    if !report.errors.is_empty(){return json!({"error":report.errors.join("; "),"log":report.log,"retests":retest_reports,"builds":build_reports,"optimizations":optimization_reports});}
    let rows=rows();
    let rows=match rows {Ok(rows)=>rows,Err(error)=>return json!({"error":error,"log":report.log,"retests":retest_reports,"builds":build_reports,"optimizations":optimization_reports})};

    json!({"research_context":research_context,"retests":retest_reports,"builds":build_reports,"optimizations":optimization_reports, "log": report.log, "databank": rows.len(),"results":rows })
}

#[derive(serde::Deserialize)]
#[serde(default)]
struct OptimizationInputs {
    kind:String, count:usize, seed:u64, windows:usize, oos:f64,
    min_is_trades:usize, min_oos_trades:usize,
}
impl Default for OptimizationInputs {
    fn default()->Self{Self{kind:"simple".into(),count:200,seed:42,windows:5,oos:0.3,min_is_trades:10,min_oos_trades:10}}
}
impl OptimizationInputs {
    fn parse(value:&serde_json::Value)->Result<Self,String>{
        let input:Self=serde_json::from_value(value.clone()).map_err(|e|format!("Invalid optimization settings: {e}"))?;
        if !["simple","walk-forward","wf-matrix"].contains(&input.kind.as_str()){return Err("Unknown optimization kind".into());}
        let max=if input.kind=="simple"{2000}else{200};
        if !(1..=max).contains(&input.count){return Err(format!("count must be between 1 and {max} for {}",input.kind));}
        if !(2..=12).contains(&input.windows){return Err("windows must be between 2 and 12".into());}
        if !input.oos.is_finite() || !(0.05..=0.5).contains(&input.oos){return Err("oos must be between 0.05 and 0.5".into());}
        if input.min_is_trades==0 || input.min_oos_trades==0{return Err("Minimum trade counts must be positive".into());}
        Ok(input)
    }
}

fn optimize_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let input=match OptimizationInputs::parse(&val){Ok(input)=>input,Err(error)=>return json!({"error":error}).to_string()};
    let kind=&input.kind;
    let (creation_guard,existing)=match reserve_web_research_request("web_optimization",&val,state){Ok(result)=>result,Err(error)=>return json!({"error":error}).to_string()};
    if let Some(id)=existing{return json!({"recovery_required":true,"message":"This request already has a saved optimization study. Inspect Saved research before retrying.","archive":{"started_run_id":id}}).to_string();}
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let quality=match research_data_quality(&series,&val,sqx_data::quality::now_ms()) {Ok(quality)=>quality,Err(error)=>return error.to_string()};
    let research_context=json!({"data_quality":quality,"data_gaps_acknowledged":val.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false),"scope":"Exploratory research; candidate selection and data reuse require separate final-test validation."});
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let cfg = match config_for_series(&val,&series){Ok(config)=>config,Err(error)=>return json!({"error":error}).to_string()};
    if let Err(error)=sqx_backtest::validate_backtest_inputs(&series.bars,&inst,&cfg){return json!({"error":error}).to_string();}
    let count=input.count;
    let seed=input.seed;
    let gen = match gen_config_from(&val){Ok(gen)=>gen,Err(error)=>return json!({"error":error}).to_string()};
    if let Err(error)=gen.validate(){return json!({"error":error}).to_string();}
    let criteria=sqx_optimizer::WalkForwardCriteria{
        min_is_trades:input.min_is_trades,
        min_oos_trades_per_window:input.min_oos_trades,
    };
    let task=if kind=="simple" {
        sqx_pipeline::TaskKind::Generate{count,min_trades:5}
    }else{
        let cells=if kind=="wf-matrix"{[3usize,4,5,6].into_iter().flat_map(|w|[0.2,0.3,0.4].into_iter().map(move |oos|(w,oos))).collect::<Vec<_>>()}else{vec![(input.windows,input.oos)]};
        sqx_pipeline::TaskKind::WalkForwardStudy{studies:cells.into_iter().map(|(windows,oos_fraction)|sqx_pipeline::WalkForwardSpec{windows,candidates:count,oos_fraction,criteria:criteria.clone()}).collect()}
    };
    let pipeline=sqx_pipeline::Pipeline{name:"Dashboard optimization study".into(),tasks:vec![sqx_pipeline::Task{kind:task}]};
    let request=json!({"research_kind":"cli_pipeline_started","origin":"web_optimization","attempt_id":format!("{:032x}",rand::random::<u128>()),"web_request":val,"config":cfg,"seed":seed,"periods_per_year":pyp});
    let dataset=json!({"symbol":series.symbol,"timeframe":series.timeframe,"instrument":inst,"calendar":series.calendar,"normalization":series.normalization,"bars":series.bars,"source_context":research_context});
    let started=match retained_engine_identity(state).and_then(|engine|state.archive.save(engine,dataset,json!({"pipeline":pipeline,"generator":gen}),request,json!({"status":"execution_not_recorded"}))){
        Ok(id)=>id,Err(error)=>return json!({"error":format!("Optimization could not be archived: {error}")}).to_string()
    };
    drop(creation_guard);
    let recovered=match sqx_project::pipeline_archive::recover(&state.archive,&started,true){Ok(result)=>result,Err(error)=>return json!({"error":format!("Optimization stopped: {error}. Inspect Saved research before retrying."),"archive":{"started_run_id":started}}).to_string()};
    let receipt=json!({"started_run_id":started,"run_id":recovered.completion_id});
    let report=recovered.report;
    if !report.errors.is_empty(){return json!({"error":report.errors.join("; "),"research_context":research_context,"archive":receipt}).to_string();}
    match kind.as_str() {
        "walk-forward"=>{
            let mut response=json!(report.walk_forward_studies[0].report);
            response["kind"]=json!(kind);response["research_context"]=research_context;response["archive"]=receipt;response.to_string()
        },
        "wf-matrix"=>{
            let rows:Vec<_>=report.walk_forward_studies.iter().map(|cell|{let r=&cell.report;json!({
                "windows":cell.specification.windows,"oos":cell.specification.oos_fraction,
                "net":r.oos_net_profit,"pf":r.oos_profit_factor,"trades":r.oos_trades,"dd_pct":r.oos_max_dd_pct,
                "evidence_sufficient":r.evidence_sufficient,"candidates_attempted":r.candidates_attempted,"errors":r.errors
            })}).collect();
            json!({"research_context":research_context,"kind":kind,"seed":seed,"candidates_per_window":count,"rows":rows,"archive":receipt}).to_string()
        },
        _=>{
            let rows:Vec<_>=report.databank.iter().take(20).filter_map(|strategy|report.builds.iter().find(|row|row.evaluation.strategy==*strategy).map(|row|{
                let m=&row.evaluation.metrics;json!({"name":strategy.name,"research_context":research_context,"profit_factor":m.profit_factor,"net_profit":m.net_profit,"trades":m.number_of_trades,"dd_pct":m.max_drawdown_pct,"strategy":strategy})
            })).collect();
            databank_push(state,&rows);
            json!({"research_context":research_context,"kind":kind,"results":rows,"archive":receipt}).to_string()
        }
    }
}

fn import_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let csv = val.get("csv").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if symbol.is_empty() || csv.is_empty() {
        return json!({ "error": "symbol and csv are required" }).to_string();
    }
    let bars = match sqx_data::read_csv_bars_str(&csv) {
        Ok(b) => b,
        Err(e) => return json!({ "error": e }).to_string(),
    };
    let timeframe=match val.get("timeframe") {Some(v)=>match serde_json::from_value::<Timeframe>(v.clone()){Ok(tf)=>tf,Err(e)=>return json!({"error":e.to_string()}).to_string()},None=>Timeframe::M1};
    let calendar=match val.get("calendar").filter(|v|!v.is_null()) {Some(v)=>match serde_json::from_value::<sqx_data::quality::MarketCalendar>(v.clone()){Ok(c)=>Some(c),Err(e)=>return json!({"error":e.to_string()}).to_string()},None=>None};
    let normalization=match val.get("normalization_bundle").filter(|v|!v.is_null()) {
        Some(v)=>match serde_json::from_value::<sqx_data::normalization::Bundle>(v.clone()) {
            Ok(bundle)=>{if let Err(error)=bundle.verify_bars(&bars,normalization_bar_duration(timeframe)) {return json!({"error":error}).to_string();} Some(bundle)},
            Err(error)=>return json!({"error":error.to_string()}).to_string(),
        },None=>None,
    };
    let quality=sqx_data::quality::audit(&bars,timeframe,sqx_data::quality::now_ms(),calendar.as_ref());
    if !quality.errors.is_empty(){return json!({"error":quality.errors.join("; "),"data_quality":quality}).to_string();}
    let n = bars.len();
    let series=SymbolSeries{symbol:symbol.clone(),timeframe,calendar,normalization,bars};
    let snapshot_id=match sqx_project::datasets::Datasets::new(state.archive.root()).save(&series) {
        Ok(id)=>id,Err(error)=>return json!({"error":format!("Dataset could not be saved: {error}")}).to_string(),
    };
    let mut store=state.store.write().unwrap();store.series.retain(|s|!(s.symbol==symbol && s.timeframe==timeframe));store.series.push(series);
    json!({ "ok": true, "symbol": symbol, "key":series_key(&symbol,timeframe.label()), "bars": n, "snapshot_id":snapshot_id, "data_quality":quality }).to_string()
}

fn saved_datasets_json(action:&str,body:&str,state:&State)->String{
    let run=||->Result<serde_json::Value,String>{
        let saved=sqx_project::datasets::Datasets::new(state.archive.root());
        if action=="list" {return Ok(json!({"datasets":saved.list()?}));}
        let input:serde_json::Value=serde_json::from_str(body).map_err(|e|e.to_string())?;
        let id=input["id"].as_str().ok_or("saved dataset id is required")?;
        let series=saved.load(id)?;
        let response=json!({"ok":true,"snapshot_id":id,"symbol":series.symbol,"bars":series.bars.len(),"key":series_key(&series.symbol,series.timeframe.label())});
        let mut store=state.store.write().unwrap();store.series.retain(|s|!(s.symbol==series.symbol && s.timeframe==series.timeframe));store.series.push(series);
        Ok(response)
    };
    run().unwrap_or_else(|error|json!({"error":error})).to_string()
}

fn delete_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if symbol.is_empty() {
        return json!({ "error": "symbol required" }).to_string();
    }
    let target=match find_series(state,&symbol){Some(s)=>s,None=>return json!({"error":"unknown or ambiguous dataset"}).to_string()};
    state.store.write().unwrap().series.retain(|s| !(s.symbol == target.symbol && s.timeframe == target.timeframe));
    json!({ "ok": true }).to_string()
}

fn data_json(state: &State, symbol: &str) -> String {
    match find_series(state, symbol) {
        Some(s) => json!({
            "symbol": s.symbol,
            "timeframe": s.timeframe,
            "bars": s.bars.len(),
            "fingerprint": bars_fingerprint(&s.bars),
            "calendar":s.calendar,
            "normalization":normalization_status(&s),
            "data_quality":sqx_data::quality::audit(&s.bars,s.timeframe,sqx_data::quality::now_ms(),s.calendar.as_ref()),
            "first_time": s.bars.first().map(|b| b.time).unwrap_or(0),
            "last_time": s.bars.last().map(|b| b.time).unwrap_or(0)
        })
        .to_string(),
        None => json!({ "error": "no data" }).to_string(),
    }
}

fn metrics_json(m: &Metrics) -> serde_json::Value {
    json!({
        "net_profit": m.net_profit,
        "profit_factor": m.profit_factor,
        "return_dd": m.return_dd_ratio,
        "max_drawdown_pct": m.max_drawdown_pct,
        "number_of_trades": m.number_of_trades,
        "winning_pct": m.winning_pct,
        "avg_trade": m.avg_trade,
        "sharpe": m.sharpe_ratio,
        "sqn": m.sqn,
        "cagr": m.cagr,
        "stability": m.stability,
        "exposure_pct": m.exposure_pct
    })
}

fn gen_config_from(val:&serde_json::Value)->Result<GeneratorConfig,String> {
    let f=|key:&str,default:f64| match val.get(key){None=>Ok(default),Some(v)=>v.as_f64().ok_or_else(||format!("{key} must be numeric"))};
    let u=|key:&str,default:usize| match val.get(key){None=>Ok(default),Some(v)=>v.as_u64().and_then(|n|usize::try_from(n).ok()).ok_or_else(||format!("{key} must be a nonnegative integer"))};
    let flag=|key:&str| match val.get(key){None=>Ok(true),Some(v)=>v.as_bool().ok_or_else(||format!("{key} must be boolean"))};
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Range {indicator:String,param:String,min:f64,max:f64}
    let ranges:Vec<Range>=match val.get("param_ranges"){None=>Vec::new(),Some(value)=>serde_json::from_value(value.clone()).map_err(|e|format!("Invalid parameter ranges: {e}"))?};
    let config=GeneratorConfig{min_conditions:u("min_conditions",1)?,max_conditions:u("max_conditions",3)?,
        sl_points:(f("sl_min",0.0)?,f("sl_max",200.0)?),pt_points:(f("pt_min",0.0)?,f("pt_max",400.0)?),
        fixed_size:f("size",0.1)?,long:flag("long")?,short:flag("short")?,
        param_ranges:ranges.into_iter().map(|r|(r.indicator,r.param,r.min,r.max)).collect()};
    config.validate()?;Ok(config)
}

fn config_from(val:&serde_json::Value)->Result<BacktestConfig,String> {
    fn field<T:serde::de::DeserializeOwned>(value:&serde_json::Value,key:&str,default:T)->Result<T,String>{
        match value.get(key){None=>Ok(default),Some(raw)=>serde_json::from_value(raw.clone()).map_err(|error|format!("Invalid {key}: {error}"))}
    }
    let config=BacktestConfig{
        initial_deposit:field(val,"deposit",10000.0)?,spread_points:field(val,"spread",0.0)?,
        slippage_points:field(val,"slippage",0.0)?,commission_per_lot:field(val,"commission",0.0)?,
        bar_duration_ms:field(val,"bar_duration_ms",None)?,allow_tick_fallback:field(val,"allow_tick_fallback",false)?,
        intrabar_policy:field(val,"intrabar_policy",Default::default())?,account:field(val,"account",Default::default())?,
    };
    config.account.validate()?;
    Ok(config)
}

// Bind fresh research to dataset metadata, never to a caller-invented bar width.
// Archived resolved configurations are replayed unchanged under their engine identity.
fn config_for_series(value:&serde_json::Value,series:&SymbolSeries)->Result<BacktestConfig,String> {
    let mut config=config_from(value)?;
    let duration=match series.timeframe {
        Timeframe::Tick=>return Err("Tick timeframe cannot describe OHLC source candles".into()),
        Timeframe::MN1=>None, // Calendar months must never become nominal 30-day candles.
        timeframe=>timeframe.minutes().and_then(|minutes|minutes.checked_mul(60000)),
    };
    if config.bar_duration_ms.is_some() && config.bar_duration_ms!=duration {
        return Err(format!("bar_duration_ms conflicts with source timeframe {}",series.timeframe));
    }
    config.bar_duration_ms=duration;
    Ok(config)
}

// A single gate keeps dashboard backtests, stress tests and assistant research
// consistent. Gap acknowledgment never overrides malformed/incomplete candles.
fn normalization_bar_duration(timeframe:Timeframe)->Option<i64> {
    // The normalization format describes fixed-width candles only. Calendar
    // months cannot be verified by substituting a nominal thirty-day duration.
    match timeframe {Timeframe::MN1|Timeframe::Tick=>None,tf=>tf.minutes().and_then(|m|m.checked_mul(60000))}
}
fn normalization_status(series:&SymbolSeries)->serde_json::Value {
    match &series.normalization {
        None=>json!({"status":"not_supplied","scope":"No clock-normalization evidence supplied; UTC timestamps are user-declared."}),
        Some(bundle)=>match bundle.verify_bars(&series.bars,normalization_bar_duration(series.timeframe)) {
            Ok(verification)=>json!({"status":"verified_transformation","verification":verification}),
            Err(error)=>json!({"status":"invalid","error":error}),
        },
    }
}

fn research_data_quality(series:&SymbolSeries, request:&serde_json::Value, as_of_ms:i64) -> Result<sqx_data::quality::QualityReport,serde_json::Value> {
    let normalization=normalization_status(series);
    if normalization["status"]=="invalid" {return Err(json!({"error":normalization["error"],"normalization":normalization}));}

    let quality=sqx_data::quality::audit(&series.bars,series.timeframe,as_of_ms,series.calendar.as_ref());
    if !quality.errors.is_empty(){return Err(json!({"error":quality.errors.join("; "),"data_quality":quality}));}
    if (quality.missing_open_intervals>0 || quality.unclassified_intervals>0 || quality.irregular_steps>0)
        && !request.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false) {
        return Err(json!({"error":"Unresolved data gaps or irregular timestamps. Inspect the data-quality report and calendar; explicitly allow_data_gaps only for exploratory runs.","data_quality":quality}));
    }
    Ok(quality)
}

fn research_restore_json(body:&str,state:&State)->String {
    let result=(||->Result<String,String>{
        let bundle:sqx_project::archive::ResearchBundle=serde_json::from_str(body).map_err(|e|e.to_string())?;
        let _request_guard=if bundle.run.request["research_kind"]=="cli_pipeline_started" && matches!(bundle.run.request["origin"].as_str(),Some("web_robustness"|"web_portfolio"|"web_portfolio_combination"|"web_optimization"|"web_pipeline"|"web_generation")) {
            let (guard,existing)=reserve_web_research_request(bundle.run.request["origin"].as_str().unwrap(),&bundle.run.request["web_request"],state)?;
            if existing.is_some_and(|id|id!=bundle.run_id){return Err("Restored stress-test request key conflicts with an existing study".into());}
            guard
        }else{None};
        state.archive.restore(&bundle)
    })();
    match result{Ok(id)=>json!({"run_id":id}).to_string(),Err(error)=>json!({"error":error}).to_string()}
}

// Serialize creation for a caller-supplied request identity. The identity is
// carried inside the immutable start record, so backup/restore retains it.
fn reserve_stress_request(value:&serde_json::Value,state:&State)->Result<(Option<std::fs::File>,Option<String>),String>{
    reserve_web_research_request("web_robustness",value,state)
}
fn reserve_web_research_request(origin:&str,value:&serde_json::Value,state:&State)->Result<(Option<std::fs::File>,Option<String>),String>{
    let Some(key)=value.get("request_key") else{return Ok((None,None));};
    let key=key.as_str().ok_or("Invalid stress-test request key")?;
    let file=sqx_project::pipeline_archive::stress_request_lock(&state.archive,key)?;
    let mut existing=None;
    for id in state.archive.list()? {
        let record=state.archive.load(&id)?;
        if record.run.request["research_kind"]=="cli_pipeline_started" && matches!(record.run.request["origin"].as_str(),Some("web_robustness"|"web_portfolio"|"web_portfolio_combination"|"web_optimization"|"web_pipeline"|"web_generation")) && record.run.request["web_request"]["request_key"]==key {
            if record.run.request["origin"]!=origin || record.run.request["web_request"]!=*value{return Err("Research request key already belongs to different inputs or workflow".into());}
            if existing.replace(id).is_some(){return Err("Conflicting studies share this stress-test request key".into());}
        }
    }
    Ok((Some(file),existing))
}

fn robustness_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let (creation_guard,existing)=match reserve_stress_request(&val,state){Ok(result)=>result,Err(error)=>return json!({"error":error}).to_string()};
    if let Some(id)=existing {
        return json!({"recovery_required":true,"message":"This request already has a saved study. Inspect Saved research runs to check its status or resume; no additional study was started.","archive":{"started_run_id":id}}).to_string();
    }
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let s: Strategy = match serde_json::from_str(body) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let quality=match research_data_quality(&series,&val,sqx_data::quality::now_ms()) {Ok(quality)=>quality,Err(error)=>return error.to_string()};
    let inst = instrument_for(&series.symbol);
    let base = match config_for_series(&val,&series){Ok(config)=>config,Err(error)=>return json!({"error":error}).to_string()};
    let spec=RobustnessSpec{config:base,scenarios:vec![(0.0,0.0,0.0),(10.0,0.0,0.0),(0.0,5.0,0.0),(10.0,5.0,0.0),(10.0,5.0,7.0),(20.0,10.0,10.0)],runs:1000,skip_probability:0.1,seed:42,context:json!({"data_quality":quality,"data_gaps_acknowledged":val.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false)})};
    let pipeline=sqx_pipeline::Pipeline{name:format!("{} — stress test",s.name),tasks:vec![sqx_pipeline::Task{kind:sqx_pipeline::TaskKind::Sensitivity{
        strategy:s,scenarios:spec.scenarios.clone(),mc_runs:spec.runs,mc_seed:spec.seed,skip_probability:spec.skip_probability,
    }}]};
    let request=json!({"research_kind":"cli_pipeline_started","origin":"web_robustness","attempt_id":format!("{:032x}",rand::random::<u128>()),
        "web_request":val,"config":spec.config,"seed":spec.seed,"periods_per_year":pyp_for(&series.bars)});
    let dataset=json!({"symbol":series.symbol,"timeframe":series.timeframe,"instrument":inst,"calendar":series.calendar,
        "normalization":series.normalization,"bars":series.bars,"source_context":spec.context});
    let specification=json!({"pipeline":pipeline,"generator":GeneratorConfig::default()});
    let started=match retained_engine_identity(state).and_then(|engine|state.archive.save(engine,dataset,specification,request,json!({"status":"execution_not_recorded"}))) {
        Ok(id)=>id,Err(error)=>return json!({"error":format!("Stress test could not be started in the research archive: {error}")}).to_string()
    };
    drop(creation_guard);
    let recovered=match sqx_project::pipeline_archive::recover(&state.archive,&started,true){
        Ok(result)=>result,Err(error)=>return json!({"error":format!("Stress test stopped: {error}. Inspect Saved research before retrying."),"archive":{"started_run_id":started}}).to_string()
    };
    let mut response=match &recovered.report.robustness {
        Some(study)=>match &study.monte_carlo {
            Some(mc)=>render_robustness_report(&study.base,&study.what_if,mc,&spec),
            None=>json!({"error":study.base.errors.join("; ")}),
        },
        None=>json!({"error":recovered.report.errors.join("; ")}),
    };
    response["archive"]=json!({"run_id":recovered.completion_id,"started_run_id":started});response.to_string()
}

#[derive(serde::Serialize,serde::Deserialize)]
struct RobustnessSpec {
    config:BacktestConfig,
    scenarios:Vec<(f64,f64,f64)>,
    runs:usize,
    skip_probability:f64,
    seed:u64,
    context:serde_json::Value,
}
fn compute_robustness_report(s:&Strategy,series:&SymbolSeries,inst:&Instrument,spec:&RobustnessSpec)->serde_json::Value {
    if spec.runs==0 || spec.runs>10000 || spec.scenarios.len()>100 {return json!({"error":"Invalid robustness workload"});}
    let r = backtest(s, &series.bars, inst, &spec.config);
    if !r.errors.is_empty(){return json!({"error":r.errors.join("; ")});}
    let rows = what_if(s, &series.bars, inst, &spec.config, &spec.scenarios);
    let mc = monte_carlo_trade_manipulation(&r.trades, spec.runs, spec.skip_probability, spec.seed);
    render_robustness_report(&r,&rows,&mc,spec)
}

fn render_robustness_report(r:&BacktestResult,rows:&[sqx_robustness::WhatIfRow],mc:&sqx_robustness::MonteCarloReport,spec:&RobustnessSpec)->serde_json::Value {
    json!({
        "data_quality":spec.context["data_quality"],
        "data_gaps_acknowledged":spec.context["data_gaps_acknowledged"],
        "interpretation":"Exploratory sensitivity analysis; resampling observed trades does not establish independence, statistical power or live execution parity.",
        "what_if": rows.iter().map(|w| json!({
            "spread": w.spread, "slippage": w.slippage, "commission": w.commission,
            "net_profit": w.net_profit, "profit_factor": w.profit_factor,"errors":w.errors
        })).collect::<Vec<_>>(),
        "mc": {
            "errors":mc.errors,
            "runs": mc.runs,
            "seed":spec.seed,"skip_probability":spec.skip_probability,"samples":monte_carlo_samples_json(&mc),
            "net_p5": mc.net_profit_5pct,
            "net_p50": mc.net_profit_50pct,
            "net_p95": mc.net_profit_95pct,
            "losing_pct": mc.losing_runs_pct,
            "positive_pct":mc.positive_runs_pct,"flat_pct":mc.flat_runs_pct,
            "max_dd_95": mc.max_drawdown_95pct
        },
        "trades": r.trades.len()
    })
}

fn assistant_config_file() -> std::path::PathBuf {
    std::path::Path::new("data").join("assistant.json")
}

/// The persisted (UI-saved) assistant settings, if any.
fn assistant_config_load() -> serde_json::Value {
    std::fs::read_to_string(assistant_config_file())
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .unwrap_or_else(|| json!({}))
}

/// Normalize the persisted assistant file into `(active_provider, profiles)`.
/// Accepts the older flat `{base_url, model, api_key}` format and migrates it
/// in memory into a single `Default` profile, so nothing is lost on upgrade.
fn assistant_profiles(saved: &serde_json::Value) -> (String, serde_json::Map<String, serde_json::Value>) {
    let mut profiles: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    if let Some(p) = saved.get("profiles").and_then(|v| v.as_object()) {
        for (k, v) in p {
            profiles.insert(k.clone(), v.clone());
        }
    }
    if profiles.is_empty() {
        let base = saved.get("base_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let model = saved.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let key = saved.get("api_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !base.is_empty() || !model.is_empty() || !key.is_empty() {
            let name = saved
                .get("provider")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    // Best-effort friendly name so the dropdown can select it.
                    let b = base.to_lowercase();
                    if b.contains("deepseek") { "DeepSeek".to_string() }
                    else if b.contains("groq") { "Groq".to_string() }
                    else if b.contains("openai") { "OpenAI".to_string() }
                    else if b.contains("openrouter") { "OpenRouter".to_string() }
                    else if b.contains("mistral") { "Mistral".to_string() }
                    else if b.contains("x.ai") { "xAI".to_string() }
                    else { "Default".to_string() }
                });
            profiles.insert(name, json!({ "base_url": base, "model": model, "api_key": key }));
        }
    }
    let active = saved.get("active").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let active = if profiles.contains_key(&active) {
        active
    } else {
        profiles.keys().next().cloned().unwrap_or_default()
    };
    (active, profiles)
}

/// Resolve the LLM config for the active profile: saved values take precedence
/// over env vars, which take precedence over auto-detect (Ollama, else Groq).
fn llm_config() -> sqx_assistant::LlmConfig {
    let saved = assistant_config_load();
    let (active, profiles) = assistant_profiles(&saved);
    let profile = profiles.get(&active).cloned().unwrap_or_else(|| json!({}));
    let saved_base = profile.get("base_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let saved_model = profile.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let saved_key = profile.get("api_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let has_saved_base = !saved_base.is_empty();

    let base_url = if has_saved_base {
        saved_base
    } else if let Ok(v) = std::env::var("SQX_LLM_BASE_URL") {
        v
    } else {
        let addr = "127.0.0.1:11434".parse().unwrap();
        if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(250)).is_ok() {
            "http://127.0.0.1:11434/v1".to_string()
        } else {
            "https://api.groq.com/openai/v1".to_string()
        }
    };
    let model = if !saved_model.is_empty() {
        saved_model
    } else if let Ok(v) = std::env::var("SQX_LLM_MODEL") {
        v
    } else if base_url.contains("11434") {
        "llama3.1".to_string()
    } else {
        "llama-3.3-70b-versatile".to_string()
    };
    // A saved endpoint override must not silently inherit an unrelated
    // environment key: the env key belongs to the default (env/auto-detected)
    // endpoint, so only fall back to it when no saved base_url overrides it.
    let api_key = if !saved_key.is_empty() {
        saved_key
    } else if !has_saved_base {
        std::env::var("SQX_LLM_API_KEY").unwrap_or_default()
    } else {
        String::new()
    };
    sqx_assistant::LlmConfig { base_url, model, api_key, max_rounds: 6 }
}

/// Save one provider's profile and make it the active one. An empty `api_key`
/// means "keep the key already stored for this provider" — the field is never
/// pre-filled, so leaving it blank must not wipe a working secret.
///
/// Security: a stored key is only carried over when the endpoint is unchanged.
/// Reusing a secret against a different base_url would let a config change
/// silently redirect the credential to another host, so changing the endpoint
/// with a blank key clears it (the caller must re-enter it).
fn assistant_config_save_json(body: &str) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let mut provider = val.get("provider").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let base_url = val.get("base_url").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let model = val.get("model").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let api_key = val.get("api_key").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if provider.is_empty() {
        provider = "Default".to_string();
    }

    let saved = assistant_config_load();
    let (_, mut profiles) = assistant_profiles(&saved);
    let existing = profiles.get(&provider).cloned().unwrap_or_else(|| json!({}));
    let existing_key = existing.get("api_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let existing_base = existing.get("base_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
    // Carry the key over only when the endpoint did not change.
    let stored_key = if api_key.is_empty() {
        if base_url == existing_base { existing_key } else { String::new() }
    } else {
        api_key
    };
    let api_key_set = !stored_key.is_empty();
    profiles.insert(provider.clone(), json!({ "base_url": base_url, "model": model, "api_key": stored_key }));

    let mut out = serde_json::Map::new();
    out.insert("active".to_string(), json!(provider.clone()));
    out.insert("profiles".to_string(), serde_json::Value::Object(profiles));
    let out = serde_json::Value::Object(out);
    let _ = std::fs::create_dir_all("data");
    let _ = std::fs::write(assistant_config_file(), serde_json::to_string_pretty(&out).unwrap_or_default());
    json!({ "ok": true, "provider": provider, "api_key_set": api_key_set }).to_string()
}

fn assistant_config_json() -> String {
    let saved = assistant_config_load();
    let (active, profiles) = assistant_profiles(&saved);
    let mut list = serde_json::Map::new();
    for (name, p) in &profiles {
        let key = p.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
        list.insert(
            name.clone(),
            json!({
                "base_url": p.get("base_url").and_then(|v| v.as_str()).unwrap_or(""),
                "model": p.get("model").and_then(|v| v.as_str()).unwrap_or(""),
                "api_key_set": !key.is_empty(),
            }),
        );
    }
    // The effective config (with env/auto fallback) is what actually runs.
    let cfg = llm_config();
    json!({
        "provider": active,
        "base_url": cfg.base_url,
        "model": cfg.model,
        "api_key_set": !cfg.api_key.is_empty(),
        "profiles": serde_json::Value::Object(list),
    })
    .to_string()
}

fn assistant_json(body: &str, state: &State) -> String {
    let _permit=match state.assistant_gate.try_lock() {
        Ok(permit)=>permit,
        Err(std::sync::TryLockError::WouldBlock)=>return json!({"error":"An assistant request is already running. Wait for it to finish before starting another."}).to_string(),
        Err(std::sync::TryLockError::Poisoned(_))=>return json!({"error":"Assistant stopped unexpectedly; restart the server before retrying."}).to_string(),
    };
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let question = val.get("question").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if question.is_empty() {
        return json!({ "error": "missing question" }).to_string();
    }
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let simulation = match config_for_series(&val,&series){Ok(config)=>config,Err(error)=>return json!({"error":error}).to_string()};
    let quality=match research_data_quality(&series,&val,sqx_data::quality::now_ms()) {Ok(quality)=>quality,Err(error)=>return error.to_string()};
    let cfg = llm_config();
    let engine=match retained_engine_identity(state) {Ok(engine)=>engine,Err(error)=>return json!({"error":error}).to_string()};
    let mut a = sqx_assistant::Assistant::new(cfg, &series.symbol, series.bars.clone(), inst, pyp).with_backtest_config(simulation)
        .with_trial_archive(state.archive.clone(),engine,json!({"timeframe":series.timeframe,"calendar":series.calendar,"data_quality":quality}));
    let answer=a.run(&question);
    let mut response=match &answer {
        Ok(answer) => json!({ "answer": answer,"research_context":a.research_context(),"data_quality":quality,"candidate_evaluations":a.candidate_evaluations(),"detail_evaluations":a.detail_evaluations() }),
        Err(e) => json!({ "error": e,"research_context":a.research_context(),"data_quality":quality,"candidate_evaluations":a.candidate_evaluations(),"detail_evaluations":a.detail_evaluations() }),
    };
    if !a.candidate_evaluations().is_empty() {
        match archive_assistant_candidates(state,&series,a.research_context(),json!(quality),a.candidate_evaluations(),a.detail_evaluations(),a.trial_executions(),answer.is_ok()) {
            Ok(archive)=>response["archive"]=archive,
            Err(error)=>response["error"]=json!(format!("Assistant evidence could not be archived: {error}")),
        }
    }
    response.to_string()
}

fn archive_assistant_candidates(state:&State,series:&SymbolSeries,context:serde_json::Value,quality:serde_json::Value,candidates:&[serde_json::Value],details:&[serde_json::Value],executions:&[serde_json::Value],completed:bool)->Result<serde_json::Value,String>{
    let dataset=json!(ArchivedDataset{symbol:series.symbol.clone(),timeframe:series.timeframe,instrument:instrument_for(&series.symbol),calendar:series.calendar.clone(),normalization:series.normalization.clone(),bars:series.bars.clone()});
    let request=json!({"research_kind":"assistant_candidates","research_context":context,"data_quality":quality});
    // Provider prose, prompts and configuration are deliberately not archive inputs.
    let result=json!({"candidate_evaluations":candidates,"detail_evaluations":details,"trial_executions":executions,"session_completed":completed,"scope":"Generation and details summaries with full executed trial results. Per-trial checkpoints preserve inputs/results; provider-conversation resume and independent validation are unavailable."});
    let engine=retained_engine_identity(state)?;
    let id=state.archive.save(engine.clone(),dataset,json!({"name":"AI candidate research","kind":"assistant_candidates"}),request,result)?;
    let stored=state.archive.load(&id)?;
    Ok(json!({"run_id":id,"dataset_id":stored.run.dataset_id,"engine_id":engine,"replay_available":false}))
}

// Stable, non-cryptographic identity for detecting changes to the loaded bars.
// Encode explicitly rather than relying on Rust's unspecified default hasher.
fn bars_fingerprint(bars: &[Bar]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for b in bars {
        for word in [b.time as u64, b.open.to_bits(), b.high.to_bits(), b.low.to_bits(), b.close.to_bits(), b.volume.to_bits()] {
            for byte in word.to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    format!("fnv1a64:{hash:016x}")
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ArchivedDataset {
    symbol: String,
    timeframe: Timeframe,
    instrument: Instrument,
    #[serde(default)]
    calendar: Option<sqx_data::quality::MarketCalendar>,
    #[serde(default)]
    normalization: Option<sqx_data::normalization::Bundle>,
    bars: Vec<Bar>,
}

fn engine_identity() -> Result<String, String> {
    static ID: std::sync::OnceLock<Result<String,String>> = std::sync::OnceLock::new();
    ID.get_or_init(|| std::env::current_exe().map_err(|e|e.to_string())
        .and_then(|p|std::fs::read(p).map_err(|e|e.to_string()))
        .map(|bytes|sqx_project::archive::content_id(&bytes))).clone()
}

fn retained_engine_identity(state:&State)->Result<String,String>{
    let expected=engine_identity()?;
    let bytes=std::fs::read(std::env::current_exe().map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    state.archive.retain_engine(&bytes,&expected)
}

fn archived_backtest_json(body: &str, state: &State) -> String {
    let execute = || -> Result<serde_json::Value,String> {
        let value: serde_json::Value=serde_json::from_str(body).map_err(|e|e.to_string())?;
        let strategy: Strategy=serde_json::from_value(value.clone()).map_err(|e|e.to_string())?;
        let symbol=value.get("symbol").and_then(|v|v.as_str()).unwrap_or("");
        let series=find_series(state,symbol).ok_or("no data")?;
        let instrument=instrument_for(&series.symbol);
        let mut request=serde_json::to_value(&strategy).map_err(|e|e.to_string())?;
        request["symbol"]=json!(series.symbol);
        request["_resolved_config"]=json!(config_for_series(&value,&series)?);
        request["_data_as_of_ms"]=json!(sqx_data::quality::now_ms());
        request["allow_data_gaps"]=json!(value.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false));
        request["compare_paths"]=json!(value.get("compare_paths").and_then(|v|v.as_bool()).unwrap_or(false));
        let mut report:serde_json::Value=serde_json::from_str(&backtest_series_json(&request.to_string(),&series,&instrument)).map_err(|e|e.to_string())?;
        if report.get("error").is_some(){return Ok(report);}
        let dataset=json!(ArchivedDataset{symbol:series.symbol,timeframe:series.timeframe,instrument,calendar:series.calendar,normalization:series.normalization,bars:series.bars});
        let engine=retained_engine_identity(state)?;
        let id=state.archive.save(engine.clone(),dataset,json!(strategy),request,report.clone())?;
        let stored=state.archive.load(&id)?;
        report["archive"]=json!({"run_id":id,"dataset_id":stored.run.dataset_id,"strategy_id":stored.run.strategy_id,"engine_id":engine});
        Ok(report)
    };
    execute().unwrap_or_else(|e|json!({"error":format!("Research run could not be recorded: {e}")})).to_string()
}

fn pipeline_action_json(action:&str,body:&str,state:&State)->String {
    let result=(||->Result<serde_json::Value,String>{
        let value:serde_json::Value=serde_json::from_str(body).map_err(|e|e.to_string())?;
        if action=="restore"{return sqx_project::pipeline_archive::import(&state.archive,value);}
        let id=value["run_id"].as_str().ok_or("missing pipeline start ID")?;
        match action {
            "status"=>{
                let mut status=sqx_project::pipeline_archive::summary(&state.archive,id)?;
                status["engine_matches"]=json!(status["engine_id"]==engine_identity()?);Ok(status)
            },
            "backup"=>sqx_project::pipeline_archive::export(&state.archive,id),
            "replay"=>{
                let recovered=sqx_project::pipeline_archive::recover(&state.archive,id,value["resume"]==true)?;
                Ok(json!({"started_run_id":id,"completion_id":recovered.completion_id,"matches":true,
                    "replayed_events":recovered.replayed,"appended_events":recovered.appended,
                    "execution_valid":recovered.report.errors.is_empty(),"errors":recovered.report.errors,
                    "scope":"Exact-engine replay; not independent validation or untouched holdout evidence."}))
            },
            _=>Err("unknown pipeline action".into()),
        }
    })();
    match result{Ok(value)=>value.to_string(),Err(error)=>json!({"error":error}).to_string()}
}

fn research_list_json(state:&State)->String{
    let list=||->Result<Vec<serde_json::Value>,String>{
        state.archive.list()?.iter().map(|id|{
            let bundle=state.archive.load(id)?;
            let kind=bundle.run.request.get("research_kind").and_then(|v|v.as_str()).unwrap_or("backtest");
            let pipeline=matches!(kind,"cli_pipeline_started"|"cli_pipeline_event"|"cli_pipeline");
            let tasks=bundle.strategy["pipeline"]["tasks"].as_array();
            let workflow_kind=if pipeline {
                match tasks.filter(|tasks|tasks.len()==1).and_then(|tasks|tasks.first()).map(|task|&task["kind"]) {
                    Some(task) if task.get("Portfolio").is_some() || task.get("PortfolioCombination").is_some()=>"portfolio",
                    Some(task) if task.get("Backtest").is_some()=>"backtest",
                    Some(task) if task.get("Generate").is_some()=>"generation",
                    Some(task) if task.get("DiscoverWalkForward").is_some() || task.get("WalkForwardStudy").is_some()=>"walk_forward",
                    Some(task) if task.get("Sensitivity").is_some_and(|s|s["scenarios"].as_array().is_some_and(|rows|rows.is_empty()))=>"monte_carlo",
                    Some(task) if task.get("Robustness").is_some() || task.get("Sensitivity").is_some()=>"robustness",
                    _=>"pipeline",
                }
            }else{""};
            let name=if pipeline{json!(bundle.strategy["pipeline"]["name"].as_str().filter(|s|!s.trim().is_empty()).unwrap_or("CLI pipeline"))}else{bundle.strategy["name"].clone()};
            let symbol=bundle.dataset.get("symbol").filter(|v|v.is_string()).cloned()
                .unwrap_or_else(||bundle.dataset["instrument"]["name"].clone());
            Ok(json!({"run_id":id,"name":name,"workflow_kind":workflow_kind,"symbol":symbol,"started_run_id":bundle.run.request["started_run_id"],"dataset_id":bundle.run.dataset_id,"engine_id":bundle.run.engine_id,"metrics":bundle.run.result["metrics"],"kind":bundle.run.request.get("research_kind").and_then(|v|v.as_str()).unwrap_or("backtest")}))
        }).collect()
    };
    match list(){Ok(runs)=>json!({"runs":runs}).to_string(),Err(e)=>json!({"error":e}).to_string()}
}

fn evolution_json(action:&str,body:&str,state:&State)->String {
    let execute=||->Result<serde_json::Value,String>{
        let jobs=sqx_project::evolution::EvolutionJobs::new(state.archive.root());
        let value:serde_json::Value=serde_json::from_str(body).map_err(|e|e.to_string())?;
        match action {
            "start"=>{
                #[derive(serde::Deserialize)]
                struct Input {request_key:String,seed:u64,ga:sqx_optimizer::GeneticConfig}
                let input:Input=serde_json::from_value(value.clone()).map_err(|e|format!("Invalid evolution settings: {e}"))?;
                let symbol=value["symbol"].as_str().unwrap_or("");let series=find_series(state,symbol).ok_or("no data")?;
                let quality=research_data_quality(&series,&value,sqx_data::quality::now_ms()).map_err(|e|e.to_string())?;
                let config=config_for_series(&value,&series)?;let generator=gen_config_from(&value)?;let instrument=instrument_for(&series.symbol);
                let initial=sqx_optimizer::EvolutionState::new(input.seed,&generator,&input.ga,sqx_optimizer::Fitness::ProfitFactor,&series.bars,&instrument,&config,pyp_for(&series.bars))?;
                let context=json!({"source":"web","seed":input.seed,"timeframe":series.timeframe,"calendar":series.calendar,"data_quality":quality,"allow_data_gaps":value.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false)});
                let id=jobs.create_with_normalization(&retained_engine_identity(state)?,&input.request_key,&initial,context,series.normalization.clone())?;
                Ok(json!({"status":jobs.status(&id)?,"data_quality":quality,"interpretation":"Frozen exploratory search over all supplied data; not an untouched final test."}))
            },
            "advance"=>{
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Input {job_id:String,expected_sequence:usize}
                let input:Input=serde_json::from_value(value).map_err(|e|e.to_string())?;
                let record=jobs.advance(&input.job_id,&engine_identity()?,input.expected_sequence)?;
                Ok(json!({"status":record.committed_status()?,"metadata":jobs.metadata(&input.job_id)?,"generation":record.generation,"evaluations":record.evaluations,"ranked":record.ranked}))
            },
            "replay"=>{
                let id=value["job_id"].as_str().ok_or("job_id required")?;
                let backup=jobs.backup(id)?;
                let evaluations=backup.verify_replay(&engine_identity()?)?;
                Ok(json!({"job_id":id,"checkpoint_id":backup.generations.last().map(|v|&v.0),"generations":backup.generations.len(),"evaluations":evaluations,"interpretation":"Exact replay of this saved checkpoint; not independent strategy validation or verification of source provenance."}))
            },
            "restore"=>{
                let backup:sqx_project::evolution::EvolutionBackup=serde_json::from_value(value).map_err(|e|e.to_string())?;
                let id=jobs.restore(&backup)?;
                Ok(json!({"status":jobs.status(&id)?,"metadata":jobs.metadata(&id)?,"interpretation":"Archive integrity checked; research correctness and origin are not independently verified."}))
            },
            "backup"=>{
                let id=value["job_id"].as_str().ok_or("job_id required")?;
                serde_json::to_value(jobs.backup(id)?).map_err(|e|e.to_string())
            },
            "status"=>{
                let id=value["job_id"].as_str().ok_or("job_id required")?;
                let (status,latest)=jobs.snapshot(id)?;
                Ok(json!({"status":status,"metadata":jobs.metadata(id)?,"engine_retention":jobs.engine_retention(id)?,"latest":latest.map(|record|json!({"generation":record.generation,"evaluations":record.evaluations,"ranked":record.ranked}))}))
            },
            "list"=>{
                let engine=engine_identity()?;let mut rows=Vec::new();for id in jobs.list()?{let metadata=jobs.metadata(&id)?;let compatible=metadata["engine_id"].as_str()==Some(engine.as_str());rows.push(json!({"status":jobs.status(&id)?,"metadata":metadata,"compatible_engine":compatible}));}Ok(json!({"jobs":rows}))
            },
            _=>Err("Unknown evolution action".into()),
        }
    };
    execute().unwrap_or_else(|error|json!({"error":error})).to_string()
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ComparisonImportRequest {
    run_id:String,
    source_json:String,
    tolerances:sqx_project::reconciliation::Tolerances,
}
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ComparisonBackup {
    comparison_id:String,
    record:sqx_project::reconciliation::ComparisonRecord,
}
fn comparison_import_json(body:&str,state:&State)->String {
    let result=serde_json::from_str::<ComparisonImportRequest>(body).map_err(|e|e.to_string()).and_then(|request|sqx_project::reconciliation::save_comparison(&state.archive,&request.run_id,&request.source_json,&request.tolerances));
    match result {Ok(id)=>json!({"comparison_id":id,"deployment_ready":false}).to_string(),Err(e)=>json!({"error":e}).to_string()}
}
fn comparison_read_json(id:&str,state:&State)->String {
    match sqx_project::reconciliation::load_comparison(&state.archive,id){Ok(record)=>json!(ComparisonBackup{comparison_id:id.into(),record}).to_string(),Err(e)=>json!({"error":e}).to_string()}
}
fn comparison_restore_json(body:&str,state:&State)->String {
    let result=serde_json::from_str::<ComparisonBackup>(body).map_err(|e|e.to_string()).and_then(|backup|sqx_project::reconciliation::restore_comparison(&state.archive,&backup.comparison_id,&backup.record));
    match result {Ok(id)=>json!({"comparison_id":id,"deployment_ready":false}).to_string(),Err(e)=>json!({"error":e}).to_string()}
}
fn comparison_list_json(state:&State)->String {
    let list=||->Result<Vec<serde_json::Value>,String>{
        sqx_project::reconciliation::list_comparisons(&state.archive)?.into_iter().map(|id|{
            let record=sqx_project::reconciliation::load_comparison(&state.archive,&id)?;
            let report=&record.result.comparison.report;
            let source:sqx_project::reconciliation::PaperImport=serde_json::from_str(&record.source_json).map_err(|e|e.to_string())?;
            Ok(json!({"comparison_id":id,"run_id":record.result.run_id,"source_id":record.source_id,"source_label":source.source_label,"differing_pairs":report.comparisons.iter().filter(|pair|!pair.differences.is_empty()).count(),"matches_within_tolerance":report.matches_within_tolerance,"matched_pairs":report.comparisons.len(),"missing_trades":report.missing_expected_ids.len(),"unexpected_trades":report.unexpected_tickets.len(),"end_of_data_liquidations":record.result.end_of_data_trade_ids.len(),"deployment_ready":false}))
        }).collect()
    };
    match list(){Ok(rows)=>json!({"comparisons":rows,"interpretation":"Closed-trade correspondence only; source declarations are not authenticated and matching is not deployment approval."}).to_string(),Err(e)=>json!({"error":e}).to_string()}
}

fn research_replay_json(body:&str,state:&State)->String{
    let replay=||->Result<serde_json::Value,String>{
        let value:serde_json::Value=serde_json::from_str(body).map_err(|e|e.to_string())?;
        let id=value.get("run_id").and_then(|v|v.as_str()).ok_or("missing run_id")?;
        let bundle=state.archive.load(id)?;
        if bundle.run.engine_id!=engine_identity()?{return Err("The engine executable differs from the recorded run. Use the matching build; the original report remains available.".into());}
        if matches!(bundle.run.request["research_kind"].as_str(),Some("mcp_generation"|"mcp_generation_started"|"mcp_generation_trial"|"mcp_generation_trial_started")) {
            return Err("MCP generation records are available for inspection; dashboard replay and restart are not implemented for these records.".into());
        }
        if matches!(bundle.run.request["research_kind"].as_str(),Some("cli_pipeline_started"|"cli_pipeline_event"|"cli_pipeline")) {
            return Err("Pipeline recovery requires the complete ordered archive and the original CLI executable. Use pipeline-history or replay-pipeline with the start record ID; this dashboard action cannot replay a single pipeline record.".into());
        }
        if matches!(bundle.run.request["research_kind"].as_str(),Some("cli_backtest"|"cli_backtest_started")) {return Err("CLI backtest records contain full inputs and execution evidence; dashboard replay is not implemented for this record type.".into());}
        if matches!(bundle.run.request["research_kind"].as_str(),Some("assistant_trial"|"assistant_trial_started")) {
            return sqx_project::assistant_trials::execute(&state.archive,id,&engine_identity()?,value["resume"]==true);
        }
        if bundle.run.request["research_kind"]=="assistant_candidates" {
            return Err("AI candidate summaries are archived for inspection and backup; deterministic replay is not implemented for this record type.".into());
        }
        let data:ArchivedDataset=serde_json::from_value(bundle.dataset).map_err(|e|e.to_string())?;
        let series=SymbolSeries{symbol:data.symbol,timeframe:data.timeframe,calendar:data.calendar,normalization:data.normalization,bars:data.bars};
        let normalization=normalization_status(&series);
        if normalization["status"]=="invalid" {return Err(normalization["error"].as_str().unwrap_or("Invalid normalization evidence").into());}
        let actual:serde_json::Value=if bundle.run.request["research_kind"]=="pipeline" {
            let request=&bundle.run.request;
            let config:BacktestConfig=serde_json::from_value(request["config"].clone()).map_err(|e|e.to_string())?;
            let generator:GeneratorConfig=serde_json::from_value(request["generator"].clone()).map_err(|e|e.to_string())?;
            let count=request["count"].as_u64().filter(|n|*n<=2000).ok_or("invalid archived count")? as usize;
            let keep=request["keep"].as_u64().filter(|n|*n<=200).ok_or("invalid archived keep")? as usize;
            let pyp=request["periods_per_year"].as_f64().ok_or("invalid annualization")?;
            let seed=request["seed"].as_u64().ok_or("invalid archived pipeline seed")?;
            compute_pipeline_report(&series,&data.instrument,&config,pyp,&generator,count,keep,seed,&request["research_context"])
        }else if bundle.run.request["research_kind"]=="robustness" {
            let spec:RobustnessSpec=serde_json::from_value(bundle.run.request["spec"].clone()).map_err(|e|e.to_string())?;
            let strategy:Strategy=serde_json::from_value(bundle.strategy).map_err(|e|e.to_string())?;
            compute_robustness_report(&strategy,&series,&data.instrument,&spec)
        }else{serde_json::from_str(&backtest_series_json(&bundle.run.request.to_string(),&series,&data.instrument)).map_err(|e|e.to_string())?};
        Ok(json!({"run_id":id,"matches":actual==bundle.run.result,"recorded":bundle.run.result,"replayed":actual}))
    };
    replay().unwrap_or_else(|e|json!({"error":e})).to_string()
}

#[cfg(test)]
fn backtest_json(body: &str, state: &State) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let series = match find_series(state, &symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    backtest_series_json(body, &series, &instrument_for(&series.symbol))
}

fn backtest_series_json(body: &str, series: &SymbolSeries, inst: &Instrument) -> String {
    let val: serde_json::Value = match serde_json::from_str(body) { Ok(v)=>v,Err(e)=>return json!({"error":e.to_string()}).to_string() };
    let s: Strategy = match serde_json::from_str(body) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let pyp = pyp_for(&series.bars);
    let cfg = match val.get("_resolved_config") {
        Some(value)=>match serde_json::from_value::<BacktestConfig>(value.clone()){Ok(c)=>c,Err(e)=>return json!({"error":e.to_string()}).to_string()},
        None=>match config_for_series(&val,&series){Ok(config)=>config,Err(error)=>return json!({"error":error}).to_string()},
    };
    let cutoff=val.get("_data_as_of_ms").and_then(|v|v.as_i64()).unwrap_or_else(sqx_data::quality::now_ms);
    let quality=match research_data_quality(series,&val,cutoff) {Ok(quality)=>quality,Err(error)=>return error.to_string()};
    let result = backtest(&s, &series.bars, &inst, &cfg);
    if !result.errors.is_empty() { return json!({"error": result.errors.join("; ")}).to_string(); }
    let m = compute_metrics(&result, series.bars.len(), pyp);
    let path_sensitivity: Vec<_> = if val.get("compare_paths").and_then(|v|v.as_bool()).unwrap_or(false) {
        sqx_backtest::backtest_path_scenarios(&s, &series.bars, &inst, &cfg).into_iter().map(|(policy, run)| {
            let metrics = compute_metrics(&run, series.bars.len(), pyp);
            json!({"policy":policy, "final_equity":run.final_equity, "trades":run.trades.len(),
                "ambiguous_candles":run.ambiguous_bars.len(), "metrics":metrics_json(&metrics), "errors":run.errors})
        }).collect()
    } else { Vec::new() };
    let equity = result.equity.clone();
    json!({
        "metrics": metrics_json(&m),
        "final_equity": result.final_equity,
        "equity": equity,
        "trades": result.trades.len(),
        "ambiguous_bars": result.ambiguous_bars,
        "path_sensitivity": path_sensitivity,
        "rejected_orders": result.rejected_orders.iter().map(|r| json!({"time":r.time, "direction":format!("{:?}", r.direction), "requested_lots":r.requested_lots, "reason":r.reason})).collect::<Vec<_>>(),
        "trade_details": result.trades.iter().map(|t| json!({
            "id": t.id, "direction": format!("{:?}", t.direction),
            "open_time": t.open_time, "close_time": t.close_time,
            "open_price": t.open_price, "close_price": t.close_price,
            "size": t.size, "pnl": t.pnl, "commission": t.commission, "financing": t.financing,
            "exit_reason": format!("{:?}", t.exit_reason)
        })).collect::<Vec<_>>(),
        "run_context": {
            "engine_version": env!("CARGO_PKG_VERSION"),
            "normalization":normalization_status(series),
            "data_quality":quality,
            "data_gaps_acknowledged":val.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false),
            "account": cfg.account,
            "account_currency": cfg.account.currency, "max_leverage": cfg.account.max_leverage,
            "stop_out_ratio": cfg.account.stop_out_ratio,
            "long_financing_per_lot": cfg.account.long_financing_per_lot,
            "short_financing_per_lot": cfg.account.short_financing_per_lot,
            "symbol": series.symbol, "timeframe": series.timeframe, "calendar":series.calendar,
            "bars": series.bars.len(), "data_fingerprint": bars_fingerprint(&series.bars),
            "first_time": series.bars.first().map(|b| b.time),
            "last_time": series.bars.last().map(|b| b.time),
            "initial_deposit": cfg.initial_deposit, "spread_points": cfg.spread_points,
            "slippage_points": cfg.slippage_points, "commission_per_lot": cfg.commission_per_lot,
            "intrabar_policy": cfg.intrabar_policy,
            "entry_timing": "next_bar_open", "signal_exit_timing": "next_bar_open",
            "mode": result.execution.mode, "execution": result.execution, "seed": null
        },
        "pseudocode": render_pseudocode(&s),
        "symbol": series.symbol
    })
    .to_string()
}

fn generate_json(body: &str, state: &State) -> String {
    #[derive(serde::Deserialize)]
    struct Req {
        count: usize,
        seed: u64,
        #[serde(default)]
        symbol: String,
        #[serde(default)]
        min_conditions: Option<usize>,
        #[serde(default)]
        max_conditions: Option<usize>,
        #[serde(default)]
        sl_min: Option<f64>,
        #[serde(default)]
        sl_max: Option<f64>,
        #[serde(default)]
        pt_min: Option<f64>,
        #[serde(default)]
        pt_max: Option<f64>,
        #[serde(default)]
        size: Option<f64>,
        #[serde(default)]
        long: Option<bool>,
        #[serde(default)]
        short: Option<bool>,
        #[serde(default)]
        param_ranges: Vec<ParamRange>,
    }
    #[derive(serde::Deserialize)]
    struct ParamRange {
        indicator: String,
        param: String,
        min: f64,
        max: f64,
    }
    let req: Req = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let val:serde_json::Value=serde_json::from_str(body).expect("request already parsed");
    if !(1..=2000).contains(&req.count){return json!({"error":"count must be between 1 and 2000"}).to_string();}
    let (creation_guard,existing)=match reserve_web_research_request("web_generation",&val,state){Ok(result)=>result,Err(error)=>return json!({"error":error}).to_string()};
    if let Some(id)=existing{return json!({"recovery_required":true,"message":"This request already has a saved generation study. Inspect Saved research before retrying.","archive":{"started_run_id":id}}).to_string();}
    let series = match find_series(state, &req.symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };

    let quality=match research_data_quality(&series,&val,sqx_data::quality::now_ms()) {Ok(quality)=>quality,Err(error)=>return error.to_string()};
    let research_context=json!({"data_quality":quality,"data_gaps_acknowledged":val.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false),"scope":"Exploratory research; candidate selection and data reuse require separate final-test validation."});
    let inst = instrument_for(&series.symbol);
    let cfg = match config_for_series(&val,&series){Ok(config)=>config,Err(error)=>return json!({"error":error}).to_string()};
    if let Err(error)=sqx_backtest::validate_backtest_inputs(&series.bars,&inst,&cfg){return json!({"error":error}).to_string();}
    let pyp = pyp_for(&series.bars);
    let count = req.count;
    let gen = GeneratorConfig {
        min_conditions: req.min_conditions.unwrap_or(1),
        max_conditions: req.max_conditions.unwrap_or(3),
        sl_points: (req.sl_min.unwrap_or(0.0), req.sl_max.unwrap_or(200.0)),
        pt_points: (req.pt_min.unwrap_or(0.0), req.pt_max.unwrap_or(400.0)),
        fixed_size: req.size.unwrap_or(0.1),
        long: req.long.unwrap_or(true),
        short: req.short.unwrap_or(true),
        param_ranges: req.param_ranges.into_iter().map(|r| (r.indicator, r.param, r.min, r.max)).collect(),
    };
    if let Err(error)=gen.validate(){return json!({"error":error}).to_string();}
    let pipeline=sqx_pipeline::Pipeline{name:"Dashboard quick search".into(),tasks:vec![sqx_pipeline::Task{kind:sqx_pipeline::TaskKind::Generate{count,min_trades:5}}]};
    let request=json!({"research_kind":"cli_pipeline_started","origin":"web_generation","attempt_id":format!("{:032x}",rand::random::<u128>()),"web_request":val,"config":cfg,"seed":req.seed,"periods_per_year":pyp});
    let dataset=json!({"symbol":series.symbol,"timeframe":series.timeframe,"instrument":inst,"calendar":series.calendar,"normalization":series.normalization,"bars":series.bars,"source_context":research_context});
    let started=match retained_engine_identity(state).and_then(|engine|state.archive.save(engine,dataset,json!({"pipeline":pipeline,"generator":gen}),request,json!({"status":"execution_not_recorded"}))){Ok(id)=>id,Err(error)=>return json!({"error":format!("Generation could not be archived: {error}")}).to_string()};
    drop(creation_guard);
    let recovered=match sqx_project::pipeline_archive::recover(&state.archive,&started,true){Ok(result)=>result,Err(error)=>return json!({"error":format!("Generation stopped: {error}. Inspect Saved research before retrying."),"archive":{"started_run_id":started}}).to_string()};
    let receipt=json!({"started_run_id":started,"run_id":recovered.completion_id});
    if !recovered.report.errors.is_empty(){return json!({"error":recovered.report.errors.join("; "),"research_context":research_context,"archive":receipt}).to_string();}
    let rows:Vec<_>=recovered.report.databank.iter().take(20).filter_map(|strategy|recovered.report.builds.iter().find(|r|r.evaluation.strategy==*strategy).map(|row|{
        let m=&row.evaluation.metrics;json!({"name":strategy.name,"research_context":research_context,"profit_factor":m.profit_factor,"net_profit":m.net_profit,"trades":m.number_of_trades,"dd_pct":m.max_drawdown_pct,"strategy":strategy,"source_run_id":receipt["run_id"],"source_started_run_id":started,"source_candidate_index":row.candidate_index})
    })).collect();
    databank_push(state,&rows);
    json!({"research_context":research_context,"results":rows,"symbol":series.symbol,"archive":receipt}).to_string()
}

fn portfolio_json(body: &str, state: &State) -> String {
    #[derive(serde::Deserialize)]
    struct Req {
        count: usize,
        seed: u64,
        top: usize,
        min_k: usize,
        max_k: usize,
        #[serde(default)]
        symbol: String,
    }
    let req: Req = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    let val:serde_json::Value=serde_json::from_str(body).expect("request already parsed");
    if !(10..=2000).contains(&req.count) || !(2..=12).contains(&req.top) || req.min_k<2 || req.min_k>req.max_k || req.max_k>req.top {
        return json!({"error":"Portfolio requires 10..2000 candidates, a shortlist of 2..12, and 2 <= min_k <= max_k <= top"}).to_string();
    }
    let (creation_guard,existing)=match reserve_web_research_request("web_portfolio",&val,state){Ok(result)=>result,Err(error)=>return json!({"error":error}).to_string()};
    if let Some(id)=existing{return json!({"recovery_required":true,"message":"This request already has a saved portfolio study. Inspect Saved research before retrying.","archive":{"started_run_id":id}}).to_string();}
    let series = match find_series(state, &req.symbol) {
        Some(s) => s,
        None => return json!({ "error": "no data" }).to_string(),
    };
    let quality=match research_data_quality(&series,&val,sqx_data::quality::now_ms()) {Ok(quality)=>quality,Err(error)=>return error.to_string()};
    let inst = instrument_for(&series.symbol);
    let pyp = pyp_for(&series.bars);
    let cfg=match config_for_series(&val,&series){Ok(config)=>config,Err(error)=>return json!({"error":error}).to_string()};
    if let Err(error)=sqx_backtest::validate_backtest_inputs(&series.bars,&inst,&cfg){return json!({"error":error}).to_string();}
    let research_context=json!({"data_quality":quality,"config":cfg,"data_gaps_acknowledged":val.get("allow_data_gaps").and_then(|v|v.as_bool()).unwrap_or(false),"scope":"Exploratory analysis; portfolio combinations are weighted independent simulations, not shared-account execution or independent validation."});
    let count = req.count.clamp(10, 2000);
    let gen = GeneratorConfig::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(req.seed);
    let strategies: Vec<Strategy> = (0..count)
        .map(|i| random_strategy(&mut rng, &format!("p{i:04}"), &gen))
        .collect();
    let pipeline=sqx_pipeline::Pipeline{name:"Dashboard portfolio search".into(),tasks:vec![sqx_pipeline::Task{kind:sqx_pipeline::TaskKind::Portfolio{
        strategies,selection:sqx_portfolio::SearchConfig{min_trades:5,shortlist_limit:req.top,min_subset:req.min_k,max_subset:req.max_k},
    }}]};
    let request=json!({"research_kind":"cli_pipeline_started","origin":"web_portfolio","attempt_id":format!("{:032x}",rand::random::<u128>()),"web_request":val,"config":cfg,"seed":req.seed,"periods_per_year":pyp});
    let dataset=json!({"symbol":series.symbol,"timeframe":series.timeframe,"instrument":inst,"calendar":series.calendar,"normalization":series.normalization,"bars":series.bars,"source_context":research_context});
    let specification=json!({"pipeline":pipeline,"generator":gen});
    let started=match retained_engine_identity(state).and_then(|engine|state.archive.save(engine,dataset,specification,request,json!({"status":"execution_not_recorded"}))){
        Ok(id)=>id,Err(error)=>return json!({"error":format!("Portfolio could not be archived: {error}")}).to_string()
    };
    drop(creation_guard);
    let recovered=match sqx_project::pipeline_archive::recover(&state.archive,&started,true){Ok(result)=>result,Err(error)=>return json!({"error":format!("Portfolio stopped: {error}. Inspect Saved research before retrying."),"archive":{"started_run_id":started}}).to_string()};
    let receipt=json!({"started_run_id":started,"run_id":recovered.completion_id});
    let study=match recovered.report.portfolio{Some(study)=>study,None=>return json!({"error":recovered.report.errors.join("; "),"archive":receipt}).to_string()};
    if !study.errors.is_empty(){return json!({"error":study.errors.join("; "),"research_context":research_context,"archive":receipt}).to_string();}
    let pf=match study.combined{Some(pf)=>pf,None=>return json!({"error":"Portfolio has no combined result","archive":receipt}).to_string()};
    let comps: Vec<_> = pf
        .components
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "weight": c.weight,
                "net_profit": c.net_profit,
                "max_drawdown_pct": c.max_drawdown_pct,
                "trades": c.trades
            })
        })
        .collect();
    json!({
        "archive":receipt,
        "research_context":research_context,
        "account_currency":pf.account_currency,
        "metrics": metrics_json(&pf.metrics),
        "equity": pf.equity,
        "correlation": pf.correlation,
        "components": comps,
        "symbol": series.symbol
    })
    .to_string()
}

fn pseudocode_json(body: &str) -> String {
    let s: Strategy = match serde_json::from_str(body) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("{e}") }).to_string(),
    };
    json!({ "pseudocode": render_pseudocode(&s) }).to_string()
}


fn experiment_json(action:&str,body:&str,state:&State)->String {
    use sqx_project::experiment::{Experiments,ExperimentData,ExperimentPlan};
    let run=||->Result<serde_json::Value,String>{
        let value:serde_json::Value=serde_json::from_str(body).map_err(|e|e.to_string())?;
        let store=Experiments::new(state.archive.root());
        if action=="list" {return Ok(json!({"experiments":store.list()?}));}
        if action=="restore" {
            let backup:sqx_project::experiment::ExperimentBackup=serde_json::from_value(value).map_err(|e|e.to_string())?;
            let id=store.restore(&backup)?;return store.status(&id);
        }
        if action=="create" {
            let series=find_series(state,value["symbol"].as_str().unwrap_or("")).ok_or("dataset not found or ambiguous")?;
            let quality=sqx_data::quality::audit(&series.bars,series.timeframe,sqx_data::quality::now_ms(),series.calendar.as_ref());
            if !quality.errors.is_empty() || quality.missing_open_intervals>0 || quality.unclassified_intervals>0 || quality.irregular_steps>0 {
                return Err("Resolve data-quality errors and gaps before reserving a holdout".into());
            }
            let time=value["holdout_start_time"].as_i64().ok_or("holdout_start_time is required")?;
            let split=series.bars.partition_point(|bar|bar.time<time);
            let minimum=|key:&str|value[key].as_u64().filter(|n|*n>0).map(|n|n as usize).ok_or_else(||format!("{key} must be positive"));
            let disclosure=value["prior_exposure_disclosed"].as_bool().ok_or("Disclose whether these data have already been inspected or tested")?;
            let config=config_for_series(&value,&series)?;
            let plan=ExperimentPlan{schema:1,engine_id:retained_engine_identity(state)?,dataset_id:String::new(),holdout_start:split,config,
                min_development_trades:minimum("min_development_trades")?,min_holdout_trades:minimum("min_holdout_trades")?,
                periods_per_year:pyp_for(&series.bars[..split]),prior_exposure_disclosed:disclosure};
            let data=ExperimentData{instrument:instrument_for(&series.symbol),bars:series.bars,
                provenance:json!({"symbol":series.symbol,"timeframe":series.timeframe,"calendar":series.calendar,"quality":quality,"normalization":series.normalization})};
            let id=store.create(&data,plan)?;
            return store.status(&id);
        }
        let id=value["experiment_id"].as_str().ok_or("experiment_id is required")?;
        match action {
            "status"=>store.status(id),
            "backup"=>serde_json::to_value(store.backup(id)?).map_err(|e|e.to_string()),
            "trial"=>{
                let strategy:Strategy=serde_json::from_value(value["strategy"].clone()).map_err(|e|e.to_string())?;
                store.trial(id,&engine_identity()?,&strategy)
            },
            "search" | "job_start"=>{
                let count=value["count"].as_u64().filter(|n|(1..=2000).contains(n)).ok_or("count must be between 1 and 2000")?;
                let seed=value["seed"].as_u64().ok_or("seed is required")?;
                let config=GeneratorConfig::default();let engine=engine_identity()?;
                let mut rng=rand::rngs::StdRng::seed_from_u64(seed);
                let candidates=(0..count).map(|i|random_strategy(&mut rng,&format!("experiment-{seed}-{i}"),&config)).collect();
                let spec=sqx_project::experiment::SearchSpec{seed,generator:json!(config),candidates};
                static NEXT_SEARCH:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
                let generated_key=format!("{}-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos(),NEXT_SEARCH.fetch_add(1,std::sync::atomic::Ordering::Relaxed));
                let key=value["request_key"].as_str().unwrap_or(&generated_key);
                let mut job=store.start_search(id,&engine,key,&spec)?;
                if action=="job_start" {return Ok(job);}
                let job_id=job["job_id"].as_u64().unwrap() as usize;let mut trials=Vec::new();
                while job["state"]!="complete" {
                    job=store.advance_search(id,&engine,job_id)?;
                    if let Some(trial)=job.get("last_trial"){trials.push(trial.clone());}
                }
                Ok(json!({"trials":trials,"job":job,"status":store.status(id)?}))
            },
            "job_step"=>store.advance_search(id,&engine_identity()?,value["job_id"].as_u64().ok_or("job_id is required")? as usize),

            "reveal"=>store.reveal(id,&engine_identity()?,value["trial"].as_u64().ok_or("trial is required")? as usize),
            _=>Err("unknown experiment operation".into()),
        }
    };
    run().unwrap_or_else(|e|json!({"error":e})).to_string()
}
