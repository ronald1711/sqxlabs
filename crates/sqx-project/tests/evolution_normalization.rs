use serde_json::json;
use sqx_project::{archive::content_id,evolution::{EvolutionJobs,EvolutionBackup,EvolutionJob}};
use sqx_optimizer::{EvolutionState,GeneratorConfig,GeneticConfig,Fitness};
use sqx_core::Instrument;
use sqx_backtest::BacktestConfig;

#[test]
fn large_sources_are_separate_from_context_and_reverified_on_restore(){
    let root=std::env::temp_dir().join(format!("sqx-evolution-normalization-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let mut utc=String::from("timestamp,open,high,low,close,volume\n");
    for i in 0..80 {utc.push_str(&format!("{},1,1.01,0.99,1,1\n",1700000040+i*60));}
    let raw=utc.replacen("timestamp,","timestamp_broker,",1);
    // The source itself exceeds the metadata limit; metadata remains bounded.
    let policy=json!({"schema":1,"server":"synthetic","description":"x".repeat(1024*1024),"intervals":[{"raw_start_ms":1700000040000i64,"raw_end_ms":1700004840000i64,"offset_seconds":0,"source":"synthetic UTC"}]}).to_string();
    let manifest=json!({"schema":1,"server":"synthetic","raw_sha256":content_id(raw.as_bytes()),"policy_sha256":content_id(policy.as_bytes()),"normalized_sha256":content_id(utc.as_bytes()),"rows":80,"bar_duration_ms":60000,"clock":"UTC","scope":"synthetic"}).to_string();
    let bundle=sqx_data::normalization::Bundle{manifest_json:manifest,policy_json:policy,raw_csv:raw,utc_csv:utc.clone()};
    let mut config=BacktestConfig::default();config.bar_duration_ms=Some(60000);
    let state=EvolutionState::new(42,&GeneratorConfig::default(),&GeneticConfig{population_size:2,generations:1,elite_count:1,..Default::default()},Fitness::ProfitFactor,&sqx_data::read_csv_bars_str(&utc).unwrap(),&Instrument::fx("EURUSD"),&config,100.0).unwrap();
    let jobs=EvolutionJobs::new(&root);
    let context=json!({"timeframe":"M1","normalization":{"bundle":bundle,"verification":bundle.verify().unwrap()}});
    let id=jobs.create_with_context("engine","source",&state,context.clone()).unwrap();
    let backup=jobs.backup(&id).unwrap();backup.verify().unwrap();
    let stored:EvolutionJob=serde_json::from_str(&backup.job_json).unwrap();
    assert!(serde_json::to_vec(&stored.context).unwrap().len()<1024*1024);
    assert!(stored.context["normalization"].get("bundle").is_none());
    assert_eq!(stored.normalization.unwrap().policy_json,bundle.policy_json);
    let restored=EvolutionJobs::new(root.join("restored"));restored.restore(&backup).unwrap();
    assert_eq!(restored.backup(&id).unwrap().job_json,backup.job_json);
    let forge=|mut value:serde_json::Value|->EvolutionBackup {
        value["initial"]["bars"][0]["close"]=json!(1.001);
        let job_json=value.to_string();EvolutionBackup{job_id:content_id(job_json.as_bytes()),job_json,..backup.clone()}
    };
    let altered=forge(serde_json::from_str(&backup.job_json).unwrap());
    assert!(restored.restore(&altered).unwrap_err().contains("supplied candles"));
    // Legacy sources embedded inside context are also checked, not silently ignored.
    let mut legacy:serde_json::Value=serde_json::from_str(&backup.job_json).unwrap();
    legacy.as_object_mut().unwrap().remove("normalization");legacy["context"]=context;
    let raw=legacy.to_string();let legacy_backup=EvolutionBackup{job_id:content_id(raw.as_bytes()),job_json:raw,..backup.clone()};
    legacy_backup.verify().unwrap();assert!(forge(legacy).verify().unwrap_err().contains("supplied candles"));
    let mut bad=serde_json::from_str::<serde_json::Value>(&backup.job_json).unwrap();
    bad["context"]["normalization"]["verification"]["server"]=json!("different");
    let raw=bad.to_string();let bad=EvolutionBackup{job_id:content_id(raw.as_bytes()),job_json:raw,..backup.clone()};
    assert!(bad.verify().unwrap_err().contains("verification differs"));
    assert!(jobs.create_with_context("engine","oversized-metadata",&state,json!({"note":"x".repeat(1024*1024)})).unwrap_err().contains("context exceeds"));
    std::fs::remove_dir_all(root).unwrap();
}
