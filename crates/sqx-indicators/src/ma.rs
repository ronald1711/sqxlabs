use crate::Series;

/// Simple moving average.
pub fn sma(src: &[f64], n: usize) -> Series {
    if n == 0 { return vec![f64::NAN; src.len()]; }
    let mut out = vec![f64::NAN; src.len()];
    let mut acc = 0.0;
    for i in 0..src.len() {
        acc += src[i];
        if i >= n { acc -= src[i - n]; }
        if i + 1 >= n { out[i] = acc / n as f64; }
    }
    out
}

/// Exponential moving average (seeded with an SMA of the first `n` values).
pub fn ema(src: &[f64], n: usize) -> Series {
    if n == 0 { return vec![f64::NAN; src.len()]; }
    let k = 2.0 / (n as f64 + 1.0);
    let mut out = vec![f64::NAN; src.len()];
    let mut prev: Option<f64> = None;
    for i in 0..src.len() {
        let val = match prev {
            Some(p) => src[i] * k + p * (1.0 - k),
            None => {
                if i + 1 >= n { src[..=i].iter().sum::<f64>() / n as f64 } else { continue; }
            }
        };
        out[i] = val;
        prev = Some(val);
    }
    out
}

/// Wilder's smoothed moving average (RMA), used by RSI/ATR/ADX. k = 1/n.
pub fn rma(src: &[f64], n: usize) -> Series {
    if n == 0 { return vec![f64::NAN; src.len()]; }
    let k = 1.0 / n as f64;
    let mut out = vec![f64::NAN; src.len()];
    let mut prev: Option<f64> = None;
    for i in 0..src.len() {
        let val = match prev {
            Some(p) => src[i] * k + p * (1.0 - k),
            None => {
                if i + 1 >= n { src[..=i].iter().sum::<f64>() / n as f64 } else { continue; }
            }
        };
        out[i] = val;
        prev = Some(val);
    }
    out
}

/// Smoothed moving average (Wilder RMA) — alias kept for clarity.
pub fn smma(src: &[f64], n: usize) -> Series {
    rma(src, n)
}

/// Linear-weighted moving average.
pub fn lwma(src: &[f64], n: usize) -> Series {
    if n == 0 { return vec![f64::NAN; src.len()]; }
    let denom = (n * (n + 1)) as f64 / 2.0;
    let mut out = vec![f64::NAN; src.len()];
    for i in 0..src.len() {
        if i + 1 >= n {
            let start = i + 1 - n;
            let mut acc = 0.0;
            for (j, v) in src[start..=i].iter().enumerate() {
                acc += *v * (j + 1) as f64;
            }
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
