use std::{fs,process::Command};
use serde_json::{json,Value};
#[test]
fn backtest_requires_timeframe_and_reports_source_quality(){
    let root=std::env::temp_dir().join(format!("sqx-cli-source-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&root).unwrap();let data=root.join("bars.csv");let strategy=root.join("strategy.json");let ticks=root.join("ticks.csv");
    let mut spec=serde_json::to_value(sqx_strategy::Strategy::new("source evidence")).unwrap();
    spec["long"]=json!(true);spec["signals"]=json!([{"kind":"LongEntry","conditions":[{"left":{"Const":1.0},"cmp":"Gt","right":{"Const":0.0}}],"join":"And","fuzzy":null}]);
    spec["money_management"]=json!({"FixedSize":{"size":0.1}});
    fs::write(&strategy,spec.to_string()).unwrap();
    fs::write(&data,"timestamp,open,high,low,close,volume\n1700000000,1,1.1,0.9,1,1\n1700000060,1,1.1,0.9,1,1\n").unwrap();
    fs::write(&ticks,"timestamp_ms,bid,ask\n1700000000123,1,1.0001\n1700000060456,1,1.0001\n").unwrap();
    let run=|args:&[&str]|Command::new(env!("CARGO_BIN_EXE_sqx-cli")).current_dir(&root).args(["backtest","--strategy"]).arg(&strategy).arg("--data").arg(&data).args(args).output().unwrap();
    let missing=run(&[]);assert!(!missing.status.success());assert!(String::from_utf8_lossy(&missing.stderr).contains("--timeframe"));
    let good=run(&["--timeframe","M1"]);assert!(good.status.success(),"{}",String::from_utf8_lossy(&good.stderr));
    let output=String::from_utf8(good.stdout).unwrap();assert!(output.contains("Backtest checkpoint:"));assert!(output.contains("Backtest archive:"));
    let automatic=sqx_project::archive::Archive::new(root.join("data/research"));assert_eq!(automatic.list().unwrap().len(),2,"unflagged backtest must save both checkpoint and result");
let context:Value=serde_json::from_str(output.lines().find_map(|line|line.strip_prefix("Research context: ")).unwrap()).unwrap();
    assert_eq!(context["config"]["bar_duration_ms"],60000);assert_eq!(context["timeframe"],"M1");assert_eq!(context["data_quality"]["unclassified_intervals"],0);
    let tick=run(&["--timeframe","M1","--ticks",ticks.to_str().unwrap()]);assert!(tick.status.success(),"{}",String::from_utf8_lossy(&tick.stderr));
    let archive_root=root.join("research");
    let saved=run(&["--timeframe","M1","--ticks",ticks.to_str().unwrap(),"--archive-dir",archive_root.to_str().unwrap()]);
    assert!(saved.status.success(),"{}",String::from_utf8_lossy(&saved.stderr));
    let stdout=String::from_utf8(saved.stdout).unwrap();
    let receipt:Value=serde_json::from_str(stdout.lines().find_map(|l|l.strip_prefix("Backtest archive: ")).unwrap()).unwrap();
    let bundle=sqx_project::archive::Archive::new(&archive_root).load(receipt["run_id"].as_str().unwrap()).unwrap();
    assert_eq!(bundle.run.request["research_kind"],"cli_backtest");
    let retained=archive_root.join("engines").join(format!("{}.bin",bundle.run.engine_id));
    assert_eq!(fs::read(&retained).unwrap(),fs::read(env!("CARGO_BIN_EXE_sqx-cli")).unwrap());
    assert_eq!(bundle.dataset["ticks"].as_array().unwrap().len(),2);
    assert_eq!(bundle.dataset["ticks"][1]["time"],1700000060456i64);
    assert_eq!(bundle.run.result["trades"][0]["open_time"],1700000060456i64);
    assert_eq!(bundle.dataset["bars"].as_array().unwrap().len(),2);
    assert_eq!(bundle.run.result["execution"]["account_evidence_schema"],1);
    assert!(!bundle.run.result["trades"].as_array().unwrap().is_empty());assert!(bundle.run.result["equity"].is_array());
    assert_eq!(bundle.strategy["name"],"source evidence");
    let replay=|id:&str|Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["replay-backtest","--archive-dir"]).arg(&archive_root).args(["--run-id",id]).output().unwrap();
    // Replay must not consult mutable source files.
    let original_ticks=fs::read(&ticks).unwrap();fs::remove_file(&ticks).unwrap();
    let checkpoint:Value=serde_json::from_str(stdout.lines().find_map(|l|l.strip_prefix("Backtest checkpoint: ")).unwrap()).unwrap();
    let started_id=checkpoint["run_id"].as_str().unwrap();
    assert_eq!(bundle.run.request["started_run_id"],started_id);
    let started=sqx_project::archive::Archive::new(&archive_root).load(started_id).unwrap();
    assert_eq!(started.run.result["status"],"execution_not_recorded");
    let recovered_root=root.join("recovered");let recovered=sqx_project::archive::Archive::new(&recovered_root);
    recovered.restore(&started).unwrap();assert_eq!(recovered.list().unwrap().len(),1);
    let list=||{
        let out=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["list-backtests","--archive-dir"]).arg(&recovered_root).output().unwrap();
        assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    let pending=list();assert_eq!(pending["checkpoints"][0]["checkpoint_id"],started_id);assert_eq!(pending["checkpoints"][0]["state"],"result_not_recorded");assert_eq!(pending["checkpoints"][0]["matching_engine"],true);
    let resume=||Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["replay-backtest","--resume","--archive-dir"]).arg(&recovered_root).args(["--run-id",started_id]).output().unwrap();
    for _ in 0..2 {
        let resumed=resume();assert!(resumed.status.success(),"{}",String::from_utf8_lossy(&resumed.stderr));
        assert!(String::from_utf8_lossy(&resumed.stdout).contains(receipt["run_id"].as_str().unwrap()));
        assert_eq!(recovered.list().unwrap().len(),2,"restart must converge on the same immutable result");
        let status=list();assert_eq!(status["checkpoints"][0]["state"],"result_recorded");assert_eq!(status["checkpoints"][0]["results"][0]["run_id"],receipt["run_id"]);assert_eq!(status["checkpoints"][0]["results"][0]["execution_valid"],true);
    }
    let mut contradictory_request=bundle.run.request.clone();contradictory_request["config"]["initial_deposit"]=json!(12345.0);
    recovered.save(bundle.run.engine_id.clone(),bundle.dataset.clone(),bundle.strategy.clone(),contradictory_request,bundle.run.result.clone()).unwrap();
    let corrupt_listing=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).args(["list-backtests","--archive-dir"]).arg(&recovered_root).output().unwrap();
    assert!(!corrupt_listing.status.success());assert!(String::from_utf8_lossy(&corrupt_listing.stderr).contains("checkpoint identity"));
    let verified=replay(receipt["run_id"].as_str().unwrap());
    assert!(verified.status.success(),"{}",String::from_utf8_lossy(&verified.stderr));
    assert!(String::from_utf8_lossy(&verified.stdout).contains("\"matches\":true"));
    fs::write(&ticks,original_ticks).unwrap();
    let archive=sqx_project::archive::Archive::new(&archive_root);
    let mut wrong_result=bundle.run.result.clone();wrong_result["final_equity"]=json!(123.0);
    let altered=archive.save(bundle.run.engine_id.clone(),bundle.dataset.clone(),bundle.strategy.clone(),bundle.run.request.clone(),wrong_result).unwrap();
    let mismatch=replay(&altered);assert!(!mismatch.status.success());assert!(String::from_utf8_lossy(&mismatch.stderr).contains("differs from the recorded result"));
    let wrong_engine=archive.save("0".repeat(64),bundle.dataset.clone(),bundle.strategy.clone(),bundle.run.request.clone(),bundle.run.result.clone()).unwrap();
    let refused=replay(&wrong_engine);assert!(!refused.status.success());assert!(String::from_utf8_lossy(&refused.stderr).contains("engine executable differs"));
    let blocked=root.join("blocked");fs::write(&blocked,"file").unwrap();
    let failed=run(&["--timeframe","M1","--archive-dir",blocked.to_str().unwrap()]);
    assert!(!failed.status.success());assert!(String::from_utf8_lossy(&failed.stderr).contains("could not be archived"));
    assert!(!String::from_utf8_lossy(&failed.stdout).contains("Backtest archive:"));
    let conflict=run(&["--timeframe","M1","--ticks",ticks.to_str().unwrap(),"--tick-bar-ms","30000"]);assert!(!conflict.status.success());assert!(String::from_utf8_lossy(&conflict.stderr).contains("conflicts with source timeframe"));
    fs::write(&data,"timestamp,open,high,low,close,volume\n1700000000,1,1.1,0.9,1,1\n1700000120,1,1.1,0.9,1,1\n").unwrap();
    let gap=run(&["--timeframe","M1"]);assert!(!gap.status.success());assert!(String::from_utf8_lossy(&gap.stderr).contains("Unresolved data gaps"));
    let exploratory=run(&["--timeframe","M1","--allow-data-gaps"]);assert!(exploratory.status.success());
    assert!(String::from_utf8_lossy(&exploratory.stdout).contains(&format!("\"data_gaps_acknowledged\":{}",json!(true))));
    let overlap=run(&["--timeframe","M5","--allow-data-gaps"]);assert!(!overlap.status.success());assert!(String::from_utf8_lossy(&overlap.stderr).contains("overlapping candle"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn analysis_uses_declared_source_and_explicit_simulation_settings(){
    let root=std::env::temp_dir().join(format!("sqx-cli-analysis-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();
    let data=root.join("bars.csv");let strategy=root.join("strategy.json");let config=root.join("config.json");
    fs::write(&strategy,serde_json::to_string(&sqx_strategy::Strategy::new("analysis evidence")).unwrap()).unwrap();
    fs::write(&data,"timestamp,open,high,low,close,volume\n1700000000,1,1.1,0.9,1,1\n1700000060,1,1.1,0.9,1,1\n").unwrap();
    fs::write(&config,json!({"initial_deposit":32100,"spread_points":7,"slippage_points":2,"commission_per_lot":5}).to_string()).unwrap();
    let run=|command:&str,args:&[&str]|Command::new(env!("CARGO_BIN_EXE_sqx-cli")).current_dir(&root).arg(command).arg("--strategy").arg(&strategy).arg("--data").arg(&data).args(args).output().unwrap();
    let missing=run("analyze",&[]);assert!(!missing.status.success());assert!(String::from_utf8_lossy(&missing.stderr).contains("--timeframe"));
    let parse=|out:std::process::Output|->Value{
        assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
        serde_json::from_str(String::from_utf8(out.stdout).unwrap().lines().find_map(|s|s.strip_prefix("Research context: ")).unwrap()).unwrap()
    };
    let analysis=parse(run("analyze",&["--timeframe","M1","--config",config.to_str().unwrap()]));
    let backtest=parse(run("backtest",&["--timeframe","M1","--deposit","32100","--spread","7","--slippage","2","--commission","5"]));
    assert_eq!(analysis["config"],backtest["config"]);assert_eq!(analysis["execution"],backtest["execution"]);assert_eq!(analysis["data_quality"],backtest["data_quality"]);
    fs::write(&config,json!({"bar_duration_ms":30000}).to_string()).unwrap();
    let conflict=run("analyze",&["--timeframe","M1","--config",config.to_str().unwrap()]);assert!(!conflict.status.success());assert!(String::from_utf8_lossy(&conflict.stderr).contains("conflicts with source timeframe"));
    fs::write(&data,"timestamp,open,high,low,close,volume\n1700000000,1,1.1,0.9,1,1\n1700000120,1,1.1,0.9,1,1\n").unwrap();
    let gap=run("analyze",&["--timeframe","M1"]);assert!(!gap.status.success());assert!(String::from_utf8_lossy(&gap.stderr).contains("Unresolved data gaps"));
    assert_eq!(parse(run("analyze",&["--timeframe","M1","--allow-data-gaps"]))["data_gaps_acknowledged"],true);
    fs::remove_dir_all(root).unwrap();
}
