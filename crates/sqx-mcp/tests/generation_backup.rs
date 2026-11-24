use std::{fs,io::Write,path::Path,process::{Command,Stdio}};
use serde_json::{json,Value};
struct Temp(std::path::PathBuf);
impl Drop for Temp{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
fn call(store:&Path,name:&str,arguments:Value)->Value {
    let mut child=Command::new(env!("CARGO_BIN_EXE_sqx-mcp")).env("SQX_MCP_RESEARCH_DIR",store)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    writeln!(child.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}})).unwrap();
    let output=child.wait_with_output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let response:Value=serde_json::from_slice(&output.stdout).unwrap();response["result"].clone()
}
fn body(result:Value)->Value {
    assert_ne!(result["isError"],true,"{result}");
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}
#[test]
fn protocol_backup_survives_source_removal_and_replays_in_a_fresh_process(){
    let root=Temp(std::env::temp_dir().join(format!("sqx-mcp-backup-{:032x}",rand::random::<u128>())));
    fs::create_dir_all(&root.0).unwrap();let data=root.0.join("bars.csv");let source=root.0.join("source");let target=root.0.join("target");
    let mut csv="timestamp,open,high,low,close,volume\n".to_owned();
    for i in 0..200{csv.push_str(&format!("{},1.1,1.12,1.08,1.11,10\n",1700000000i64+i*60));}
    fs::write(&data,csv).unwrap();
    call(&source,"generate",json!({"source":"csv","data":data,"symbol":"EURUSD","timeframe":"M1","count":2,"seed":42}));
    let history=body(call(&source,"generation_history",json!({})));
    let id=history["searches"].as_object().unwrap().keys().next().unwrap();
    let backup=body(call(&source,"generation_export",json!({"search_id":id})));
    assert_eq!(backup["recorded_completion"],true);assert_eq!(backup["records"].as_array().unwrap().len(),6);
    fs::remove_dir_all(&source).unwrap();fs::remove_file(&data).unwrap();
    for arguments in [json!({"backup":backup,"destination":"unexpected"}),json!({"backup":{}})] {
        assert_eq!(call(&target,"generation_import",arguments)["isError"],true);
        assert!(sqx_project::archive::Archive::new(&target).list().unwrap().is_empty());
    }
    let imported=body(call(&target,"generation_import",json!({"backup":backup})));
    assert_eq!(imported["engine_retention"]["status"],"missing");assert_eq!(imported["recorded_completion"],true);
    assert_eq!(body(call(&target,"generation_export",json!({"search_id":id}))),backup);
    let archive=sqx_project::archive::Archive::new(&target);let before=archive.list().unwrap();
    body(call(&target,"generation_import",json!({"backup":backup})));
    assert_eq!(archive.list().unwrap(),before);
    let replay=call(&target,"generation_resume",json!({"search_id":id}));
    // Execution errors, if any, are reported separately from replay coverage.
    let report:Value=serde_json::from_str(replay["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(report["previous_results_replayed"],2);assert_eq!(report["new_results_recorded"],0);
    assert_eq!(archive.list().unwrap(),before);
}
