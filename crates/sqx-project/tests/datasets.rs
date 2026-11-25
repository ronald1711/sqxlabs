use sqx_project::datasets::Datasets;
use sqx_data::SymbolSeries;
use sqx_core::{Bar,Timeframe};
struct Temp(std::path::PathBuf);
impl Temp{fn new()->Self{Self(std::env::temp_dir().join(format!("sqx-imports-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())))}}
impl Drop for Temp{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
#[test]
fn versions_survive_reopening_deduplicate_and_reject_corruption(){
 let temp=Temp::new();let store=Datasets::new(&temp.0);
 let original=SymbolSeries{symbol:"EURUSD".into(),timeframe:Timeframe::M1,calendar:None,source:None,normalization:None,bars:vec![Bar::new(1700000040000,1.0,1.1,0.9,1.0,1.0)]};
 let first=store.save(&original).unwrap();assert_eq!(store.save(&original).unwrap(),first);
 let mut second=original.clone();second.bars[0].close=1.01;let next=store.save(&second).unwrap();assert_ne!(first,next);
 let reopened=Datasets::new(&temp.0);assert_eq!(reopened.list().unwrap().len(),2);
 assert_eq!(serde_json::to_value(reopened.load(&first).unwrap()).unwrap(),serde_json::to_value(&original).unwrap());
 assert_eq!(reopened.load(&next).unwrap().bars,second.bars);
 std::fs::write(temp.0.join("imports").join(format!("{first}.json")),b"{}").unwrap();
 assert!(reopened.load(&first).unwrap_err().contains("integrity"));assert!(reopened.save(&original).is_err());
 assert!(reopened.load("../../outside").is_err());assert_eq!(reopened.load(&next).unwrap().bars,second.bars);
}

#[test]
fn unknown_feed_survives_archive_and_changes_create_a_new_version(){
 let temp=Temp::new();let store=Datasets::new(&temp.0);
 let legacy=r#"{"symbol":"EURUSD","timeframe":"M1","calendar":null,"normalization":null,"bars":[]}"#;
 let mut series:SymbolSeries=serde_json::from_str(legacy).unwrap();
 assert!(series.source.is_none());
 assert!(serde_json::to_value(&series).unwrap().get("source").is_none());
 series.source=Some(sqx_data::calendar::CalendarBinding{source:"mixed".into(),instrument:"EURUSD".into(),source_confirmed:false,utc_timestamps_confirmed:false});
 series.bars=vec![Bar::new(1700000040000,1.0,1.1,0.9,1.0,1.0)];
 let original=store.save(&series).unwrap();
 let saved=store.load(&original).unwrap();
 assert_eq!(saved.source.as_ref().unwrap().source,"mixed");
 assert!(!saved.source.unwrap().source_confirmed);
 series.source.as_mut().unwrap().source="identified feed".into();
 let revised=store.save(&series).unwrap();assert_ne!(original,revised);
 assert_eq!(store.load(&original).unwrap().source.unwrap().source,"mixed");
}
