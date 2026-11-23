use sqx_project::reconciliation::*;
use sqx_core::Direction;
use std::collections::BTreeMap;
fn trade()->ExecutionTrade{ExecutionTrade{direction:Direction::Long,open_time:1000,close_time:2000,open_price:100.0,close_price:102.0,size:1.0,pnl:195.0,commission:4.0,financing:1.0}}
fn tolerance()->Tolerances{Tolerances{time_ms:10,price:0.25,size:0.0,money:0.5}}
fn expected()->BTreeMap<usize,ExecutionTrade>{BTreeMap::from([(7,trade())])}
fn observed()->ObservedTrade{ObservedTrade{ticket:"broker-42".into(),expected_trade_id:Some(7),execution:trade()}}
#[test]
fn explicit_correspondence_and_boundary_tolerances(){
 let mut row=observed();row.execution.open_time+=10;row.execution.open_price+=0.25;row.execution.pnl+=0.5;
 let report=compare(&expected(),&[row.clone()],&tolerance()).unwrap();assert!(report.matches_within_tolerance);
 row.execution.open_time+=1;row.execution.open_price+=0.01;row.execution.pnl+=0.01;
 let report=compare(&expected(),&[row],&tolerance()).unwrap();assert!(!report.matches_within_tolerance);
 assert_eq!(report.comparisons[0].differences,vec!["open_time","open_price","pnl"]);
}
#[test]
fn missing_extra_and_empty_sets_never_pass(){
 let report=compare(&expected(),&[],&tolerance()).unwrap();assert_eq!(report.missing_expected_ids,vec![7]);assert!(!report.matches_within_tolerance);
 let mut extra=observed();extra.expected_trade_id=None;extra.ticket="extra".into();
 let report=compare(&expected(),&[observed(),extra],&tolerance()).unwrap();assert_eq!(report.unexpected_tickets,vec!["extra"]);assert!(!report.matches_within_tolerance);
 assert!(!compare(&BTreeMap::new(),&[],&tolerance()).unwrap().matches_within_tolerance);
}
#[test]
fn direction_size_and_cost_mismatches_are_separate(){
 let mut row=observed();row.execution.direction=Direction::Short;row.execution.size=2.0;row.execution.commission=6.0;row.execution.financing=-1.0;
 let report=compare(&expected(),&[row],&tolerance()).unwrap();
 assert_eq!(report.comparisons[0].differences,vec!["direction","size","commission","financing"]);
}
#[test]
fn ambiguous_or_unknown_correspondence_is_rejected(){
 assert!(compare(&expected(),&[observed(),observed()],&tolerance()).is_err());
 let mut duplicate=observed();duplicate.ticket="second".into();assert!(compare(&expected(),&[observed(),duplicate.clone()],&tolerance()).is_err());
 duplicate.expected_trade_id=Some(99);assert!(compare(&expected(),&[duplicate],&tolerance()).is_err());
}
#[test]
fn malformed_numbers_and_timestamp_extremes_fail_safely(){
 for value in [f64::NAN,f64::INFINITY,-1.0]{let mut tol=tolerance();tol.money=value;assert!(compare(&expected(),&[observed()],&tol).is_err());}
 let mut row=observed();row.execution.pnl=f64::NAN;assert!(compare(&expected(),&[row],&tolerance()).is_err());
 let mut row=observed();row.execution.close_time=0;assert!(compare(&expected(),&[row],&tolerance()).is_err());
 let mut ex=expected();ex.get_mut(&7).unwrap().open_time=i64::MIN;ex.get_mut(&7).unwrap().close_time=i64::MIN;
 let mut row=observed();row.execution.open_time=i64::MAX;row.execution.close_time=i64::MAX;
 let report=compare(&ex,&[row],&tolerance()).unwrap();assert_eq!(report.comparisons[0].differences,vec!["open_time","close_time"]);
}
fn scope()->ComparisonScope{ComparisonScope{market:"EURUSD".into(),quote_currency:"USD".into(),account_currency:"USD".into(),contract_size:100000.0,start_ms:1000,end_ms:2000}}
#[test]
fn imported_reports_require_matching_economic_context(){
 let base=scope();assert!(compare_scoped(&base,&expected(),&base,&[observed()],&tolerance()).unwrap().report.matches_within_tolerance);
 for field in 0..6 {
  let mut other=base.clone();match field {0=>other.market="GBPUSD".into(),1=>other.quote_currency="JPY".into(),2=>other.account_currency="EUR".into(),3=>other.contract_size=1000.0,4=>other.start_ms=999,_=>other.end_ms=2001}
  assert!(compare_scoped(&base,&expected(),&other,&[observed()],&tolerance()).is_err());
 }
}
#[test]
fn period_boundaries_and_invalid_scope_cannot_be_hidden_by_tolerances(){
 let mut row=observed();row.execution.open_time=999;
 assert!(compare_scoped(&scope(),&expected(),&scope(),&[row],&tolerance()).is_err());
 let mut expected=expected();expected.get_mut(&7).unwrap().close_time=2001;
 assert!(compare_scoped(&scope(),&expected,&scope(),&[observed()],&tolerance()).is_err());
 for field in 0..4 {
  let mut bad=scope();match field {0=>bad.market=" ".into(),1=>bad.account_currency=" USD".into(),2=>bad.contract_size=f64::NAN,_=>bad.end_ms=bad.start_ms}
  assert!(compare_scoped(&bad,&BTreeMap::new(),&bad,&[],&tolerance()).is_err());
 }
}

#[test]
fn archived_comparison_uses_verified_snapshot_and_survives_backup_restore(){
 use serde_json::json;
 use sqx_project::archive::Archive;
 let root=std::env::temp_dir().join(format!("sqx-paper-archive-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
 let archive=Archive::new(root.join("original"));let restored=Archive::new(root.join("restored"));
 let mut detail=json!(trade());detail["id"]=json!(7);detail["exit_reason"]=json!("EndOfData");
 let dataset=json!({"instrument":sqx_core::Instrument::fx("EURUSD"),"timeframe":sqx_core::Timeframe::M1,"bars":[sqx_core::Bar::new(0,100.0,103.0,99.0,102.0,1.0)]});
 let result=json!({"trades":1,"trade_details":[detail],"run_context":{"account_currency":"USD"}});
 let id=archive.save("recorded-engine".into(),dataset.clone(),json!({"name":"frozen strategy"}),json!({}),result.clone()).unwrap();
 let mut declared=scope();declared.start_ms=0;declared.end_ms=59999;
 let compared=compare_archived(&archive,&id,&declared,&[observed()],&tolerance()).unwrap();
 assert!(compared.comparison.report.matches_within_tolerance);assert_eq!(compared.end_of_data_trade_ids,vec![7]);assert_eq!(compared.engine_id,"recorded-engine");
 let bundle=archive.load(&id).unwrap();restored.restore(&bundle).unwrap();
 assert_eq!(serde_json::to_value(&compared).unwrap(),serde_json::to_value(compare_archived(&restored,&id,&declared,&[observed()],&tolerance()).unwrap()).unwrap());
 let failed=archive.save("engine".into(),dataset.clone(),json!({}),json!({}),json!({"error":"bad run"})).unwrap();assert!(compare_archived(&archive,&failed,&declared,&[],&tolerance()).is_err());
 let wrong_kind=archive.save("engine".into(),dataset.clone(),json!({}),json!({"research_kind":"pipeline"}),result.clone()).unwrap();assert!(compare_archived(&archive,&wrong_kind,&declared,&[],&tolerance()).is_err());
 let mut missing=result.clone();missing["trade_details"][0].as_object_mut().unwrap().remove("financing");
 let missing_id=archive.save("engine".into(),dataset,json!({}),json!({}),missing).unwrap();assert!(compare_archived(&archive,&missing_id,&declared,&[observed()],&tolerance()).is_err());
 let source=serde_json::to_string_pretty(&PaperImport{schema:1,source_label:"Synthetic test fixture, not broker evidence".into(),scope:declared.clone(),trades:vec![observed()]}).unwrap();
 let comparison_id=save_comparison(&archive,&id,&source,&tolerance()).unwrap();
 assert_eq!(comparison_id,save_comparison(&archive,&id,&source,&tolerance()).unwrap());
 let record=load_comparison(&archive,&comparison_id).unwrap();assert_eq!(record.source_json,source);
 let portable=Archive::new(root.join("portable"));restore_comparison(&portable,&comparison_id,&record).unwrap();
 assert_eq!(serde_json::to_value(load_comparison(&portable,&comparison_id).unwrap()).unwrap(),serde_json::to_value(&record).unwrap());
 assert!(portable.list().unwrap().is_empty(),"comparison restore must not manufacture a new research attempt");
 let mut tampered=record.clone();tampered.result.comparison.report.matches_within_tolerance=false;
 let rehashed=sqx_project::archive::content_id(&serde_json::to_vec(&tampered).unwrap());
 assert!(restore_comparison(&portable,&rehashed,&tampered).is_err(),"rehashed fabricated results must still fail recomputation");
 let mut badsource:serde_json::Value=serde_json::from_str(&source).unwrap();badsource["unexpected_field"]=json!(true);
 assert!(save_comparison(&archive,&id,&badsource.to_string(),&tolerance()).is_err());
 let altered_source=format!("{source}\n");let second=save_comparison(&archive,&id,&altered_source,&tolerance()).unwrap();assert_ne!(second,comparison_id);
 std::fs::write(root.join("restored/runs").join(format!("{id}.json")),b"{}").unwrap();assert!(compare_archived(&restored,&id,&declared,&[observed()],&tolerance()).is_err());
 std::fs::remove_dir_all(root).unwrap();
}
