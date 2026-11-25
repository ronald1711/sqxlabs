use std::{fs,io::BufRead,process::{Command,Stdio}};
use serde_json::Value;
use sqx_project::archive::Archive;
#[test]
fn portfolio_survives_process_death_and_restores_all_candidates_and_subset_choices(){
 let root=std::env::temp_dir().join(format!("sqx-cli-portfolio-kill-{:032x}",rand::random::<u128>()));fs::create_dir_all(&root).unwrap();
 struct Cleanup(std::path::PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}let _cleanup=Cleanup(root.clone());
 let executable=root.join("frozen-cli");fs::copy(env!("CARGO_BIN_EXE_sqx-cli"),&executable).unwrap();
 let csv=root.join("source.csv");let mut data="timestamp,open,high,low,close,volume\n".to_string();
 for i in 0..1500{let p=1.1+(i as f64*0.07).sin()*0.01;data.push_str(&format!("{},{p},{},{},{p},1\n",1700000000+i*60,p+0.001,p-0.001));}fs::write(&csv,data).unwrap();
 let archive=Archive::new(root.join("archive"));
 struct Child(std::process::Child);impl Drop for Child{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
 let mut child=Child(Command::new(&executable).args(["portfolio","--data"]).arg(&csv).args(["--archive-dir"]).arg(archive.root()).args(["--symbol","EURUSD","--timeframe","M1","--count","40","--seed","123","--top","4","--min-k","2","--max-k","3"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap());
 let mut line=String::new();std::io::BufReader::new(child.0.stdout.take().unwrap()).read_line(&mut line).unwrap();
 let receipt:Value=serde_json::from_str(line.trim().strip_prefix("Portfolio checkpoint: ").expect("checkpoint before calculation")).unwrap();let id=receipt["run_id"].as_str().unwrap();
 let start=archive.load(id).unwrap();let deadline=std::time::Instant::now()+std::time::Duration::from_secs(120);let mut seen=std::collections::BTreeSet::new();
 loop{
  let mut found=false;
  for entry in fs::read_dir(archive.root().join("runs")).unwrap(){let path=entry.unwrap().path();if path.extension().and_then(|x|x.to_str())!=Some("json") || !seen.insert(path.clone()){continue;}
   let run:Value=serde_json::from_slice(&fs::read(path).unwrap()).unwrap();if run["result"]["kind"]=="portfolio" && run["result"]["event"]["kind"]=="candidate_outcome"{found=true;break;}
  }
  if found{break;}assert!(child.0.try_wait().unwrap().is_none(),"study completed before checkpoint observation");assert!(std::time::Instant::now()<deadline);std::thread::sleep(std::time::Duration::from_millis(5));
 }
 child.0.kill().unwrap();child.0.wait().unwrap();
 let records:Vec<_>=archive.list().unwrap().iter().map(|id|archive.load(id).unwrap()).collect();
 assert!(!records.iter().any(|row|row.run.request["research_kind"]=="cli_pipeline"));
 let outcomes=records.iter().filter(|row|row.run.result["event"]["kind"]=="candidate_outcome").count();assert!((1..40).contains(&outcomes));
 assert_eq!(start.run.request["origin"],"cli_portfolio");assert_eq!(start.run.request["seed"],123);
 assert_eq!(start.strategy["pipeline"]["tasks"][0]["kind"]["Portfolio"]["selection"]["shortlist_limit"],4);
 let export=Command::new(&executable).args(["export-pipeline","--archive-dir"]).arg(archive.root()).args(["--run-id",id]).output().unwrap();assert!(export.status.success());
 let backup=root.join("backup.json");fs::write(&backup,export.stdout).unwrap();fs::remove_file(csv).unwrap();fs::remove_dir_all(archive.root()).unwrap();
 let restored=Archive::new(root.join("restored"));let imported=Command::new(&executable).args(["import-pipeline","--archive-dir"]).arg(restored.root()).args(["--input"]).arg(&backup).output().unwrap();assert!(imported.status.success(),"{}",String::from_utf8_lossy(&imported.stderr));
 let replay=|archive:&Archive|{let output=Command::new(&executable).args(["replay-pipeline","--archive-dir"]).arg(archive.root()).args(["--run-id",id,"--resume"]).output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));serde_json::from_slice::<Value>(&output.stdout).unwrap()};
 let resumed=replay(&restored);assert_eq!(resumed["execution_valid"],true);assert!(resumed["appended_events"].as_u64().unwrap()>0);assert!(resumed["replayed_events"].as_u64().unwrap()>0);
 let done=restored.load(resumed["completion_id"].as_str().unwrap()).unwrap();let study=&done.run.result["report"]["portfolio"];
 assert_eq!(study["candidates"].as_array().unwrap().len(),40);assert_eq!(study["shortlist"].as_array().unwrap().len(),4);assert_eq!(study["subset"]["attempted"],10);
 let fresh=Archive::new(root.join("uninterrupted"));fresh.restore(&start).unwrap();let expected=replay(&fresh);assert_eq!(resumed["completion_id"],expected["completion_id"]);
 let again=replay(&restored);assert_eq!(again["appended_events"],0);assert_eq!(again["completion_id"],expected["completion_id"]);
}

#[test]
fn invalid_portfolio_bounds_fail_before_creating_a_study(){
 let root=std::env::temp_dir().join(format!("sqx-cli-portfolio-bounds-{:032x}",rand::random::<u128>()));
 for args in [vec!["--count","0"],vec!["--count","2001"],vec!["--top","13"],vec!["--min-k","1"],vec!["--min-k","4","--max-k","2"],vec!["--top","2","--max-k","3"]]{
  let output=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["portfolio","--data","missing.csv","--symbol","EURUSD","--timeframe","M1","--archive-dir"]).arg(&root).args(args).output().unwrap();
  assert!(!output.status.success());assert!(String::from_utf8_lossy(&output.stderr).contains("Portfolio requires"));assert!(!root.exists());
 }
}
