use crate::{highest, lowest, ma::*, Series};

/// Relative Strength Index (Wilder smoothing).
pub fn rsi(src: &[f64], n: usize) -> Series {
    if n == 0 || src.len() <= n { return vec![f64::NAN; src.len()]; }
    let len = src.len();
    let mut out = vec![f64::NAN; len];
    // Seed with the first n changes (Wilder's initial average).
    let (mut gain_sum, mut loss_sum) = (0.0, 0.0);
    for i in 1..=n {
        let d = src[i] - src[i - 1];
        if d >= 0.0 { gain_sum += d; } else { loss_sum += -d; }
    }
    let mut avg_gain = gain_sum / n as f64;
    let mut avg_loss = loss_sum / n as f64;
    out[n] = if avg_loss == 0.0 { 100.0 } else { 100.0 - 100.0 / (1.0 + avg_gain / avg_loss) };
    for i in (n + 1)..len {
        let d = src[i] - src[i - 1];
        let (g, l) = if d >= 0.0 { (d, 0.0) } else { (0.0, -d) };
        avg_gain = (avg_gain * (n as f64 - 1.0) + g) / n as f64;
        avg_loss = (avg_loss * (n as f64 - 1.0) + l) / n as f64;
        out[i] = if avg_loss == 0.0 { 100.0 } else { 100.0 - 100.0 / (1.0 + avg_gain / avg_loss) };
    }
    out
}

/// Commodity Channel Index.
pub fn cci(bars_high: &[f64], bars_low: &[f64], bars_close: &[f64], n: usize) -> Series {
    let len = bars_close.len();
    let tp: Vec<f64> = (0..len).map(|i| (bars_high[i] + bars_low[i] + bars_close[i]) / 3.0).collect();
    let ma = sma(&tp, n);
    let mut out = vec![f64::NAN; len];
    for i in 0..len {
        if i + 1 >= n {
            let start = i + 1 - n;
            let m = ma[i];
            let dev = (start..=i).map(|j| (tp[j] - m).abs()).sum::<f64>() / n as f64;
            out[i] = if dev == 0.0 { 0.0 } else { (tp[i] - m) / (0.015 * dev) };
        }
    }
    out
}

/// Stochastic %K.
pub fn stoch_k(bars_high: &[f64], bars_low: &[f64], bars_close: &[f64], n: usize) -> Series {
    let hh = highest(bars_high, n);
    let ll = lowest(bars_low, n);
    (0..bars_close.len()).map(|i| {
        let d = hh[i] - ll[i];
        if hh[i].is_nan() { f64::NAN }
        else if d == 0.0 { 50.0 }
        else { (bars_close[i] - ll[i]) / d * 100.0 }
    }).collect()
}

/// Stochastic %D (SMA of %K).
pub fn stoch_d(k: &[f64], n: usize) -> Series {
    sma(k, n)
}

/// MACD line = EMA(fast) - EMA(slow).
pub fn macd(src: &[f64], fast: usize, slow: usize) -> Series {
    let f = ema(src, fast);
    let s = ema(src, slow);
    (0..src.len()).map(|i| f[i] - s[i]).collect()
}

/// MACD signal line = EMA of the MACD line.
pub fn macd_signal(macd_line: &[f64], sig: usize) -> Series {
    ema(macd_line, sig)
}

/// MACD histogram = MACD - signal.
pub fn macd_hist(macd_line: &[f64], signal: &[f64]) -> Series {
    (0..macd_line.len()).map(|i| macd_line[i] - signal[i]).collect()
}

/// Momentum: close - close[n].
pub fn momentum(src: &[f64], n: usize) -> Series {
    (0..src.len()).map(|i| {
        if i >= n { src[i] - src[i - n] } else { f64::NAN }
    }).collect()
}

/// Rate of change in percent: (close/close[n] - 1) * 100.
pub fn roc(src: &[f64], n: usize) -> Series {
    (0..src.len()).map(|i| {
        if i >= n && src[i - n] != 0.0 { (src[i] / src[i - n] - 1.0) * 100.0 } else { f64::NAN }
    }).collect()
}

/// Williams %R.
pub fn williams_r(bars_high: &[f64], bars_low: &[f64], bars_close: &[f64], n: usize) -> Series {
    let hh = highest(bars_high, n);
    let ll = lowest(bars_low, n);
    (0..bars_close.len()).map(|i| {
        let d = hh[i] - ll[i];
        if hh[i].is_nan() { f64::NAN }
        else if d == 0.0 { -50.0 }
        else { -(hh[i] - bars_close[i]) / d * 100.0 }
    }).collect()
}

/// Awesome Oscillator: SMA(median, 5) - SMA(median, 34).
pub fn awesome_oscillator(bars_high: &[f64], bars_low: &[f64]) -> Series {
    let median: Vec<f64> = (0..bars_high.len()).map(|i| (bars_high[i] + bars_low[i]) / 2.0).collect();
    let fast = sma(&median, 5);
    let slow = sma(&median, 34);
    (0..median.len()).map(|i| fast[i] - slow[i]).collect()
}

/// Bears Power: low - EMA(close, 13).
pub fn bears_power(bars_low: &[f64], bars_close: &[f64]) -> Series {
    let e = ema(bars_close, 13);
    (0..bars_low.len()).map(|i| bars_low[i] - e[i]).collect()
}

/// Bulls Power: high - EMA(close, 13).
pub fn bulls_power(bars_high: &[f64], bars_close: &[f64]) -> Series {
    let e = ema(bars_close, 13);
    (0..bars_high.len()).map(|i| bars_high[i] - e[i]).collect()
}

/// DeMarker oscillator.
pub fn demarker(bars_high: &[f64], bars_low: &[f64], n: usize) -> Series {
    let len = bars_high.len();
    let mut demax = vec![0.0; len];
    let mut demin = vec![0.0; len];
    for i in 1..len {
        if bars_high[i] > bars_high[i - 1] { demax[i] = bars_high[i] - bars_high[i - 1]; }
        if bars_low[i] < bars_low[i - 1] { demin[i] = bars_low[i - 1] - bars_low[i]; }
    }
    let ma_max = sma(&demax, n);
    let ma_min = sma(&demin, n);
    (0..len).map(|i| {
        let d = ma_max[i] + ma_min[i];
        if ma_max[i].is_nan() { f64::NAN }
        else if d == 0.0 { 0.5 }
        else { ma_max[i] / d }
    }).collect()
}

/// True strength indicator: exponential RSI (alias for Wilder RSI).
pub fn osma(macd_line: &[f64], signal: &[f64]) -> Series {
    (0..macd_line.len()).map(|i| macd_line[i] - signal[i]).collect()
}
