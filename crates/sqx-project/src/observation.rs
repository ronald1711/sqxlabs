//! Observation coverage includes the candle's contents, not just its opening instant.
use serde_json::Value;
use sqx_core::{Bar,Timeframe};

pub(crate) struct Range {pub start:i64,pub end:i64,pub extent_known:bool}
pub(crate) fn duration(config:&Value)->Result<Option<i64>,String>{
    serde_json::from_value(config["bar_duration_ms"].clone()).map_err(|e|format!("invalid observation duration: {e}"))
}
pub(crate) fn range(bars:&[Bar],duration:Option<i64>,timeframe:&Value)->Result<Range,String>{
    let first=bars.first().ok_or("observation range has no candles")?;
    let last=bars.last().unwrap();
    if bars.windows(2).any(|w|w[0].time>=w[1].time){return Err("observation candles are not strictly chronological".into());}
    let (end,extent_known)=end(last.time,duration,timeframe)?;
    Ok(Range{start:first.time,end,extent_known})
}
pub(crate) fn end(last_time:i64,duration:Option<i64>,timeframe:&Value)->Result<(i64,bool),String>{
    if duration.is_some_and(|d|d<=0){return Err("observation duration must be positive".into());}
    let timeframe:Option<Timeframe>=serde_json::from_value(timeframe.clone()).map_err(|e|format!("invalid observation timeframe: {e}"))?;
    let end=match timeframe {
        Some(Timeframe::Tick)=>return Err("tick timeframe cannot describe observation candles".into()),
        Some(Timeframe::MN1)=>{
            if duration.is_some(){return Err("calendar-month observation cannot use a fixed duration".into());}
            let (start,end)=Timeframe::MN1.bucket_bounds(last_time).ok_or("monthly observation boundary overflow")?;
            if start!=last_time{return Err("monthly observation must start at its declared UTC calendar boundary".into());}
            Some(end.checked_sub(1).ok_or("monthly observation boundary overflow")?)
        },
        other=>{
            let declared=other.map(|tf|tf.minutes().and_then(|m|m.checked_mul(60000)).ok_or("observation timeframe overflow")).transpose()?;
            if duration.is_some() && declared.is_some() && duration!=declared{return Err("observation duration conflicts with timeframe".into());}
            duration.or(declared).map(|width|last_time.checked_add(width-1).ok_or("observation boundary overflow")).transpose()?
        },
    };
    Ok((end.unwrap_or(last_time),end.is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn calendar_months_use_real_utc_boundaries_and_unknown_extents_stay_unknown(){
        // Independently specified UTC midnight epochs: Feb 2023, Feb 2024, Jan 2024.
        for (start,exclusive_end) in [(1675209600000,1677628800000),(1706745600000,1709251200000),(1704067200000,1706745600000)] {
            assert_eq!(end(start,None,&json!("MN1")).unwrap(),(exclusive_end-1,true));
        }
        assert!(end(1706745600000,Some(30*86400000),&json!("MN1")).is_err());
        assert!(end(1706745600001,None,&json!("MN1")).is_err());
        assert_eq!(end(3600000,Some(3600000),&json!("H1")).unwrap(),(7199999,true));
        assert_eq!(end(3600000,None,&Value::Null).unwrap(),(3600000,false));
        assert!(end(0,Some(60000),&json!("H1")).is_err());
        assert!(end(i64::MAX,Some(2),&Value::Null).is_err());
        assert!(end(0,Some(0),&Value::Null).is_err());
    }
}
