use std::{fs,io::{BufRead,BufReader,Write},process::{Child,Command,Stdio},sync::mpsc,time::Duration};
use serde_json::{json,Value};
struct Process(Child);
impl Drop for Process {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
struct Directory(std::path::PathBuf);
impl Drop for Directory {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
#[test]
fn killed_crosscheck_resumes_all_markets_without_source_files(){
    let root=Directory(std::env::temp_dir().join(format!("sqx-crosscheck-crash-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())));
    fs::create_dir_all(&root.0).unwrap();
    let data=root.0.join("bars.csv");let strategy=root.0.join("strategy.json");let archive_path=root.0.join("research");
    let mut csv=std::io::BufWriter::new(fs::File::create(&data).unwrap());
    writeln!(csv,"timestamp,open,high,low,close,volume").unwrap();
    // Enough work after publication to kill the process before its result is saved.
    for i in 0..50_000i64 {writeln!(csv,"{},1,1.001,0.999,1,10",1700000000+i*60).unwrap();}
    csv.flush().unwrap();drop(csv);
    let mut spec=serde_json::to_value(sqx_strategy::Strategy::new("crash recovery")).unwrap();
    spec["long"]=json!(true);spec["signals"]=json!([{"kind":"LongEntry","conditions":[{"left":{"Const":1},"cmp":"Gt","right":{"Const":0}}],"join":"And","fuzzy":null}]);
    spec["money_management"]=json!({"FixedSize":{"size":0.1}});
    fs::write(&strategy,spec.to_string()).unwrap();
    let manifest=root.0.join("markets.json");
    fs::write(&manifest,json!({"schema":1,"markets":[{"symbol":"EURUSD","data":"bars.csv","timeframe":"M1"},{"symbol":"GBPUSD","data":"bars.csv","timeframe":"M1"}]}).to_string()).unwrap();
    let mut child=Process(Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["crosscheck","--strategy"]).arg(&strategy).arg("--manifest").arg(&manifest).arg("--archive-dir").arg(&archive_path).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap());
    let stdout=child.0.stdout.take().unwrap();let (tx,rx)=mpsc::channel();
    let reader=std::thread::spawn(move||{let mut line=String::new();let result=BufReader::new(stdout).read_line(&mut line).map(|_|line);let _=tx.send(result);});
    let line=rx.recv_timeout(Duration::from_secs(30)).expect("checkpoint publication timed out").unwrap();
    let receipt:Value=serde_json::from_str(line.strip_prefix("Crosscheck checkpoint: ").expect("checkpoint must precede calculation output")).unwrap();
    assert!(child.0.try_wait().unwrap().is_none(),"process must be alive when interrupted");
    child.0.kill().unwrap();assert!(!child.0.wait().unwrap().success());reader.join().unwrap();
    let archive=sqx_project::archive::Archive::new(&archive_path);
    assert_eq!(archive.list().unwrap().len(),1,"kill must precede result publication");
    let id=receipt["run_id"].as_str().unwrap();let started=archive.load(id).unwrap();
    assert_eq!(started.run.request["research_kind"],"cli_crosscheck_started");
    fs::remove_file(&data).unwrap();fs::remove_file(&strategy).unwrap();fs::remove_file(&manifest).unwrap();
    let resumed=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["replay-crosscheck","--resume","--archive-dir"]).arg(&archive_path).args(["--run-id",id]).output().unwrap();
    assert!(resumed.status.success(),"{}",String::from_utf8_lossy(&resumed.stderr));
    let output=String::from_utf8(resumed.stdout).unwrap();let final_receipt:Value=serde_json::from_str(output.trim().strip_prefix("Crosscheck archive: ").unwrap()).unwrap();
    let final_id=final_receipt["run_id"].as_str().unwrap();let result=archive.load(final_id).unwrap();
    assert_eq!(result.run.request["started_run_id"],id);
    assert_eq!(result.run.result["report"]["requested_markets"],2);
    let executions=result.run.result["executions"].as_array().unwrap();assert_eq!(executions.len(),2);
    for execution in executions {
        assert_eq!(execution["trades"].as_array().unwrap().len(),1);
        assert_eq!(execution["equity"].as_array().unwrap().len(),50_000);
        assert!(execution["errors"].as_array().unwrap().is_empty());
    }
    let replay=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["replay-crosscheck","--archive-dir"]).arg(&archive_path).args(["--run-id",final_id]).output().unwrap();
    assert!(replay.status.success(),"{}",String::from_utf8_lossy(&replay.stderr));
}
