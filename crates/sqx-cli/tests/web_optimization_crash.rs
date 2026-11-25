use std::{fs,io::{Read,Write},net::{TcpListener,TcpStream},path::Path,process::{Child,Command},time::{Duration,Instant}};
use serde_json::{json,Value};
use sqx_project::archive::Archive;

struct Server(Child);
impl Drop for Server {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
fn start(root:&Path,data:Option<&Path>)->(Server,String){
    let socket=TcpListener::bind("127.0.0.1:0").unwrap();let addr=socket.local_addr().unwrap().to_string();drop(socket);
    let mut command=Command::new(root.join("frozen-cli"));
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
fn killed_web_optimization_restores_trials_and_matches_uninterrupted_search(){
    let root=std::env::temp_dir().join(format!("sqx-web-optimization-kill-{:032x}",rand::random::<u128>()));fs::create_dir_all(&root).unwrap();
    struct Cleanup(std::path::PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
    let _cleanup=Cleanup(root.clone());
    fs::copy(env!("CARGO_BIN_EXE_sqx-cli"),root.join("frozen-cli")).unwrap();
    let data=root.join("source.csv");let mut csv="timestamp,open,high,low,close,volume\n".to_string();
    for i in 0..900{let p=1.1+(i as f64*0.07).sin()*0.01;csv.push_str(&format!("{},{p},{},{},{p},1\n",1700000000+i*60,p+0.001,p-0.001));}fs::write(&data,csv).unwrap();
    let request=json!({"symbol":"EURUSD","kind":"walk-forward","count":32,"windows":3,"oos":0.3,"min_is_trades":1,"min_oos_trades":1,"seed":123,"allow_data_gaps":true,"request_key":"interrupted-optimization"});
    let (mut server,addr)=start(&root,Some(&data));let pending=send(&addr,"/api/optimize",&request);
    let archive=Archive::new(root.join("data/research"));let deadline=Instant::now()+Duration::from_secs(90);
    let mut seen=std::collections::BTreeSet::new();
    let started=loop{
        let mut found=None;
        if archive.root().join("runs").exists(){for entry in fs::read_dir(archive.root().join("runs")).unwrap(){
            let path=entry.unwrap().path();if path.extension().and_then(|x|x.to_str())!=Some("json") || !seen.insert(path.clone()){continue;}
            let run:Value=serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            if run["result"]["kind"]=="walk_forward" && run["result"]["event"]["kind"]=="candidate_outcome"{found=Some(run["request"]["started_run_id"].as_str().unwrap().to_owned());break;}
        }}
        if let Some(id)=found{break id;}
        assert!(server.0.try_wait().unwrap().is_none());assert!(Instant::now()<deadline,"no durable candidate outcome");std::thread::sleep(Duration::from_millis(5));
    };
    server.0.kill().unwrap();server.0.wait().unwrap();drop(pending);
    assert_eq!(sqx_project::pipeline_archive::summary(&archive,&started).unwrap()["status"],"incomplete");
    let start_record=archive.load(&started).unwrap();assert_eq!(start_record.run.request["origin"],"web_optimization");
    let backup=sqx_project::pipeline_archive::export(&archive,&started).unwrap();let retained=backup["events"].as_array().unwrap().len();
    fs::remove_file(&data).unwrap();fs::remove_dir_all(archive.root()).unwrap();
    let (_restored,addr)=start(&root,None);
    let imported=response(send(&addr,"/api/pipeline/restore",&backup));assert_eq!(imported["status"],"incomplete","{imported}");
    let before=archive.list().unwrap();let repeated=response(send(&addr,"/api/optimize",&request));
    assert_eq!(repeated["recovery_required"],true,"{repeated}");assert_eq!(repeated["archive"]["started_run_id"],started);assert_eq!(archive.list().unwrap(),before);
    let completed=response(send(&addr,"/api/pipeline/replay",&json!({"run_id":started,"resume":true})));
    assert_eq!(completed["matches"],true,"{completed}");assert_eq!(completed["replayed_events"],retained);assert!(completed["appended_events"].as_u64().unwrap()>0);
    let done=archive.load(completed["completion_id"].as_str().unwrap()).unwrap();let ids=done.run.result["event_ids"].as_array().unwrap();
    for (i,event) in backup["events"].as_array().unwrap().iter().enumerate(){assert_eq!(ids[i],event["run_id"]);}
    let cells=done.run.result["report"]["walk_forward_studies"].as_array().unwrap();assert_eq!(cells.len(),1);assert_eq!(cells[0]["report"]["candidates_attempted"],96);
    assert_eq!(cells[0]["specification"]["criteria"]["min_is_trades"],1);assert_eq!(cells[0]["specification"]["criteria"]["min_oos_trades_per_window"],1);
    assert_eq!(completed["execution_valid"],true);
    let fresh=Archive::new(root.join("uninterrupted"));fresh.restore(&start_record).unwrap();
    let expected=Command::new(root.join("frozen-cli")).args(["replay-pipeline","--archive-dir"]).arg(fresh.root()).args(["--run-id",&started,"--resume"]).output().unwrap();assert!(expected.status.success(),"{}",String::from_utf8_lossy(&expected.stderr));
    let expected:Value=serde_json::from_slice(&expected.stdout).unwrap();assert_eq!(completed["completion_id"],expected["completion_id"]);
    let again=response(send(&addr,"/api/pipeline/replay",&json!({"run_id":started,"resume":true})));assert_eq!(again["appended_events"],0);assert_eq!(again["completion_id"],completed["completion_id"]);
}
