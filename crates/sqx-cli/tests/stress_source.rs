use std::{fs,process::Command};
use serde_json::{json,Value};
#[test]
fn stress_and_portfolio_commands_bind_sources_and_reject_failed_execution(){
    let root=std::env::temp_dir().join(format!("sqx-cli-stress-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();
    let data=root.join("bars.csv");let strategy=root.join("strategy.json");let config=root.join("config.json");
    fs::write(&strategy,serde_json::to_string(&sqx_strategy::Strategy::new("stress source")).unwrap()).unwrap();
    fs::write(&config,json!({"account":{"currency":"JPY"},"spread_points":7}).to_string()).unwrap();
    let csv="timestamp,open,high,low,close,volume\n1700000000,150,151,149,150,1\n1700000060,150,151,149,150,1\n";fs::write(&data,csv).unwrap();
    let run=|name:&str,args:&[&str]|{
        let mut cmd=Command::new(env!("CARGO_BIN_EXE_sqx-cli"));cmd.arg(name).arg("--data").arg(&data);
        if name=="robustness" {cmd.arg("--archive-dir").arg(root.join("archive"));}
        if name=="portfolio"{cmd.args(["--count","2"]);}else{cmd.arg("--strategy").arg(&strategy);if name=="montecarlo"{cmd.args(["--runs","2"]);}else{cmd.args(["--mc-runs","2","--retest-runs","2","--windows","2"]);}}
        cmd.args(args).output().unwrap()
    };
    for name in ["montecarlo","robustness","portfolio"]{
        let missing=run(name,&[]);assert!(!missing.status.success());assert!(String::from_utf8_lossy(&missing.stderr).contains("--symbol"));
        let output=run(name,&["--symbol","USDJPY","--timeframe","M1","--config",config.to_str().unwrap()]);assert_eq!(output.status.success(),name=="portfolio","{}",String::from_utf8_lossy(&output.stderr));
        let text=String::from_utf8(output.stdout).unwrap();let context:Value=serde_json::from_str(text.lines().find_map(|s|s.strip_prefix("Research context: ")).unwrap()).unwrap();
        if name=="robustness" {
            let errors=context["errors"].as_array().expect("aggregate errors recorded");
            for stage in ["Monte Carlo:","MC retest:","Walk-forward:"] {
                assert!(errors.iter().any(|e|e.as_str().unwrap().starts_with(stage)),"missing {stage}: {context}");
                assert!(String::from_utf8_lossy(&output.stderr).contains(stage));
            }
        }
        assert_eq!(context["symbol"],"USDJPY");assert_eq!(context["config"]["account"]["currency"],"JPY");assert_eq!(context["config"]["bar_duration_ms"],60000);
        let invalid=run(name,&["--symbol","USDJPY","--timeframe","M5","--allow-data-gaps"]);assert!(!invalid.status.success());assert!(String::from_utf8_lossy(&invalid.stderr).contains("overlapping candle"));
    }
    let mut bad=json!(sqx_strategy::Strategy::new("invalid size"));bad["money_management"]=json!({"FixedSize":{"size":0.0}});fs::write(&strategy,bad.to_string()).unwrap();
    let failed=run("montecarlo",&["--symbol","USDJPY","--timeframe","M1"]);assert!(!failed.status.success(),"{}",String::from_utf8_lossy(&failed.stdout));assert!(failed.stdout.is_empty(),"failed simulation must not become a Monte Carlo report");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn monte_carlo_reports_invalid_requests_as_failures_and_preserves_valid_runs(){
    let root=std::env::temp_dir().join(format!("sqx-mc-status-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&root).unwrap();
    let data=root.join("bars.csv");let strategy=root.join("strategy.json");
    let mut csv="timestamp,open,high,low,close,volume\n".to_string();
    for i in 0..120 {csv.push_str(&format!("{},1,1,1,1,1\n",1700000000+i*60));}
    fs::write(&data,csv).unwrap();
    let mut s=json!(sqx_strategy::Strategy::new("repeat entry and exit"));
    s["signals"]=json!(["LongEntry","LongExit"].map(|kind|json!({"kind":kind,"conditions":[{"left":{"Const":1.0},"cmp":"Gt","right":{"Const":0.0}}],"join":"And","fuzzy":null})));
    fs::write(&strategy,s.to_string()).unwrap();
    for (runs,skip,valid,reason) in [("3","0.1",true,""),("0","0.1",false,"at least one run"),("3","1.1",false,"skip_prob")] {
        let output=Command::new(env!("CARGO_BIN_EXE_sqx-cli"))
            .args(["montecarlo","--symbol","EURUSD","--timeframe","M1","--runs",runs,"--skip",skip])
            .arg("--strategy").arg(&strategy).arg("--data").arg(&data).output().unwrap();
        let stdout=String::from_utf8(output.stdout).unwrap();
        let context:Value=serde_json::from_str(stdout.lines().find_map(|s|s.strip_prefix("Research context: ")).expect("report context retained")).unwrap();
        assert_eq!(output.status.success(),valid,"{stdout}");
        assert_eq!(context["monte_carlo_errors"].as_array().unwrap().is_empty(),valid);
        if !valid {assert!(String::from_utf8_lossy(&output.stderr).contains(reason));}
    }
    fs::remove_dir_all(root).unwrap();
}
