//! Bounded protocol-independent logs. Only application-owned categories are accepted.
use std::{fs::{self,File,OpenOptions},io::{self,Read,Write},path::{Path,PathBuf},time::{Duration,Instant,SystemTime,UNIX_EPOCH}};
use serde_json::json;

#[derive(Clone)]
pub struct Config {pub directory:PathBuf,pub max_bytes:u64,pub max_files:usize,pub retention_days:u64}
impl Config {
    pub fn from_env()->Result<Self,String>{
        fn number(name:&str,default:u64,min:u64,max:u64)->Result<u64,String>{
            match std::env::var(name){Ok(v)=>v.parse::<u64>().ok().filter(|v|(*v>=min)&&(*v<=max)).ok_or_else(||format!("Invalid {name}; allowed range {min}..{max}")),Err(std::env::VarError::NotPresent)=>Ok(default),Err(_)=>Err(format!("Invalid {name}"))}
        }
        Ok(Self{directory:std::env::var_os("SQX_MCP_LOG_DIR").map(PathBuf::from).unwrap_or_else(||PathBuf::from("data/mcp-logs")),max_bytes:number("SQX_MCP_LOG_MAX_BYTES",1_048_576,4096,10_485_760)?,max_files:number("SQX_MCP_LOG_MAX_FILES",5,2,100)? as usize,retention_days:number("SQX_MCP_LOG_RETENTION_DAYS",14,1,90)?})
    }
}
fn open_private(path:&Path)->io::Result<File>{
    let mut options=OpenOptions::new();options.create(true).append(true);
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    options.open(path)
}
fn timestamp()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(u64::MAX as u128) as u64}
pub struct Log {config:Config,session:String,sequence:u64,client_kind:&'static str,configuration_id:Option<String>,lease:Option<sqx_project::mcp_runtime::Lease>,initialized:bool,handshake_started:bool,pub degraded:bool}
impl Log {
    pub fn new(config:Config)->Self{
        let lease=sqx_project::mcp_runtime::Lease::acquire(&config.directory).map_err(|_|eprintln!("SQX_MCP_STATUS_UNAVAILABLE: process status cannot be recorded; protocol continues.")).ok();
        Self{config,session:format!("{:032x}",rand::random::<u128>()),sequence:0,client_kind:"unknown",configuration_id:super::PROCESS_CONFIG.get().map(|c|c.identity()),lease,initialized:false,handshake_started:false,degraded:false}
    }
    pub fn observe_client(&mut self,name:Option<&str>){
        self.handshake_started=true;
        // A client's declared name is not authentication. Never store the original string.
        self.client_kind=match name {Some("SqxLabs configuration probe")=>"probe",Some("claude-ai"|"Claude Desktop"|"claude-desktop")=>"claude_declared",Some("ChatGPT"|"chatgpt")=>"chatgpt_declared",Some(_)=>"other",None=>"unknown"};
    }
    pub fn request(&mut self)->u64{self.sequence+=1;self.sequence}
    pub fn event(&mut self,request:u64,method:&str,tool:Option<&str>,status:&str,duration_ms:u128){
        let method=match method{"initialize"|"notifications/initialized"|"tools/list"|"tools/call"|"process"|"parse"=>method,_=>"unknown"};
        let status=match status{"started"|"completed"|"tool_error"|"parse_error"|"unsupported_method"|"stdin_error"|"stopped"=>status,_=>"unknown"};
        // Tool strings must be selected from our catalog at the call site, never from params directly.
        let tool=tool.filter(|name|super::tools_list().iter().any(|t|t["name"].as_str()==Some(name)));
        if self.handshake_started&&method=="notifications/initialized"&&status=="completed"{self.initialized=true;}
        let row=json!({"schema":1,"configuration_id":self.configuration_id,"client_kind":self.client_kind,"utc_unix_ms":timestamp(),"pid":std::process::id(),"session":self.session,"request_sequence":request,"method":method,"tool":tool,"status":status,"duration_ms":duration_ms.min(u64::MAX as u128) as u64});
        if self.append(&row.to_string()).is_err(){
            self.degraded=true;
            eprintln!("SQX_MCP_LOG_UNAVAILABLE: diagnostic event could not be stored; protocol continues. Check log directory permissions, space and concurrent writers.");
        }
        if let Some(lease)=self.lease.as_mut(){if lease.update(self.configuration_id.as_deref(),&self.session,self.client_kind,self.initialized,self.degraded).is_err(){eprintln!("SQX_MCP_STATUS_UNAVAILABLE: process status cannot be updated; protocol continues.");self.lease=None;}}
    }
        
    // Called under the writer lock. Retention follows event timestamps, not file mtime.
    fn maintain(&self,now:u64)->io::Result<()>{
        let cutoff=now.saturating_sub(self.config.retention_days*86_400_000);
        for entry in fs::read_dir(&self.config.directory)? {
            let entry=entry?;let name=entry.file_name();let Some(name)=name.to_str() else{continue};
            let index=if name=="mcp.jsonl"{0}else{
                let Some(number)=name.strip_prefix("mcp.").and_then(|x|x.strip_suffix(".jsonl")) else{continue};
                let Ok(index)=number.parse::<usize>() else{continue};
                if index==0||number!=index.to_string(){continue;}index
            };
            if !entry.file_type()?.is_file(){return Err(io::Error::other("invalid diagnostic file type"));}
            if index>=self.config.max_files{fs::remove_file(entry.path())?;continue;}
            let mut raw=String::new();File::open(entry.path())?.take(10_485_761).read_to_string(&mut raw)?;
            if raw.len()>10_485_760{return Err(io::Error::other("diagnostic file exceeds read limit"));}
            let mut kept=Vec::new();let mut bytes=0;let mut repaired=false;
            for line in raw.lines().rev(){
                let Ok(value)=serde_json::from_str::<serde_json::Value>(line) else{repaired=true;continue};
                let Some(at)=value["utc_unix_ms"].as_u64() else{repaired=true;continue};
                if at<cutoff{continue;}
                if bytes+line.len()+1<=self.config.max_bytes as usize{kept.push(line);bytes+=line.len()+1;}
            }
            kept.reverse();let normalized=if kept.is_empty(){String::new()}else{kept.join("\n")+"\n"};
            if normalized!=raw{
                let temporary=self.config.directory.join("rewrite.tmp");
                // A previous interrupted rewrite never replaces the source until fully written.
                if temporary.exists(){fs::remove_file(&temporary)?;}
                let mut output=open_private(&temporary)?;output.write_all(normalized.as_bytes())?;output.sync_all()?;
                fs::rename(temporary,entry.path())?;
            }
            if repaired{eprintln!("SQX_MCP_LOG_REPAIRED: incomplete or invalid diagnostic records were removed; history is incomplete.");}
        }
        Ok(())
    }
    fn append(&self,line:&str)->io::Result<()>{
        fs::create_dir_all(&self.config.directory)?;
        let lock=open_private(&self.config.directory.join("writer.lock"))?;
        let deadline=Instant::now()+Duration::from_secs(2);
        loop {match lock.try_lock(){Ok(())=>break,Err(std::fs::TryLockError::WouldBlock) if Instant::now()<deadline=>std::thread::sleep(Duration::from_millis(2)),Err(_)=>return Err(io::Error::other("log lock unavailable"))}}
        let active=self.config.directory.join("mcp.jsonl");
        self.maintain(timestamp())?;
        if fs::metadata(&active).map(|m|m.len()).unwrap_or(0)+line.len() as u64+1>self.config.max_bytes {
            for n in (1..self.config.max_files).rev(){
                let target=self.config.directory.join(format!("mcp.{n}.jsonl"));
                let source=if n==1{active.clone()}else{self.config.directory.join(format!("mcp.{}.jsonl",n-1))};
                if target.exists(){fs::remove_file(&target)?;}
                if source.exists(){fs::rename(source,target)?;}
            }
        }
        let mut out=open_private(&active)?;writeln!(out,"{line}")?;out.sync_data()
    }
}

#[cfg(test)]
mod tests {
 use super::*;
 fn cfg()->Config{Config{directory:std::env::temp_dir().join(format!("sqx-log-{:032x}",rand::random::<u128>())),max_bytes:4096,max_files:3,retention_days:1}}
 #[test] fn log_bounds_rotates_and_never_records_supplied_secrets(){
  let c=cfg();let mut log=Log::new(c.clone());
  for _ in 0..100{let id=log.request();log.event(id,"SECRET",Some("SECRET"),"SECRET",12);}
  assert!(!log.degraded);let files:Vec<_>=fs::read_dir(&c.directory).unwrap().flatten().filter(|x|x.path().extension().is_some_and(|v|v=="jsonl")).collect();assert!(files.len()<=3);
  for f in files{let text=fs::read_to_string(f.path()).unwrap();assert!(text.len()<=4096);assert!(!text.contains("SECRET"));for l in text.lines(){let v:serde_json::Value=serde_json::from_str(l).unwrap();assert_eq!(v["method"],"unknown");assert!(v["tool"].is_null());}}
  fs::remove_dir_all(c.directory).unwrap();
 }
 #[test] fn unwritable_destination_is_visible_and_nonfatal(){let c=cfg();fs::write(&c.directory,"not a directory").unwrap();let mut log=Log::new(c.clone());log.event(0,"process",None,"started",0);assert!(log.degraded);fs::remove_file(c.directory).unwrap();}
 #[test] fn concurrent_writers_keep_complete_records(){let mut c=cfg();c.max_bytes=1_048_576;let threads:Vec<_>=(0..4).map(|_|{let c=c.clone();std::thread::spawn(move||{let mut l=Log::new(c);for n in 0..20{l.event(n,"tools/list",None,"completed",0);}assert!(!l.degraded);})}).collect();for t in threads{t.join().unwrap();}let text=fs::read_to_string(c.directory.join("mcp.jsonl")).unwrap();assert_eq!(text.lines().count(),80);for l in text.lines(){serde_json::from_str::<serde_json::Value>(l).unwrap();}fs::remove_dir_all(c.directory).unwrap();}
 #[test] fn retention_uses_event_age_repairs_partial_tail_and_honors_lowered_limits(){
  let c=cfg();fs::create_dir_all(&c.directory).unwrap();let now=timestamp();
  let old=json!({"utc_unix_ms":now-172_800_000,"status":"old"}).to_string();
  let recent=json!({"utc_unix_ms":now,"status":"recent"}).to_string();
  fs::write(c.directory.join("mcp.jsonl"),format!("{old}\n{recent}\n{{partial")).unwrap();
  fs::write(c.directory.join("mcp.9.jsonl"),&recent).unwrap();
  fs::write(c.directory.join("unrelated.jsonl"),"keep me").unwrap();
  let mut log=Log::new(c.clone());log.event(1,"process",None,"started",0);assert!(!log.degraded);
  let text=fs::read_to_string(c.directory.join("mcp.jsonl")).unwrap();assert!(!text.contains("old"));assert!(!text.contains("partial"));assert!(text.contains("recent"));assert!(!c.directory.join("mcp.9.jsonl").exists());assert_eq!(fs::read_to_string(c.directory.join("unrelated.jsonl")).unwrap(),"keep me");
  for line in text.lines(){serde_json::from_str::<serde_json::Value>(line).unwrap();}
  fs::remove_dir_all(c.directory).unwrap();
 }

}
