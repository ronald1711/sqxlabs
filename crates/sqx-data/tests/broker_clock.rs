use std::{fs,path::PathBuf,process::Command};
struct Temp(PathBuf);
impl Drop for Temp {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
#[test]
fn raw_broker_csv_is_rejected_and_explicit_normalization_preserves_rows() {
    let root=Temp(std::env::temp_dir().join(format!("sqx-broker-clock-{}",std::process::id())));
    fs::create_dir_all(&root.0).unwrap();
    let policy=root.0.join("policy.json");
    fs::write(&policy,serde_json::json!({"schema":1,"server":"fixture","intervals":[{"raw_start_ms":1700000000000i64,"raw_end_ms":1700000600000i64,"offset_seconds":7200,"source":"synthetic clock fixture"}]}).to_string()).unwrap();
    let script=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/mt5_normalize.py");
    for ticks in [false,true] {
        let raw=if ticks {"timestamp_broker_ms,bid,ask\n1700000000123,1.123456789012345,1.123456789012349\n1700000000123,1.123456789012346,1.123456789012350\n"}else{"timestamp_broker,open,high,low,close,volume\n1700000000,1,1,1,1,1\n"};
        let input=root.0.join(if ticks{"ticks.csv"}else{"bars.csv"});fs::write(&input,raw).unwrap();
        let error=if ticks {sqx_data::read_ticks_csv(input.to_str().unwrap()).unwrap_err()}else{sqx_data::read_csv_bars_str(raw).unwrap_err()};
        assert!(error.contains("raw broker-clock"),"{error}");
        let mut command=Command::new("python3");
        command.arg(&script).arg("--input").arg(&input).arg("--policy").arg(&policy).args(["--server","fixture","--out"]).arg(root.0.join("bundles"));
        if !ticks {command.args(["--bar-duration-ms","60000"]);}
        let output=command.output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        let receipt:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();
        let bundle=sqx_data::normalization::Bundle::load(std::path::Path::new(receipt["bundle"].as_str().unwrap())).unwrap();
        let verified=bundle.verify().unwrap();assert_eq!(verified.kind,if ticks{"ticks"}else{"bars"});
        let mut altered=bundle.clone();altered.utc_csv=altered.utc_csv.replace("1699992800","1699992801");
        assert!(altered.verify().unwrap_err().contains("hash mismatch"));
        // Recomputing all declared hashes must not excuse a false transformation.
        use sha2::{Digest,Sha256};
        let mut manifest:serde_json::Value=serde_json::from_str(&altered.manifest_json).unwrap();
        manifest["normalized_sha256"]=serde_json::json!(format!("{:x}",Sha256::digest(altered.utc_csv.as_bytes())));
        altered.manifest_json=manifest.to_string();
        assert!(altered.verify().unwrap_err().contains("does not follow normalization policy"));
        let utc=receipt["utc_csv"].as_str().unwrap();
        if ticks {
            let rows=sqx_data::read_ticks_csv(utc).unwrap();assert_eq!(rows.len(),2);
            assert_eq!(rows[0].time,1699992800123);assert_eq!(rows[1].time,rows[0].time);
            bundle.verify_ticks(&rows).unwrap();
            let mut reordered=rows.clone();reordered.reverse();
            assert!(bundle.verify_ticks(&reordered).unwrap_err().contains("supplied ticks"));
            assert!(bundle.verify_ticks(&rows[..1]).unwrap_err().contains("supplied ticks"));
            assert_eq!(rows[0].bid,1.123456789012345);assert_eq!(rows[1].bid,1.123456789012346);
        } else {
            let rows=sqx_data::read_csv_bars_str(&fs::read_to_string(utc).unwrap()).unwrap();assert_eq!(rows.len(),1);assert_eq!(rows[0].time,1699992800000);
        }
        assert_eq!(fs::read_to_string(&input).unwrap(),raw);
        assert_eq!(fs::read_to_string(PathBuf::from(receipt["bundle"].as_str().unwrap()).join("raw.csv")).unwrap(),raw);
    }
}
