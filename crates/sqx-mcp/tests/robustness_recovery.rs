use std::{fs,io::Write,process::{Command,Stdio}};
use serde_json::{json,Value};
use sqx_project::archive::Archive;
fn rpc(executable:&std::path::Path,root:&std::path::Path,name:&str,args:Value)->Value{
 let mut child=Command::new(executable).env("SQX_MCP_RESEARCH_DIR",root).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
 writeln!(child.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}})).unwrap();
 let output=child.wait_with_output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
 let response:Value=serde_json::from_slice(&output.stdout).unwrap();response["result"].clone()
}
fn payload(response:Value)->Value{
 assert_ne!(response["isError"],true,"{response}");serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap()
}
#[test]
fn mcp_robustness_survives_process_death_source_deletion_and_portable_restore(){
 let root=std::env::temp_dir().join(format!("sqx-mcp-stress-kill-{:032x}",rand::random::<u128>()));fs::create_dir_all(&root).unwrap();
 struct Cleanup(std::path::PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}let _cleanup=Cleanup(root.clone());
 let executable=root.join("frozen-mcp");fs::copy(env!("CARGO_BIN_EXE_sqx-mcp"),&executable).unwrap();
 let csv=root.join("source.csv");let mut data="timestamp,open,high,low,close,volume\n".to_string();
 for i in 0..400{data.push_str(&format!("{},1.1,1.101,1.099,1.1,1\n",1700000000+i*60));}fs::write(&csv,data).unwrap();
 let mut strategy=sqx_strategy::Strategy::new("MCP recovery control");strategy.long=true;strategy.short=false;
 strategy.stop_loss=Some(sqx_strategy::StopLoss::Points(20.0));strategy.profit_target=Some(sqx_strategy::ProfitTarget::Points(30.0));
 for kind in [sqx_strategy::SignalKind::LongEntry,sqx_strategy::SignalKind::LongExit]{strategy.signals.push(sqx_strategy::Signal::new(kind,vec![sqx_strategy::Condition::new(sqx_strategy::Operand::Const(1.0),sqx_strategy::Comparator::Gt,sqx_strategy::Operand::Const(0.0))],sqx_strategy::Join::And));}
 let archive=Archive::new(root.join("archive"));
 struct Child(std::process::Child);impl Drop for Child{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
 let mut child=Child(Command::new(&executable).env("SQX_MCP_RESEARCH_DIR",archive.root()).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
 writeln!(child.0.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"robustness","arguments":{"data":csv,"symbol":"EURUSD","timeframe":"M1","strategy":strategy}}})).unwrap();
 let deadline=std::time::Instant::now()+std::time::Duration::from_secs(120);let mut seen=std::collections::BTreeSet::new();
 loop{
  let mut found=false;
  for id in archive.list().unwrap_or_default(){if seen.insert(id.clone()){let record=archive.load(&id).unwrap();if record.run.result["event"].get("TradeManipulationOutcome").is_some(){found=true;break;}}}
  if found{break;}assert!(child.0.try_wait().unwrap().is_none(),"study ended before checkpoint");assert!(std::time::Instant::now()<deadline,"checkpoint deadline");std::thread::sleep(std::time::Duration::from_millis(10));
 }
 child.0.kill().unwrap();child.0.wait().unwrap();fs::remove_file(csv).unwrap();
 let history=payload(rpc(&executable,archive.root(),"research_history",json!({})));let studies=history["studies"].as_array().unwrap();assert_eq!(studies.len(),1);assert_eq!(studies[0]["status"],"incomplete");
 let id=studies[0]["started_run_id"].as_str().unwrap();let start=archive.load(id).unwrap();
 assert_eq!(start.run.request["origin"],"mcp_robustness");assert_eq!(start.strategy["pipeline"]["tasks"][0]["kind"]["Robustness"]["mc_runs"],500);
 let before=archive.list().unwrap();let invalid=rpc(&executable,archive.root(),"research_resume",json!({"run_id":id,"seed":999}));assert_eq!(invalid["isError"],true);assert_eq!(before,archive.list().unwrap());
 let backup=payload(rpc(&executable,archive.root(),"research_export",json!({"run_id":id})));fs::remove_dir_all(archive.root()).unwrap();
 let restored=Archive::new(root.join("restored"));payload(rpc(&executable,restored.root(),"research_import",json!({"backup":backup})));
 let resumed=payload(rpc(&executable,restored.root(),"research_resume",json!({"run_id":id})));
 assert!(resumed["replayed_events"].as_u64().unwrap()>0);assert!(resumed["appended_events"].as_u64().unwrap()>0);
 let report=&resumed["report"]["robustness"];
 assert_eq!(report["monte_carlo"]["samples"].as_array().unwrap().len(),500);assert_eq!(report["retest"]["requested_runs"],200);assert_eq!(report["walk_forward"]["window_results"].as_array().unwrap().len(),4);
 let fresh=Archive::new(root.join("fresh"));fresh.restore(&start).unwrap();let uninterrupted=payload(rpc(&executable,fresh.root(),"research_resume",json!({"run_id":id})));
 assert_eq!(resumed["completion_id"],uninterrupted["completion_id"]);assert_eq!(resumed["report"],uninterrupted["report"]);
 let completed=restored.list().unwrap();let again=payload(rpc(&executable,restored.root(),"research_resume",json!({"run_id":id})));assert_eq!(again["appended_events"],0);assert_eq!(completed,restored.list().unwrap());
}
