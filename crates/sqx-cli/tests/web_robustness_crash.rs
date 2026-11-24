use std::{fs,io::{Read,Write},net::{TcpListener,TcpStream},path::Path,process::{Child,Command},time::{Duration,Instant}};
use serde_json::{json,Value};
use sqx_project::archive::Archive;

struct Server(Child);
impl Drop for Server {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
fn start(root:&Path,data:Option<&Path>)->(Server,String){
    let socket=TcpListener::bind("127.0.0.1:0").unwrap();let addr=socket.local_addr().unwrap().to_string();drop(socket);
    let mut command=Command::new(env!("CARGO_BIN_EXE_sqx-cli"));
    command.current_dir(root).args(["serve","--addr",&addr,"--data-timeframe","M1"]);
    if let Some(data)=data{command.arg("--data").arg(data).args(["--symbol","EURUSD"]);}else{
        fs::create_dir_all(root.join("empty")).unwrap();command.arg("--data-dir").arg(root.join("empty"));
    }
    let log_path=root.join(if data.is_some(){"initial-server.log"}else{"restored-server.log"});
    let log=fs::File::create(&log_path).unwrap();
    let mut server=Server(command.stdout(log.try_clone().unwrap()).stderr(log).spawn().unwrap());
    let deadline=Instant::now()+Duration::from_secs(30);
    while TcpStream::connect(&addr).is_err(){
        assert!(server.0.try_wait().unwrap().is_none(),"server exited before listening: {}",fs::read_to_string(&log_path).unwrap());
        assert!(Instant::now()<deadline,"server startup timed out");std::thread::sleep(Duration::from_millis(20));
    }
    (server,addr)
}
fn send(addr:&str,path:&str,body:&Value)->TcpStream{
    let body=body.to_string();let mut stream=TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(180))).unwrap();
    write!(stream,"POST {path} HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://{addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();stream
}
fn response(mut stream:TcpStream)->Value{
    let mut text=String::new();stream.read_to_string(&mut text).unwrap();assert!(text.starts_with("HTTP/1.1 200"),"{text}");
    serde_json::from_str(text.split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[test]
fn killed_web_stress_test_restores_partial_backup_and_resumes_without_sources(){
    let root=std::env::temp_dir().join(format!("sqx-web-stress-kill-{:032x}",rand::random::<u128>()));fs::create_dir_all(&root).unwrap();
    struct Cleanup(std::path::PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
    let _cleanup=Cleanup(root.clone());let data=root.join("source.csv");
    let mut csv="timestamp,open,high,low,close,volume\n".to_string();
    for i in 0..120{csv.push_str(&format!("{},1.1,1.101,1.099,1.1,1\n",1700000000+i*60));}fs::write(&data,csv).unwrap();
    let mut strategy=json!(sqx_strategy::Strategy::new("interrupted web research"));
    strategy["signals"]=json!(["LongEntry","LongExit"].map(|kind|json!({"kind":kind,"conditions":[{"left":{"Const":1.0},"cmp":"Gt","right":{"Const":0.0}}],"join":"And","fuzzy":null})));
    strategy["symbol"]=json!("EURUSD");strategy["allow_data_gaps"]=json!(true);
    let (mut server,addr)=start(&root,Some(&data));let pending=send(&addr,"/api/robustness",&strategy);
    let archive=Archive::new(root.join("data/research"));let deadline=Instant::now()+Duration::from_secs(90);
    let mut seen=std::collections::BTreeSet::new();
    let started=loop{
        let mut found=None;
        if archive.root().join("runs").exists(){for entry in fs::read_dir(archive.root().join("runs")).unwrap(){
            let path=entry.unwrap().path();if path.extension().and_then(|x|x.to_str())!=Some("json"){continue;}
            let id=path.file_stem().unwrap().to_str().unwrap().to_owned();if !seen.insert(id.clone()){continue;}
            let record=archive.load(&id).unwrap();
            if record.run.result["event"].get("TradeManipulationOutcome").is_some(){found=Some(record.run.request["started_run_id"].as_str().unwrap().to_owned());break;}
        }}
        if let Some(id)=found{break id;}
        assert!(server.0.try_wait().unwrap().is_none());assert!(Instant::now()<deadline,"no durable resampling outcome");std::thread::sleep(Duration::from_millis(5));
    };
    server.0.kill().unwrap();server.0.wait().unwrap();drop(pending);
    let summary=sqx_project::pipeline_archive::summary(&archive,&started).unwrap();assert_eq!(summary["status"],"incomplete");
    let start_record=archive.load(&started).unwrap();assert_eq!(start_record.run.request["origin"],"web_robustness");
    let backup=sqx_project::pipeline_archive::export(&archive,&started).unwrap();let retained=backup["events"].as_array().unwrap().len();
    fs::remove_file(&data).unwrap();fs::remove_dir_all(archive.root()).unwrap();
    let (_restored,addr)=start(&root,None);
    let imported=response(send(&addr,"/api/pipeline/restore",&backup));assert_eq!(imported["status"],"incomplete","{imported}");
    let completed=response(send(&addr,"/api/pipeline/replay",&json!({"run_id":started,"resume":true})));
    assert_eq!(completed["matches"],true,"{completed}");assert_eq!(completed["replayed_events"],retained);assert!(completed["appended_events"].as_u64().unwrap()>0);
    let done=archive.load(completed["completion_id"].as_str().unwrap()).unwrap();
    let ids=done.run.result["event_ids"].as_array().unwrap();assert_eq!(ids.len(),2016);
    for (index,event) in backup["events"].as_array().unwrap().iter().enumerate(){assert_eq!(ids[index],event["run_id"],"retained prefix changed");}
    let study=&done.run.result["report"]["robustness"];assert_eq!(study["what_if"].as_array().unwrap().len(),6);
    assert_eq!(study["monte_carlo"]["samples"].as_array().unwrap().len(),1000);assert!(study["retest"].is_null());assert!(study["walk_forward"].is_null());
    assert!(done.run.result["report"]["errors"].as_array().unwrap().is_empty());
    assert_eq!(completed["execution_valid"],true);
}
