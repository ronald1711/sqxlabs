use std::{fs,process::Command};
#[test]
fn cli_does_not_repair_or_drop_invalid_csv_rows(){
    let root=std::env::temp_dir().join(format!("sqx-cli-csv-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&root).unwrap();
    let data=root.join("bars.csv");let strategy=root.join("strategy.json");fs::write(&strategy,serde_json::to_string(&sqx_strategy::Strategy::new("CSV validation")).unwrap()).unwrap();
    for bad in ["1700000060,1,1.1,0.9,1,unknown","1700000060,1,1.1","9223372036854775807,1,1.1,0.9,1,1","1700000060,1,1.1,0.9,1,1,extra","1700000060,1,1.1,0.9,1,-1","1700000060,1,NaN,0.9,1,1","1700000000,1,1.1,0.9,1,1"] {
        fs::write(&data,format!("timestamp,open,high,low,close,volume\n1700000000,1,1.1,0.9,1,1\n{bad}\n")).unwrap();
        let result=Command::new(env!("CARGO_BIN_EXE_sqx-cli")).current_dir(&root).args(["backtest","--timeframe","M1","--strategy"]).arg(&strategy).arg("--data").arg(&data).output().unwrap();
        assert!(!result.status.success(),"accepted invalid row {bad}: {}",String::from_utf8_lossy(&result.stdout));
        assert!(!String::from_utf8_lossy(&result.stderr).contains("panicked"),"bad CSV must produce a diagnostic, not a panic");
        assert!(String::from_utf8_lossy(&result.stderr).contains("line 3"),"{}",String::from_utf8_lossy(&result.stderr));
    }
    fs::remove_dir_all(root).unwrap();
}
