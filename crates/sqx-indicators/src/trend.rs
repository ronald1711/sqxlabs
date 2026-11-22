use crate::{ma::*, Series};
use sqx_core::Bar;

/// Directional movement system. Returns (+DI, -DI, ADX).
pub fn adx(bars: &[Bar], n: usize) -> (Series, Series, Series) {
    let len = bars.len();
    let mut plus_dm = vec![0.0; len];
    let mut minus_dm = vec![0.0; len];
    let tr = crate::true_range(bars);
    for i in 1..len {
        let up = bars[i].high - bars[i - 1].high;
        let dn = bars[i - 1].low - bars[i].low;
        if up > dn && up > 0.0 { plus_dm[i] = up; }
        if dn > up && dn > 0.0 { minus_dm[i] = dn; }
    }
    let atr = rma(&tr, n);
    let sm_p = rma(&plus_dm, n);
    let sm_m = rma(&minus_dm, n);
    let mut plus_di = vec![f64::NAN; len];
    let mut minus_di = vec![f64::NAN; len];
    let mut dx = vec![f64::NAN; len];
    for i in 0..len {
        if !atr[i].is_nan() {
            plus_di[i] = 100.0 * sm_p[i] / atr[i];
            minus_di[i] = 100.0 * sm_m[i] / atr[i];
            let d = plus_di[i] + minus_di[i];
            dx[i] = if d == 0.0 { 0.0 } else { (plus_di[i] - minus_di[i]).abs() / d * 100.0 };
        }
    }
    let adx = rma(&dx, n);
    (plus_di, minus_di, adx)
}

/// Aroon up/down.
pub fn aroon(bars_high: &[f64], bars_low: &[f64], n: usize) -> (Series, Series) {
    let len = bars_high.len();
    let mut up = vec![f64::NAN; len];
    let mut down = vec![f64::NAN; len];
    for i in 0..len {
        if i + 1 >= n {
            let start = i + 1 - n;
            let (mut hi, mut lo) = (f64::NEG_INFINITY, f64::INFINITY);
            let (mut hi_idx, mut lo_idx) = (0usize, 0usize);
            for (j, v) in bars_high[start..=i].iter().enumerate() {
                if *v > hi { hi = *v; hi_idx = j; }
            }
            for (j, v) in bars_low[start..=i].iter().enumerate() {
                if *v < lo { lo = *v; lo_idx = j; }
            }
            up[i] = (hi_idx + 1) as f64 / n as f64 * 100.0;
            down[i] = (lo_idx + 1) as f64 / n as f64 * 100.0;
        }
    }
    (up, down)
}

/// Linear regression slope and intercept over the trailing `n` values.
/// Returns (slope, intercept) series.
pub fn linreg(src: &[f64], n: usize) -> (Series, Series) {
    let len = src.len();
    let mut slope = vec![f64::NAN; len];
    let mut intercept = vec![f64::NAN; len];
    let x_mean = (n as f64 - 1.0) / 2.0;
    let x_var: f64 = (0..n).map(|x| (x as f64 - x_mean).powi(2)).sum();
    for i in 0..len {
        if i + 1 >= n {
            let start = i + 1 - n;
            let y_mean = src[start..=i].iter().sum::<f64>() / n as f64;
            let mut cov = 0.0;
            for (x, v) in src[start..=i].iter().enumerate() {
                cov += (x as f64 - x_mean) * (v - y_mean);
            }
            let s = cov / x_var;
            slope[i] = s;
            intercept[i] = y_mean - s * x_mean;
        }
    }
    (slope, intercept)
}

/// Parabolic SAR.
pub fn parabolic_sar(bars: &[Bar], step: f64, max_step: f64) -> Series {
    let len = bars.len();
    let mut out = vec![f64::NAN; len];
    if len == 0 { return out; }
    let mut af = step;
    let mut up = true;
    let mut sar = bars[0].low;
    let mut ep = bars[0].high;
    out[0] = sar;
    for i in 1..len {
        sar = sar + af * (ep - sar);
        if up {
            if bars[i].low < sar {
                up = false;
                sar = ep;
                ep = bars[i].low;
                af = step;
            } else {
                if bars[i].high > ep { ep = bars[i].high; af = (af + step).min(max_step); }
            }
        } else if bars[i].high > sar {
            up = true;
            sar = ep;
            ep = bars[i].high;
            af = step;
        } else if bars[i].low < ep {
            ep = bars[i].low;
            af = (af + step).min(max_step);
        }
        out[i] = sar;
    }
    out
}

/// SuperTrend. Returns (trend, line) where trend is +1 (up) / -1 (down).
pub fn supertrend(bars: &[Bar], n: usize, mult: f64) -> (Series, Series) {
    let len = bars.len();
    let atr = crate::atr(bars, n);
    let mut line = vec![f64::NAN; len];
    let mut trend = vec![f64::NAN; len];
    let mut upper_band = vec![f64::NAN; len];
    let mut lower_band = vec![f64::NAN; len];
    let mut prev_line = f64::NAN;
    let mut prev_trend = 1.0;
    for i in 0..len {
        if atr[i].is_nan() { continue; }
        let mid = (bars[i].high + bars[i].low) / 2.0;
        let basic_upper = mid + mult * atr[i];
        let basic_lower = mid - mult * atr[i];
        upper_band[i] = if i > 0 && !upper_band[i - 1].is_nan() && basic_upper < upper_band[i - 1] && bars[i - 1].close > upper_band[i - 1] {
            basic_upper
        } else if i > 0 && !upper_band[i - 1].is_nan() && bars[i - 1].close > upper_band[i - 1] {
            upper_band[i - 1].max(basic_upper)
        } else {
            basic_upper
        };
        lower_band[i] = if i > 0 && !lower_band[i - 1].is_nan() && basic_lower > lower_band[i - 1] && bars[i - 1].close < lower_band[i - 1] {
            basic_lower
        } else if i > 0 && !lower_band[i - 1].is_nan() && bars[i - 1].close < lower_band[i - 1] {
            lower_band[i - 1].min(basic_lower)
        } else {
            basic_lower
        };
        let t = if !prev_line.is_nan() && bars[i].close > prev_line { 1.0 }
        else if !prev_line.is_nan() && bars[i].close < prev_line { -1.0 }
        else { prev_trend };
        line[i] = if t == 1.0 { lower_band[i] } else { upper_band[i] };
        trend[i] = t;
        prev_line = line[i];
        prev_trend = t;
    }
    (trend, line)
}

/// Volume-weighted average price (cumulative over the whole series).
pub fn vwap(bars: &[Bar]) -> Series {
    let mut out = vec![f64::NAN; bars.len()];
    let (mut pv, mut vol) = (0.0, 0.0);
    for i in 0..bars.len() {
        let t = bars[i].typical();
        pv += t * bars[i].volume;
        vol += bars[i].volume;
        if vol != 0.0 { out[i] = pv / vol; }
    }
    out
}
