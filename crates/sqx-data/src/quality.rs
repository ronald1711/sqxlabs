//! Explicit data-quality evidence. Calendars are supplied UTC schedules, never
//! guessed from a ticker. UTC date overrides support holidays and DST schedules.
use chrono::{Datelike, DateTime, NaiveDate, Timelike, Utc};
use serde::{Deserialize,Serialize};
use sqx_core::{Bar,Timeframe};
use std::collections::BTreeMap;

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct WeeklySession { pub weekday: u32, pub start_minute:u32, pub end_minute:u32 }
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct MarketCalendar {
    pub name:String,
    /// Monday=0. Sessions are [start,end), split overnight sessions at midnight.
    pub weekly:Vec<WeeklySession>,
    /// YYYY-MM-DD -> UTC minute intervals. Empty means closed; overrides weekly.
    #[serde(default)]
    pub date_overrides:BTreeMap<String,Vec<(u32,u32)>>,
    /// Versioned timezone resolution. Legacy UTC schedules remain readable.
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub frozen:Option<crate::calendar::FrozenCalendar>,
}
impl MarketCalendar {
    pub fn validate(&self)->Result<(),String>{
        if let Some(frozen)=&self.frozen {
            if !self.weekly.is_empty()||!self.date_overrides.is_empty(){return Err("Frozen calendar cannot also contain legacy UTC rules".into());}
            return frozen.validate();
        }
        if self.weekly.iter().any(|s|s.weekday>6 || s.start_minute>=s.end_minute || s.end_minute>1440)
            || self.date_overrides.iter().any(|(d,ranges)|NaiveDate::parse_from_str(d,"%Y-%m-%d").is_err() || ranges.iter().any(|(a,b)|a>=b || *b>1440)){
            return Err("invalid UTC trading calendar".into());
        }
        Ok(())
    }
    /// Whether any tradable time intersects [start,end). Candle labels can be
    /// midnight even when a daily session starts later; opening-time checks alone
    /// would falsely classify missing daily/monthly candles as market closures.
    fn overlaps_session(&self,start:i64,end:i64)->Option<bool>{
        if let Some(frozen)=&self.frozen{return frozen.overlaps(start,end);}
        if start>=end{return None;}
        let mut cursor=start;
        while cursor<end {
            let dt=DateTime::<Utc>::from_timestamp_millis(cursor)?;
            let midnight=dt.date_naive().and_hms_opt(0,0,0)?.and_utc().timestamp_millis();
            let next=midnight.checked_add(86400000)?;
            let overlaps=|a:u32,b:u32| {
                let open=midnight.checked_add(i64::from(a)*60000);
                let close=midnight.checked_add(i64::from(b)*60000);
                open.zip(close).is_some_and(|(open,close)|open<end && close>start)
            };
            let open=if let Some(ranges)=self.date_overrides.get(&dt.format("%Y-%m-%d").to_string()) {
                ranges.iter().any(|(a,b)|overlaps(*a,*b))
            }else{
                self.weekly.iter().filter(|s|s.weekday==dt.weekday().num_days_from_monday()).any(|s|overlaps(s.start_minute,s.end_minute))
            };
            if open{return Some(true);}
            cursor=next;
        }
        Some(false)
    }
    pub fn is_open(&self,time:i64)->bool{
        if let Some(frozen)=&self.frozen{return time.checked_add(1).and_then(|end|frozen.overlaps(time,end)).unwrap_or(false);}
        let Some(dt)=DateTime::<Utc>::from_timestamp_millis(time)else{return false;};
        let minute=dt.hour()*60+dt.minute();
        if let Some(ranges)=self.date_overrides.get(&dt.format("%Y-%m-%d").to_string()){
            return ranges.iter().any(|(a,b)|minute>=*a && minute<*b);
        }
        self.weekly.iter().any(|s|s.weekday==dt.weekday().num_days_from_monday() && minute>=s.start_minute && minute<s.end_minute)
    }
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum GapKind { ScheduledClosed, MissingOpen, Unknown }

/// Missing candle slots, grouped into adjacent intervals of the same class.
/// Times are UTC milliseconds; end_time is exclusive.
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct GapInterval {
    pub start_time:i64,
    pub end_time:i64,
    pub intervals:usize,
    pub kind:GapKind,
}

#[derive(Debug,Clone,Default,Serialize,Deserialize)]
pub struct QualityReport {
    pub errors:Vec<String>,
    pub bars:usize,
    pub incomplete_bars:usize,
    pub missing_open_intervals:usize,
    pub scheduled_closed_intervals:usize,
    pub unclassified_intervals:usize,
    pub irregular_steps:usize,
    pub calendar:Option<String>,
    #[serde(default)]
    pub calendar_status:String,
    #[serde(default)]
    pub calendar_snapshot_id:Option<String>,
    #[serde(default)]
    pub gaps:Vec<GapInterval>,
    #[serde(default)]
    pub gap_details_truncated:bool,
}

impl QualityReport {
    fn record_gap(&mut self,start:i64,end:i64,kind:GapKind){
        if let Some(last)=self.gaps.last_mut(){
            if last.kind==kind && last.end_time==start {
                last.end_time=end;last.intervals+=1;return;
            }
        }
        // Counts above remain complete even when detailed output is bounded.
        if self.gaps.len()<200 {self.gaps.push(GapInterval{start_time:start,end_time:end,intervals:1,kind});}
        else {self.gap_details_truncated=true;}
    }
}

/// Open-time candles end after the declared interval; monthly candles use the
/// actual next calendar-month boundary, not an approximation of thirty days.
pub fn candle_end(time:i64,tf:Timeframe)->Option<i64>{
    if tf==Timeframe::MN1 {
        return tf.bucket_bounds(time).map(|(_,end)|end);
    }
    time.checked_add(tf.minutes()?.checked_mul(60_000)?)
}

pub fn audit(bars:&[Bar],tf:Timeframe,as_of_ms:i64,calendar:Option<&MarketCalendar>)->QualityReport{
    let mut report=QualityReport{bars:bars.len(),calendar:calendar.map(|c|c.name.clone()),..Default::default()};
    report.calendar_status=match calendar {
        None=>"unknown",
        Some(c)=>match &c.frozen {None=>"legacy_declared_utc",Some(f) if f.classification_ready() && f.profile.evidence==crate::calendar::EvidenceStatus::MarketAssumption=>"assumed_market_calendar",Some(f) if f.classification_ready()=>"documented_binding",Some(_)=>"unconfirmed_binding"},
    }.into();
    report.calendar_snapshot_id=calendar.and_then(|c|c.frozen.as_ref()).map(|f|f.identity());
    if let Some(c)=calendar{if let Err(e)=c.validate(){report.errors.push(e);return report;}}
    if tf==Timeframe::Tick{report.errors.push("OHLC candles require a candle timeframe".into());return report;}
    if bars.is_empty(){report.errors.push("dataset contains no candles".into());return report;}
    let mut gap_checks=0usize;
    for (i,b) in bars.iter().enumerate(){
        if tf==Timeframe::MN1 && tf.bucket_bounds(b.time).is_none_or(|(start,_)|start!=b.time){
            report.errors.push(format!("monthly candle at row {} must start at the UTC calendar-month boundary",i+1));
        }
        if [b.open,b.high,b.low,b.close].iter().any(|p|!p.is_finite() || *p<=0.0)
            || !b.volume.is_finite() || b.volume<0.0 || b.low>b.open.min(b.close) || b.high<b.open.max(b.close) || b.low>b.high {
            report.errors.push(format!("invalid OHLCV at row {}",i+1));
        }
        if candle_end(b.time,tf).is_none_or(|end|end>as_of_ms){report.incomplete_bars+=1;}
        if let Some(calendar)=calendar {
            match candle_end(b.time,tf).and_then(|end|calendar.overlaps_session(b.time,end)) {
                Some(false)=>report.errors.push(format!("candle at row {} is outside declared trading sessions",i+1)),
                None if calendar.frozen.is_some()=>{}, // Outside evidence or unconfirmed binding is unknown, not closed.
                None=>report.errors.push(format!("calendar interval unavailable at row {}",i+1)),
                Some(true)=>{},
            }
        }
        if i==0{continue;}
        if b.time<=bars[i-1].time{report.errors.push(format!("duplicate or reversed timestamp at row {}",i+1));continue;}
        let mut next=candle_end(bars[i-1].time,tf);
        if next.is_some_and(|end|end>b.time){
            report.irregular_steps+=1;
            report.errors.push(format!("overlapping candle at row {}: opens before the previous candle closes",i+1));
            continue;
        }
        while let Some(t)=next.filter(|t|*t<b.time){
            gap_checks+=1;
            if gap_checks>1_000_000{report.errors.push("gap audit exceeds one million intervals; inspect timeframe/range".into());return report;}
            let kind=match calendar {
                Some(c)=>match candle_end(t,tf).and_then(|end|c.overlaps_session(t,end)) {
                    Some(true)=>{report.missing_open_intervals+=1;GapKind::MissingOpen},
                    Some(false)=>{report.scheduled_closed_intervals+=1;GapKind::ScheduledClosed},
                    None if c.frozen.is_some()=>{report.unclassified_intervals+=1;GapKind::Unknown},
                    None=>{report.errors.push("calendar gap interval unavailable".into());return report;}
                },
                None=>{report.unclassified_intervals+=1;GapKind::Unknown},
            };
            next=candle_end(t,tf);
            if let Some(end)=next{report.record_gap(t,end,kind);}
        }
        if next!=Some(b.time){report.irregular_steps+=1;}
    }
    if report.incomplete_bars>0{report.errors.push(format!("{} candles are not closed at the specified cutoff",report.incomplete_bars));}
    report
}
pub fn now_ms()->i64 {std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d|d.as_millis() as i64).unwrap_or(0)}

#[cfg(test)]
mod tests {
    use super::*;
    fn bar(time:i64)->Bar{Bar::new(time,1.0,1.1,0.9,1.0,1.0)}
    fn ms(text:&str)->i64{DateTime::parse_from_rfc3339(text).unwrap().timestamp_millis()}
    #[test]
    fn overlapping_candles_are_invalid_even_if_all_timestamps_increase(){
        let overlapping=audit(&[bar(0),bar(60000)],Timeframe::M5,600000,None);
        assert!(overlapping.errors.iter().any(|e|e.contains("overlap")),"{:?}",overlapping);
        assert_eq!(overlapping.missing_open_intervals,0);
        let adjacent=audit(&[bar(0),bar(300000)],Timeframe::M5,600000,None);
        assert!(adjacent.errors.is_empty());assert_eq!(adjacent.irregular_steps,0);
        let gap=audit(&[bar(0),bar(600000)],Timeframe::M5,900000,None);
        assert!(gap.errors.is_empty());assert_eq!(gap.unclassified_intervals,1);
    }

    #[test]
    fn complete_boundary_and_calendar_month_are_exact(){
        assert_eq!(audit(&[bar(0)],Timeframe::M1,59999,None).incomplete_bars,1);
        assert!(audit(&[bar(0)],Timeframe::M1,60000,None).errors.is_empty());
        assert_eq!(candle_end(ms("2024-02-01T00:00:00Z"),Timeframe::MN1),Some(ms("2024-03-01T00:00:00Z")));
    }
    #[test]
    fn monthly_audit_rejects_partial_open_times_and_counts_calendar_gaps(){
        let feb=ms("2024-02-01T00:00:00Z");let march=ms("2024-03-01T00:00:00Z");
        let april=ms("2024-04-01T00:00:00Z");let may=ms("2024-05-01T00:00:00Z");
        let complete=audit(&[bar(feb),bar(march)],Timeframe::MN1,april,None);
        assert!(complete.errors.is_empty());assert_eq!(complete.unclassified_intervals,0);assert_eq!(complete.irregular_steps,0);
        let missing=audit(&[bar(feb),bar(april)],Timeframe::MN1,may,None);
        assert_eq!(missing.unclassified_intervals,1);assert_eq!(missing.irregular_steps,0);
        let partial=audit(&[bar(ms("2024-02-15T00:00:00Z"))],Timeframe::MN1,march,None);
        assert!(partial.errors.iter().any(|error|error.contains("calendar-month boundary")));
        assert_eq!(candle_end(feb,Timeframe::MN1),Timeframe::MN1.bucket_bounds(feb).map(|(_,end)|end));
    }
    #[test]
    fn distinguishes_market_closures_from_missing_open_data(){
        let calendar=MarketCalendar{name:"UTC weekdays".into(),weekly:(0..5).map(|weekday|WeeklySession{weekday,start_minute:0,end_minute:1440}).collect(),date_overrides:BTreeMap::new(),frozen:None};
        let bars=[bar(ms("2024-01-05T00:00:00Z")),bar(ms("2024-01-09T00:00:00Z"))];
        let q=audit(&bars,Timeframe::D1,ms("2024-01-10T00:00:00Z"),Some(&calendar));
        assert_eq!(q.scheduled_closed_intervals,2);assert_eq!(q.missing_open_intervals,1);
        assert_eq!(q.gaps.len(),2);
        assert_eq!((q.gaps[0].start_time,q.gaps[0].end_time,q.gaps[0].intervals,q.gaps[0].kind),
            (ms("2024-01-06T00:00:00Z"),ms("2024-01-08T00:00:00Z"),2,GapKind::ScheduledClosed));
        assert_eq!((q.gaps[1].start_time,q.gaps[1].end_time,q.gaps[1].intervals,q.gaps[1].kind),
            (ms("2024-01-08T00:00:00Z"),ms("2024-01-09T00:00:00Z"),1,GapKind::MissingOpen));
        let unknown=audit(&bars,Timeframe::D1,i64::MAX,None);assert_eq!(unknown.unclassified_intervals,3);
        assert_eq!(unknown.gaps.len(),1);assert_eq!(unknown.gaps[0].kind,GapKind::Unknown);
        let mut holiday=calendar;holiday.date_overrides.insert("2024-01-08".into(),vec![]);
        assert_eq!(audit(&bars,Timeframe::D1,i64::MAX,Some(&holiday)).scheduled_closed_intervals,3);
    }
    #[test]
    fn daily_calendar_checks_the_entire_candle_interval(){
        let calendar=MarketCalendar{name:"UTC day session".into(),weekly:(0..5).map(|weekday|WeeklySession{weekday,start_minute:9*60,end_minute:17*60}).collect(),date_overrides:BTreeMap::new(),frozen:None};
        let bars=[bar(ms("2024-01-08T00:00:00Z")),bar(ms("2024-01-10T00:00:00Z"))];
        let report=audit(&bars,Timeframe::D1,ms("2024-01-11T00:00:00Z"),Some(&calendar));
        assert!(report.errors.is_empty(),"{:?}",report.errors);
        assert_eq!(report.missing_open_intervals,1,"Tuesday includes a trading session even though midnight is closed");
        assert_eq!(report.scheduled_closed_intervals,0);
        let weekend=audit(&[bar(ms("2024-01-13T00:00:00Z"))],Timeframe::D1,ms("2024-01-14T00:00:00Z"),Some(&calendar));
        assert!(weekend.errors.iter().any(|e|e.contains("outside declared trading sessions")));
    }

    #[test]
    fn session_interval_boundaries_and_holiday_overrides_are_exact(){
        let mut calendar=MarketCalendar{name:"Monday UTC".into(),weekly:vec![WeeklySession{weekday:0,start_minute:540,end_minute:1020}],date_overrides:BTreeMap::new(),frozen:None};
        for (time,valid) in [("2024-01-08T08:59:00Z",false),("2024-01-08T09:00:00Z",true),("2024-01-08T16:59:00Z",true),("2024-01-08T17:00:00Z",false)] {
            let report=audit(&[bar(ms(time))],Timeframe::M1,ms("2024-01-09T00:00:00Z"),Some(&calendar));
            assert_eq!(report.errors.is_empty(),valid,"{time}: {:?}",report.errors);
        }
        calendar.date_overrides.insert("2024-01-08".into(),vec![]);
        let holiday=audit(&[bar(ms("2024-01-08T00:00:00Z"))],Timeframe::D1,ms("2024-01-09T00:00:00Z"),Some(&calendar));
        assert!(holiday.errors.iter().any(|e|e.contains("outside declared trading sessions")));
        // A monthly label can itself fall on a closed day, yet contain Mondays.
        let month=audit(&[bar(ms("2024-06-01T00:00:00Z"))],Timeframe::MN1,ms("2024-07-01T00:00:00Z"),Some(&calendar));
        assert!(month.errors.is_empty(),"{:?}",month.errors);
    }

    #[test]
    fn duplicate_nan_and_bad_calendar_are_explicit_errors(){
        let mut bars=[bar(0),bar(0)];bars[1].high=f64::NAN;
        assert_eq!(audit(&bars,Timeframe::M1,i64::MAX,None).errors.len(),2);
        let calendar=MarketCalendar{name:"invalid".into(),weekly:vec![WeeklySession{weekday:8,start_minute:0,end_minute:1440}],date_overrides:BTreeMap::new(),frozen:None};
        assert!(calendar.validate().is_err());
    }
    #[test]
    fn csv_never_silently_skips_or_zero_fills_bad_records(){
        assert!(crate::read_csv_bars_str("1,1,2\n").is_err());
        assert!(crate::read_csv_bars_str("1,1,2,0.5,1,unknown\n").is_err());
        assert!(crate::read_csv_bars_str("1,1,2,0.5,1,0\n1,1,2,0.5,1,0\n").is_err());
        assert!(crate::read_csv_bars_str("9223372036854775807,1,2,0.5,1,0\n").is_err());
    }
    #[test]
    fn detailed_gap_limit_does_not_hide_total_missing_candles(){
        let bars:Vec<_>=(0..=205).map(|n|bar(n*120000)).collect();
        let report=audit(&bars,Timeframe::M1,i64::MAX,None);
        assert_eq!(report.unclassified_intervals,205);
        assert_eq!(report.gaps.len(),200);
        assert!(report.gap_details_truncated);
        assert_eq!(report.gaps[0].start_time,60000);
        assert_eq!(report.gaps[199].end_time,24000000);
    }
}
