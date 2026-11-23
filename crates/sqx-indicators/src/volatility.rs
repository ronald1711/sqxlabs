use crate::{highest, lowest, ma::*, Series};
use sqx_core::Bar;

/// True range (single bar, requires previous close).
pub fn true_range(bars: &[Bar]) -> Series {
    let mut out = vec![f64::NAN; bars.len()];
    for i in 0..bars.len() {
        if i == 0 { out[i] = bars[i].high - bars[i].low; continue; }
        let prev_close = bars[i - 1].close;
        let tr = (bars[i].high - bars[i].low)
            .max((bars[i].high - prev_close).abs())
            .max((bars[i].low - prev_close).abs());
        out[i] = tr;
    }
    out
}

/// Average true range (Wilder smoothing).
pub fn atr(bars: &[Bar], n: usize) -> Series {
    // Wilder/TA-Lib seed the ATR over TR[1..=n], skipping the partial TR[0]
    // (high-low with no previous close).
    rma_skip_first(&true_range(bars), n)
}

/// Bollinger Bands (population stddev). Returns (upper, middle, lower).
pub fn bollinger(src: &[f64], n: usize, mult: f64) -> (Series, Series, Series) {
    let mid = sma(src, n);
    let sd = crate::stddev(src, n);
    let upper: Series = (0..src.len()).map(|i| mid[i] + mult * sd[i]).collect();
    let lower: Series = (0..src.len()).map(|i| mid[i] - mult * sd[i]).collect();
    (upper, mid, lower)
}

/// Keltner Channel. Returns (upper, middle, lower).
pub fn keltner(bars: &[Bar], n: usize, mult: f64) -> (Series, Series, Series) {
    let closes = crate::closes(bars);
    let mid = ema(&closes, n);
    let atr = atr(bars, n);
    let upper: Series = (0..closes.len()).map(|i| mid[i] + mult * atr[i]).collect();
    let lower: Series = (0..closes.len()).map(|i| mid[i] - mult * atr[i]).collect();
    (upper, mid, lower)
}

/// Rolling high/low over `n` bars. Returns (highest, lowest).
pub fn hl_channel(bars_high: &[f64], bars_low: &[f64], n: usize) -> (Series, Series) {
    (highest(bars_high, n), lowest(bars_low, n))
}
