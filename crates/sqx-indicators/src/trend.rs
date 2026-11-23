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
    // Skip the degenerate index-0 TR/DM values when seeding Wilder's average.
    let atr = rma_skip_first(&tr, n);
    let sm_p = rma_skip_first(&plus_dm, n);
    let sm_m = rma_skip_first(&minus_dm, n);
    let mut plus_di = vec![f64::NAN; len];
    let mut minus_di = vec![f64::NAN; len];
    let mut dx = vec![f64::NAN; len];
    for i in 0..len {
        if atr[i].is_nan() {
            continue;
        }
        if atr[i] > 0.0 {
            plus_di[i] = 100.0 * sm_p[i] / atr[i];
            minus_di[i] = 100.0 * sm_m[i] / atr[i];
            let d = plus_di[i] + minus_di[i];
            dx[i] = if d == 0.0 { 0.0 } else { (plus_di[i] - minus_di[i]).abs() / d * 100.0 };
        } else {
            // Zero true range means the bar did not move at all (a flat,
            // halted or limit-locked session). There is no directional
            // information, so report neutral DIs and a zero DX rather than
            // dividing by zero and poisoning every later ADX with NaN.
            plus_di[i] = 0.0;
            minus_di[i] = 0.0;
            dx[i] = 0.0;
        }
    }
    let adx = rma(&dx, n);
    (plus_di, minus_di, adx)
}

/// Aroon up/down.
///
/// Follows Chande's original (and TA-Lib's) convention: the lookback window
/// spans `n + 1` bars (`n` periods back plus today), and Aroon is
/// `100 * (n - periods_since_extreme) / n`, ranging 0..100 with 100 when the
/// extreme is on the current bar and 0 when it is `n` bars back.
pub fn aroon(bars_high: &[f64], bars_low: &[f64], n: usize) -> (Series, Series) {
    let len = bars_high.len();
    let mut up = vec![f64::NAN; len];
    let mut down = vec![f64::NAN; len];
    for i in 0..len {
        if i >= n {
            let start = i - n;
            let (mut hi, mut lo) = (f64::NEG_INFINITY, f64::INFINITY);
            let (mut hi_idx, mut lo_idx) = (0usize, 0usize);
            // `>=` / `<=` keep the MOST RECENT occurrence of the extreme.
            for (j, v) in bars_high[start..=i].iter().enumerate() {
                if *v >= hi { hi = *v; hi_idx = j; }
            }
            for (j, v) in bars_low[start..=i].iter().enumerate() {
                if *v <= lo { lo = *v; lo_idx = j; }
            }
            // Relative index: 0 = n bars ago, n = today.
            up[i] = hi_idx as f64 / n as f64 * 100.0;
            down[i] = lo_idx as f64 / n as f64 * 100.0;
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
///
/// Follows Wilder's original rules. The two non-obvious parts are:
///
/// * **The clamp.** The computed SAR may never enter the range of the previous
///   two bars; without it the stop can sit *inside* price and can then never be
///   hit, which silently turns the indicator into a hold-forever signal.
/// * **The seed.** The initial direction is taken from the first two bars
///   rather than being hardcoded long. Index 0 is NAN: a single bar carries no
///   direction, so PSAR has a one-bar warm-up like every other indicator here.
pub fn parabolic_sar(bars: &[Bar], step: f64, max_step: f64) -> Series {
    let len = bars.len();
    let mut out = vec![f64::NAN; len];
    if len == 0 {
        return out;
    }
    let mut up = len < 2 || bars[1].close >= bars[0].close;
    let mut af = step;
    let (mut sar, mut ep) = if up {
        (bars[0].low, bars[0].high)
    } else {
        (bars[0].high, bars[0].low)
    };
    for i in 1..len {
        sar += af * (ep - sar);
        if up {
            // Never let the stop rise into the last two bars' range.
            sar = sar.min(bars[i - 1].low);
            if i >= 2 {
                sar = sar.min(bars[i - 2].low);
            }
            if bars[i].low < sar {
                up = false;
                sar = ep;
                ep = bars[i].low;
                af = step;
            } else if bars[i].high > ep {
                ep = bars[i].high;
                af = (af + step).min(max_step);
            }
        } else {
            // Mirror image: the stop may not fall into the last two bars' range.
            sar = sar.max(bars[i - 1].high);
            if i >= 2 {
                sar = sar.max(bars[i - 2].high);
            }
            if bars[i].high > sar {
                up = true;
                sar = ep;
                ep = bars[i].high;
                af = step;
            } else if bars[i].low < ep {
                ep = bars[i].low;
                af = (af + step).min(max_step);
            }
        }
        out[i] = sar;
    }
    out
}

/// SuperTrend. Returns (trend, line) where trend is +1 (up) / -1 (down).
///
/// Matches the standard formulation (the one TradingView's ta.supertrend and
/// every MetaTrader port implement). The essential rule is that each band is a
/// **ratchet**: it may only narrow toward price while price stays inside the
/// channel, and it resets to the raw band once a bar closes beyond it. Without
/// that reset the bands drift and the trend never flips - which makes the
/// indicator hold a losing position through an entire reversal.
pub fn supertrend(bars: &[Bar], n: usize, mult: f64) -> (Series, Series) {
    let len = bars.len();
    let atr = crate::atr(bars, n);
    let mut line = vec![f64::NAN; len];
    let mut trend = vec![f64::NAN; len];
    let mut upper_band = vec![f64::NAN; len];
    let mut lower_band = vec![f64::NAN; len];
    let mut prev_trend: Option<f64> = None;
    for i in 0..len {
        if atr[i].is_nan() {
            continue;
        }
        let mid = (bars[i].high + bars[i].low) / 2.0;
        let basic_upper = mid + mult * atr[i];
        let basic_lower = mid - mult * atr[i];

        let (prev_upper, prev_lower) = if i > 0 {
            (upper_band[i - 1], lower_band[i - 1])
        } else {
            (f64::NAN, f64::NAN)
        };
        let prev_close = if i > 0 { bars[i - 1].close } else { f64::NAN };

        // Keep the previous band while it is still the tighter one and price
        // has not closed beyond it; otherwise reset to the raw band.
        upper_band[i] =
            if !prev_upper.is_nan() && !(basic_upper < prev_upper || prev_close > prev_upper) {
                prev_upper
            } else {
                basic_upper
            };
        lower_band[i] =
            if !prev_lower.is_nan() && !(basic_lower > prev_lower || prev_close < prev_lower) {
                prev_lower
            } else {
                basic_lower
            };

        let t = if let Some(pt) = prev_trend {
            if pt > 0.0 {
                // Long: flip only on a close below the trailing lower band.
                if bars[i].close < lower_band[i] { -1.0 } else { 1.0 }
            } else if bars[i].close > upper_band[i] {
                1.0
            } else {
                -1.0
            }
        } else if bars[i].close >= mid {
            // First evaluable bar: seed from the side of the channel price is on.
            1.0
        } else {
            -1.0
        };

        line[i] = if t > 0.0 { lower_band[i] } else { upper_band[i] };
        trend[i] = t;
        prev_trend = Some(t);
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
