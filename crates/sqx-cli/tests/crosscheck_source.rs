use std::{fs,process::Command};
use serde_json::{json,Value};
#[test]
fn crosscheck_uses_explicit_manifest_and_retains_failed_market_evidence(){
    let root=std::env::temp_dir().join(format!("sqx-cli-crosscheck-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();
    let manifest=root.join("markets.json");let strategy=root.join("strategy.json");
    fs::write(root.join("arbitrary.csv"),"timestamp,open,high,low,close,volume\n1700000000,1,1.1,0.9,1,1\n1700000060,1,1.1,0.9,1,1\n").unwrap();
    fs::write(root.join("jpy.json"),json!({"account":{"currency":"JPY"},"commission_per_lot":5}).to_string()).unwrap();
    fs::write(&strategy,json!(sqx_strategy::Strategy::new("manifest test")).to_string()).unwrap();
    let input=json!({"schema":1,"markets":[{"symbol":"EURUSD","data":"arbitrary.csv","timeframe":"M1"},{"symbol":"USDJPY","data":"arbitrary.csv","timeframe":"M1","config":"jpy.json"}]});fs::write(&manifest,input.to_string()).unwrap();
    let run=||Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["crosscheck","--manifest"]).arg(&manifest).arg("--strategy").arg(&strategy).output().unwrap();
    let parse=|output:&std::process::Output|->Value{serde_json::from_str(String::from_utf8_lossy(&output.stdout).lines().find_map(|s|s.strip_prefix("Research context: ")).unwrap()).unwrap()};
    let valid=run();assert!(valid.status.success(),"{}",String::from_utf8_lossy(&valid.stderr));let report=parse(&valid);
    assert_eq!(report["requested_markets"],2);assert_eq!(report["eligible_markets"],0);assert_eq!(report["markets"][1]["context"]["symbol"],"USDJPY");assert_eq!(report["markets"][1]["context"]["config"]["account"]["currency"],"JPY");assert_eq!(report["markets"][0]["status"],"insufficient_trades");
    let mut duplicate=input.clone();duplicate["markets"][1]=duplicate["markets"][0].clone();fs::write(&manifest,duplicate.to_string()).unwrap();let failed=run();assert!(!failed.status.success());assert!(failed.stdout.is_empty());
    let mut missing=input.clone();missing["markets"][1]["data"]=json!("missing.csv");fs::write(&manifest,missing.to_string()).unwrap();let failed=run();assert!(!failed.status.success());assert!(failed.stdout.is_empty(),"preflight must precede the comparison");
    fs::write(&manifest,input.to_string()).unwrap();let mut bad=json!(sqx_strategy::Strategy::new("invalid size"));bad["money_management"]=json!({"FixedSize":{"size":0.0}});fs::write(&strategy,bad.to_string()).unwrap();
    let failed=run();assert!(!failed.status.success());let report=parse(&failed);assert_eq!(report["invalid_markets"],2);assert_eq!(report["eligible_markets"],0);
    for market in report["markets"].as_array().unwrap(){assert_eq!(market["status"],"invalid");assert!(market["metrics"].is_null());assert!(!market["errors"].as_array().unwrap().is_empty());}
    fs::remove_dir_all(root).unwrap();
}
