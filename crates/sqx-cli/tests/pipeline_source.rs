use std::{fs,process::Command};
use serde_json::{json,Value};
#[test]
fn pipeline_binds_seed_and_settings_and_assistant_checks_data_before_provider_access(){
    let root=std::env::temp_dir().join(format!("sqx-cli-pipeline-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();
    let data=root.join("bars.csv");let config=root.join("config.json");
    fs::write(&config,json!({"account":{"currency":"JPY"},"spread_points":7}).to_string()).unwrap();
    fs::write(&data,"timestamp,open,high,low,close,volume\n1700000000,150,151,149,150,1\n1700000060,150,151,149,150,1\n").unwrap();
    let run=|extra:&[&str]|Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["pipeline","--data"]).arg(&data).args(["--count","2","--keep","1"]).args(extra).output().unwrap();
    assert!(!run(&[]).status.success());
    let out=run(&["--symbol","USDJPY","--timeframe","M1","--config",config.to_str().unwrap(),"--seed","731"]);assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let text=String::from_utf8(out.stdout).unwrap();let context:Value=serde_json::from_str(text.lines().find_map(|s|s.strip_prefix("Research context: ")).unwrap()).unwrap();
    assert_eq!(context["seed"],731);assert_eq!(context["symbol"],"USDJPY");assert_eq!(context["config"]["account"]["currency"],"JPY");assert_eq!(context["pipeline"]["build_evaluations"],2);
    fs::write(&data,"timestamp,open,high,low,close,volume\n1700000000,150,151,149,150,1\n1700000120,150,151,149,150,1\n").unwrap();
    let gap=run(&["--symbol","USDJPY","--timeframe","M1"]);assert!(!gap.status.success());assert!(String::from_utf8_lossy(&gap.stderr).contains("Unresolved data gaps"));
    let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
    let endpoint=format!("http://{}/v1",listener.local_addr().unwrap());
    let assistant=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["assistant","--data"]).arg(&data).args(["--symbol","USDJPY","--timeframe","M1","--base-url",&endpoint,"--api-key","fixture-key","--ask","backtest this data"]).output().unwrap();
    assert!(!assistant.status.success());assert!(String::from_utf8_lossy(&assistant.stderr).contains("Unresolved data gaps"));
    assert_eq!(listener.accept().unwrap_err().kind(),std::io::ErrorKind::WouldBlock,"invalid research input must not contact the provider");
    fs::remove_dir_all(root).unwrap();
}
