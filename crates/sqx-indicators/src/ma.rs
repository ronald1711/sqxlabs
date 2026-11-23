use crate::Series;

/// Simple moving average.
///
/// A window that still contains a NaN is treated as "not warmed up yet", so the
/// output stays NaN until the trailing `n` values are all finite. This lets a
/// NaN-prefixed input (the warm-up of an upstream indicator) pass through
/// without poisoning every later value.
pub fn sma(src: &[f64], n: usize) -> Series {
    if n == 0 { return vec![f64::NAN; src.len()]; }
    let mut out = vec![f64::NAN; src.len()];
    for i in 0..src.len() {
        if i + 1 >= n {
            let start = i + 1 - n;
            let window = &src[start..=i];
            if window.iter().any(|v| v.is_nan()) {
                continue;
            }
            out[i] = window.iter().sum::<f64>() / n as f64;
        }
    }
    out
}

/// Exponential moving average (seeded with an SMA of the first `n` values).
///
/// NaN values during the seed window are skipped, so a NaN-prefixed input does
/// not poison the whole average (mirrors [`rma`]).
pub fn ema(src: &[f64], n: usize) -> Series {
    if n == 0 { return vec![f64::NAN; src.len()]; }
    let k = 2.0 / (n as f64 + 1.0);
    let mut out = vec![f64::NAN; src.len()];
    let mut prev: Option<f64> = None;
    let mut seed: Vec<f64> = Vec::with_capacity(n);
    let mut seeding = true;
    for i in 0..src.len() {
        let s = src[i];
        if seeding {
            if s.is_nan() {
                continue;
            }
            seed.push(s);
            if seed.len() < n {
                continue;
            }
            let init = seed.iter().sum::<f64>() / n as f64;
            out[i] = init;
            prev = Some(init);
            seeding = false;
        } else {
            let val = s * k + prev.unwrap() * (1.0 - k);
            out[i] = val;
            prev = Some(val);
        }
    }
    out
}

/// Wilder's smoothed moving average (RMA), used by RSI/ATR/ADX. k = 1/n.
///
/// Seeds with the SMA of the first `n` *finite* values. NaN values during the
/// seed window are skipped, so a NaN-prefixed input (such as ADX's DX series,
/// which is undefined until the ATR/DI warm-up completes) no longer poisons the
/// entire average with NaN.
pub fn rma(src: &[f64], n: usize) -> Series {
    if n == 0 { return vec![f64::NAN; src.len()]; }
    let k = 1.0 / n as f64;
    let mut out = vec![f64::NAN; src.len()];
    let mut prev: Option<f64> = None;
    let mut seed: Vec<f64> = Vec::with_capacity(n);
    let mut seeding = true;
    for i in 0..src.len() {
        let s = src[i];
        if seeding {
            if s.is_nan() {
                continue;
            }
            seed.push(s);
            if seed.len() < n {
                continue;
            }
            let init = seed.iter().sum::<f64>() / n as f64;
            out[i] = init;
            prev = Some(init);
            seeding = false;
        } else {
            let val = s * k + prev.unwrap() * (1.0 - k);
            out[i] = val;
            prev = Some(val);
        }
    }
    out
}

/// Smoothed moving average (Wilder RMA) — alias kept for clarity.
pub fn smma(src: &[f64], n: usize) -> Series {
    rma(src, n)
}

/// Wilder smoothing seeded with the simple average of `src[1..=n]`, skipping
/// the degenerate index-0 value.
///
/// The True Range and directional-movement series both carry an index-0 value
/// with no previous bar behind it (TR[0] is just high-low; +DM[0]/-DM[0] are 0).
/// Wilder's original ATR/ADX seed the smoothed series over the first `n` *real*
/// values, i.e. `1..=n`, and emit the first output at index `n` - which is
/// exactly what TA-Lib does. Using the generic [`rma`] here instead would shift
/// every subsequent value by one bar's worth of seed.
pub fn rma_skip_first(src: &[f64], n: usize) -> Series {
    let mut out = vec![f64::NAN; src.len()];
    if n == 0 || src.len() <= n {
        return out;
    }
    let k = 1.0 / n as f64;
    let seed: f64 = src[1..=n].iter().sum::<f64>() / n as f64;
    out[n] = seed;
    let mut prev = seed;
    for i in (n + 1)..src.len() {
        prev = src[i] * k + prev * (1.0 - k);
        out[i] = prev;
    }
    out
}

/// Linear-weighted moving average.
///
/// Like [`sma`], a window that still contains a NaN keeps the output NaN, so a
/// NaN-prefixed input does not poison later values.
pub fn lwma(src: &[f64], n: usize) -> Series {
    if n == 0 { return vec![f64::NAN; src.len()]; }
    let denom = (n * (n + 1)) as f64 / 2.0;
    let mut out = vec![f64::NAN; src.len()];
    for i in 0..src.len() {
        if i + 1 >= n {
            let start = i + 1 - n;
            let window = &src[start..=i];
            if window.iter().any(|v| v.is_nan()) {
                continue;
            }
            let acc: f64 = window.iter().enumerate().map(|(j, v)| v * (j + 1) as f64).sum();
            out[i] = acc / denom;
        }
    }
    out
}

/// Triple exponential moving average.
pub fn tema(src: &[f64], n: usize) -> Series {
    let e1 = ema(src, n);
    let e2 = ema(&e1, n);
    let e3 = ema(&e2, n);
    (0..src.len()).map(|i| 3.0 * e1[i] - 3.0 * e2[i] + e3[i]).collect()
}

/// Hull moving average.
pub fn hma(src: &[f64], n: usize) -> Series {
    let half = (n / 2).max(1);
    let sqrtn = (n as f64).sqrt() as usize;
    let wma_half = lwma(src, half);
    let wma_full = lwma(src, n);
    let diff: Vec<f64> = (0..src.len()).map(|i| 2.0 * wma_half[i] - wma_full[i]).collect();
    lwma(&diff, sqrtn.max(1))
}

/// Kaufman adaptive moving average.
pub fn kama(src: &[f64], n: usize) -> Series {
    let fast = 2.0 / 3.0;
    let slow = 2.0 / 31.0;
    let mut out = vec![f64::NAN; src.len()];
    let mut prev: Option<f64> = None;
    for i in 0..src.len() {
        if i < n {
            // Seed: simple average of the available window once it is full.
            if i + 1 == n {
                let s = src[..=i].iter().sum::<f64>() / n as f64;
                out[i] = s;
                prev = Some(s);
            }
            continue;
        }
        let change = (src[i] - src[i - n]).abs();
        let mut volatility = 0.0;
        for j in (i - n + 1)..=i {
            volatility += (src[j] - src[j - 1]).abs();
        }
        let er = if volatility == 0.0 { 0.0 } else { change / volatility };
        let sc = (er * (fast - slow) + slow).powi(2);
        let p = prev.unwrap_or(src[i]);
        let val = p + sc * (src[i] - p);
        out[i] = val;
        prev = Some(val);
    }
    out
}
