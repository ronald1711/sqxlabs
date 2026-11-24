use std::{fs,io::Write,process::{Child,Command,Stdio},time::{Duration,Instant}};
use serde_json::{json,Value};
struct ChildGuard(Child);
impl Drop for ChildGuard{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
struct Temp(std::path::PathBuf);
impl Drop for Temp{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
#[test]
fn killed_generation_retains_trials_and_can_be_inspected_without_source_files(){
    let root=Temp(std::env::temp_dir().join(format!("sqx-mcp-crash-{:032x}",rand::random::<u128>())));
    fs::create_dir_all(&root.0).unwrap();let data=root.0.join("bars.csv");let store=root.0.join("archive");
    let mut csv="timestamp,open,high,low,close,volume\n".to_owned();
    for i in 0..1000 {let p=1.1+(i as f64*0.3).sin()*0.01;csv.push_str(&format!("{},{},{},{},{},10\n",1700000000i64+i*60,p,p+0.003,p-0.003,p+0.0002));}
    fs::write(&data,csv).unwrap();
    let mut child=ChildGuard(Command::new(env!("CARGO_BIN_EXE_sqx-mcp")).env("SQX_MCP_RESEARCH_DIR",&store)
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    writeln!(child.0.stdin.as_mut().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"generate","arguments":{"source":"csv","data":data,"symbol":"EURUSD","timeframe":"M1","count":2000,"seed":42}}})).unwrap();
    child.0.stdin.take();
    let archive=sqx_project::archive::Archive::new(&store);let deadline=Instant::now()+Duration::from_secs(60);
    loop {
        assert!(child.0.try_wait().unwrap().is_none(),"generation exited before interruption");
        if archive.list().unwrap().len()>=4 {break;}
        assert!(Instant::now()<deadline,"candidate evidence was not published");
        std::thread::sleep(Duration::from_millis(20));
    }
    child.0.kill().unwrap();child.0.wait().unwrap();
    fs::remove_file(&data).unwrap();
    let mut reader=Command::new(env!("CARGO_BIN_EXE_sqx-mcp")).env("SQX_MCP_RESEARCH_DIR",&store)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    writeln!(reader.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"generation_history","arguments":{}}})).unwrap();
    let output=reader.wait_with_output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let response:Value=serde_json::from_slice(&output.stdout).unwrap();assert!(response["result"].get("isError").is_none(),"{response}");
    let history:Value=serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let searches=history["searches"].as_object().unwrap();assert_eq!(searches.len(),1);
    let search=searches.values().next().unwrap();assert_eq!(search["planned_candidates"],2000);
    let results=search["trial_results"].as_array().unwrap();assert!(!results.is_empty());assert!(results.len()<2000);
    assert!(search["completion_records"].as_array().unwrap().is_empty());
    assert!(search["trial_checkpoints"].as_array().unwrap().len()>=results.len());
    assert_eq!(archive.engine_retention(search["engine_id"].as_str().unwrap())["status"],"verified");
}
