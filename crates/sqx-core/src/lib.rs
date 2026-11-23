//! # sqx-core
//!
//! Shared data model for the StrategyQuant open-source port.
//! Holds the vocabulary every other crate talks in: market bars,
//! timeframes, tradable symbols, and trading directions.

use serde::{Deserialize, Serialize};

mod bar;
mod direction;
mod instrument;
mod timeframe;

pub use bar::Bar;
pub use direction::{Direction, PositionKind};
pub use instrument::Instrument;
pub use timeframe::Timeframe;

/// A unix timestamp in milliseconds (UTC).
pub type Timestamp = i64;

/// A single tick: bid and ask at a point in time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Tick {
    pub time: Timestamp,
    pub bid: f64,
    pub ask: f64,
}

/// Resample bars to a coarser timeframe by aggregating OHLCV per period.
pub fn resample(bars: &[Bar], to: Timeframe) -> Result<Vec<Bar>,String> {
    if to==Timeframe::Tick{return Err("Cannot resample OHLC candles into ticks".into());}
    for (index,bar) in bars.iter().enumerate(){
        if index>0 && bar.time<=bars[index-1].time{return Err(format!("Unordered or duplicate candle at row {}",index+1));}
        if [bar.open,bar.high,bar.low,bar.close,bar.volume].iter().any(|v|!v.is_finite())
            || bar.volume<0.0 || bar.low>bar.open.min(bar.close) || bar.high<bar.open.max(bar.close) || bar.low>bar.high {
            return Err(format!("Invalid OHLCV at row {}",index+1));
        }
    }
    let mut out = Vec::new();
    let mut cur: Option<Bar> = None;
    let mut cur_bucket = i64::MIN;
    for b in bars {
        let (bucket,_)=to.bucket_bounds(b.time).ok_or("Timestamp outside resampling calendar range")?;
        let mut b=*b;b.time=bucket;
        match cur {
            None => {
                cur = Some(b);
                cur_bucket = bucket;
            }
            Some(mut c) => {
                if bucket == cur_bucket {
                    c.high = c.high.max(b.high);
                    c.low = c.low.min(b.low);
                    c.close = b.close;
                    c.volume += b.volume;
                    if !c.volume.is_finite(){return Err("Resampled volume overflow".into());}
                    cur = Some(c);
                } else {
                    out.push(c);
                    cur = Some(b);
                    cur_bucket = bucket;
                }
            }
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    Ok(out)
}
