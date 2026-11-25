use std::{fs,io::Write,process::{Command,Stdio}};
use serde_json::{json,Value};
struct Temp(std::path::PathBuf);
impl Drop for Temp {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
fn root()->Temp{let t=Temp(std::env::temp_dir().join(format!("sqx-mcp-protocol-log-{:032x}",rand::random::<u128>())));fs::create_dir_all(&t.0).unwrap();t}
fn run(root:&std::path::Path,input:&str,log:&std::path::Path)->std::process::Output {
 let mut child=Command::new(env!("CARGO_BIN_EXE_sqx-mcp")).current_dir(root).env("SQX_MCP_LOG_DIR",log).env("SQX_MCP_RESEARCH_DIR",root.join("research")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
 child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();child.wait_with_output().unwrap()
}
#[test] fn protocol_is_clean_and_secret_inputs_are_not_logged(){
 let root=root();let logs=root.0.join("logs");
 let requests=[json!({"jsonrpc":"2.0","id":"SECRET-id","method":"initialize","params":{"clientInfo":{"name":"SECRET-client"}}}),json!({"jsonrpc":"2.0","method":"notifications/initialized"}),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"SECRET-tool","arguments":{"key":"SECRET-key"}}}),json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"backtest","arguments":{"source":"csv","data":"missing-SECRET-file","symbol":"EURUSD","timeframe":"M1","strategy":{}}}})];
 let input=requests.iter().map(|v|format!("{v}\n")).collect::<String>()+"not-json-SECRET\n";
 let output=run(&root.0,&input,&logs);assert!(output.status.success());
 let replies:Vec<Value>=String::from_utf8(output.stdout).unwrap().lines().map(|l|serde_json::from_str(l).unwrap()).collect();assert_eq!(replies.len(),5);assert_eq!(replies[0]["result"]["serverInfo"]["name"],"sqxlabs");assert!(replies[1]["result"]["tools"].as_array().unwrap().len()>0);assert_eq!(replies[2]["result"]["isError"],true);assert_eq!(replies[3]["result"]["isError"],true);assert_eq!(replies[4]["error"]["code"],-32700);
 let raw=fs::read_to_string(logs.join("mcp.jsonl")).unwrap();assert!(!raw.contains("SECRET"));let events:Vec<Value>=raw.lines().map(|l|serde_json::from_str(l).unwrap()).collect();assert!(events.iter().any(|e|e["method"]=="initialize"&&e["status"]=="completed"));assert!(events.iter().any(|e|e["tool"]=="backtest"&&e["status"]=="tool_error"));assert!(events.iter().all(|e|e["session"].as_str().is_some()&&e["pid"].as_u64().is_some()));
}
#[test] fn logging_failure_uses_stderr_and_keeps_protocol_working(){
 let root=root();let logs=root.0.join("not-a-directory");fs::write(&logs,"file").unwrap();let output=run(&root.0,"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n",&logs);assert!(output.status.success());let reply:Value=serde_json::from_slice(&output.stdout).unwrap();assert!(reply["result"]["tools"].is_array());assert!(String::from_utf8(output.stderr).unwrap().contains("SQX_MCP_LOG_UNAVAILABLE"));
}
