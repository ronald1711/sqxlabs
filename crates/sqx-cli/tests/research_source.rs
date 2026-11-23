use std::{fs,process::Command};
use serde_json::{json,Value};
#[test]
fn generator_and_walk_forward_bind_instrument_costs_and_quality(){
    let root=std::env::temp_dir().join(format!("sqx-cli-research-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();
    let data=root.join("bars.csv");let config=root.join("config.json");let mut csv="timestamp,open,high,low,close,volume\n".to_string();
    for i in 0..300{csv.push_str(&format!("{},150,151,149,150,1\n",1700000000+i*60));}fs::write(&data,&csv).unwrap();
    fs::write(&config,json!({"initial_deposit":25000,"spread_points":7,"commission_per_lot":5,"account":{"currency":"JPY"}}).to_string()).unwrap();
    let run=|name:&str,args:&[&str]|{
        let mut cmd=Command::new(env!("CARGO_BIN_EXE_sqx-cli"));cmd.arg(name).arg("--data").arg(&data);
        if name=="generate"{cmd.args(["--count","2"]);}else{cmd.args(["--windows","2","--candidates","1"]);}
        cmd.args(args).output().unwrap()
    };
    for name in ["generate","optimize"] {
        let missing=run(name,&[]);assert!(!missing.status.success());assert!(String::from_utf8_lossy(&missing.stderr).contains("--symbol"));
        let args=["--symbol","USDJPY","--timeframe","M1","--config",config.to_str().unwrap()];
        let good=run(name,&args);assert!(good.status.success(),"{}",String::from_utf8_lossy(&good.stderr));
        let output=String::from_utf8(good.stdout).unwrap();let context:Value=serde_json::from_str(output.lines().find_map(|s|s.strip_prefix("Research context: ")).unwrap()).unwrap();
        assert_eq!(context["symbol"],"USDJPY");assert_eq!(context["config"]["bar_duration_ms"],60000);assert_eq!(context["config"]["spread_points"].as_f64(),Some(7.0));assert_eq!(context["config"]["account"]["currency"],"JPY");
        let overlap=run(name,&["--symbol","USDJPY","--timeframe","M5","--allow-data-gaps"]);assert!(!overlap.status.success());assert!(String::from_utf8_lossy(&overlap.stderr).contains("overlapping candle"));
    }
    fs::write(&data,csv.lines().enumerate().filter(|(i,_)|*i!=2).map(|(_,line)|format!("{line}\n")).collect::<String>()).unwrap();
    for name in ["generate","optimize"]{
        let gap=run(name,&["--symbol","USDJPY","--timeframe","M1"]);assert!(!gap.status.success());assert!(String::from_utf8_lossy(&gap.stderr).contains("Unresolved data gaps"));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_walk_forward_returns_failure_with_preserved_context(){
    let root=std::env::temp_dir().join(format!("sqx-invalid-wf-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&root).unwrap();
    let data=root.join("bars.csv");
    let mut csv="timestamp,open,high,low,close,volume\n".to_owned();
    for i in 0..120 {csv.push_str(&format!("{},1,1,1,1,1\n",1700000000+i*60));}
    fs::write(&data,csv).unwrap();
    let output=Command::new(env!("CARGO_BIN_EXE_sqx-cli"))
        .args(["optimize","--symbol","EURUSD","--timeframe","M1","--windows","0"])
        .arg("--data").arg(&data).output().unwrap();
    fs::remove_dir_all(root).unwrap();
    let stdout=String::from_utf8(output.stdout).unwrap();
    let context:Value=serde_json::from_str(stdout.lines().find_map(|line|line.strip_prefix("Research context: ")).expect("invalid report must retain research context")).unwrap();
    assert_eq!(context["walk_forward"]["evidence_sufficient"],false);
    assert!(!context["walk_forward"]["errors"].as_array().unwrap().is_empty());
    assert!(!output.status.success(),"invalid walk-forward must fail for automation");
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid walk-forward"));
}
