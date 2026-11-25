//! Explicit local MCP process settings, separate from browser simulation settings.
use serde::{Deserialize,Serialize};
use std::path::{Path,PathBuf};
use serde_json::{json,Value};
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
 pub schema:u32,pub executable:PathBuf,pub working_directory:PathBuf,
 pub research_directory:PathBuf,pub log_directory:PathBuf,
 pub log_max_bytes:u64,pub log_max_files:usize,pub log_retention_days:u64,
 pub source:Source,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum Source {Request,Csv{path:PathBuf,symbol:String,timeframe:String},Questdb{host:String,port:u16,symbol:String,timeframe:String}}
impl Config {
 pub fn validate(&self)->Result<(),String>{
  if self.schema!=1{return Err("Unsupported MCP configuration schema".into());}
  for (name,path) in [("Executable",&self.executable),("Working directory",&self.working_directory),("Research directory",&self.research_directory),("Log directory",&self.log_directory)]{
   if !path.is_absolute(){return Err(format!("{name} must be an absolute path"));}
  }
  if !self.executable.is_file(){return Err("MCP executable is missing or not a file".into());}
  if !self.working_directory.is_dir(){return Err("MCP working directory is missing".into());}
  if !(4096..=10_485_760).contains(&self.log_max_bytes)||!(2..=100).contains(&self.log_max_files)||!(1..=90).contains(&self.log_retention_days){return Err("Log limits: 4096–10485760 bytes, 2–100 files, 1–90 days".into());}
  match &self.source{
   Source::Request=>{},
   Source::Csv{path,symbol,timeframe}=>{if !path.is_absolute()||!path.is_file(){return Err("CSV source must be an existing absolute file path".into());}validate_market(symbol,timeframe)?;},
   Source::Questdb{host,port,symbol,timeframe}=>{if host.is_empty()||host.len()>253||!host.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'.'||b==b'-')||*port==0{return Err("QuestDB requires a hostname or IPv4 address and port 1–65535".into());}validate_market(symbol,timeframe)?;},
  }
  Ok(())
 }
 pub fn load(path:&Path)->Result<Self,String>{
  use std::io::Read;
  let file=std::fs::File::open(path).map_err(|_|"Cannot open MCP configuration")?;
  let mut bytes=Vec::new();file.take(16_385).read_to_end(&mut bytes).map_err(|_|"Cannot read MCP configuration")?;
  if bytes.len()>16_384{return Err("MCP configuration exceeds 16 KiB".into());}
  let config:Self=serde_json::from_slice(&bytes).map_err(|_|"Invalid MCP configuration structure")?;config.validate()?;Ok(config)
 }
 pub fn identity(&self)->String{crate::archive::content_id(&serde_json::to_vec(self).expect("serializable config"))}
 pub fn client_config(&self,profile:&Path)->Result<Value,String>{
  self.validate()?;if !profile.is_absolute(){return Err("Profile path must be absolute".into());}
  Ok(json!({"mcpServers":{"sqxlabs":{"command":self.executable,"args":[],"env":{"SQX_MCP_CONFIG":profile}}}}))
 }
 /// An explicit source choice in a request replaces the whole default source.
 /// Do not combine a caller's CSV path with a hidden profile symbol/timeframe.
 pub fn apply_source(&self,args:&Value)->Value{
  if ["source","data","symbol","timeframe"].iter().any(|key|args.get(key).is_some()){return args.clone();}
  let mut args=args.clone();if !args.is_object(){return args;}
  match &self.source{
   Source::Request=>{},
   Source::Csv{path,symbol,timeframe}=>{args["source"]=json!("csv");args["data"]=json!(path);args["symbol"]=json!(symbol);args["timeframe"]=json!(timeframe);},
   Source::Questdb{symbol,timeframe,..}=>{args["source"]=json!("questdb");args["symbol"]=json!(symbol);args["timeframe"]=json!(timeframe);},
  }args
 }
}
fn validate_market(symbol:&str,timeframe:&str)->Result<(),String>{
 if symbol.trim().is_empty()||symbol.len()>80{return Err("Source symbol must contain 1–80 characters".into());}
 let tf: sqx_core::Timeframe=timeframe.parse().map_err(|_|"Invalid source timeframe")?;
 if tf==sqx_core::Timeframe::Tick{return Err("Tick cannot describe OHLC source bars".into());}Ok(())
}
#[cfg(test)]mod tests{
 use super::*;
 fn cfg()->Config{Config{schema:1,executable:std::env::current_exe().unwrap(),working_directory:std::env::current_dir().unwrap(),research_directory:std::env::temp_dir().join("sqx-research"),log_directory:std::env::temp_dir().join("sqx-logs"),log_max_bytes:4096,log_max_files:3,log_retention_days:7,source:Source::Questdb{host:"192.168.10.33".into(),port:9000,symbol:"EURUSD".into(),timeframe:"M5".into()}}}
 #[test]fn exact_source_and_client_config(){let c=cfg();c.validate().unwrap();let p=std::env::temp_dir().join("profile.json");let generated=c.client_config(&p).unwrap();assert_eq!(generated["mcpServers"]["sqxlabs"]["env"]["SQX_MCP_CONFIG"],json!(p));let args=c.apply_source(&json!({"count":2}));assert_eq!(args["source"],"questdb");assert_eq!(args["symbol"],"EURUSD");let explicit=json!({"data":"other.csv"});assert_eq!(c.apply_source(&explicit),explicit);let mut changed=c.clone();changed.log_retention_days=8;assert_ne!(changed.identity(),c.identity());}
 #[test]fn invalid_paths_limits_and_unknown_fields_rejected(){let mut c=cfg();c.executable=PathBuf::from("relative");assert!(c.validate().is_err());let mut c=cfg();c.log_retention_days=0;assert!(c.validate().is_err());let mut value=json!(cfg());value["api_key"]=json!("do not persist");assert!(serde_json::from_value::<Config>(value).is_err());}
}
