use std::{fs,process::Command};
#[test]
fn separate_cli_processes_start_resume_and_reject_stale_retries(){
 let root=std::env::temp_dir().join(format!("sqx-ga-cli-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();let data=root.join("bars.csv");let store=root.join("store");
 let mut csv="timestamp,open,high,low,close,volume\n".to_string();for i in 0..1500{let p=1.1+(i as f64*0.3).sin()*0.01;csv.push_str(&format!("{},{},{},{},{},10\n",1700000000i64+i*60,p,p+0.003,p-0.003,p+0.0002));}fs::write(&data,csv).unwrap();
 let config=root.join("config.json");fs::write(&config,"{\"spread_points\":7}").unwrap();
 let start=||Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["start-evolution","--symbol","EURUSD","--timeframe","M1","--data"]).arg(&data).arg("--config").arg(&config).arg("--store").arg(&store).args(["--request-key","fixture","--population","20","--generations","1","--min-trades","1"]).output().unwrap();
 let first=start();assert!(first.status.success(),"{}",String::from_utf8_lossy(&first.stderr));let status:serde_json::Value=serde_json::from_slice(&first.stdout).unwrap();let id=status["job_id"].as_str().unwrap();assert_eq!(status["committed_generations"],0);assert_eq!(start().stdout,first.stdout);
 fs::remove_file(&data).unwrap();fs::write(&config,"not JSON anymore").unwrap();
 let resume=|sequence:&str|Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["resume-evolution","--store"]).arg(&store).args(["--job-id",id,"--expected-sequence",sequence]).output().unwrap();
 let one=resume("0");assert!(one.status.success(),"{}",String::from_utf8_lossy(&one.stderr));let status:serde_json::Value=serde_json::from_slice(&one.stdout).unwrap();assert_eq!(status["committed_generations"],1);assert_eq!(status["complete"],false);
 assert!(!resume("0").status.success());let two=resume("1");assert!(two.status.success(),"{}",String::from_utf8_lossy(&two.stderr));let status:serde_json::Value=serde_json::from_slice(&two.stdout).unwrap();assert_eq!(status["complete"],true);assert_eq!(status["committed_generations"],2);
 let final_status=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["evolution-status","--store"]).arg(&store).args(["--job-id",id]).output().unwrap();assert!(final_status.status.success());let report:serde_json::Value=serde_json::from_slice(&final_status.stdout).unwrap();assert!(!report["latest_ranked"].as_array().unwrap().is_empty());
 assert_eq!(report["metadata"]["context"]["timeframe"],"M1");assert_eq!(report["metadata"]["backtest_config"]["spread_points"].as_f64(),Some(7.0));assert_eq!(report["metadata"]["backtest_config"]["bar_duration_ms"],60000);
 fs::remove_dir_all(root).unwrap();
}
