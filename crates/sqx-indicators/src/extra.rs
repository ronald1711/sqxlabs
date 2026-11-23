//! Additional indicators: Ichimoku, Vortex, Kaufman ER, Gann HiLo, Aroon
//! oscillator, Stochastic RSI, Donchian channel, OBV and MFI.

use crate::{highest, highest_prior, lowest, lowest_prior, ma::*, rsi, sum, Series};

fn mid(h: &[f64], l: &[f64], n: usize) -> Series {
    let hh = highest(h, n);
    let ll = lowest(l, n);
    (0..h.len()).map(|i| (hh[i] + ll[i]) / 2.0).collect()
}

pub fn ichimoku_tenkan(h: &[f64], l: &[f64]) -> Series {
    mid(h, l, 9)
}

pub fn ichimoku_kijun(h: &[f64], l: &[f64]) -> Series {
    mid(h, l, 26)
}

pub fn ichimoku_senkou_a(h: &[f64], l: &[f64]) -> Series {
    let t = mid(h, l, 9);
    let k = mid(h, l, 26);
    let mut out = vec![f64::NAN; h.len()];
    for i in 26..h.len() {
        out[i] = (t[i - 26] + k[i - 26]) / 2.0;
    }
    out
}

pub fn ichimoku_senkou_b(h: &[f64], l: &[f64]) -> Series {
    let b = mid(h, l, 52);
    let mut out = vec![f64::NAN; h.len()];
    for i in 26..h.len() {
        out[i] = b[i - 26];
    }
    out
}

/// Vortex indicator: returns (+VI, -VI).
pub fn vortex(h: &[f64], l: &[f64], c: &[f64], n: usize) -> (Series, Series) {
    let len = h.len();
    let mut plus_vm = vec![0.0; len];
    let mut minus_vm = vec![0.0; len];
    let mut tr = vec![0.0; len];
    tr[0] = h[0] - l[0];
    for i in 1..len {
        plus_vm[i] = (h[i] - l[i - 1]).abs();
        minus_vm[i] = (l[i] - h[i - 1]).abs();
        tr[i] = (h[i] - l[i]).max((h[i] - c[i - 1]).abs()).max((l[i] - c[i - 1]).abs());
    }
    let sp = sum(&plus_vm, n);
    let sm = sum(&minus_vm, n);
    let st = sum(&tr, n);
    let plus: Series = (0..len).map(|i| if st[i].is_nan() { f64::NAN } else { sp[i] / st[i] }).collect();
    let minus: Series = (0..len).map(|i| if st[i].is_nan() { f64::NAN } else { sm[i] / st[i] }).collect();
    (plus, minus)
}

/// Kaufman efficiency ratio.
pub fn kaufman_er(src: &[f64], n: usize) -> Series {
    let len = src.len();
    let mut out = vec![f64::NAN; len];
    for i in n..len {
        let change = (src[i] - src[i - n]).abs();
        let mut vol = 0.0;
        for j in (i - n + 1)..=i {
            vol += (src[j] - src[j - 1]).abs();
        }
        out[i] = if vol == 0.0 { 0.0 } else { change / vol };
    }
    out
}

/// Gann HiLo: high of the period while above the MA, low while below.
pub fn gann_hilo(h: &[f64], l: &[f64], c: &[f64], n: usize) -> Series {
    let ma = sma(c, n);
    let hh = highest(h, n);
    let ll = lowest(l, n);
    (0..c.len()).map(|i| if c[i] > ma[i] { hh[i] } else { ll[i] }).collect()
}

/// Gann HiLo trend: +1 above the MA, -1 below.
pub fn gann_hilo_trend(c: &[f64], n: usize) -> Series {
    let ma = sma(c, n);
    (0..c.len()).map(|i| if c[i] > ma[i] { 1.0 } else { -1.0 }).collect()
}

/// Aroon oscillator: Aroon up - Aroon down.
pub fn aroon_osc(h: &[f64], l: &[f64], n: usize) -> Series {
    let (up, down) = crate::aroon(h, l, n);
    (0..h.len()).map(|i| up[i] - down[i]).collect()
}

/// Stochastic RSI.
pub fn stoch_rsi(src: &[f64], rsi_n: usize, stoch_n: usize) -> Series {
    let r = rsi(src, rsi_n);
    let hh = highest(&r, stoch_n);
    let ll = lowest(&r, stoch_n);
    (0..src.len())
        .map(|i| {
            if !r[i].is_finite() || !hh[i].is_finite() || !ll[i].is_finite() {
                f64::NAN
            } else {
                let d = hh[i] - ll[i];
                if d == 0.0 { 50.0 } else { (r[i] - ll[i]) / d * 100.0 }
            }
        })
        .collect()
}

pub fn donchian_upper(h: &[f64], n: usize) -> Series {
    highest_prior(h, n)
}

pub fn donchian_lower(l: &[f64], n: usize) -> Series {
    lowest_prior(l, n)
}

pub fn donchian_middle(h: &[f64], l: &[f64], n: usize) -> Series {
    let u = highest_prior(h, n);
    let d = lowest_prior(l, n);
    (0..h.len()).map(|i| (u[i] + d[i]) / 2.0).collect()
}

/// On-balance volume.
pub fn obv(c: &[f64], v: &[f64]) -> Series {
    let len = c.len();
    let mut out = vec![f64::NAN; len];
    if len == 0 {
        return out;
    }
    let mut acc = v[0];
    out[0] = acc;
    for i in 1..len {
        if c[i] > c[i - 1] {
            acc += v[i];
        } else if c[i] < c[i - 1] {
            acc -= v[i];
        }
        out[i] = acc;
    }
    out
}

/// Money flow index.
pub fn mfi(h: &[f64], l: &[f64], c: &[f64], v: &[f64], n: usize) -> Series {
    let len = h.len();
    let mut pos = vec![0.0; len];
    let mut neg = vec![0.0; len];
    let tp: Vec<f64> = (0..len).map(|i| (h[i] + l[i] + c[i]) / 3.0).collect();
    for i in 1..len {
        let flow = tp[i] * v[i];
        if tp[i] > tp[i - 1] {
            pos[i] = flow;
        } else if tp[i] < tp[i - 1] {
            neg[i] = flow;
        }
    }
    let sp = sum(&pos, n);
    let sn = sum(&neg, n);
    (0..len)
        .map(|i| {
            if sp[i].is_nan() {
                f64::NAN
            } else if sn[i] == 0.0 {
                if sp[i] == 0.0 { 50.0 } else { 100.0 }
            } else {
                100.0 - 100.0 / (1.0 + sp[i] / sn[i])
            }
        })
        .collect()
}
