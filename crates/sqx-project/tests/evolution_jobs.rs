use sqx_project::evolution::EvolutionJobs;
use sqx_optimizer::{EvolutionState,GeneratorConfig,GeneticConfig,Fitness};
use sqx_core::{Bar,Instrument};
use sqx_backtest::BacktestConfig;
struct Temp(std::path::PathBuf);
impl Temp{fn new()->Self{static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);Self(std::env::temp_dir().join(format!("sqx-ga-job-{}-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed))))}}
impl Drop for Temp{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn initial()->EvolutionState{
 let bars:Vec<_>=(0..1500).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;Bar::new(i*60000,p,p+0.003,p-0.003,p+0.0002,10.0)}).collect();
 EvolutionState::new(42,&GeneratorConfig::default(),&GeneticConfig{population_size:20,elite_count:2,generations:1,min_trades:1,..Default::default()},Fitness::NetProfit,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),525600.0).unwrap()
}
#[test]
fn restart_and_orphan_generation_blob_preserve_exact_future(){
 let temp=Temp::new();let jobs=EvolutionJobs::new(&temp.0);let initial=initial();let id=jobs.create("engine","request-1",&initial).unwrap();assert_eq!(jobs.create("engine","request-1",&initial).unwrap(),id);assert!(jobs.create("different-engine","request-1",&initial).is_err());
 assert!(jobs.advance(&id,"other-engine",0).is_err());assert_eq!(jobs.status(&id).unwrap().committed_generations,0);
 let first=jobs.advance(&id,"engine",0).unwrap();assert_eq!(first.evaluations.len(),20);assert!(!first.state.is_complete());
 // Simulate a crash after durable object creation but before the journal link:
 // remove only our disposable fixture's publication link, retaining the object.
 let directory=temp.0.join("evolution_history").join(&id);
 for file in std::fs::read_dir(&directory).unwrap(){let path=file.unwrap().path();if path.extension().is_some_and(|s|s=="json"){std::fs::remove_file(path).unwrap();}}
 let reopened=EvolutionJobs::new(&temp.0);assert_eq!(reopened.status(&id).unwrap().committed_generations,0);
 let recovered=reopened.advance(&id,"engine",0).unwrap();assert_eq!(serde_json::to_value(&recovered).unwrap(),serde_json::to_value(&first).unwrap());
 assert!(reopened.advance(&id,"engine",0).is_err());
 let final_record=EvolutionJobs::new(&temp.0).advance(&id,"engine",1).unwrap();assert!(final_record.state.is_complete());assert_eq!(final_record.evaluations.len(),20);
 let status=jobs.status(&id).unwrap();assert!(status.complete);assert_eq!(status.committed_generations,2);assert!(jobs.advance(&id,"engine",2).is_err());
 let mut uninterrupted=initial;let _=uninterrupted.advance(|_,_,_,_|Ok(())).unwrap();let expected=uninterrupted.advance(|_,_,_,_|Ok(())).unwrap();assert_eq!(final_record.ranked,expected.ranked.iter().map(|(strategy,metrics)|serde_json::json!({"strategy":strategy,"metrics":metrics})).collect::<Vec<_>>());
}
#[test]
fn concurrent_advances_publish_one_generation_and_corruption_is_rejected(){
 let temp=Temp::new();let jobs=EvolutionJobs::new(&temp.0);let id=jobs.create("engine","concurrent",&initial()).unwrap();
 let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));
 let results=std::thread::scope(|scope|{
  let handles:Vec<_>=(0..2).map(|_|{let jobs=jobs.clone();let id=id.clone();let barrier=barrier.clone();scope.spawn(move||{barrier.wait();jobs.advance(&id,"engine",0)})}).collect();
  handles.into_iter().map(|h|h.join().unwrap()).collect::<Vec<_>>()
 });assert_eq!(results.iter().filter(|r|r.is_ok()).count(),1);assert_eq!(jobs.status(&id).unwrap().committed_generations,1);
 let directory=temp.0.join("evolution_history").join(&id);let path=std::fs::read_dir(directory).unwrap().map(|f|f.unwrap().path()).find(|p|p.extension().is_some_and(|s|s=="json")).unwrap();std::fs::write(path,b"{}").unwrap();assert!(jobs.status(&id).is_err());assert!(jobs.advance(&id,"engine",1).is_err());
}

#[test]
fn delayed_response_keeps_its_own_checkpoint_after_another_advance(){
 let temp=Temp::new();let jobs=EvolutionJobs::new(&temp.0);
 let id=jobs.create("engine","delayed-response",&initial()).unwrap();
 let first=jobs.advance(&id,"engine",0).unwrap();
 let first_status=first.committed_status().unwrap();
 assert_eq!(serde_json::to_value(&first_status).unwrap(),serde_json::to_value(jobs.status(&id).unwrap()).unwrap());
 let second=jobs.advance(&id,"engine",1).unwrap();
 // An earlier HTTP response can be assembled after a later generation commits.
 let delayed=first.committed_status().unwrap();
 assert_eq!(delayed.committed_generations,1);assert!(!delayed.complete);
 assert_eq!(delayed.checkpoint_id,first_status.checkpoint_id);
 let (current,evidence)=jobs.snapshot(&id).unwrap();
 assert_eq!(current.committed_generations,2);assert!(current.complete);
 assert_eq!(serde_json::to_value(evidence.unwrap()).unwrap(),serde_json::to_value(&second).unwrap());
 assert_eq!(serde_json::to_value(current).unwrap(),serde_json::to_value(second.committed_status().unwrap()).unwrap());
 assert!(jobs.advance(&id,"engine",delayed.committed_generations).is_err());
}

#[test]
fn frozen_context_survives_reopening_and_participates_in_identity(){
 let temp=Temp::new();let jobs=EvolutionJobs::new(&temp.0);let state=initial();
 let context=serde_json::json!({"seed":42,"data_quality":{"bars":1500,"calendar":null},"allow_data_gaps":true});
 let id=jobs.create_with_context("engine","with-context",&state,context.clone()).unwrap();
 assert_eq!(jobs.create_with_context("engine","with-context",&state,context.clone()).unwrap(),id);
 assert_eq!(EvolutionJobs::new(&temp.0).metadata(&id).unwrap()["context"],context);
 let changed=serde_json::json!({"seed":43});
 assert!(jobs.create_with_context("engine","with-context",&state,changed).is_err());
 let path=temp.0.join("evolution_jobs").join(format!("{id}.json"));
 let mut job:serde_json::Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
 job["context"]["seed"]=serde_json::json!(43);
 std::fs::write(path,serde_json::to_vec(&job).unwrap()).unwrap();
 assert!(jobs.metadata(&id).is_err());
 // Older jobs have no context field: keep their serialized shape and content IDs.
 let old_temp=Temp::new();let old_jobs=EvolutionJobs::new(&old_temp.0);
 let legacy=old_jobs.create("engine","legacy",&state).unwrap();
 let bytes=std::fs::read(old_temp.0.join("evolution_jobs").join(format!("{legacy}.json"))).unwrap();
 let decoded:sqx_project::evolution::EvolutionJob=serde_json::from_slice(&bytes).unwrap();
 assert!(decoded.context.is_null());assert_eq!(serde_json::to_vec(&decoded).unwrap(),bytes);
 assert!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap().get("context").is_none());
 assert!(old_jobs.metadata(&legacy).unwrap()["context"].is_null());
}

#[test]
fn portable_backup_preserves_full_history_and_detects_broken_components(){
 let temp=Temp::new();let jobs=EvolutionJobs::new(&temp.0);let id=jobs.create("engine","backup",&initial()).unwrap();
 let empty=jobs.backup(&id).unwrap();empty.verify().unwrap();assert!(empty.generations.is_empty());assert!(empty.claims.is_empty());
 jobs.advance(&id,"engine",0).unwrap();jobs.advance(&id,"engine",1).unwrap();
 let backup=jobs.backup(&id).unwrap();assert_eq!(backup.generations.len(),2);assert_eq!(backup.claims.len(),2);
 let encoded=serde_json::to_vec(&backup).unwrap();let portable:sqx_project::evolution::EvolutionBackup=serde_json::from_slice(&encoded).unwrap();portable.verify().unwrap();assert_eq!(encoded,serde_json::to_vec(&portable).unwrap());
 for (hash,raw) in &portable.generations{assert_eq!(*hash,sqx_project::archive::content_id(raw.as_bytes()));let record:serde_json::Value=serde_json::from_str(raw).unwrap();assert_eq!(record["evaluations"].as_array().unwrap().len(),20);}
 let mut changed=portable.clone();changed.job_json.push(' ');assert!(changed.verify().is_err());
 let mut changed=portable.clone();changed.generations[0].1.push(' ');assert!(changed.verify().is_err());
 let mut changed=portable.clone();changed.generations.reverse();assert!(changed.verify().is_err());
 let mut changed=portable.clone();changed.generations.remove(0);assert!(changed.verify().is_err());
 let mut changed=portable.clone();changed.claims.reverse();assert!(changed.verify().is_err());
 let mut changed=portable.clone();let mut claim:serde_json::Value=serde_json::from_str(&changed.claims[0].1).unwrap();claim["population_size"]=serde_json::json!(999);changed.claims[0].1=claim.to_string();changed.claims[0].0=sqx_project::archive::content_id(changed.claims[0].1.as_bytes());assert!(changed.verify().is_err());
 drop(jobs);std::fs::remove_dir_all(&temp.0).unwrap();portable.verify().unwrap(); // Does not consult live data or original store.
}

#[test]
fn restore_resumes_exactly_and_never_rolls_back_newer_history(){
 let source=Temp::new();let jobs=EvolutionJobs::new(&source.0);let id=jobs.create("engine","restore",&initial()).unwrap();
 jobs.advance(&id,"engine",0).unwrap();let backup=jobs.backup(&id).unwrap();
 let target=Temp::new();let restored=EvolutionJobs::new(&target.0);assert_eq!(restored.restore(&backup).unwrap(),id);
 assert_eq!(serde_json::to_value(restored.backup(&id).unwrap()).unwrap(),serde_json::to_value(&backup).unwrap());
 assert_eq!(restored.exposure().unwrap()[0].claimed_generations,1);
 let expected=jobs.advance(&id,"engine",1).unwrap();let actual=restored.advance(&id,"engine",1).unwrap();assert_eq!(serde_json::to_value(actual).unwrap(),serde_json::to_value(expected).unwrap());
 restored.restore(&backup).unwrap();assert_eq!(restored.status(&id).unwrap().committed_generations,2);assert_eq!(restored.exposure().unwrap()[0].claimed_generations,2);
 assert!(restored.advance(&id,"other-engine",2).is_err());
 // Simulate interruption before header publication, retaining staged evidence.
 std::fs::remove_file(target.0.join("evolution_jobs").join(format!("{id}.json"))).unwrap();
 restored.restore(&jobs.backup(&id).unwrap()).unwrap();assert_eq!(restored.status(&id).unwrap().committed_generations,2);
 let conflict=Temp::new();let conflict_jobs=EvolutionJobs::new(&conflict.0);conflict_jobs.create("different-engine","restore",&initial()).unwrap();assert!(conflict_jobs.restore(&backup).is_err());assert_eq!(conflict_jobs.list().unwrap().len(),1);
 let bad_target=Temp::new();let bad_jobs=EvolutionJobs::new(&bad_target.0);let mut corrupt=backup.clone();corrupt.generations[0].1.push(' ');assert!(bad_jobs.restore(&corrupt).is_err());assert!(!bad_target.0.exists());
}

#[test]
fn replay_checks_results_and_next_state_beyond_content_integrity(){
 let temp=Temp::new();let jobs=EvolutionJobs::new(&temp.0);let id=jobs.create("engine","replay",&initial()).unwrap();
 assert!(jobs.backup(&id).unwrap().verify_replay("engine").is_err());
 jobs.advance(&id,"engine",0).unwrap();let first=jobs.backup(&id).unwrap();
 assert!(first.verify_replay("wrong-engine").is_err());assert_eq!(first.verify_replay("engine").unwrap(),20);
 let unchanged:sqx_project::evolution::GenerationRecord=serde_json::from_str(&first.generations[0].1).unwrap();assert_eq!(serde_json::to_string(&unchanged).unwrap(),first.generations[0].1);
 for field in ["metrics","ranking","next_state"] {
  let mut forged=first.clone();let mut record:serde_json::Value=serde_json::from_str(&forged.generations[0].1).unwrap();
  match field {
   "metrics"=>record["evaluations"][0]["metrics"]["net_profit"]=serde_json::json!(999999),
   "ranking"=>record["ranked"]=serde_json::json!([]),
   _=>record["state"]["population"][0]["name"]=serde_json::json!("altered continuation"),
  }
  let typed:sqx_project::evolution::GenerationRecord=serde_json::from_value(record).unwrap();
  forged.generations[0].1=serde_json::to_string(&typed).unwrap();forged.generations[0].0=sqx_project::archive::content_id(forged.generations[0].1.as_bytes());
  forged.verify().unwrap();assert!(forged.verify_replay("engine").unwrap_err().contains("replay differs"),"{field}");
 }
 jobs.advance(&id,"engine",1).unwrap();let complete=jobs.backup(&id).unwrap();let before=serde_json::to_vec(&complete).unwrap();
 assert_eq!(complete.verify_replay("engine").unwrap(),40);
 assert_eq!(serde_json::to_vec(&jobs.backup(&id).unwrap()).unwrap(),before);
}

#[test]
fn rehashed_checkpoints_cannot_change_frozen_research_inputs(){
 let source=Temp::new();let jobs=EvolutionJobs::new(&source.0);
 let id=jobs.create("engine","frozen-inputs",&initial()).unwrap();jobs.advance(&id,"engine",0).unwrap();
 let backup=jobs.backup(&id).unwrap();backup.verify().unwrap();
 for pointer in ["/state/bars/0/close","/state/config/initial_deposit","/state/periods_per_year"] {
  let mut altered=backup.clone();let mut record:serde_json::Value=serde_json::from_str(&altered.generations[0].1).unwrap();
  *record.pointer_mut(pointer).unwrap()=serde_json::json!(if pointer.ends_with("close"){1.1003}else{123.0});
  let raw=record.to_string();altered.generations[0]=(sqx_project::archive::content_id(raw.as_bytes()),raw);
  assert!(altered.verify().is_err(),"accepted changed frozen input: {pointer}");
  let target=Temp::new();let restored=EvolutionJobs::new(&target.0);
  assert!(restored.restore(&altered).is_err());assert!(!target.0.exists());
  restored.restore(&backup).unwrap();
  let history=target.0.join("evolution_history").join(&id);
  let old=history.join(format!("{:020}-{}.json",0,backup.generations[0].0));
  std::fs::remove_file(old).unwrap();
  std::fs::write(history.join(format!("{:020}-{}.json",0,altered.generations[0].0)),&altered.generations[0].1).unwrap();
  assert!(restored.status(&id).unwrap_err().contains("frozen research inputs"));
  assert!(restored.advance(&id,"engine",1).unwrap_err().contains("frozen research inputs"));
 }
}
