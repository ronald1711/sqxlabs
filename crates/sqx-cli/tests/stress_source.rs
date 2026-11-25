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
        if matches!(name,"robustness"|"montecarlo"|"portfolio") {cmd.arg("--archive-dir").arg(root.join("archive"));}
        if name=="portfolio"{cmd.args(["--count","2"]);}else{cmd.arg("--strategy").arg(&strategy);if name=="montecarlo"{cmd.args(["--runs","2"]);}else{cmd.args(["--mc-runs","2","--retest-runs","2","--windows","2"]);}}
        cmd.args(args).output().unwrap()
    };
    for name in ["montecarlo","robustness","portfolio"]{
        let missing=run(name,&[]);assert!(!missing.status.success());assert!(String::from_utf8_lossy(&missing.stderr).contains("--symbol"));
        let output=run(name,&["--symbol","USDJPY","--timeframe","M1","--config",config.to_str().unwrap()]);assert!(!output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        let text=String::from_utf8(output.stdout).unwrap();let context:Value=serde_json::from_str(text.lines().find_map(|s|s.strip_prefix("Research context: ")).unwrap()).unwrap();
        if name=="portfolio"{
            let receipt:Value=serde_json::from_str(text.lines().find_map(|s|s.strip_prefix("Portfolio archive: ")).unwrap()).unwrap();
            let saved=sqx_project::archive::Archive::new(root.join("archive")).load(receipt["run_id"].as_str().unwrap()).unwrap();
            assert_eq!(saved.run.result["report"]["portfolio"]["candidates"].as_array().unwrap().len(),2);
            assert!(!saved.run.result["report"]["errors"].as_array().unwrap().is_empty());
            assert!(saved.run.result["report"]["portfolio"]["selected"].as_array().unwrap().is_empty());
        }
        if name=="robustness" {
            let errors=context["errors"].as_array().expect("aggregate errors recorded");
            for stage in ["Monte Carlo:","MC retest:","Walk-forward:"] {
                assert!(errors.iter().any(|e|e.as_str().unwrap().starts_with(stage)),"missing {stage}: {context}");
                assert!(String::from_utf8_lossy(&output.stderr).contains(stage));
            }
            let receipt:Value=serde_json::from_str(text.lines().find_map(|s|s.strip_prefix("Robustness archive: ")).expect("failed study still has a completion receipt")).unwrap();
            let archive=sqx_project::archive::Archive::new(root.join("archive"));
            let completed=archive.load(receipt["run_id"].as_str().unwrap()).unwrap();
            let report=&completed.run.result["report"];
            assert_eq!(report["errors"],context["errors"]);
            assert_eq!(report["robustness"]["retest"]["skipped_runs"],2);
            assert!(!report["robustness"]["monte_carlo"]["errors"].as_array().unwrap().is_empty());
            let replay=Command::new(env!("CARGO_BIN_EXE_sqx-cli"))
                .args(["replay-pipeline","--archive-dir"]).arg(root.join("archive"))
                .args(["--run-id",receipt["started_run_id"].as_str().unwrap()]).output().unwrap();
            assert!(replay.status.success(),"repeatability of an invalid study can still be checked: {}",String::from_utf8_lossy(&replay.stderr));
            let replay:Value=serde_json::from_slice(&replay.stdout).unwrap();
            assert_eq!(replay["completion_id"],receipt["run_id"]);
            assert_eq!(replay["execution_valid"],false,"repeatability must not promote invalid research");
        }
        assert_eq!(context["symbol"],"USDJPY");assert_eq!(context["config"]["account"]["currency"],"JPY");assert_eq!(context["config"]["bar_duration_ms"],60000);
        let invalid=run(name,&["--symbol","USDJPY","--timeframe","M5","--allow-data-gaps"]);assert!(!invalid.status.success());assert!(String::from_utf8_lossy(&invalid.stderr).contains("overlapping candle"));
    }
    let mut bad=json!(sqx_strategy::Strategy::new("invalid size"));bad["money_management"]=json!({"FixedSize":{"size":0.0}});fs::write(&strategy,bad.to_string()).unwrap();
    let failed=run("montecarlo",&["--symbol","USDJPY","--timeframe","M1"]);assert!(!failed.status.success());
    let stdout=String::from_utf8(failed.stdout).unwrap();
    let receipt:Value=serde_json::from_str(stdout.lines().find_map(|s|s.strip_prefix("Monte Carlo archive: ")).expect("invalid base execution is archived")).unwrap();
    let saved=sqx_project::archive::Archive::new(root.join("archive")).load(receipt["run_id"].as_str().unwrap()).unwrap();
    assert!(!saved.run.result["report"]["robustness"]["base"]["errors"].as_array().unwrap().is_empty());
    assert!(saved.run.result["report"]["robustness"]["monte_carlo"].is_null(),"failed base must not produce a distribution");
    let failed=run("robustness",&["--symbol","USDJPY","--timeframe","M1"]);assert!(!failed.status.success());
    let stdout=String::from_utf8(failed.stdout).unwrap();
    let receipt:Value=serde_json::from_str(stdout.lines().find_map(|s|s.strip_prefix("Robustness archive: ")).expect("invalid base execution is archived")).unwrap();
    let archive=sqx_project::archive::Archive::new(root.join("archive"));
    let completed=archive.load(receipt["run_id"].as_str().unwrap()).unwrap();
    let study=&completed.run.result["report"]["robustness"];
    assert!(!study["base"]["errors"].as_array().unwrap().is_empty());
    assert!(study["monte_carlo"].is_null());assert!(study["retest"].is_null());assert!(study["walk_forward"].is_null());
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
            .arg("--archive-dir").arg(root.join("archive"))
            .arg("--strategy").arg(&strategy).arg("--data").arg(&data).output().unwrap();
        let stdout=String::from_utf8(output.stdout).unwrap();
        let context:Value=serde_json::from_str(stdout.lines().find_map(|s|s.strip_prefix("Research context: ")).expect("report context retained")).unwrap();
        assert_eq!(output.status.success(),valid,"{stdout}");
        assert_eq!(context["monte_carlo_errors"].as_array().unwrap().is_empty(),valid);
        let receipt:Value=serde_json::from_str(stdout.lines().find_map(|s|s.strip_prefix("Monte Carlo archive: ")).unwrap()).unwrap();
        let archive=sqx_project::archive::Archive::new(root.join("archive"));
        let saved=archive.load(receipt["run_id"].as_str().unwrap()).unwrap();
        let study=&saved.run.result["report"]["robustness"];
        assert_eq!(study["monte_carlo"]["errors"],context["monte_carlo_errors"]);
        assert!(study["what_if"].as_array().unwrap().is_empty());assert!(study["retest"].is_null());assert!(study["walk_forward"].is_null());
        let outcomes=saved.run.result["event_ids"].as_array().unwrap().iter().filter(|id|archive.load(id.as_str().unwrap()).unwrap().run.result["event"].get("TradeManipulationOutcome").is_some()).count();
        assert_eq!(outcomes,if valid{3}else{0});
        if !valid {assert!(String::from_utf8_lossy(&output.stderr).contains(reason));}
    }
    let archive=sqx_project::archive::Archive::new(root.join("archive"));let before=archive.list().unwrap();
    let nonfinite=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["montecarlo","--symbol","EURUSD","--timeframe","M1","--skip","NaN"])
        .arg("--archive-dir").arg(archive.root()).arg("--strategy").arg(&strategy).arg("--data").arg(&data).output().unwrap();
    assert!(!nonfinite.status.success());assert!(String::from_utf8_lossy(&nonfinite.stderr).contains("skip_prob must be finite"));
    assert_eq!(archive.list().unwrap(),before,"nonfinite input must not silently become JSON null in an executable checkpoint");
    fs::remove_dir_all(root).unwrap();
}
