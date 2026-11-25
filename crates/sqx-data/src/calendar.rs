//! Versioned local-time schedules compiled to immutable UTC session intervals.
//! Audits use the frozen intervals, never the machine's current timezone rules.
use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use crate::quality::WeeklySession;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum EvidenceStatus { Documented, Provisional, MarketAssumption }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarProfile {
    pub schema:u32,
    pub id:String,
    pub version:String,
    pub name:String,
    pub source:String,
    pub instruments:Vec<String>,
    /// IANA name. Sessions and date overrides use this local clock.
    pub timezone:String,
    /// Explicit broker-clock offset relative to the named zone, e.g. NY+7.
    #[serde(default)] pub clock_shift_minutes:i32,
    pub weekly:Vec<WeeklySession>,
    #[serde(default)] pub date_overrides:BTreeMap<String,Vec<(u32,u32)>>,
    pub valid_from:String,
    /// Exclusive local date, so historical assumptions have bounded scope.
    pub valid_until:String,
    pub evidence:EvidenceStatus,
    pub references:Vec<String>,
    pub reviewed_on:String,
    pub notes:String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarBinding {
    /// Descriptive feed identity, never credentials or an authenticated URL.
    pub source:String,
    pub instrument:String,
    pub source_confirmed:bool,
    pub utc_timestamps_confirmed:bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UtcSession { pub start:i64, pub end:i64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenCalendar {
    pub schema:u32,
    pub profile:CalendarProfile,
    pub profile_sha256:String,
    pub binding:CalendarBinding,
    pub tzdb_version:String,
    pub coverage_start:i64,
    pub coverage_end:i64,
    pub sessions:Vec<UtcSession>,
}
fn date(s:&str)->Result<NaiveDate,String>{NaiveDate::parse_from_str(s,"%Y-%m-%d").map_err(|_|"Calendar dates must use YYYY-MM-DD".into())}
impl CalendarProfile {
    pub fn identity(&self)->String {format!("{:x}",Sha256::digest(serde_json::to_vec(self).expect("finite calendar fields")))}
    pub fn validate(&self)->Result<(),String>{
        if self.schema!=1 || [&self.id,&self.version,&self.name,&self.source].iter().any(|s|s.trim().is_empty()||s.len()>200){return Err("Calendar identity, version, name and source are required".into());}
        if self.instruments.is_empty()||self.instruments.len()>100||self.instruments.iter().any(|s|s.is_empty()||s.len()>100){return Err("Calendar requires explicit instruments".into());}
        self.timezone.parse::<chrono_tz::Tz>().map_err(|_|"Unknown IANA calendar timezone")?;
        if !(-1440..=1440).contains(&self.clock_shift_minutes){return Err("Calendar clock shift exceeds one day".into());}
        if date(&self.valid_from)? >= date(&self.valid_until)? {return Err("Calendar validity range is empty or reversed".into());}
        date(&self.reviewed_on)?;
        if self.weekly.len()>100 || self.date_overrides.len()>4000 || self.weekly.iter().any(|s|s.weekday>6||s.start_minute>=s.end_minute||s.end_minute>1440){return Err("Invalid weekly calendar sessions".into());}
        for (d,ranges) in &self.date_overrides {date(d)?;if ranges.len()>50||ranges.iter().any(|(a,b)|a>=b||*b>1440){return Err("Invalid calendar date override".into());}}
        if self.notes.len()>10000||self.references.len()>20||self.references.iter().any(|s|s.is_empty()||s.len()>2000){return Err("Calendar evidence exceeds its bounds".into());}
        if self.evidence!=EvidenceStatus::Provisional && self.references.is_empty(){return Err("Documented calendars and market assumptions require source references".into());}
        Ok(())
    }
    pub fn freeze(&self,binding:CalendarBinding,start:i64,end:i64)->Result<FrozenCalendar,String>{
        self.validate()?;
        if !self.instruments.contains(&binding.instrument){return Err("Calendar does not cover this instrument".into());}
        if self.evidence!=EvidenceStatus::MarketAssumption && binding.source_confirmed && binding.source!=self.source{return Err("Confirmed feed does not match calendar source".into());}
        if binding.source.len()>200||binding.source.trim().is_empty(){return Err("Feed identity is required; use unknown or mixed when appropriate".into());}
        let length=end.checked_sub(start).ok_or("Invalid calendar range")?;
        if length<=0||length>3660*86_400_000_i64{return Err("Calendar snapshot must cover more than zero and at most ten years".into());}
        let zone=self.timezone.parse::<chrono_tz::Tz>().map_err(|_|"Unknown calendar timezone")?;
        let from=date(&self.valid_from)?;let until=date(&self.valid_until)?;
        let mut frozen=FrozenCalendar{schema:1,profile:self.clone(),profile_sha256:self.identity(),binding,tzdb_version:chrono_tz::IANA_TZDB_VERSION.into(),coverage_start:start,coverage_end:start,sessions:vec![]};
        let mut cursor=start.div_euclid(60000).checked_mul(60000).ok_or("Invalid calendar start")?;
        let mut covered=false;
        // Walking UTC minutes handles both copies of a repeated local minute and
        // never invents a nonexistent local minute at a spring transition.
        while cursor<end {
            let next=cursor.checked_add(60000).ok_or("Invalid calendar end")?;
            let local=DateTime::<Utc>::from_timestamp_millis(cursor).ok_or("Calendar time out of range")?.with_timezone(&zone).naive_local().checked_add_signed(Duration::minutes(i64::from(self.clock_shift_minutes))).ok_or("Calendar clock out of range")?;
            if local.date()>=from && local.date()<until {
                let a=cursor.max(start);let b=next.min(end);
                if !covered {frozen.coverage_start=a;covered=true;}
                frozen.coverage_end=b;
                let minute=local.hour()*60+local.minute();
                let open=if let Some(ranges)=self.date_overrides.get(&local.date().to_string()){ranges.iter().any(|(a,b)|minute>=*a&&minute<*b)}else{self.weekly.iter().any(|s|s.weekday==local.weekday().num_days_from_monday()&&minute>=s.start_minute&&minute<s.end_minute)};
                if open {
                    if let Some(last)=frozen.sessions.last_mut().filter(|r|r.end==a){last.end=b;}else{frozen.sessions.push(UtcSession{start:a,end:b});}
                }
            }
            cursor=next;
        }
        frozen.validate()?;Ok(frozen)
    }
}
impl FrozenCalendar {
    pub fn into_market_calendar(self)->crate::quality::MarketCalendar {
        crate::quality::MarketCalendar{name:self.profile.name.clone(),weekly:vec![],date_overrides:BTreeMap::new(),frozen:Some(self)}
    }
    pub fn identity(&self)->String {format!("{:x}",Sha256::digest(serde_json::to_vec(self).expect("finite frozen calendar fields")))}
    pub fn validate(&self)->Result<(),String>{
        self.profile.validate()?;
        if self.schema!=1||self.profile_sha256!=self.profile.identity()||self.coverage_start>self.coverage_end||self.tzdb_version.is_empty(){return Err("Invalid frozen calendar identity or coverage".into());}
        if !self.profile.instruments.contains(&self.binding.instrument)||(self.profile.evidence!=EvidenceStatus::MarketAssumption&&self.binding.source_confirmed&&self.binding.source!=self.profile.source){return Err("Frozen calendar source/instrument mismatch".into());}
        let mut end=self.coverage_start;
        for r in &self.sessions {if r.start<end||r.start>=r.end||r.end>self.coverage_end{return Err("Invalid frozen UTC sessions".into());}end=r.end;}
        Ok(())
    }
    pub fn classification_ready(&self)->bool {self.binding.utc_timestamps_confirmed && (self.profile.evidence==EvidenceStatus::MarketAssumption || (self.profile.evidence==EvidenceStatus::Documented && self.binding.source_confirmed))}
    /// None means unknown, including an interval partly outside bounded evidence.
    pub fn overlaps(&self,start:i64,end:i64)->Option<bool>{
        if !self.classification_ready()||start>=end||start<self.coverage_start||end>self.coverage_end{return None;}
        let i=self.sessions.partition_point(|r|r.end<=start);
        Some(self.sessions.get(i).is_some_and(|r|r.start<end))
    }
}
