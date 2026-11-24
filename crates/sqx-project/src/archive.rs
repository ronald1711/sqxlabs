//! Immutable content-addressed research records. No stored executable is run.
use std::{fs::{self, OpenOptions}, io::Write, path::{Path, PathBuf}, sync::atomic::{AtomicU64, Ordering}};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
pub fn content_id(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> { serde_json::to_vec(value).map_err(|e|e.to_string()) }
fn valid_id(id: &str) -> bool { id.len()==64 && id.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub schema: u32,
    pub engine_id: String,
    pub dataset_id: String,
    pub strategy_id: String,
    pub request: Value,
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchBundle {
    pub run_id: String,
    pub run: RunRecord,
    pub dataset: Value,
    pub strategy: Value,
}

#[derive(Debug, Clone)]
pub struct Archive { root: PathBuf }
// An operation-local validation set for immutable shared inputs. Never retained
// across requests; each run payload is read and hash-checked on every access.
pub(crate) struct ReadSession<'a>{archive:&'a Archive,validated:std::collections::BTreeSet<(&'static str,String)>}
impl ReadSession<'_>{
    pub(crate) fn load_run(&mut self,id:&str)->Result<RunRecord,String>{
        let run=self.archive.read_run(id)?;
        for (kind,input_id) in [("datasets",&run.dataset_id),("strategies",&run.strategy_id)]{
            let key=(kind,input_id.clone());
            if !self.validated.contains(&key){
                let _:Value=serde_json::from_slice(&self.archive.read_blob(kind,input_id)?).map_err(|e|e.to_string())?;
                self.validated.insert(key);
            }
        }
        Ok(run)
    }
}
impl Archive {
    pub(crate) fn read_session(&self)->ReadSession<'_>{ReadSession{archive:self,validated:Default::default()}}
    pub fn new(root: impl Into<PathBuf>) -> Self { Self {root:root.into()} }
    fn path(&self, kind: &str, id: &str) -> Result<PathBuf,String> {
        if !valid_id(id) { return Err("invalid archive identifier".into()); }
        let extension=if kind=="engines" {"bin"} else {"json"};
        Ok(self.root.join(kind).join(format!("{id}.{extension}")))
    }
    pub(crate) fn write_blob(&self, kind: &str, bytes: &[u8]) -> Result<String,String> {
        let id=content_id(bytes);let dest=self.path(kind,&id)?;
        let dir=dest.parent().unwrap();fs::create_dir_all(dir).map_err(|e|e.to_string())?;
        if dest.exists() { self.read_blob(kind,&id)?;return Ok(id); }
        let temp=dir.join(format!(".pending-{}-{}",std::process::id(),NEXT_TEMP.fetch_add(1,Ordering::Relaxed)));
        let result=(||{
            let mut file=OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e|e.to_string())?;
            file.write_all(bytes).and_then(|_|file.sync_all()).map_err(|e|e.to_string())?;
            // Atomic publication without replacing any existing content-addressed object.
            match fs::hard_link(&temp,&dest) {
                Ok(())=>(),
                Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>{self.read_blob(kind,&id)?;},
                Err(e)=>return Err(e.to_string()),
            }
            fs::File::open(dir).and_then(|d|d.sync_all()).map_err(|e|e.to_string())?;
            Ok(id)
        })();
        let _=fs::remove_file(temp);result
    }
    pub(crate) fn read_blob(&self, kind: &str, id: &str) -> Result<Vec<u8>,String> {
        let bytes=fs::read(self.path(kind,id)?).map_err(|e|e.to_string())?;
        if content_id(&bytes)!=id {return Err(format!("archive integrity failure: {kind}/{id}"));}
        Ok(bytes)
    }
    pub fn save(&self, engine_id: String, dataset: Value, strategy: Value, request: Value, result: Value)->Result<String,String>{
        let dataset_id=self.write_blob("datasets",&encode(&dataset)?)?;
        let strategy_id=self.write_blob("strategies",&encode(&strategy)?)?;
        let run=RunRecord{schema:1,engine_id,dataset_id,strategy_id,request,result};
        self.write_blob("runs",&encode(&run)?)
    }
    fn read_run(&self,id:&str)->Result<RunRecord,String>{
        let run:RunRecord=serde_json::from_slice(&self.read_blob("runs",id)?).map_err(|e|e.to_string())?;
        if run.schema!=1{return Err("unsupported research archive schema".into());}
        Ok(run)
    }
    pub fn load(&self,id:&str)->Result<ResearchBundle,String>{
        let run=self.read_run(id)?;
        let dataset=serde_json::from_slice(&self.read_blob("datasets",&run.dataset_id)?).map_err(|e|e.to_string())?;
        let strategy=serde_json::from_slice(&self.read_blob("strategies",&run.strategy_id)?).map_err(|e|e.to_string())?;
        Ok(ResearchBundle{run_id:id.into(),run,dataset,strategy})
    }
    pub fn list(&self)->Result<Vec<String>,String>{
        let dir=self.root.join("runs");if !dir.exists(){return Ok(Vec::new());}
        let mut ids=Vec::new();
        let mut reader=self.read_session();
        for entry in fs::read_dir(dir).map_err(|e|e.to_string())? {
            let path=entry.map_err(|e|e.to_string())?.path();
            if path.extension().and_then(|v|v.to_str())!=Some("json"){continue;}
            if let Some(id)=path.file_stem().and_then(|v|v.to_str()).filter(|id|valid_id(id)) {
                reader.load_run(id)?;
                ids.push(id.into());
            }
        }
        ids.sort();Ok(ids)
    }
    /// Validate the whole bundle before publishing any record. The run is published last.
    pub fn restore(&self,bundle:&ResearchBundle)->Result<String,String>{
        if bundle.run.schema!=1 || content_id(&encode(&bundle.run)?)!=bundle.run_id
            || content_id(&encode(&bundle.dataset)?)!=bundle.run.dataset_id
            || content_id(&encode(&bundle.strategy)?)!=bundle.run.strategy_id {
            return Err("research backup failed integrity validation".into());
        }
        self.save(bundle.run.engine_id.clone(),bundle.dataset.clone(),bundle.strategy.clone(),bundle.run.request.clone(),bundle.run.result.clone())
    }
    pub fn root(&self)->&Path{&self.root}

    /// Retain exact engine bytes before publishing research. Never executes them.
    /// Existing objects are verified and never replaced, even when corrupted.
    pub fn retain_engine(&self, bytes:&[u8], expected:&str)->Result<String,String>{
        if content_id(bytes)!=expected {return Err("engine bytes differ from the expected identity".into());}
        let path=self.path("engines",expected)?;
        if let Ok(metadata)=fs::symlink_metadata(&path) {
            if !metadata.is_file(){return Err("retained engine is not a regular file".into());}
        }
        self.write_blob("engines",bytes)
    }

    /// Inspect retained bytes without executing them. A matching hash is not proof
    /// of runtime compatibility, source provenance or independent correctness.
    pub fn engine_retention(&self, id:&str)->Value {
        let check=||->Result<&'static str,String>{
            let path=self.path("engines",id)?;
            let metadata=match fs::symlink_metadata(&path) {
                Ok(metadata)=>metadata,
                Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok("missing"),
                Err(e)=>return Err(e.to_string()),
            };
            if !metadata.is_file(){return Err("retained engine is not a regular file".into());}
            let mut file=fs::File::open(path).map_err(|e|e.to_string())?;
            let mut hash=Sha256::new();
            // Stream large debug executables rather than retaining another full copy.
            std::io::copy(&mut file,&mut hash).map_err(|e|e.to_string())?;
            if format!("{:x}",hash.finalize())!=id{return Err("retained engine integrity failure".into());}
            Ok("verified")
        };
        match check() {
            Ok(status)=>serde_json::json!({"status":status}),
            Err(error)=>serde_json::json!({"status":"unverified","error":error}),
        }
    }

    /// This captures the executable available at call time. Call before research;
    /// do not replace a running process's executable in place.
    pub fn retain_current_engine(&self)->Result<String,String>{
        let bytes=fs::read(std::env::current_exe().map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let id=content_id(&bytes);
        self.retain_engine(&bytes,&id)
    }
}

#[cfg(test)]
mod read_session_tests{
    use super::*;
    use serde_json::json;
    #[test]
    fn scoped_reads_validate_inputs_once_but_never_cache_runs_or_failed_validation(){
        let dir=std::env::temp_dir().join(format!("sqx-reader-{:032x}",rand::random::<u128>()));
        struct Cleanup(PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}let _cleanup=Cleanup(dir.clone());
        let archive=Archive::new(dir);let id=archive.save("engine".into(),json!([1,2,3]),json!({"rules":[]}),json!({}),json!({})).unwrap();
        let bundle=archive.load(&id).unwrap();let input=archive.path("datasets",&bundle.run.dataset_id).unwrap();let original=fs::read(&input).unwrap();
        fs::write(&input,b"corrupted").unwrap();let mut reader=archive.read_session();
        assert!(reader.load_run(&id).is_err());assert!(reader.validated.is_empty());
        assert!(reader.load_run(&id).is_err(),"a failed input check must not be cached");
        fs::write(&input,&original).unwrap();reader.load_run(&id).unwrap();assert_eq!(reader.validated.len(),2);
        reader.load_run(&id).unwrap();assert_eq!(reader.validated.len(),2);
        let path=archive.path("runs",&id).unwrap();let run=fs::read(&path).unwrap();fs::write(&path,b"corrupted").unwrap();
        assert!(reader.load_run(&id).is_err(),"run payloads must always be checked");fs::write(&path,run).unwrap();
        fs::remove_file(&input).unwrap();assert!(archive.read_session().load_run(&id).is_err(),"a new request must validate shared inputs again");
    }
}
