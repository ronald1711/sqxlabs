//! Durable generation-boundary evolution jobs. Publication joins evidence and next state.
use std::{fs::{self,File,OpenOptions},path::PathBuf};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use sqx_optimizer::EvolutionState;
use crate::archive::Archive;
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionJob {pub schema:u32,pub engine_id:String,pub request_key:String,pub initial:EvolutionState,#[serde(default,skip_serializing_if="Value::is_null")] pub context:Value,
 #[serde(default,skip_serializing_if="Option::is_none")] pub normalization:Option<sqx_data::normalization::Bundle>}
impl EvolutionJob {
 fn verify_normalization(&self)->Result<(),String>{
  let legacy=self.context["normalization"].get("bundle").filter(|v|!v.is_null())
   .map(|v|serde_json::from_value::<sqx_data::normalization::Bundle>(v.clone()).map_err(err)).transpose()?;
  if self.normalization.is_some() && legacy.is_some(){return Err("Duplicate evolution normalization sources".into());}
  if let Some(bundle)=self.normalization.as_ref().or(legacy.as_ref()) {
   if encode(bundle)?.len()>20*1024*1024{return Err("Evolution normalization source exceeds 20 MiB".into());}
   let verified=bundle.verify_bars(self.initial.source_bars(),self.initial.backtest_config().bar_duration_ms)?;
   if let Some(claim)=self.context["normalization"].get("verification").filter(|v|!v.is_null()) {
    if serde_json::to_value(verified).map_err(err)?!=*claim{return Err("Evolution normalization verification differs from source".into());}
   }
  } else if self.context["normalization"].get("verification").is_some_and(|v|!v.is_null()) {
   return Err("Evolution normalization verification has no source bundle".into());
  }
  Ok(())
 }
}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationRecord {
 pub schema:u32,pub job_id:String,pub sequence:usize,pub previous:Option<String>,
 pub generation:usize,pub state:EvolutionState,pub evaluations:Vec<Value>,pub ranked:Vec<Value>,
}
impl GenerationRecord {
 /// Status of this committed response, independent of subsequent writers.
 pub fn committed_status(&self)->Result<EvolutionStatus,String>{
  Ok(EvolutionStatus{job_id:self.job_id.clone(),committed_generations:self.sequence.checked_add(1).ok_or("Evolution sequence overflow")?,checkpoint_id:Some(crate::archive::content_id(&encode(self)?)),complete:self.state.is_complete(),next_generation:self.state.generation()})
 }
}
#[derive(Debug,Clone,Serialize)]
pub struct EvolutionStatus {pub job_id:String,pub committed_generations:usize,pub checkpoint_id:Option<String>,pub complete:bool,pub next_generation:usize}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
struct GenerationClaim {schema:u32,job_id:String,sequence:usize,previous:Option<String>,generation:usize,population_size:usize}
#[derive(Debug,Clone,Serialize)]
pub struct EvolutionExposure {pub job_id:String,pub instrument:sqx_core::Instrument,pub start:i64,pub end:i64,pub claimed_generations:usize,pub extent_known:bool}
/// Portable raw JSON preserves exact hashed bytes, including floating-point spellings.
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionBackup {
 pub schema:u32,pub job_id:String,pub job_json:String,
 pub generations:Vec<(String,String)>,pub claims:Vec<(String,String)>,
}
impl EvolutionBackup {
 /// Recompute all committed generations from the frozen initial checkpoint.
 /// Does not publish progress or evaluate an uncommitted claim.
 pub fn verify_replay(&self,engine_id:&str)->Result<usize,String>{
  self.verify()?;
  let job:EvolutionJob=serde_json::from_str(&self.job_json).map_err(err)?;
  if job.engine_id!=engine_id{return Err("Evolution replay requires the original engine executable".into());}
  if self.generations.is_empty(){return Err("No committed generation is available for replay".into());}
  let mut state=job.initial;let mut previous=None;let mut total=0usize;
  for (sequence,(hash,raw)) in self.generations.iter().enumerate(){
   let mut evaluations=Vec::new();
   let step=state.advance(|generation,index,evaluation,eligible|{evaluations.push(json!({"generation":generation,"candidate_index":index,"strategy":evaluation.strategy,"metrics":evaluation.metrics,"errors":evaluation.errors,"eligible":eligible}));Ok(())})?;
   total=total.checked_add(evaluations.len()).ok_or("Replay evaluation count overflow")?;
   let expected=GenerationRecord{schema:1,job_id:self.job_id.clone(),sequence,previous,generation:step.generation,state:state.clone(),evaluations,ranked:step.ranked.iter().map(|(strategy,metrics)|json!({"strategy":strategy,"metrics":metrics})).collect()};
   if encode(&expected)?!=raw.as_bytes(){return Err(format!("Evolution replay differs at generation sequence {sequence}"));}
   previous=Some(hash.clone());
  }
  Ok(total)
 }
 /// Structural and content integrity only; not authenticated origin or independent replay.
 pub fn verify(&self)->Result<(),String>{
  if self.schema!=1 || crate::archive::content_id(self.job_json.as_bytes())!=self.job_id{return Err("Evolution backup job integrity failure".into());}
  let job:EvolutionJob=serde_json::from_str(&self.job_json).map_err(err)?;
  job.verify_normalization()?;
  if job.schema!=1 || job.initial.generation()!=0 || job.initial.is_complete(){return Err("Invalid initial evolution job".into());}
  let (_,last)=job.initial.observation_range().ok_or("Evolution dataset is empty")?;
  crate::observation::end(last,job.initial.backtest_config().bar_duration_ms,&job.context["timeframe"])?;
  let mut previous:Option<&str>=None;let mut state=&job.initial;let mut records=Vec::new();
  for (sequence,(hash,raw)) in self.generations.iter().enumerate(){
   if crate::archive::content_id(raw.as_bytes())!=*hash{return Err("Evolution backup generation integrity failure".into());}
   let record:GenerationRecord=serde_json::from_str(raw).map_err(err)?;
   if record.schema!=1 || record.job_id!=self.job_id || record.sequence!=sequence || record.previous.as_deref()!=previous{return Err("Evolution backup generation chain failure".into());}
   previous=Some(hash);records.push(record);
  }
  for record in &records {
   if state.is_complete() || record.generation!=state.generation() || record.evaluations.len()!=state.population_size(){return Err("Evolution backup generation coverage failure".into());}
   state=&record.state;
  }
  let mut prior=None;
  for (hash,raw) in &self.claims {
   if crate::archive::content_id(raw.as_bytes())!=*hash{return Err("Evolution backup claim integrity failure".into());}
   let claim:GenerationClaim=serde_json::from_str(raw).map_err(err)?;
   if claim.schema!=1 || claim.job_id!=self.job_id || claim.sequence>records.len() || prior.is_some_and(|p|claim.sequence!=p+1){return Err("Evolution backup claim sequence failure".into());}
   let source=if claim.sequence==0{&job.initial}else{&records[claim.sequence-1].state};
   let predecessor=if claim.sequence==0{None}else{Some(self.generations[claim.sequence-1].0.as_str())};
   if source.is_complete() || claim.previous.as_deref()!=predecessor || claim.generation!=source.generation() || claim.population_size!=source.population_size(){return Err("Evolution backup claim checkpoint failure".into());}
   prior=Some(claim.sequence);
  }
  Ok(())
 }
}
#[derive(Clone)]
pub struct EvolutionJobs {archive:Archive}
fn encode<T:Serialize>(v:&T)->Result<Vec<u8>,String>{serde_json::to_vec(v).map_err(|e|e.to_string())}
fn err(e:impl std::fmt::Display)->String{e.to_string()}
impl EvolutionJobs {
 pub fn new(root:impl Into<PathBuf>)->Self{Self{archive:Archive::new(root)}}
 pub fn create(&self,engine_id:&str,request_key:&str,state:&EvolutionState)->Result<String,String>{
  self.create_with_context(engine_id,request_key,state,Value::Null)
 }
 pub fn create_with_context(&self,engine_id:&str,request_key:&str,state:&EvolutionState,context:Value)->Result<String,String>{
  self.create_with_normalization(engine_id,request_key,state,context,None)
 }
 pub fn create_with_normalization(&self,engine_id:&str,request_key:&str,state:&EvolutionState,mut context:Value,mut normalization:Option<sqx_data::normalization::Bundle>)->Result<String,String>{
  if !context.is_null() && !context.is_object(){return Err("Evolution context must be an object".into());}
  if let Some(source)=context["normalization"].get("bundle").filter(|v|!v.is_null()).cloned() {
   if normalization.is_some(){return Err("Duplicate evolution normalization sources".into());}
   normalization=Some(serde_json::from_value(source).map_err(err)?);
   context["normalization"].as_object_mut().unwrap().remove("bundle");
  }
  if encode(&context)?.len()>1024*1024{return Err("Evolution context exceeds 1 MiB".into());}
  if engine_id.trim().is_empty() || request_key.trim().is_empty() || request_key.len()>256{return Err("Engine identity and bounded request key are required".into());}
  if state.generation()!=0 || state.is_complete(){return Err("A new evolution job requires an initial checkpoint".into());}
  let (_,last)=state.observation_range().ok_or("Evolution dataset is empty")?;
  crate::observation::end(last,state.backtest_config().bar_duration_ms,&context["timeframe"])?;
  let job=EvolutionJob{schema:1,engine_id:engine_id.into(),request_key:request_key.into(),initial:state.clone(),context,normalization};
  job.verify_normalization()?;
  let bytes=encode(&job)?;let expected_id=crate::archive::content_id(&bytes);
  fs::create_dir_all(self.archive.root()).map_err(err)?;
  let lock=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(self.archive.root().join("evolution-registry.lock")).map_err(err)?;lock.lock().map_err(err)?;
  let directory=self.archive.root().join("evolution_jobs");
  if directory.exists(){for item in fs::read_dir(directory).map_err(err)?{
   let path=item.map_err(err)?.path();if path.extension().is_some_and(|v|v=="json"){
    let id=path.file_stem().and_then(|s|s.to_str()).ok_or("Invalid evolution job filename")?;
    let existing=self.job(id)?;
    if existing.request_key==request_key {if id==expected_id{return Ok(id.into());}else{return Err("Evolution request key already belongs to different frozen inputs or engine".into());}}
   }
  }}
  self.archive.write_blob("evolution_jobs",&bytes)
 }
 fn job(&self,id:&str)->Result<EvolutionJob,String>{
  let job:EvolutionJob=serde_json::from_slice(&self.archive.read_blob("evolution_jobs",id)?).map_err(err)?;
  if job.schema!=1{return Err("Unsupported evolution job schema".into());}job.verify_normalization()?;Ok(job)
 }
 fn lock(&self,id:&str)->Result<(File,PathBuf),String>{
  self.job(id)?; // Checks identifier and integrity before using it as a directory name.
  let dir=self.archive.root().join("evolution_history").join(id);fs::create_dir_all(&dir).map_err(err)?;
  File::open(self.archive.root()).and_then(|file|file.sync_all()).map_err(err)?;
  File::open(dir.parent().unwrap()).and_then(|file|file.sync_all()).map_err(err)?;
  let lock=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(dir.join("lock")).map_err(err)?;
  lock.lock().map_err(err)?;Ok((lock,dir))
 }
 fn latest(&self,id:&str,dir:&std::path::Path)->Result<(usize,Option<(String,GenerationRecord)>),String>{
  let mut paths=Vec::new();for item in fs::read_dir(dir).map_err(err)?{let path=item.map_err(err)?.path();if path.extension().is_some_and(|v|v=="json"){paths.push(path);}}
  paths.sort();let mut latest:Option<(String,GenerationRecord)>=None;let count=paths.len();
  for (sequence,path) in paths.into_iter().enumerate(){
   let name=path.file_stem().and_then(|s|s.to_str()).ok_or("Invalid checkpoint filename")?;
   let (prefix,hash)=name.split_once('-').ok_or("Invalid checkpoint filename")?;
   if prefix!=format!("{sequence:020}"){return Err("Evolution history contains a gap or duplicate sequence".into());}
   let bytes=fs::read(&path).map_err(err)?;
   if crate::archive::content_id(&bytes)!=hash{return Err("Evolution checkpoint integrity failure".into());}
   let record:GenerationRecord=serde_json::from_slice(&bytes).map_err(err)?;
   if record.schema!=1 || record.job_id!=id || record.sequence!=sequence || record.previous.as_deref()!=latest.as_ref().map(|v|v.0.as_str()) || latest.as_ref().is_some_and(|v|v.1.state.is_complete()) {return Err("Invalid evolution history chain".into());}
   latest=Some((hash.into(),record));
  }Ok((count,latest))
 }
 fn claim(&self,id:&str,sequence:usize,previous:Option<String>,state:&EvolutionState)->Result<(),String>{
  let claim=GenerationClaim{schema:1,job_id:id.into(),sequence,previous,generation:state.generation(),population_size:state.population_size()};
  let hash=self.archive.write_blob("evolution_claim_objects",&encode(&claim)?)?;
  let directory=self.archive.root().join("evolution_claims").join(id);fs::create_dir_all(&directory).map_err(err)?;
  File::open(self.archive.root()).and_then(|f|f.sync_all()).map_err(err)?;File::open(directory.parent().unwrap()).and_then(|f|f.sync_all()).map_err(err)?;
  let target=directory.join(format!("{sequence:020}.json"));let source=self.archive.root().join("evolution_claim_objects").join(format!("{hash}.json"));
  match fs::hard_link(source,&target){Ok(())=>{},Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>{if fs::read(&target).map_err(err)?!=encode(&claim)?{return Err("Evolution claim differs from the resumed checkpoint".into());}},Err(e)=>return Err(err(e))}
  File::open(directory).and_then(|f|f.sync_all()).map_err(err)
 }
 /// Read prior data-use claims without waiting for an active generation to finish.
 pub fn exposure(&self)->Result<Vec<EvolutionExposure>,String>{
  let directory=self.archive.root().join("evolution_jobs");if !directory.exists(){return Ok(Vec::new());}
  let mut output=Vec::new();
  for file in fs::read_dir(directory).map_err(err)?{
   let path=file.map_err(err)?.path();if path.extension().is_none_or(|e|e!="json"){continue;}
   let id=path.file_stem().and_then(|s|s.to_str()).ok_or("Invalid evolution job filename")?;let job=self.job(id)?;
   let history=self.archive.root().join("evolution_history").join(id);
   let committed=if history.exists(){self.latest(id,&history)?.0}else{0};
   let claims=self.archive.root().join("evolution_claims").join(id);let mut count=committed;
   if claims.exists(){let mut paths=fs::read_dir(claims).map_err(err)?.map(|e|e.map(|v|v.path())).collect::<Result<Vec<_>,_>>().map_err(err)?;paths.retain(|p|p.extension().is_some_and(|s|s=="json"));paths.sort();
    let first=paths.first().and_then(|p|p.file_stem()).and_then(|s|s.to_str()).and_then(|s|s.parse::<usize>().ok()).unwrap_or(0);
    if first>committed{return Err("Evolution claims lack preceding committed history".into());}
    for (offset,path) in paths.iter().enumerate(){
     let sequence=first.checked_add(offset).ok_or("Evolution claim sequence overflow")?;
     if path.file_stem().and_then(|s|s.to_str())!=Some(format!("{sequence:020}").as_str()){return Err("Evolution claim sequence has a gap".into());}
     let bytes=fs::read(path).map_err(err)?;let hash=crate::archive::content_id(&bytes);self.archive.read_blob("evolution_claim_objects",&hash)?;
     let claim:GenerationClaim=serde_json::from_slice(&bytes).map_err(err)?;if claim.schema!=1 || claim.job_id!=id || claim.sequence!=sequence{return Err("Invalid evolution claim".into());}count=count.max(sequence+1);
    }
   }
   if count>0{let (start,last)=job.initial.observation_range().ok_or("Evolution dataset is empty")?;let (end,extent_known)=crate::observation::end(last,job.initial.backtest_config().bar_duration_ms,&job.context["timeframe"])?;output.push(EvolutionExposure{job_id:id.into(),instrument:job.initial.instrument().clone(),start,end,claimed_generations:count,extent_known});}
  }
  output.sort_by(|a,b|a.job_id.cmp(&b.job_id));Ok(output)
 }
 pub fn list(&self)->Result<Vec<String>,String>{
  let directory=self.archive.root().join("evolution_jobs");if !directory.exists(){return Ok(Vec::new());}
  let mut ids=Vec::new();for entry in fs::read_dir(directory).map_err(err)?{let path=entry.map_err(err)?.path();if path.extension().is_some_and(|e|e=="json"){
   let id=path.file_stem().and_then(|s|s.to_str()).ok_or("Invalid evolution job filename")?;self.job(id)?;ids.push(id.into());
  }}ids.sort();Ok(ids)
 }
 /// Restore only compatible append-only history. Publish a new job header last.
 pub fn restore(&self,backup:&EvolutionBackup)->Result<String,String>{
  backup.verify()?;
  let job:EvolutionJob=serde_json::from_str(&backup.job_json).map_err(err)?;
  fs::create_dir_all(self.archive.root()).map_err(err)?;
  let lock_file=|name:&str|->Result<File,String>{let f=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(self.archive.root().join(name)).map_err(err)?;f.lock().map_err(err)?;Ok(f)};
  // Same order as advance/final-test publication; no final test may miss restored use.
  let _commit=lock_file(".experiment-commit.lock")?;
  let _registry=lock_file("evolution-registry.lock")?;
  for id in self.list()?{if id!=backup.job_id && self.job(&id)?.request_key==job.request_key{return Err("Restored request key conflicts with an existing job".into());}}
  let history=self.archive.root().join("evolution_history").join(&backup.job_id);
  let claims=self.archive.root().join("evolution_claims").join(&backup.job_id);
  fs::create_dir_all(&history).map_err(err)?;fs::create_dir_all(&claims).map_err(err)?;
  let _job=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(history.join("lock")).map_err(err)?;_job.lock().map_err(err)?;
  self.latest(&backup.job_id,&history)?;
  let mut objects:Vec<(&str,&str,&str,PathBuf)>=Vec::new();
  for (sequence,(hash,raw)) in backup.generations.iter().enumerate(){
   let prefix=format!("{sequence:020}-");
   for entry in fs::read_dir(&history).map_err(err)?{let path=entry.map_err(err)?.path();if path.file_name().and_then(|s|s.to_str()).is_some_and(|s|s.starts_with(&prefix)) && path.file_stem().and_then(|s|s.to_str())!=Some(format!("{prefix}{hash}").as_str()){return Err("Restore conflicts with committed generation history".into());}}
   objects.push(("evolution_generations",hash,raw,history.join(format!("{prefix}{hash}.json"))));
  }
  // Claims publish before result links: a crash must not make inspected data appear unused.
  let mut claim_objects=Vec::new();
  for (hash,raw) in &backup.claims{let claim:GenerationClaim=serde_json::from_str(raw).map_err(err)?;claim_objects.push(("evolution_claim_objects",hash.as_str(),raw.as_str(),claims.join(format!("{:020}.json",claim.sequence))));}
  claim_objects.extend(objects);let objects=claim_objects;
  for (_,_,raw,target) in &objects{if target.exists() && fs::read(target).map_err(err)?!=raw.as_bytes(){return Err("Restore conflicts with existing evolution evidence".into());}}
  for (kind,hash,raw,target) in objects{
   let stored=self.archive.write_blob(kind,raw.as_bytes())?;if stored!=hash{return Err("Restore content identity changed".into());}
   let source=self.archive.root().join(kind).join(format!("{hash}.json"));
   match fs::hard_link(source,target){Ok(())=>{},Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>{},Err(e)=>return Err(err(e))}
   File::open(&claims).and_then(|f|f.sync_all()).map_err(err)?;File::open(&history).and_then(|f|f.sync_all()).map_err(err)?;
  }
  for directory in [&history,&claims]{File::open(directory.parent().unwrap()).and_then(|f|f.sync_all()).map_err(err)?;}
  File::open(self.archive.root()).and_then(|f|f.sync_all()).map_err(err)?;
  let restored=self.archive.write_blob("evolution_jobs",backup.job_json.as_bytes())?;
  File::open(self.archive.root()).and_then(|f|f.sync_all()).map_err(err)?;Ok(restored)
 }
 /// Capture completed history and any interrupted-generation claim under one job lock.
 pub fn backup(&self,id:&str)->Result<EvolutionBackup,String>{
  let (_lock,dir)=self.lock(id)?;self.latest(id,&dir)?;
  let read_objects=|directory:&std::path::Path,kind:&str|->Result<Vec<(String,String)>,String>{
   if !directory.exists(){return Ok(Vec::new());}
   let mut paths=fs::read_dir(directory).map_err(err)?.map(|entry|entry.map(|v|v.path())).collect::<Result<Vec<_>,_>>().map_err(err)?;
   paths.retain(|p|p.extension().is_some_and(|v|v=="json"));paths.sort();
   paths.into_iter().map(|path|{let bytes=fs::read(path).map_err(err)?;let hash=crate::archive::content_id(&bytes);self.archive.read_blob(kind,&hash)?;Ok((hash,String::from_utf8(bytes).map_err(err)?))}).collect()
  };
  let backup=EvolutionBackup{schema:1,job_id:id.into(),job_json:String::from_utf8(self.archive.read_blob("evolution_jobs",id)?).map_err(err)?,generations:read_objects(&dir,"evolution_generations")?,claims:read_objects(&self.archive.root().join("evolution_claims").join(id),"evolution_claim_objects")?};
  backup.verify()?;Ok(backup)
 }
 pub fn metadata(&self,id:&str)->Result<Value,String>{
  let job=self.job(id)?;let range=job.initial.observation_range();
  Ok(json!({"job_id":id,"engine_id":job.engine_id,"request_key":job.request_key,"instrument":job.initial.instrument(),"observation_range":range,"population_size":job.initial.population_size(),"context":job.context,"backtest_config":job.initial.backtest_config(),"genetic_config":job.initial.genetic_config(),"fitness":job.initial.fitness()}))
 }
 pub fn latest_record(&self,id:&str)->Result<Option<GenerationRecord>,String>{
  let (_lock,dir)=self.lock(id)?;Ok(self.latest(id,&dir)?.1.map(|row|row.1))
 }
 /// Return status and its evidence from the same locked checkpoint.
 pub fn snapshot(&self,id:&str)->Result<(EvolutionStatus,Option<GenerationRecord>),String>{
  let job=self.job(id)?;let (_lock,dir)=self.lock(id)?;let (count,latest)=self.latest(id,&dir)?;
  let state=latest.as_ref().map(|v|&v.1.state).unwrap_or(&job.initial);
  let status=EvolutionStatus{job_id:id.into(),committed_generations:count,checkpoint_id:latest.as_ref().map(|v|v.0.clone()),complete:state.is_complete(),next_generation:state.generation()};
  Ok((status,latest.map(|row|row.1)))
 }
 pub fn status(&self,id:&str)->Result<EvolutionStatus,String>{Ok(self.snapshot(id)?.0)}
 /// Compare-and-advance prevents a retried/lost-response request from running an extra
 /// generation. After a sequence conflict, read status before deciding what to request.
 pub fn advance(&self,id:&str,engine_id:&str,expected_sequence:usize)->Result<GenerationRecord,String>{
  let job=self.job(id)?;if job.engine_id!=engine_id{return Err("Evolution resume requires the original engine executable".into());}
  let registry=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(self.archive.root().join(".experiment-commit.lock")).map_err(err)?;registry.lock().map_err(err)?;
  let (_lock,dir)=self.lock(id)?;let (count,latest)=self.latest(id,&dir)?;
  if count!=expected_sequence{return Err("Evolution checkpoint changed; refresh status before advancing".into());}
  let previous=latest.as_ref().map(|v|v.0.clone());
  let mut state=latest.map(|v|v.1.state).unwrap_or(job.initial);
  if state.is_complete(){return Err("Evolution checkpoint is already complete".into());}
  self.claim(id,count,previous.clone(),&state)?;drop(registry);
  let mut evaluations=Vec::new();
  let step=state.advance(|generation,index,evaluation,eligible|{evaluations.push(json!({"generation":generation,"candidate_index":index,"strategy":evaluation.strategy,"metrics":evaluation.metrics,"errors":evaluation.errors,"eligible":eligible}));Ok(())})?;
  let record=GenerationRecord{schema:1,job_id:id.into(),sequence:count,previous,generation:step.generation,state,evaluations,ranked:step.ranked.iter().map(|(strategy,metrics)|json!({"strategy":strategy,"metrics":metrics})).collect()};
  let hash=self.archive.write_blob("evolution_generations",&encode(&record)?)?;
  let source=self.archive.root().join("evolution_generations").join(format!("{hash}.json"));
  fs::hard_link(source,dir.join(format!("{count:020}-{hash}.json"))).map_err(err)?;
  File::open(&dir).and_then(|file|file.sync_all()).map_err(err)?;
  Ok(record)
 }
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn interrupted_pre_execution_claim_remains_visible_to_final_test_audit(){
  use crate::experiment::{Experiments,ExperimentData,ExperimentPlan};
  use sqx_core::{Bar,Instrument};use sqx_backtest::BacktestConfig;use sqx_optimizer::{GeneratorConfig,GeneticConfig,Fitness};
  let root=std::env::temp_dir().join(format!("sqx-ga-claim-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
  let bars:Vec<_>=(0..100).map(|i|Bar::new(i*60000,1.0,1.1,0.9,1.0,1.0)).collect();let instrument=Instrument::fx("EURUSD");let config=BacktestConfig::default();
  let state=EvolutionState::new(42,&GeneratorConfig::default(),&GeneticConfig::default(),Fitness::NetProfit,&bars,&instrument,&config,252.0).unwrap();let jobs=EvolutionJobs::new(&root);let id=jobs.create("engine","claim",&state).unwrap();
  let experiments=Experiments::new(&root);let data=ExperimentData{bars,instrument,provenance:json!({"source":"synthetic fixture"})};
  let plan=ExperimentPlan{schema:1,engine_id:"engine".into(),dataset_id:String::new(),holdout_start:70,config,min_development_trades:1,min_holdout_trades:1,periods_per_year:252.0,prior_exposure_disclosed:false};
  let experiment=experiments.create(&data,plan).unwrap();assert_eq!(experiments.status(&experiment).unwrap()["current_exposure_audit"]["classification"],"no_known_registry_overlap");
  // Simulate process loss immediately after the durable claim, before evaluation.
  jobs.claim(&id,0,None,&state).unwrap();jobs.claim(&id,0,None,&state).unwrap();
  let interrupted_backup=jobs.backup(&id).unwrap();interrupted_backup.verify().unwrap();assert!(interrupted_backup.generations.is_empty());assert_eq!(interrupted_backup.claims.len(),1);
  let restored_jobs=EvolutionJobs::new(root.join("restored"));restored_jobs.restore(&interrupted_backup).unwrap();assert_eq!(restored_jobs.status(&id).unwrap().committed_generations,0);assert_eq!(restored_jobs.exposure().unwrap()[0].claimed_generations,1);
  let reopened=EvolutionJobs::new(&root);assert_eq!(reopened.status(&id).unwrap().committed_generations,0);assert_eq!(reopened.exposure().unwrap()[0].claimed_generations,1);
  let audit=experiments.status(&experiment).unwrap()["current_exposure_audit"].clone();assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");assert_eq!(audit["same_market_evolution_generation_claims"],1);assert_eq!(audit["known_overlaps"][0]["source"],"evolution_generation_claim");
  reopened.advance(&id,"engine",0).unwrap();assert_eq!(reopened.exposure().unwrap()[0].claimed_generations,1);
  std::fs::remove_dir_all(root).unwrap();
 }
 #[test]
 fn hourly_evolution_claim_includes_the_last_candle_contents(){
  use crate::experiment::{Experiments,ExperimentData,ExperimentPlan};
  use sqx_core::{Bar,Instrument};use sqx_backtest::BacktestConfig;use sqx_optimizer::{GeneratorConfig,GeneticConfig,Fitness};
  let root=std::env::temp_dir().join(format!("sqx-ga-extent-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
  let bars=vec![Bar::new(3600000,1.0,1.0,1.0,1.0,1.0)];let instrument=Instrument::fx("EURUSD");let mut config=BacktestConfig::default();config.bar_duration_ms=Some(3600000);
  let state=EvolutionState::new(42,&GeneratorConfig::default(),&GeneticConfig::default(),Fitness::NetProfit,&bars,&instrument,&config,252.0).unwrap();let jobs=EvolutionJobs::new(&root);
  assert!(jobs.create_with_context("engine","bad",&state,json!({"timeframe":"M1"})).is_err());
  let id=jobs.create_with_context("engine","claim",&state,json!({"timeframe":"H1"})).unwrap();jobs.claim(&id,0,None,&state).unwrap();
  let data=ExperimentData{bars:(0..100).map(|i|Bar::new(i*60000,1.0,1.0,1.0,1.0,1.0)).collect(),instrument,provenance:json!({"timeframe":"M1"})};
  let mut config=BacktestConfig::default();config.bar_duration_ms=Some(60000);
  let plan=ExperimentPlan{schema:1,engine_id:"engine".into(),dataset_id:String::new(),holdout_start:70,config,min_development_trades:1,min_holdout_trades:1,periods_per_year:525600.0,prior_exposure_disclosed:false};
  let experiments=Experiments::new(&root);let experiment=experiments.create(&data,plan).unwrap();let audit=experiments.status(&experiment).unwrap()["current_exposure_audit"].clone();
  assert_eq!(audit["classification"],"known_reuse_or_disclosed_exposure");assert_eq!(audit["known_overlaps"][0]["end"],7199999);assert_eq!(audit["observation_extent_complete"],true);
  std::fs::remove_dir_all(root).unwrap();
 }

}
