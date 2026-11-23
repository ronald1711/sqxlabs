use std::{fs,process::Command};
#[test]
fn evolution_log_records_inputs_rejections_and_completion_without_overwriting(){
 let root=std::env::temp_dir().join(format!("sqx-evolve-audit-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();
 let data=root.join("bars.csv");let log=root.join("research.jsonl");
 fs::write(&data,"timestamp,open,high,low,close,volume\n1700000000,1,1.1,0.9,1,10\n1700000060,1,1.1,0.9,1,10\n").unwrap();
 let run=||Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["evolve","--symbol","EURUSD","--timeframe","M1","--data"]).arg(&data).args(["--population","6","--generations","2","--audit-log"]).arg(&log).output().unwrap();
 let output=run();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
 let original=fs::read(&log).unwrap();let events:Vec<serde_json::Value>=std::str::from_utf8(&original).unwrap().lines().map(|line|serde_json::from_str(line).unwrap()).collect();
 assert_eq!(events[0]["backtest_config"]["bar_duration_ms"],60000);assert_eq!(events[0]["research_context"]["timeframe"],"M1");assert_eq!(events[0]["research_context"]["validated_through_ms"],1700000120000i64);assert!(events[0]["research_context"]["as_of_ms"].is_null());
 assert_eq!(events.len(),8);assert_eq!(events[0]["kind"],"started");assert_eq!(events[0]["bars"].as_array().unwrap().len(),2);assert_eq!(events[0]["engine_id"].as_str().unwrap().len(),64);
 for (index,event) in events[1..7].iter().enumerate(){assert_eq!(event["kind"],"evaluation");assert_eq!(event["candidate_index"],index);assert_eq!(event["generation"],0);assert_eq!(event["eligible"],false);assert!(event["strategy"].is_object());assert!(event["metrics"].is_object());assert!(event["errors"].as_array().unwrap().is_empty());}
 assert_eq!(events[7]["kind"],"completed");assert_eq!(events[7]["attempted"],6);assert!(events[7]["ranked"].as_array().unwrap().is_empty());
 let verify=|path:&std::path::Path|Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["verify-evolution","--audit-log"]).arg(path).output().unwrap();
 let verified=verify(&log);assert!(verified.status.success(),"{}",String::from_utf8_lossy(&verified.stderr));assert!(String::from_utf8_lossy(&verified.stdout).contains("6 candidate evaluations"));
 let changed=root.join("changed.jsonl");
 for case in 0..5 {
  let mut modified=events.clone();match case {
   0=>modified[1]["eligible"]=serde_json::json!(true),
   1=>{modified.pop();},
   2=>modified[7]["attempted"]=serde_json::json!(99),
   3=>modified.push(serde_json::json!({"kind":"completed"})),
   _=>modified[0]["engine_id"]=serde_json::json!("wrong-engine"),
  }
  let text=modified.iter().map(|event|serde_json::to_string(event).unwrap()+"\n").collect::<String>();fs::write(&changed,text).unwrap();assert!(!verify(&changed).status.success(),"case {case}");
 }
 fs::write(&changed,&original[..original.len()-1]).unwrap();assert!(!verify(&changed).status.success());
 assert!(!run().status.success());assert_eq!(fs::read(&log).unwrap(),original);
 let failed=root.join("failed.jsonl");let output=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["evolve","--symbol","EURUSD","--timeframe","M1","--data"]).arg(&data).args(["--population","0","--audit-log"]).arg(&failed).output().unwrap();assert!(!output.status.success());
 let failed:Vec<serde_json::Value>=fs::read_to_string(failed).unwrap().lines().map(|line|serde_json::from_str(line).unwrap()).collect();assert_eq!(failed.last().unwrap()["kind"],"failed");assert!(!failed.iter().any(|event|event["kind"]=="completed"));
 fs::remove_dir_all(root).unwrap();
}

#[test]
fn replay_verifies_nonempty_evolved_population(){
 let root=std::env::temp_dir().join(format!("sqx-evolve-replay-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();
 let data=root.join("bars.csv");let log=root.join("research.jsonl");let mut csv="timestamp,open,high,low,close,volume\n".to_string();
 for i in 0..1500 {let p=1.1+(i as f64*0.3).sin()*0.01;csv.push_str(&format!("{},{},{},{},{},10\n",1700000000i64+i*60,p,p+0.003,p-0.003,p+0.0002));}fs::write(&data,csv).unwrap();
 let result=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["evolve","--symbol","EURUSD","--timeframe","M1","--data"]).arg(&data).args(["--population","20","--generations","1","--min-trades","1","--audit-log"]).arg(&log).output().unwrap();assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
 let text=fs::read_to_string(&log).unwrap();let terminal:serde_json::Value=serde_json::from_str(text.lines().last().unwrap()).unwrap();assert_eq!(terminal["attempted"],40);assert!(!terminal["ranked"].as_array().unwrap().is_empty());
 let replay=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["verify-evolution","--audit-log"]).arg(&log).output().unwrap();assert!(replay.status.success(),"{}",String::from_utf8_lossy(&replay.stderr));
 assert_eq!(fs::read_to_string(&log).unwrap(),text);fs::remove_dir_all(root).unwrap();
}
