//! Local MCP profile persistence and explicit bounded probe endpoints.
use std::{path::{Path,PathBuf},sync::Mutex};
use serde_json::{json,Value};
use sqx_project::mcp_config::{Config,Source};
static MUTATION:Mutex<()>=Mutex::new(());
fn storage()->Result<PathBuf,String>{std::env::current_dir().map(|p|p.join("data/mcp")).map_err(|_|"Cannot locate MCP settings directory".into())}
fn write_json(path:&Path,value:&Value)->Result<(),String>{
 use std::io::Write;
 let parent=path.parent().ok_or("Invalid settings destination")?;std::fs::create_dir_all(parent).map_err(|_|"Cannot create MCP settings directory")?;
 let temporary=parent.join(format!("write-{:032x}.tmp",rand::random::<u128>()));
 let result=(||{let mut options=std::fs::OpenOptions::new();options.create_new(true).write(true);
 #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
 let mut file=options.open(&temporary).map_err(|_|"Cannot create MCP settings file")?;
 file.write_all(serde_json::to_string_pretty(value).map_err(|_|"Cannot encode MCP settings")?.as_bytes()).map_err(|_|"Cannot write MCP settings")?;
 file.sync_all().map_err(|_|"Cannot sync MCP settings")?;std::fs::rename(&temporary,path).map_err(|_|"Cannot publish MCP settings")?;Ok(())})();
 if result.is_err(){let _=std::fs::remove_file(temporary);}result
}
fn current(root:&Path)->Result<(Config,PathBuf),String>{
 let bytes=std::fs::read(root.join("active.json")).map_err(|_|"No saved MCP profile")?;
 let pointer:Value=serde_json::from_slice(&bytes).map_err(|_|"Invalid MCP profile pointer")?;
 let id=pointer["configuration_id"].as_str().filter(|id|id.len()==64&&id.bytes().all(|b|b.is_ascii_hexdigit())).ok_or("Invalid MCP profile identity")?;
 let path=root.join("profiles").join(format!("{id}.json"));let config=Config::load(&path)?;
 if config.identity()!=id{return Err("MCP profile identity mismatch".into());}Ok((config,path))
}
fn template()->Result<Config,String>{
 let cwd=std::env::current_dir().map_err(|_|"Cannot locate working directory")?;
 let mut exe=std::env::current_exe().map_err(|_|"Cannot locate executable")?;
 exe.set_file_name(if cfg!(windows){"sqx-mcp.exe"}else{"sqx-mcp"});
 Ok(Config{schema:1,executable:exe,working_directory:cwd.clone(),research_directory:cwd.join("data/research"),log_directory:cwd.join("data/mcp-logs"),log_max_bytes:1_048_576,log_max_files:5,log_retention_days:14,source:Source::Request})
}
fn read(root:&Path)->Result<Value,String>{
 if !root.join("active.json").exists(){return Ok(json!({"saved":false,"config":template()?,"probe":null,"external_client_connection":"unknown","process_status":"unknown"}));}
 let (config,path)=current(root)?;
 let probe=std::fs::read(root.join(format!("{}.probe.json",config.identity()))).ok().and_then(|v|serde_json::from_slice::<Value>(&v).ok());
 Ok(json!({"saved":true,"configuration_id":config.identity(),"config":config,"client_config":config.client_config(&path)?,"probe":probe,"external_client_connection":"unknown","process_status":"unknown","scope":"Saving does not restart any existing MCP process. The profile applies when a client next starts it."}))
}
pub fn route(action:&str,body:&str)->String{
 let result=(||->Result<Value,String>{let root=storage()?;
 if action=="read"{return read(&root);}
 let _guard=MUTATION.try_lock().map_err(|_|"Another MCP configuration operation is running")?;
 let body:Value=serde_json::from_str(body).map_err(|_|"Invalid MCP settings request")?;
 match action {
 "save"=>{let config:Config=serde_json::from_value(body).map_err(|_|"Invalid MCP configuration fields")?;config.validate()?;
  let id=config.identity();let path=root.join("profiles").join(format!("{id}.json"));write_json(&path,&json!(config))?;write_json(&root.join("active.json"),&json!({"configuration_id":id}))?;read(&root)
 },
 "probe"=>{let (config,path)=current(&root)?;if body["configuration_id"]!=config.identity(){return Err("Save or reload the current profile before testing".into());}
  let mut result=match sqx_project::mcp_probe::probe(&config,&path,std::time::Duration::from_secs(8)){Ok(v)=>v,Err(error)=>json!({"configuration_id":config.identity(),"handshake":"failed","error":error})};
  result["tested_at_utc_ms"]=json!(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis());
  write_json(&root.join(format!("{}.probe.json",config.identity())),&result)?;Ok(result)
 },_=>Err("Unknown MCP settings action".into())}
 })();match result{Ok(v)=>v.to_string(),Err(e)=>json!({"error":e}).to_string()}
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn immutable_profile_pointer_roundtrip_and_tamper_rejection(){
  let root=std::env::temp_dir().join(format!("sqx-mcp-settings-{:032x}",rand::random::<u128>()));
  let config=Config{schema:1,executable:std::env::current_exe().unwrap(),working_directory:std::env::current_dir().unwrap(),research_directory:root.join("archive"),log_directory:root.join("logs"),log_max_bytes:4096,log_max_files:2,log_retention_days:7,source:Source::Request};
  let id=config.identity();let path=root.join("profiles").join(format!("{id}.json"));write_json(&path,&json!(config)).unwrap();write_json(&root.join("active.json"),&json!({"configuration_id":id})).unwrap();
  let loaded=read(&root).unwrap();assert_eq!(loaded["config"],json!(config));assert_eq!(loaded["process_status"],"unknown");assert_eq!(loaded["external_client_connection"],"unknown");assert_eq!(loaded["client_config"]["mcpServers"]["sqxlabs"]["env"]["SQX_MCP_CONFIG"],json!(path));
  write_json(&root.join("active.json"),&json!({"configuration_id":"../outside"})).unwrap();assert!(current(&root).is_err());
  write_json(&root.join("active.json"),&json!({"configuration_id":id})).unwrap();let mut changed=config.clone();changed.log_retention_days=8;write_json(&path,&json!(changed)).unwrap();assert!(current(&root).is_err());std::fs::remove_dir_all(root).unwrap();
 }
}
