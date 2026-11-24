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
impl Archive {
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
    pub fn load(&self,id:&str)->Result<ResearchBundle,String>{
        let run:RunRecord=serde_json::from_slice(&self.read_blob("runs",id)?).map_err(|e|e.to_string())?;
        if run.schema!=1{return Err("unsupported research archive schema".into());}
        let dataset=serde_json::from_slice(&self.read_blob("datasets",&run.dataset_id)?).map_err(|e|e.to_string())?;
        let strategy=serde_json::from_slice(&self.read_blob("strategies",&run.strategy_id)?).map_err(|e|e.to_string())?;
        Ok(ResearchBundle{run_id:id.into(),run,dataset,strategy})
    }
    pub fn list(&self)->Result<Vec<String>,String>{
        let dir=self.root.join("runs");if !dir.exists(){return Ok(Vec::new());}
        let mut ids=Vec::new();
        for entry in fs::read_dir(dir).map_err(|e|e.to_string())? {
            let path=entry.map_err(|e|e.to_string())?.path();
            if path.extension().and_then(|v|v.to_str())!=Some("json"){continue;}
            if let Some(id)=path.file_stem().and_then(|v|v.to_str()).filter(|id|valid_id(id)) {self.load(id)?;ids.push(id.into());}
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

    /// This captures the executable available at call time. Call before research;
    /// do not replace a running process's executable in place.
    pub fn retain_current_engine(&self)->Result<String,String>{
        let bytes=fs::read(std::env::current_exe().map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let id=content_id(&bytes);
        self.retain_engine(&bytes,&id)
    }
}
