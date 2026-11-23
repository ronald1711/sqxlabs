use std::{fs,io::{Read,Write},net::{TcpListener,TcpStream},process::{Command,Stdio},time::Duration};
struct Server(std::process::Child);
impl Drop for Server{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
#[test]
fn dashboard_keeps_declared_csv_timeframe_and_rejects_ambiguous_sources(){
    let root=std::env::temp_dir().join(format!("sqx-serve-source-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();let data=root.join("bars.csv");
    fs::write(&data,"timestamp,open,high,low,close,volume\n1700000000,150,151,149,150,1\n1700000300,150,151,149,150,1\n").unwrap();
    for args in [vec![],vec!["--symbol","USDJPY"],vec!["--symbol","USDJPY","--data-timeframe","M5","--questdb"]]{
        let out=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["serve","--data"]).arg(&data).args(args).current_dir(&root).output().unwrap();assert!(!out.status.success());
    }
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();let addr=listener.local_addr().unwrap();drop(listener);
    let mut server=Server(Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["serve","--data"]).arg(&data).args(["--symbol","USDJPY","--data-timeframe","M5","--addr",&addr.to_string()]).current_dir(&root).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    let mut connection=None;
    for _ in 0..100 {if let Ok(stream)=TcpStream::connect(addr){connection=Some(stream);break;}assert!(server.0.try_wait().unwrap().is_none(),"server exited during startup");std::thread::sleep(Duration::from_millis(20));}
    let mut stream=connection.expect("server becomes ready");stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    write!(stream,"GET /api/datasources HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").unwrap();
    let mut response=String::new();stream.read_to_string(&mut response).unwrap();assert!(response.starts_with("HTTP/1.1 200"),"{response}");
    let json:serde_json::Value=serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap();assert_eq!(json["symbols"][0]["symbol"],"USDJPY");assert_eq!(json["symbols"][0]["timeframe"],"M5");assert_eq!(json["symbols"][0]["bars"],2);
    drop(server);fs::remove_dir_all(root).unwrap();
}

#[test]
fn dashboard_manifest_preserves_per_dataset_metadata_and_stages_before_startup(){
    use serde_json::{json,Value};
    let root=std::env::temp_dir().join(format!("sqx-serve-manifest-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();
    fs::write(root.join("arbitrary.csv"),"timestamp,open,high,low,close,volume\n1700000000,1,1.1,0.9,1,1\n1700000300,1,1.1,0.9,1,1\n").unwrap();
    fs::write(root.join("calendar.json"),json!({"name":"Tuesday UTC","weekly":[{"weekday":1,"start_minute":0,"end_minute":1440}]}).to_string()).unwrap();
    let manifest=root.join("datasets.json");let spec=json!({"schema":1,"datasets":[{"symbol":"EURUSD","data":"arbitrary.csv","timeframe":"M1","calendar":"calendar.json"},{"symbol":"GBPUSD","data":"arbitrary.csv","timeframe":"M5"}]});fs::write(&manifest,spec.to_string()).unwrap();
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();let addr=listener.local_addr().unwrap();drop(listener);
    let mut server=Server(Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["serve","--data-manifest"]).arg(&manifest).args(["--addr",&addr.to_string()]).current_dir(&root).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    let mut ready=false;for _ in 0..100{if TcpStream::connect(addr).is_ok(){ready=true;break;}assert!(server.0.try_wait().unwrap().is_none());std::thread::sleep(Duration::from_millis(20));}assert!(ready);
    let request=|path:&str|->Value{
        let mut stream=TcpStream::connect(addr).unwrap();stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();write!(stream,"GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").unwrap();let mut response=String::new();stream.read_to_string(&mut response).unwrap();assert!(response.starts_with("HTTP/1.1 200"),"{response}");serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap()
    };
    let sources=request("/api/datasources");assert_eq!(sources["symbols"][0]["symbol"],"EURUSD");assert_eq!(sources["symbols"][0]["timeframe"],"M1");assert_eq!(sources["symbols"][1]["timeframe"],"M5");
    let detail=request("/api/data?symbol=EURUSD");assert_eq!(detail["calendar"]["name"],"Tuesday UTC");assert_eq!(detail["data_quality"]["missing_open_intervals"],4);
    drop(server);
    let run=||Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["serve","--data-manifest"]).arg(&manifest).current_dir(&root).output().unwrap();
    let mut bad=spec.clone();bad["datasets"][1]=bad["datasets"][0].clone();fs::write(&manifest,bad.to_string()).unwrap();let duplicate=run();assert!(!duplicate.status.success());assert!(duplicate.stdout.is_empty());assert!(String::from_utf8_lossy(&duplicate.stderr).contains("duplicate symbol/timeframe"));
    let mut bad=spec;bad["datasets"][1]["data"]=json!("missing.csv");fs::write(&manifest,bad.to_string()).unwrap();let missing=run();assert!(!missing.status.success());assert!(missing.stdout.is_empty());
    fs::remove_dir_all(root).unwrap();
}
