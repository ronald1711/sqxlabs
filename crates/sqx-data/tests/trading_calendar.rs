use sqx_data::{calendar::{CalendarBinding,CalendarProfile,EvidenceStatus},quality::{audit,MarketCalendar,WeeklySession,GapKind}};
use sqx_core::{Bar,Timeframe};
use std::collections::BTreeMap;
fn ms(s:&str)->i64 {chrono::DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()}
fn profile(zone:&str)->CalendarProfile {CalendarProfile{schema:1,id:"independent-test".into(),version:"1".into(),name:"Synthetic reference schedule".into(),source:"test-feed".into(),instruments:vec!["EURUSD".into()],timezone:zone.into(),clock_shift_minutes:0,weekly:vec![],date_overrides:BTreeMap::new(),valid_from:"2026-01-01".into(),valid_until:"2027-01-01".into(),evidence:EvidenceStatus::Documented,references:vec!["Synthetic test specification, not broker evidence".into()],reviewed_on:"2026-11-25".into(),notes:"Explicit synthetic schedule with hand specified UTC expectations".into()}}
fn binding()->CalendarBinding {CalendarBinding{source:"test-feed".into(),instrument:"EURUSD".into(),source_confirmed:true,utc_timestamps_confirmed:true}}
fn bar(t:i64)->Bar {Bar::new(t,1.,1.1,0.9,1.,1.)}
fn fx()->CalendarProfile {let mut p=profile("America/New_York");p.weekly=(0..4).map(|weekday|WeeklySession{weekday,start_minute:0,end_minute:1440}).collect();p.weekly.push(WeeklySession{weekday:4,start_minute:0,end_minute:1020});p.weekly.push(WeeklySession{weekday:6,start_minute:1020,end_minute:1440});p}
#[test]
fn new_york_weekend_boundaries_follow_us_not_european_dst(){
 let p=fx();
 // These expected UTC values are explicit, not computed using the resolver.
 for (day,opening) in [("2026-03-01","2026-03-01T22:00:00Z"),("2026-03-08","2026-03-08T21:00:00Z"),("2026-03-22","2026-03-22T21:00:00Z"),("2026-10-25","2026-10-25T21:00:00Z"),("2026-11-01","2026-11-01T22:00:00Z")] {
  let start=ms(&format!("{day}T00:00:00Z"));let f=p.freeze(binding(),start,start+86400000).unwrap();let open=ms(opening);
  assert_eq!(f.overlaps(open-60000,open),Some(false),"{day}");assert_eq!(f.overlaps(open,open+60000),Some(true),"{day}");
 }
}
#[test]
fn missing_and_repeated_local_hours_are_not_guessed(){
 let mut p=profile("Europe/Amsterdam");p.weekly=vec![WeeklySession{weekday:6,start_minute:120,end_minute:180}];
 let spring=ms("2026-03-29T00:00:00Z");let f=p.freeze(binding(),spring,spring+4*3600000).unwrap();assert!(f.sessions.is_empty(),"02:00-03:00 does not exist on spring transition");
 let autumn=ms("2026-10-25T00:00:00Z");let f=p.freeze(binding(),autumn,autumn+4*3600000).unwrap();assert_eq!(f.sessions.len(),1);assert_eq!((f.sessions[0].start,f.sessions[0].end),(autumn,autumn+2*3600000));
}
#[test]
fn pauses_holidays_and_short_sessions_override_weekly_hours(){
 let mut p=profile("UTC");p.weekly=(0..5).flat_map(|weekday|[WeeklySession{weekday,start_minute:0,end_minute:720},WeeklySession{weekday,start_minute:725,end_minute:1440}]).collect();
 p.date_overrides.insert("2026-12-25".into(),vec![]);p.date_overrides.insert("2026-12-24".into(),vec![(0,780)]);
 let start=ms("2026-12-23T00:00:00Z");let f=p.freeze(binding(),start,ms("2026-12-26T00:00:00Z")).unwrap();
 for (s,expected) in [("2026-12-23T11:59:00Z",true),("2026-12-23T12:00:00Z",false),("2026-12-23T12:04:00Z",false),("2026-12-23T12:05:00Z",true),("2026-12-24T12:00:00Z",true),("2026-12-24T13:00:00Z",false),("2026-12-25T10:00:00Z",false)] {let t=ms(s);assert_eq!(f.overlaps(t,t+60000),Some(expected),"{s}");}
}
#[test]
fn unknown_source_or_clock_never_turns_gaps_into_confirmed_closures(){
 let p=fx();let first=ms("2026-11-20T21:59:00Z");let last=ms("2026-11-22T22:00:00Z");
 for (source,clock,expected) in [(true,true,"documented_binding"),(false,true,"unconfirmed_binding"),(true,false,"unconfirmed_binding")] {
  let mut b=binding();b.source_confirmed=source;b.utc_timestamps_confirmed=clock;if !source{b.source="mixed".into();}
  let f=p.freeze(b,first,last+60000).unwrap();let c=MarketCalendar{name:p.name.clone(),weekly:vec![],date_overrides:BTreeMap::new(),frozen:Some(f)};
  let q=audit(&[bar(first),bar(last)],Timeframe::M1,last+60000,Some(&c));assert!(q.errors.is_empty());assert_eq!(q.calendar_status,expected);
  if source&&clock {assert_eq!(q.scheduled_closed_intervals,2880);assert_eq!(q.unclassified_intervals,0);assert_eq!(q.gaps[0].kind,GapKind::ScheduledClosed);}else{assert_eq!(q.unclassified_intervals,2880);assert_eq!(q.scheduled_closed_intervals,0);}
 }
}
#[test]
fn archived_snapshot_survives_later_calendar_changes_and_coverage_is_bounded(){
 let mut p=fx();let start=ms("2026-11-20T00:00:00Z");let end=start+4*86400000;let frozen=p.freeze(binding(),start,end).unwrap();let bytes=serde_json::to_vec(&frozen).unwrap();
 p.version="2".into();p.weekly.clear();let changed=p.freeze(binding(),start,end).unwrap();assert_ne!(changed.identity(),frozen.identity());
 let restored: sqx_data::calendar::FrozenCalendar=serde_json::from_slice(&bytes).unwrap();assert_eq!(restored.identity(),frozen.identity());assert_eq!(restored.overlaps(start,start+60000),Some(true));assert_eq!(changed.overlaps(start,start+60000),Some(false));assert_eq!(restored.overlaps(start-60000,start),None);
 let mut mismatch=binding();mismatch.source="other-feed".into();assert!(p.freeze(mismatch,start,end).is_err());
 p.timezone="not/a/zone".into();assert!(p.freeze(binding(),start,end).is_err());
}
