use serde_json::json;
use sqx_project::{archive::Archive, assistant_trials::{execute, list}};
use sqx_backtest::BacktestConfig;
use sqx_core::{Bar,Instrument};
use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Comparator,Operand,Join,MoneyManagement};
static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {fn new()->Self{Self(std::env::temp_dir().join(format!("sqx-ai-replay-{}-{:?}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed))))}}
impl Drop for Temp {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn checkpoint(archive:&Archive, invalid:bool)->String {
    let mut strategy=Strategy::new("frozen");strategy.long=true;strategy.short=false;
    strategy.money_management=MoneyManagement::FixedSize{size:if invalid {0.0}else{0.1}};
    strategy.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));
    let bars:Vec<_>=(0..4).map(|i|Bar::new(i*60000,1.0+i as f64*0.01,1.1,0.9,1.0,1.0)).collect();
    archive.save("engine".into(),json!({"symbol":"EURUSD","bars":bars,"instrument":Instrument::fx("EURUSD")}),json!(strategy),json!({"research_kind":"assistant_trial_started","session_id":"fixture-session","tool":"generate","tool_call":1,"index":0,"config":BacktestConfig::default(),"periods_per_year":100.0,"seed":7,"source_context":{}}),json!({"status":"execution_not_recorded"})).unwrap()
}
#[test]
fn pending_trial_restarts_replays_and_deduplicates_without_provider() {
    let temp=Temp::new();let archive=Archive::new(&temp.0);let id=checkpoint(&archive,false);
    assert_eq!(list(&archive,"other").unwrap()["checkpoints"][0]["matching_engine"],false);
    assert!(execute(&archive,&id,"other",true).unwrap_err().contains("engine executable differs"));
    assert!(execute(&archive,&id,"engine",false).unwrap_err().contains("record type"));
    assert_eq!(archive.list().unwrap().len(),1);
    assert_eq!(list(&archive,"engine").unwrap()["checkpoints"][0]["state"],"result_not_recorded");
    let resumed=execute(&archive,&id,"engine",true).unwrap();
    let result_id=resumed["run_id"].as_str().unwrap();
    assert_eq!(resumed["execution_valid"],true);
    assert_eq!(execute(&archive,result_id,"engine",false).unwrap()["matches"],true);
    assert_eq!(execute(&archive,&id,"engine",true).unwrap(),resumed);
    assert_eq!(archive.list().unwrap().len(),2);
    let result=archive.load(result_id).unwrap();
    assert!(!result.run.result["trades"].as_array().unwrap().is_empty());
    assert_eq!(list(&archive,"engine").unwrap()["checkpoints"][0]["state"],"result_recorded");
    // A single result bundle remains independently replayable after portable restore.
    let restored=Archive::new(temp.0.join("restored"));restored.restore(&result).unwrap();
    assert_eq!(list(&restored,"engine").unwrap()["standalone_results"].as_array().unwrap().len(),1);
    assert_eq!(execute(&restored,result_id,"engine",false).unwrap()["matches"],true);
}
#[test]
fn altered_results_and_contradictory_links_are_not_silently_accepted() {
    let temp=Temp::new();let archive=Archive::new(&temp.0);let id=checkpoint(&archive,false);
    let resumed=execute(&archive,&id,"engine",true).unwrap();
    let original=archive.load(resumed["run_id"].as_str().unwrap()).unwrap();
    let mut changed=original.run.result.clone();changed["equity"][0][1]=json!(123.0);
    let altered=archive.save("engine".into(),original.dataset.clone(),original.strategy.clone(),original.run.request.clone(),changed).unwrap();
    assert_eq!(execute(&archive,&altered,"engine",false).unwrap()["matches"],false);
    let before=archive.list().unwrap();
    assert!(execute(&archive,&id,"engine",true).unwrap_err().contains("differs from an existing linked result"));
    assert_eq!(archive.list().unwrap(),before);
    let mut request=original.run.request;request["index"]=json!(1);
    let contradictory=archive.save("engine".into(),original.dataset,original.strategy,request,original.run.result).unwrap();
    assert!(execute(&archive,&contradictory,"engine",false).unwrap_err().contains("checkpoint identity"));
    assert!(list(&archive,"engine").unwrap_err().contains("checkpoint identity"));
    assert!(execute(&archive,&id,"engine",true).unwrap_err().contains("checkpoint identity"));
}
#[test]
fn invalid_trial_results_remain_distinct_from_replay_agreement() {
    let temp=Temp::new();let archive=Archive::new(&temp.0);let id=checkpoint(&archive,true);
    let resumed=execute(&archive,&id,"engine",true).unwrap();
    assert_eq!(resumed["execution_valid"],false);
    assert!(!resumed["errors"].as_array().unwrap().is_empty());
    let replay=execute(&archive,resumed["run_id"].as_str().unwrap(),"engine",false).unwrap();
    assert_eq!(replay["matches"],true);assert_eq!(replay["execution_valid"],false);
}
