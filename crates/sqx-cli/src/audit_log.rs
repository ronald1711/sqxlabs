//! Append-only CLI evidence stream. A missing terminal event means incomplete execution.
use std::{fs::{File,OpenOptions},io::Write,path::Path};
pub struct AuditLog {file:File}
impl AuditLog {
    pub fn create(path:&Path)->std::io::Result<Self>{
        let file=OpenOptions::new().write(true).create_new(true).open(path)?;
        // Make the directory entry durable before recording research events.
        let parent=path.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or_else(||Path::new("."));
        File::open(parent)?.sync_all()?;
        Ok(Self{file})
    }
    pub fn append(&mut self,event:&serde_json::Value)->std::io::Result<()>{
        let mut bytes=serde_json::to_vec(event)?;bytes.push(b'\n');
        self.file.write_all(&bytes)?;self.file.sync_all()
    }
}

fn next_event(reader:&mut impl std::io::BufRead)->Result<Option<serde_json::Value>,String>{
    use std::io::{BufRead,Read};
    const LIMIT:u64=64*1024*1024;
    let mut bytes=Vec::new();
    let n=(&mut *reader).take(LIMIT+1).read_until(b'\n',&mut bytes).map_err(|e|e.to_string())?;
    if n==0{return Ok(None);}
    if n as u64>LIMIT{return Err("Audit event exceeds the 64 MiB replay limit".into());}
    if bytes.last()!=Some(&b'\n'){return Err("Incomplete audit event: missing final newline".into());}
    serde_json::from_slice(&bytes).map(Some).map_err(|e|format!("Invalid audit event: {e}"))
}
/// Replays frozen input, streaming expected events rather than loading a potentially
/// multi-generation log into memory. It never writes the log or executes stored code.
pub fn verify(path:&Path)->Result<usize,String>{
    use serde_json::json;
    let mut reader=std::io::BufReader::new(File::open(path).map_err(|e|e.to_string())?);
    let header=next_event(&mut reader)?.ok_or("Empty audit log")?;
    if header["kind"]!="started" || header["schema"]!=1 || header["fitness"]!="ProfitFactor"{return Err("Unsupported evolution log header".into());}
    let executable=std::fs::read(std::env::current_exe().map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if header["engine_id"]!=sqx_project::archive::content_id(&executable){return Err("Evolution replay requires the original engine executable".into());}
    let bars:Vec<sqx_core::Bar>=serde_json::from_value(header["bars"].clone()).map_err(|e|e.to_string())?;
    let instrument:sqx_core::Instrument=serde_json::from_value(header["instrument"].clone()).map_err(|e|e.to_string())?;
    let config:sqx_backtest::BacktestConfig=serde_json::from_value(header["backtest_config"].clone()).map_err(|e|e.to_string())?;
    let generator:sqx_optimizer::GeneratorConfig=serde_json::from_value(header["generator"].clone()).map_err(|e|e.to_string())?;
    let ga:sqx_optimizer::GeneticConfig=serde_json::from_value(header["genetic"].clone()).map_err(|e|e.to_string())?;
    let pyp=header["periods_per_year"].as_f64().filter(|v|v.is_finite()&&*v>0.0).ok_or("Invalid archived annualization")?;
    let seed=header["seed"].as_u64().ok_or("Invalid archived seed")?;
    let mut attempted=0;let mut invalid=0;let mut eligible=0;
    let ranked=sqx_optimizer::genetic_evolve_observed(seed,&generator,&ga,sqx_optimizer::Fitness::ProfitFactor,&bars,&instrument,&config,pyp,|generation,index,evaluation,qualifies|{
        let recorded=next_event(&mut reader)?.ok_or("Incomplete log: missing candidate evaluation")?;
        let actual=json!({"kind":"evaluation","generation":generation,"candidate_index":index,"strategy":evaluation.strategy,"metrics":evaluation.metrics,"errors":evaluation.errors,"eligible":qualifies});
        if recorded!=actual{return Err(format!("Replay mismatch at generation {generation}, candidate {index}"));}
        attempted+=1;if qualifies{eligible+=1;}if !evaluation.errors.is_empty(){invalid+=1;}Ok(())
    })?;
    let terminal=next_event(&mut reader)?.ok_or("Incomplete log: missing completed event")?;
    let expected=json!({"kind":"completed","attempted":attempted,"invalid":invalid,"eligible":eligible,"ranked":ranked.iter().map(|(strategy,metrics)|json!({"strategy":strategy,"metrics":metrics})).collect::<Vec<_>>()});
    if terminal!=expected{return Err("Final ranking/counts do not match a completed search".into());}
    if next_event(&mut reader)?.is_some(){return Err("Unexpected events after completion".into());}
    Ok(attempted)
}
