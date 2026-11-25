//! Bounded local stdio handshake. Never invokes a research tool.
use std::{io::{BufRead,BufReader,Read,Write},process::{Child,Command,Stdio},sync::mpsc,time::{Duration,Instant}};
use serde_json::{json,Value};
use crate::mcp_config::Config;
struct Process(Child);
impl Drop for Process{fn drop(&mut self){
 #[cfg(unix)] unsafe{libc::kill(-(self.0.id() as i32),libc::SIGKILL);}
 let _=self.0.kill();let _=self.0.wait();
}}
fn reader(pipe:impl Read+Send+'static)->mpsc::Receiver<Result<Value,&'static str>>{
 let (tx,rx)=mpsc::channel();std::thread::spawn(move||{
  let mut reader=BufReader::new(pipe);let mut total=0;
  loop{let mut line=Vec::new();match reader.by_ref().take(1_048_577).read_until(b'\n',&mut line){
   Ok(0)=>break,Ok(_)=>{total+=line.len();if line.len()>1_048_576||total>2_097_152{let _=tx.send(Err("MCP response exceeded limit"));break;}
    let result=serde_json::from_slice(&line).map_err(|_|"Invalid MCP JSON response");if tx.send(result).is_err(){break;}
   },Err(_)=>{let _=tx.send(Err("Cannot read MCP response"));break;}
  }}
 });rx
}
fn receive(rx:&mpsc::Receiver<Result<Value,&'static str>>,id:u64,deadline:Instant)->Result<Value,String>{
 let remaining=deadline.saturating_duration_since(Instant::now());
 let value=rx.recv_timeout(remaining).map_err(|_|"MCP response missing or timed out")?.map_err(String::from)?;
 if value["jsonrpc"]!="2.0"||value["id"]!=id||value.get("error").is_some(){return Err("MCP protocol response mismatch".into());}Ok(value["result"].clone())
}
pub fn probe(config:&Config,profile:&std::path::Path,timeout:Duration)->Result<Value,String>{
 config.validate()?;
 // Match the file the subprocess will read, not merely the browser's form values.
 if Config::load(profile)?!=*config{return Err("Saved MCP profile changed; save and test again".into());}
 let executable=std::fs::read(&config.executable).map_err(|_|"Cannot read MCP executable")?;
 let executable_id=crate::archive::content_id(&executable);
 let deadline=Instant::now()+timeout;
 let mut command=Command::new(&config.executable);command.current_dir(&config.working_directory).env("SQX_MCP_CONFIG",profile).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
 #[cfg(unix)] {use std::os::unix::process::CommandExt;command.process_group(0);}
 let mut process=Process(command.spawn().map_err(|_|"Cannot start MCP executable")?);
 let (error_tx,error_rx)=mpsc::channel();
 let stderr=process.0.stderr.take().ok_or("MCP stderr unavailable")?;
 std::thread::spawn(move||{let mut bytes=Vec::new();let read=stderr.take(65_537).read_to_end(&mut bytes);let text=String::from_utf8_lossy(&bytes);let _=error_tx.send((read.is_err()||bytes.len()>65_536,text.contains("SQX_MCP_LOG_UNAVAILABLE"),text.contains("SQX_MCP_LOG_REPAIRED")));});
 let rx=reader(process.0.stdout.take().ok_or("MCP stdout unavailable")?);
 let mut input=process.0.stdin.take().ok_or("MCP stdin unavailable")?;
 writeln!(input,"{}",json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"SqxLabs configuration probe","version":"1"}}})).map_err(|_|"Cannot write MCP handshake")?;
 let init=receive(&rx,1,deadline)?;
 if init["serverInfo"]["name"]!="sqxlabs"||init["protocolVersion"]!="2024-11-05"||!init["capabilities"]["tools"].is_object(){return Err("Executable did not advertise the expected SqxLabs MCP protocol".into());}
 writeln!(input,"{}",json!({"jsonrpc":"2.0","method":"notifications/initialized"})).map_err(|_|"Cannot acknowledge MCP initialization")?;
 writeln!(input,"{}",json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).map_err(|_|"Cannot request MCP tool list")?;
 let result=receive(&rx,2,deadline)?;
 let tools=result["tools"].as_array().ok_or("MCP tool list missing")?;
 if tools.is_empty()||tools.iter().any(|t|!t["name"].is_string()||!t["inputSchema"].is_object()){return Err("MCP tool list invalid".into());}
 drop(input);
 loop{if let Some(status)=process.0.try_wait().map_err(|_|"Cannot inspect MCP process")?{if !status.success(){return Err("MCP test process exited unsuccessfully".into());}break;}if Instant::now()>=deadline{return Err("MCP process did not exit after handshake test".into());}std::thread::sleep(Duration::from_millis(10));}
 let (stderr_limit,logging_error,repaired)=error_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).map_err(|_|"MCP diagnostic stream did not close")?;
 if stderr_limit{return Err("MCP diagnostic output exceeded limit or could not be read".into());}
 Ok(json!({"logging_warning":logging_error,"log_history_repaired":repaired,"configuration_id":config.identity(),"executable_id":executable_id,"handshake":"passed","tool_count":tools.len(),"research_executed":false,"probe_process":"exited","external_client_connection":"unknown","scope":"This local probe does not establish a Claude or ChatGPT connection."}))
}
