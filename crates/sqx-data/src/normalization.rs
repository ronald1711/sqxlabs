//! Verify a clock-normalization bundle before attaching its exact source evidence.
use serde::{Deserialize, Serialize};
use sha2::{Digest,Sha256};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub manifest_json:String,
    pub policy_json:String,
    pub raw_csv:String,
    pub utc_csv:String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema:u32, server:String, raw_sha256:String, policy_sha256:String,
    normalized_sha256:String, rows:usize, bar_duration_ms:Option<i64>,
    #[serde(default)] legacy_broker_clock:bool,
    clock:String, scope:String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema:u32, server:String, intervals:Vec<Interval>,
    #[serde(default)] description:Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Interval {raw_start_ms:i64,raw_end_ms:i64,offset_seconds:i64,source:String}
#[derive(Debug, Serialize)]
pub struct Verified {
    pub bundle_id:String,
    pub server:String,
    pub rows:usize,
    pub bar_duration_ms:Option<i64>,
    pub kind:&'static str,
    pub scope:&'static str,
}
fn hash(text:&str)->String {format!("{:x}",Sha256::digest(text.as_bytes()))}
fn err(error:impl std::fmt::Display)->String{error.to_string()}
impl Bundle {
    pub fn load(path:&Path)->Result<Self,String> {
        let read=|name:&str|std::fs::read_to_string(path.join(name)).map_err(err);
        Ok(Self{manifest_json:read("manifest.json")?,policy_json:read("policy.json")?,raw_csv:read("raw.csv")?,utc_csv:read("utc.csv")?})
    }
    pub fn verify_bars(&self,bars:&[sqx_core::Bar],duration:Option<i64>)->Result<Verified,String> {
        let verified=self.verify()?;
        if verified.kind!="bars" || verified.bar_duration_ms!=duration {return Err("normalization bundle does not match the declared candle timeframe".into());}
        if crate::read_csv_bars_str(&self.utc_csv)?!=bars {return Err("normalization bundle does not match the supplied candles".into());}
        Ok(verified)
    }
    pub fn verify_ticks(&self,ticks:&[sqx_core::Tick])->Result<Verified,String> {
        let verified=self.verify()?;
        if verified.kind!="ticks" {return Err("normalization bundle does not contain ticks".into());}
        if crate::read_ticks_csv_str(&self.utc_csv)?!=ticks {return Err("normalization bundle does not match the supplied ticks".into());}
        Ok(verified)
    }
    pub fn verify(&self)->Result<Verified,String> {
        let m:Manifest=serde_json::from_str(&self.manifest_json).map_err(err)?;
        let p:Policy=serde_json::from_str(&self.policy_json).map_err(err)?;
        if m.schema!=1||p.schema!=1||m.server.trim().is_empty()||m.server!=p.server||p.intervals.is_empty()||m.rows==0||m.clock.trim().is_empty()||m.scope.trim().is_empty() {
            return Err("invalid normalization manifest or policy identity".into());
        }
        let _=&p.description;
        if hash(&self.raw_csv)!=m.raw_sha256||hash(&self.utc_csv)!=m.normalized_sha256||hash(&self.policy_json)!=m.policy_sha256 {
            return Err("normalization bundle hash mismatch".into());
        }
        let mut previous:Option<(i64,i64)>=None;
        for interval in &p.intervals {
            if interval.raw_start_ms>=interval.raw_end_ms || !(-50400..=50400).contains(&interval.offset_seconds) || interval.source.trim().is_empty() {
                return Err("invalid normalization interval".into());
            }
            let offset=interval.offset_seconds*1000;
            let start=interval.raw_start_ms.checked_sub(offset).ok_or("normalized interval overflow")?;
            let end=interval.raw_end_ms.checked_sub(offset).ok_or("normalized interval overflow")?;
            if previous.is_some_and(|(raw_end,utc_end)|interval.raw_start_ms<raw_end||start<utc_end) {
                return Err("overlapping or unordered normalization intervals".into());
            }
            previous=Some((interval.raw_end_ms,end));
        }
        let mut raw=self.raw_csv.trim_start_matches('\u{feff}').lines();
        let mut utc=self.utc_csv.lines();
        let raw_header=raw.next().ok_or("missing raw CSV header")?;
        let utc_header=utc.next().ok_or("missing UTC CSV header")?;
        let (scale,kind)=match utc_header {
            "timestamp,open,high,low,close,volume" if raw_header=="timestamp_broker,open,high,low,close,volume" || (m.legacy_broker_clock&&raw_header==utc_header)=>(1000i64,"bars"),
            "timestamp_ms,bid,ask" if raw_header=="timestamp_broker_ms,bid,ask" || (m.legacy_broker_clock&&raw_header==utc_header)=>(1i64,"ticks"),
            _=>return Err("normalization CSV clock schema mismatch".into()),
        };
        if (kind=="bars"&&!m.bar_duration_ms.is_some_and(|d|d>0)) || (kind=="ticks"&&m.bar_duration_ms.is_some()) {
            return Err("normalization candle duration mismatch".into());
        }
        let mut count=0;
        let mut prior:Option<(i64,i64)>=None;
        loop {
            let (source,normalized)=match (raw.next(),utc.next()) {(None,None)=>break,(Some(a),Some(b))=>(a,b),_=>return Err("normalization row count mismatch".into())};
            let (source_time,source_payload)=source.split_once(',').ok_or("invalid raw CSV row")?;
            let (utc_time,utc_payload)=normalized.split_once(',').ok_or("invalid UTC CSV row")?;
            if source_payload!=utc_payload {return Err("normalization changed non-time CSV fields".into());}
            let source_time=source_time.parse::<i64>().map_err(err)?.checked_mul(scale).ok_or("raw timestamp overflow")?;
            let utc_time=utc_time.parse::<i64>().map_err(err)?.checked_mul(scale).ok_or("UTC timestamp overflow")?;
            let interval=p.intervals.iter().find(|r|r.raw_start_ms<=source_time&&source_time<r.raw_end_ms).ok_or("timestamp outside normalization policy")?;
            if m.bar_duration_ms.is_some_and(|d|source_time.checked_add(d).is_none_or(|end|end>interval.raw_end_ms)) {
                return Err("candle crosses normalization policy boundary".into());
            }
            if source_time.checked_sub(interval.offset_seconds*1000)!=Some(utc_time) {return Err("UTC timestamp does not follow normalization policy".into());}
            if prior.is_some_and(|(a,b)|source_time<a||utc_time<b||(kind=="bars"&&(source_time==a||utc_time==b))) {
                return Err("invalid normalized chronology".into());
            }
            prior=Some((source_time,utc_time));count+=1;
        }
        if count!=m.rows {return Err("normalization row count mismatch".into());}
        if kind=="bars" {crate::read_csv_bars_str(&self.utc_csv)?;}else{crate::read_ticks_csv_str(&self.utc_csv)?;}
        Ok(Verified{bundle_id:hash(&self.manifest_json),server:m.server,rows:count,bar_duration_ms:m.bar_duration_ms,kind,scope:"Verified clock transformation and CSV structure; broker identity, policy truth and market-data completeness are not independently authenticated."})
    }
}
