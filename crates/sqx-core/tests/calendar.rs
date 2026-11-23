use sqx_core::{Timeframe,Bar,resample};
fn time(date:&str)->i64{chrono::DateTime::parse_from_rfc3339(date).unwrap().timestamp_millis()}
#[test]
fn month_bounds_follow_calendar_and_leap_year(){
    for (inside,start,end) in [
        ("2024-02-29T12:00:00Z","2024-02-01T00:00:00Z","2024-03-01T00:00:00Z"),
        ("2023-02-28T12:00:00Z","2023-02-01T00:00:00Z","2023-03-01T00:00:00Z"),
        ("2024-12-31T23:59:00Z","2024-12-01T00:00:00Z","2025-01-01T00:00:00Z")
    ] {assert_eq!(Timeframe::MN1.bucket_bounds(time(inside)),Some((time(start),time(end))));}
}
#[test]
fn weeks_start_on_monday_and_negative_epoch_minutes_work(){
    let expected=Some((time("2024-02-26T00:00:00Z"),time("2024-03-04T00:00:00Z")));
    assert_eq!(Timeframe::W1.bucket_bounds(time("2024-03-03T23:59:00Z")),expected);
    assert_eq!(Timeframe::M1.bucket_bounds(-1),Some((-60000,0)));
    assert_eq!(Timeframe::Tick.bucket_bounds(0),None);
}
#[test]
fn resampling_uses_bucket_open_time_and_real_month_boundaries(){
    let bars=[Bar::new(time("2024-02-02T00:00:00Z"),10.0,12.0,9.0,11.0,2.0),Bar::new(time("2024-02-29T00:00:00Z"),11.0,14.0,8.0,13.0,3.0),Bar::new(time("2024-03-01T00:00:00Z"),13.0,15.0,12.0,14.0,4.0)];
    let result=resample(&bars,Timeframe::MN1).unwrap();assert_eq!(result.len(),2);
    assert_eq!(result[0],Bar::new(time("2024-02-01T00:00:00Z"),10.0,14.0,8.0,13.0,5.0));
    assert_eq!(result[1].time,time("2024-03-01T00:00:00Z"));
}
#[test]
fn resampling_never_silently_drops_or_combines_invalid_rows(){
    let bar=Bar::new(0,1.0,2.0,0.5,1.5,1.0);
    assert!(resample(&[bar,bar],Timeframe::M5).is_err());
    assert!(resample(&[bar],Timeframe::Tick).is_err());
    let mut malformed=bar;malformed.high=f64::NAN;
    assert!(resample(&[malformed],Timeframe::M5).is_err());
    let mut outside=bar;outside.time=i64::MAX;
    assert!(resample(&[outside],Timeframe::MN1).is_err());
    let mut first=bar;first.volume=f64::MAX;
    let mut second=first;second.time=60000;
    assert!(resample(&[first,second],Timeframe::M5).is_err());
}
