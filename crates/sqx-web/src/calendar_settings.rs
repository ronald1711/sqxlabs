//! Explicit dataset calendar changes and source-scoped QuestDB import settings.
use super::*;
use serde::{Deserialize,Serialize};
use sqx_data::calendar::{CalendarBinding,CalendarProfile};
use std::{path::Path,sync::Mutex};
static MUTATION:Mutex<()>=Mutex::new(());
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Selection {pub binding:CalendarBinding,pub profile:Option<CalendarProfile>}
impl Selection {
    fn validate(&self,symbol:&str)->Result<(),String>{
        if self.binding.instrument!=symbol || self.binding.source.trim().is_empty() || self.binding.source.len()>200 {return Err("Specify the original feed and matching instrument; use unknown or mixed if necessary".into());}
        if self.binding.source_confirmed && matches!(self.binding.source.as_str(),"unknown"|"mixed") {return Err("An unknown or mixed feed cannot be confirmed".into());}
        if let Some(profile)=&self.profile {profile.validate()?;if !profile.instruments.contains(&symbol.to_string()) || (profile.evidence!=sqx_data::calendar::EvidenceStatus::MarketAssumption&&self.binding.source_confirmed&&profile.source!=self.binding.source){return Err("Calendar source or instrument does not match this feed".into());}}
        Ok(())
    }
    pub(super) fn apply(&self,series:&mut SymbolSeries)->Result<(),String>{
        self.validate(&series.symbol)?;
        let calendar=if let Some(profile)=&self.profile {
            let start=series.bars.first().ok_or("No candles to assess")?.time;
            let last=series.bars.last().ok_or("No candles to assess")?.time;
            let end=sqx_data::quality::candle_end(last,series.timeframe).ok_or("Invalid candle duration")?;
            Some(profile.freeze(self.binding.clone(),start,end)?.into_market_calendar())
        }else{None};
        series.calendar=calendar;series.source=Some(self.binding.clone());Ok(())
    }
}
pub(super) fn identity(series:&SymbolSeries)->String {sqx_project::archive::content_id(&serde_json::to_vec(series).expect("serializable series"))}
fn scope(client:&QuestdbClient,symbol:&str)->String {sqx_project::archive::content_id(format!("{}\n{symbol}",client.source_scope()).as_bytes())}
fn read_selection(root:&Path,key:&str)->Result<Option<Selection>,String>{
    let path=root.join("calendar-settings").join(format!("{key}.json"));
    match std::fs::read(path){Ok(bytes)=>serde_json::from_slice(&bytes).map(Some).map_err(|_|"Stored calendar settings are invalid".into()),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(None),Err(e)=>Err(format!("Cannot read calendar settings: {e}"))}
}
fn write_selection(root:&Path,key:&str,selection:&Selection)->Result<(),String>{
    use std::io::Write;
    let dir=root.join("calendar-settings");std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let path=dir.join(format!("{key}.json"));let temp=dir.join(format!(".pending-{:032x}",rand::random::<u128>()));
    let result=(||{let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(&temp).map_err(|e|e.to_string())?;
        file.write_all(&serde_json::to_vec(selection).map_err(|e|e.to_string())?).and_then(|_|file.sync_all()).map_err(|e|e.to_string())?;
        std::fs::rename(&temp,&path).map_err(|e|e.to_string())?;Ok(())})();
    if result.is_err(){let _=std::fs::remove_file(temp);}result
}
pub(super) fn for_import(state:&State,symbol:&str)->Result<Selection,String>{
    let client=state.questdb.as_ref().ok_or("QuestDB is not configured")?;
    Ok(read_selection(state.archive.root(),&scope(client,symbol))?.unwrap_or(Selection{binding:CalendarBinding{source:"unknown".into(),instrument:symbol.into(),source_confirmed:false,utc_timestamps_confirmed:false},profile:None}))
}
pub(super) fn report(series:&SymbolSeries,as_of:i64)->serde_json::Value{
    let mut quality=sqx_data::quality::audit(&series.bars,series.timeframe,as_of,series.calendar.as_ref());
    if let Err(error)=series.validate_calendar_context(){quality.errors.push(error);}
    let unresolved=quality.missing_open_intervals>0||quality.unclassified_intervals>0||quality.irregular_steps>0;
    json!({"dataset_id":identity(series),"symbol":series.symbol,"timeframe":series.timeframe,"as_of_ms":as_of,"source":series.source,"calendar":series.calendar,"data_quality":quality,"unresolved":unresolved,"blocked":!quality.errors.is_empty(),"normalization":normalization_status(series),"scope":"Calendar assessment is not proof of complete data or trading-system correctness. No missing candles are filled."})
}
pub(super) fn route(action:&str,body:&str,state:&State)->String{
    let result=(||->Result<serde_json::Value,String>{
        let input:serde_json::Value=serde_json::from_str(body).map_err(|e|e.to_string())?;
        let key=input["symbol"].as_str().ok_or("Select a dataset first")?;
        let mut series=find_series(state,key).ok_or("Dataset missing or ambiguous")?;
        if action=="inspect"{let mut out=report(&series,sqx_data::quality::now_ms());out["questdb_available"]=json!(state.questdb.is_some());return Ok(out);}
        let _guard=MUTATION.try_lock().map_err(|_|"Another calendar change is in progress")?;
        if input["dataset_id"]!=identity(&series){return Err("Dataset changed; inspect it again before applying calendar settings".into());}
        if action=="refetch" {
            let client=state.questdb.as_ref().ok_or("QuestDB is not configured")?;
            let start=series.bars.first().ok_or("No candles")?.time;
            let end=sqx_data::quality::candle_end(series.bars.last().ok_or("No candles")?.time,series.timeframe).ok_or("Invalid candle interval")?;
            let bars=client.fetch_bars(&series.symbol,series.timeframe,Some(start),Some(end),Some(100001))?;
            if bars.len()>100000{return Err("Repair range exceeds 100000 candles; import a smaller explicit range".into());}
            let mut replacement=SymbolSeries{symbol:series.symbol.clone(),timeframe:series.timeframe,calendar:None,source:None,normalization:None,bars};
            for_import(state,&series.symbol)?.apply(&mut replacement)?;
            let old:std::collections::BTreeMap<_,_>=series.bars.iter().map(|b|(b.time,b)).collect();
            let new:std::collections::BTreeMap<_,_>=replacement.bars.iter().map(|b|(b.time,b)).collect();
            let added=new.keys().filter(|t|!old.contains_key(t)).count();
            let removed=old.keys().filter(|t|!new.contains_key(t)).count();
            let revised=new.iter().filter(|(t,b)|old.get(t).is_some_and(|old|*old!=**b)).count();
            let saved=sqx_project::datasets::Datasets::new(state.archive.root());
            saved.save(&series)?;let id=saved.save(&replacement)?;
            let mut out=report(&replacement,sqx_data::quality::now_ms());
            out["snapshot_id"]=json!(id);out["changes"]=json!({"added":added,"removed":removed,"revised":revised});
            out["loaded"]=json!(false);return Ok(out);
        }
        let selection:Selection=serde_json::from_value(input["selection"].clone()).map_err(|e|e.to_string())?;
        selection.apply(&mut series)?;
        let out=report(&series,sqx_data::quality::now_ms());
        if action=="preview" {return Ok(out);}
        if action!="save"{return Err("Unknown calendar action".into());}
        if out["blocked"]==true {return Err(format!("Calendar conflicts with the data: {}",out["data_quality"]["errors"]));}
        let mut store=state.store.write().unwrap();
        let target=store.series.iter_mut().find(|s|s.symbol==series.symbol&&s.timeframe==series.timeframe).ok_or("Dataset no longer loaded")?;
        if input["dataset_id"]!=identity(target){return Err("Dataset changed during calendar review; inspect it again".into());}
        let saved=sqx_project::datasets::Datasets::new(state.archive.root());
        saved.save(target)?;let snapshot=saved.save(&series)?;
        if input["use_for_questdb_imports"]==true {
            let client=state.questdb.as_ref().ok_or("QuestDB is not configured")?;
            write_selection(state.archive.root(),&scope(client,&series.symbol),&selection)?;
        }
        *target=series;
        let mut out=out;out["snapshot_id"]=json!(snapshot);out["ok"]=json!(true);Ok(out)
    })();result.unwrap_or_else(|error|json!({"error":error})).to_string()
}
/// Bind an explicit UI review to immutable data and the original audit cutoff.
pub(super) fn reviewed_quality(series:&SymbolSeries,request:&serde_json::Value)->Result<Option<sqx_data::quality::QualityReport>,String>{
    let Some(review)=request.get("data_review") else{return Ok(None)};
    if review["dataset_id"]!=identity(series){return Err("Dataset or calendar changed after review; review the data again".into());}
    let cutoff=review["as_of_ms"].as_i64().ok_or("Missing data review time")?;
    if cutoff>sqx_data::quality::now_ms(){return Err("Data review time is in the future".into());}
    let quality=sqx_data::quality::audit(&series.bars,series.timeframe,cutoff,series.calendar.as_ref());
    if serde_json::to_value(&quality).map_err(|e|e.to_string())?!=review["data_quality"]{return Err("Data review differs from its recorded assessment".into());}
    if request["allow_data_gaps"]==true && review["exploratory_consent"]!=true{return Err("Explicit exploratory consent is required for this data review".into());}
    Ok(Some(quality))
}

#[cfg(test)]mod tests{
 use super::*;
 static TEST_GATE:Mutex<()>=Mutex::new(());
 fn state()->State{
    let mut store=DataStore::new();store.add("EURUSD",Timeframe::M1,vec![Bar::new(0,1.,1.,1.,1.,1.),Bar::new(120000,1.,1.,1.,1.,1.)]);
    State{assistant_gate:std::sync::Mutex::new(()),archive:sqx_project::archive::Archive::new(std::env::temp_dir().join(format!("sqx-calendar-{:032x}",rand::random::<u128>()))),store:RwLock::new(store),databank:RwLock::new(vec![]),questdb:None}
 }
 #[test]fn review_requires_exact_dataset_cutoff_and_explicit_consent(){
    let state=state();let series=find_series(&state,"EURUSD").unwrap();
    let report=report(&series,180000);assert_eq!(report["unresolved"],true);assert_eq!(report["data_quality"]["unclassified_intervals"],1);
    let mut req=json!({"allow_data_gaps":true,"data_review":{"dataset_id":report["dataset_id"],"as_of_ms":180000,"data_quality":report["data_quality"],"exploratory_consent":false}});
    assert!(reviewed_quality(&series,&req).unwrap_err().contains("consent"));req["data_review"]["exploratory_consent"]=json!(true);
    assert!(research_data_quality(&series,&req,999999).is_ok());
    let mut changed=series.clone();changed.bars[0].volume=2.;assert!(reviewed_quality(&changed,&req).unwrap_err().contains("changed"));
    req["data_review"]["data_quality"]["unclassified_intervals"]=json!(0);assert!(reviewed_quality(&series,&req).unwrap_err().contains("differs"));
 }
 #[test]fn saving_unknown_source_preserves_both_versions_and_rejects_stale_edits(){
    let _test_guard=TEST_GATE.lock().unwrap();
    let state=state();let original=find_series(&state,"EURUSD").unwrap();
    let selection=Selection{binding:CalendarBinding{source:"mixed".into(),instrument:"EURUSD".into(),source_confirmed:false,utc_timestamps_confirmed:false},profile:None};
    let request=json!({"symbol":"EURUSD","dataset_id":identity(&original),"selection":selection});
    let preview:serde_json::Value=serde_json::from_str(&route("preview",&request.to_string(),&state)).unwrap();assert_eq!(preview["data_quality"]["unclassified_intervals"],1);assert!(find_series(&state,"EURUSD").unwrap().source.is_none());
    let saved:serde_json::Value=serde_json::from_str(&route("save",&request.to_string(),&state)).unwrap();assert_eq!(saved["ok"],true,"{saved}");
    let datasets=sqx_project::datasets::Datasets::new(state.archive.root());assert_eq!(datasets.list().unwrap().len(),2);
    let new=datasets.load(saved["snapshot_id"].as_str().unwrap()).unwrap();assert_eq!(new.source.unwrap().source,"mixed");
    let stale:serde_json::Value=serde_json::from_str(&route("save",&request.to_string(),&state)).unwrap();assert!(stale["error"].as_str().unwrap().contains("changed"));
    std::fs::remove_dir_all(state.archive.root()).unwrap();
 }
 #[test]fn questdb_settings_are_scoped_by_endpoint_and_instrument(){
    let state=state();let a=QuestdbClient::new(&sqx_data::QuestdbConfig{host:"127.0.0.1".into(),port:9000,user:None,password:None});
    let b=QuestdbClient::new(&sqx_data::QuestdbConfig{host:"127.0.0.2".into(),port:9000,user:None,password:None});
    let selection=Selection{binding:CalendarBinding{source:"unknown".into(),instrument:"EURUSD".into(),source_confirmed:false,utc_timestamps_confirmed:false},profile:None};
    write_selection(state.archive.root(),&scope(&a,"EURUSD"),&selection).unwrap();assert!(read_selection(state.archive.root(),&scope(&a,"EURUSD")).unwrap().is_some());
    assert!(read_selection(state.archive.root(),&scope(&b,"EURUSD")).unwrap().is_none());assert!(read_selection(state.archive.root(),&scope(&a,"GBPUSD")).unwrap().is_none());
    let mut invalid=selection.clone();invalid.binding.source_confirmed=true;assert!(invalid.validate("EURUSD").is_err());assert!(selection.validate("USDJPY").is_err());
    std::fs::remove_dir_all(state.archive.root()).unwrap();
 }
 #[test]fn refetch_is_a_saved_proposal_and_never_silently_replaces_loaded_data(){
    let _test_guard=TEST_GATE.lock().unwrap();
    use std::io::{Read,Write};
    let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let port=listener.local_addr().unwrap().port();
    let server=std::thread::spawn(move||{
        let(mut socket,_)=listener.accept().unwrap();socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let mut request=Vec::new();let mut buf=[0;1024];while !request.windows(4).any(|w|w==b"\r\n\r\n"){let n=socket.read(&mut buf).unwrap();assert!(n>0);request.extend_from_slice(&buf[..n]);}
        let request=String::from_utf8(request).unwrap();assert!(request.starts_with("GET /exec?query="));assert!(request.contains("1970-01-01T00%3A00%3A00"));assert!(request.contains("1970-01-01T00%3A03%3A00"));
        let body=json!({"dataset":[["1970-01-01T00:02:00.000000Z",1.,1.,1.,1.,1.],["1970-01-01T00:01:00.000000Z",1.,1.,1.,1.,1.],["1970-01-01T00:00:00.000000Z",1.,1.,1.,1.,1.]]}).to_string();
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let mut state=state();state.questdb=Some(QuestdbClient::new(&sqx_data::QuestdbConfig{host:"127.0.0.1".into(),port,user:None,password:None}));
    let before=find_series(&state,"EURUSD").unwrap();let request=json!({"symbol":"EURUSD","dataset_id":identity(&before)});
    let response:serde_json::Value=serde_json::from_str(&route("refetch",&request.to_string(),&state)).unwrap();server.join().unwrap();
    assert_eq!(response["changes"],json!({"added":1,"removed":0,"revised":0}),"{response}");assert_eq!(response["loaded"],false);
    assert_eq!(identity(&find_series(&state,"EURUSD").unwrap()),identity(&before));
    let saved=sqx_project::datasets::Datasets::new(state.archive.root());let proposed=saved.load(response["snapshot_id"].as_str().unwrap()).unwrap();assert_eq!(proposed.bars.len(),3);assert_eq!(proposed.source.unwrap().source,"unknown");assert_eq!(saved.list().unwrap().len(),2);
    std::fs::remove_dir_all(state.archive.root()).unwrap();
 }

}
