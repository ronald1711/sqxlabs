use std::{fs,process::Command};
#[test]
fn cloud_requires_closed_declared_data_and_complete_strategy_loading(){
    let root=std::env::temp_dir().join(format!("sqx-cli-cloud-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));let strategies=root.join("strategies");fs::create_dir_all(&strategies).unwrap();let data=root.join("bars.csv");
    fs::write(strategies.join("a.json"),serde_json::to_vec(&sqx_strategy::Strategy::new("empty")).unwrap()).unwrap();
    fs::write(&data,"timestamp,open,high,low,close,volume\n1700000000,1,1.1,0.9,1,1\n").unwrap();
    let run=|args:&[&str]|Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["cloud","--strategies"]).arg(&strategies).arg("--data").arg(&data).args(args).output().unwrap();
    assert!(!run(&[]).status.success());
    let valid=run(&["--symbol","EURUSD","--timeframe","M1"]);assert!(valid.status.success(),"{}",String::from_utf8_lossy(&valid.stderr));assert!(String::from_utf8_lossy(&valid.stdout).contains("Raw entry indications only"));
    fs::write(strategies.join("b.json"),"broken").unwrap();let broken=run(&["--symbol","EURUSD","--timeframe","M1"]);assert!(!broken.status.success());assert!(broken.stdout.is_empty());fs::remove_file(strategies.join("b.json")).unwrap();
    let future=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+3600;fs::write(&data,format!("timestamp,open,high,low,close,volume\n{future},1,1.1,0.9,1,1\n")).unwrap();
    let unfinished=run(&["--symbol","EURUSD","--timeframe","M1","--allow-data-gaps"]);assert!(!unfinished.status.success());assert!(unfinished.stdout.is_empty());assert!(String::from_utf8_lossy(&unfinished.stderr).contains("not closed"));fs::remove_dir_all(root).unwrap();
}
