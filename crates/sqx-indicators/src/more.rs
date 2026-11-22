//! A further batch of standard indicators.

use crate::{highest, lowest, ma::*, roc, sum, Series};

pub fn trix(src: &[f64], n: usize) -> Series {
    let e1 = ema(src, n);
    let e2 = ema(&e1, n);
    let e3 = ema(&e2, n);
    let mut out = vec![f64::NAN; src.len()];
    for i in 1..src.len() {
        if !e3[i].is_nan() && !e3[i - 1].is_nan() && e3[i - 1] != 0.0 {
            out[i] = (e3[i] - e3[i - 1]) / e3[i - 1] * 100.0;
        }
    }
    out
}

pub fn dpo(src: &[f64], n: usize) -> Series {
    let ma = sma(src, n);
    let shift = n / 2 + 1;
    let len = src.len();
    let mut out = vec![f64::NAN; len];
    for i in shift..len {
        out[i] = src[i] - ma[i - shift];
    }
    out
}

pub fn ad_line(h: &[f64], l: &[f64], c: &[f64], v: &[f64]) -> Series {
    let len = h.len();
    let mut acc = 0.0;
    let mut out = vec![f64::NAN; len];
    for i in 0..len {
        let r = h[i] - l[i];
        let mfv = if r != 0.0 { ((c[i] - l[i]) - (h[i] - c[i])) / r * v[i] } else { 0.0 };
        acc += mfv;
        out[i] = acc;
    }
    out
}

pub fn chaikin_osc(h: &[f64], l: &[f64], c: &[f64], v: &[f64]) -> Series {
    let ad = ad_line(h, l, c, v);
    let e3 = ema(&ad, 3);
    let e10 = ema(&ad, 10);
    (0..ad.len()).map(|i| e3[i] - e10[i]).collect()
}

pub fn cmf(h: &[f64], l: &[f64], c: &[f64], v: &[f64], n: usize) -> Series {
    let len = h.len();
    let mut mfv = vec![0.0; len];
    for i in 0..len {
        let r = h[i] - l[i];
        mfv[i] = if r != 0.0 { ((c[i] - l[i]) - (h[i] - c[i])) / r * v[i] } else { 0.0 };
    }
    let sm = sum(&mfv, n);
    let sv = sum(v, n);
    (0..len)
        .map(|i| {
            if sv[i].is_nan() || sv[i] == 0.0 {
                f64::NAN
            } else {
                sm[i] / sv[i]
            }
        })
        .collect()
}

pub fn force_index(c: &[f64], v: &[f64], n: usize) -> Series {
    let len = c.len();
    let mut raw = vec![0.0; len];
    for i in 1..len {
        raw[i] = (c[i] - c[i - 1]) * v[i];
    }
    ema(&raw, n)
}

pub fn mass_index(h: &[f64], l: &[f64]) -> Series {
    let len = h.len();
    let range: Vec<f64> = (0..len).map(|i| h[i] - l[i]).collect();
    let e9 = ema(&range, 9);
    let e9e9 = ema(&e9, 9);
    let ratio: Vec<f64> = (0..len)
        .map(|i| {
            if e9e9[i].is_nan() || e9e9[i] == 0.0 {
                f64::NAN
            } else {
                e9[i] / e9e9[i]
            }
        })
        .collect();
    sum(&ratio, 25)
}

pub fn ultimate_osc(h: &[f64], l: &[f64], c: &[f64], n1: usize, n2: usize, n3: usize) -> Series {
    let len = h.len();
    let mut bp = vec![0.0; len];
    let mut tr = vec![0.0; len];
    for i in 1..len {
        bp[i] = c[i] - l[i - 1].min(c[i - 1]);
        tr[i] = h[i].max(c[i - 1]) - l[i].min(c[i - 1]);
    }
    let s1 = sum(&bp, n1);
    let t1 = sum(&tr, n1);
    let s2 = sum(&bp, n2);
    let t2 = sum(&tr, n2);
    let s3 = sum(&bp, n3);
    let t3 = sum(&tr, n3);
    (0..len)
        .map(|i| {
            if t1[i].is_nan() || t1[i] == 0.0 || t2[i] == 0.0 || t3[i] == 0.0 {
                f64::NAN
            } else {
                100.0 * (4.0 * s1[i] / t1[i] + 2.0 * s2[i] / t2[i] + s3[i] / t3[i]) / 7.0
            }
        })
        .collect()
}

pub fn cmo(src: &[f64], n: usize) -> Series {
    let len = src.len();
    let mut out = vec![f64::NAN; len];
    for i in n..len {
        let (mut up, mut dn) = (0.0, 0.0);
        for j in (i - n + 1)..=i {
            let d = src[j] - src[j - 1];
            if d >= 0.0 {
                up += d;
            } else {
                dn += -d;
            }
        }
        out[i] = if up + dn == 0.0 { 0.0 } else { 100.0 * (up - dn) / (up + dn) };
    }
    out
}

pub fn tsi(src: &[f64], long: usize, short: usize) -> Series {
    let len = src.len();
    let mut mom = vec![0.0; len];
    for i in 1..len {
        mom[i] = src[i] - src[i - 1];
    }
    let abs: Vec<f64> = mom.iter().map(|v| v.abs()).collect();
    let e1 = ema(&mom, short);
    let e2 = ema(&e1, long);
    let a1 = ema(&abs, short);
    let a2 = ema(&a1, long);
    (0..len)
        .map(|i| if a2[i].is_nan() || a2[i] == 0.0 { f64::NAN } else { 100.0 * e2[i] / a2[i] })
        .collect()
}

pub fn rvi(h: &[f64], l: &[f64], c: &[f64], o: &[f64], n: usize) -> Series {
    let len = h.len();
    let num: Vec<f64> = (0..len).map(|i| c[i] - o[i]).collect();
    let den: Vec<f64> = (0..len).map(|i| h[i] - l[i]).collect();
    let sn = sma(&num, n);
    let sd = sma(&den, n);
    (0..len)
        .map(|i| if sd[i].is_nan() || sd[i] == 0.0 { f64::NAN } else { sn[i] / sd[i] })
        .collect()
}

pub fn coppock(src: &[f64]) -> Series {
    let r1 = roc(src, 14);
    let r2 = roc(src, 11);
    let s: Vec<f64> = (0..src.len()).map(|i| r1[i] + r2[i]).collect();
    lwma(&s, 10)
}

pub fn accel_osc(h: &[f64], l: &[f64]) -> Series {
    let ao = crate::awesome_oscillator(h, l);
    let s = sma(&ao, 5);
    (0..ao.len()).map(|i| ao[i] - s[i]).collect()
}

pub fn heikin_ashi_close(h: &[f64], l: &[f64], c: &[f64], o: &[f64]) -> Series {
    (0..h.len()).map(|i| (o[i] + h[i] + l[i] + c[i]) / 4.0).collect()
}

pub fn pivot_p(h: &[f64], l: &[f64], c: &[f64]) -> Series {
    (0..h.len()).map(|i| (h[i] + l[i] + c[i]) / 3.0).collect()
}

pub fn ulcer_index(src: &[f64], n: usize) -> Series {
    let len = src.len();
    let mut out = vec![f64::NAN; len];
    for i in 0..len {
        if i + 1 >= n {
            let start = i + 1 - n;
            let mut sum_sq = 0.0;
            for j in start..=i {
                let peak = src[start..=j].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let dd = if peak != 0.0 { (src[j] - peak) / peak * 100.0 } else { 0.0 };
                sum_sq += dd * dd;
            }
            out[i] = (sum_sq / n as f64).sqrt();
        }
    }
    out
}

pub fn r_squared(src: &[f64], n: usize) -> Series {
    let len = src.len();
    let mut out = vec![f64::NAN; len];
    for i in 0..len {
        if i + 1 >= n {
            let ys = &src[i + 1 - n..=i];
            let xm = (n - 1) as f64 / 2.0;
            let ym = ys.iter().sum::<f64>() / n as f64;
            let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
            for (k, v) in ys.iter().enumerate() {
                let dx = k as f64 - xm;
                sxy += dx * (v - ym);
                sxx += dx * dx;
                syy += (v - ym) * (v - ym);
            }
            out[i] = if sxx == 0.0 || syy == 0.0 { 0.0 } else { (sxy * sxy) / (sxx * syy) };
        }
    }
    out
}

pub fn percentile(src: &[f64], n: usize, p: f64) -> Series {
    let len = src.len();
    let mut out = vec![f64::NAN; len];
    for i in 0..len {
        if i + 1 >= n {
            let mut w: Vec<f64> = src[i + 1 - n..=i].to_vec();
            w.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let idx = (((w.len() - 1) as f64) * p).round() as usize;
            out[i] = w[idx];
        }
    }
    out
}

pub fn median(src: &[f64], n: usize) -> Series {
    percentile(src, n, 0.5)
}

pub fn skewness(src: &[f64], n: usize) -> Series {
    let len = src.len();
    let mut out = vec![f64::NAN; len];
    for i in 0..len {
        if i + 1 >= n {
            let w = &src[i + 1 - n..=i];
            let m = w.iter().sum::<f64>() / n as f64;
            let sd = (w.iter().map(|v| (v - m).powi(2)).sum::<f64>() / n as f64).sqrt();
            out[i] = if sd == 0.0 {
                0.0
            } else {
                w.iter().map(|v| ((v - m) / sd).powi(3)).sum::<f64>() / n as f64
            };
        }
    }
    out
}

pub fn bop(o: &[f64], h: &[f64], l: &[f64], c: &[f64]) -> Series {
    (0..h.len())
        .map(|i| {
            let r = h[i] - l[i];
            if r != 0.0 { (c[i] - o[i]) / r } else { 0.0 }
        })
        .collect()
}

pub fn bias(c: &[f64], n: usize) -> Series {
    let m = sma(c, n);
    (0..c.len())
        .map(|i| if m[i].is_nan() || m[i] == 0.0 { f64::NAN } else { (c[i] - m[i]) / m[i] * 100.0 })
        .collect()
}

pub fn disparity(c: &[f64], n: usize) -> Series {
    let m = sma(c, n);
    (0..c.len())
        .map(|i| if m[i].is_nan() || m[i] == 0.0 { f64::NAN } else { c[i] / m[i] * 100.0 })
        .collect()
}

pub fn ppo(c: &[f64], fast: usize, slow: usize) -> Series {
    let f = ema(c, fast);
    let s = ema(c, slow);
    (0..c.len())
        .map(|i| if s[i].is_nan() || s[i] == 0.0 { f64::NAN } else { (f[i] - s[i]) / s[i] * 100.0 })
        .collect()
}

pub fn bb_percent_b(c: &[f64], n: usize, mult: f64) -> Series {
    let (u, _m, l) = crate::bollinger(c, n, mult);
    (0..c.len())
        .map(|i| {
            let d = u[i] - l[i];
            if d.is_nan() || d == 0.0 { f64::NAN } else { (c[i] - l[i]) / d }
        })
        .collect()
}

pub fn bb_bandwidth(c: &[f64], n: usize, mult: f64) -> Series {
    let (u, m, l) = crate::bollinger(c, n, mult);
    (0..c.len())
        .map(|i| if m[i].is_nan() || m[i] == 0.0 { f64::NAN } else { (u[i] - l[i]) / m[i] * 100.0 })
        .collect()
}

pub fn zscore(c: &[f64], n: usize) -> Series {
    let m = sma(c, n);
    let sd = crate::stddev(c, n);
    (0..c.len())
        .map(|i| if sd[i].is_nan() || sd[i] == 0.0 { f64::NAN } else { (c[i] - m[i]) / sd[i] })
        .collect()
}

pub fn chaikin_volatility(h: &[f64], l: &[f64], n: usize) -> Series {
    let range: Vec<f64> = (0..h.len()).map(|i| h[i] - l[i]).collect();
    let e = ema(&range, n);
    let mut out = vec![f64::NAN; h.len()];
    for i in n..h.len() {
        if !e[i - n].is_nan() && e[i - n] != 0.0 {
            out[i] = (e[i] - e[i - n]) / e[i - n] * 100.0;
        }
    }
    out
}

pub fn psychological_line(c: &[f64], n: usize) -> Series {
    let len = c.len();
    let mut out = vec![f64::NAN; len];
    for i in n..len {
        let mut up = 0;
        for j in (i - n + 1)..=i {
            if c[j] > c[j - 1] {
                up += 1;
            }
        }
        out[i] = up as f64 / n as f64 * 100.0;
    }
    out
}

pub fn center_of_gravity(c: &[f64], n: usize) -> Series {
    let len = c.len();
    let mut out = vec![f64::NAN; len];
    for i in n.saturating_sub(1)..len {
        let start = i + 1 - n;
        let mut num = 0.0;
        let mut den = 0.0;
        for (k, j) in (start..=i).enumerate() {
            num += (k as f64 + 1.0) * c[j];
            den += c[j];
        }
        out[i] = if den != 0.0 { -num / den } else { f64::NAN };
    }
    out
}

pub fn klinger(h: &[f64], l: &[f64], c: &[f64], v: &[f64], fast: usize, slow: usize) -> Series {
    let len = h.len();
    let mut vf = vec![0.0; len];
    let mut cm = 0.0;
    let mut prev_trend = 0.0;
    for i in 1..len {
        let hlc = h[i] + l[i] + c[i];
        let phlc = h[i - 1] + l[i - 1] + c[i - 1];
        let trend = if hlc > phlc { 1.0 } else if hlc < phlc { -1.0 } else { 0.0 };
        let dm = (h[i] - l[i]).abs();
        let pdm = (h[i - 1] - l[i - 1]).abs();
        if i == 1 {
            cm = dm;
        } else if trend == prev_trend {
            cm += dm;
        } else {
            cm = pdm + dm;
        }
        prev_trend = trend;
        let r = if cm != 0.0 { (2.0 * dm / cm - 1.0).abs() } else { 0.0 };
        vf[i] = v[i] * r * trend * 100.0;
    }
    let f = ema(&vf, fast);
    let s = ema(&vf, slow);
    (0..len).map(|i| f[i] - s[i]).collect()
}

pub fn kst(c: &[f64]) -> Series {
    let r1 = roc(c, 10);
    let r2 = roc(c, 15);
    let r3 = roc(c, 20);
    let r4 = roc(c, 30);
    let s1 = sma(&r1, 10);
    let s2 = sma(&r2, 10);
    let s3 = sma(&r3, 10);
    let s4 = sma(&r4, 15);
    (0..c.len())
        .map(|i| s1[i] + 2.0 * s2[i] + 3.0 * s3[i] + 4.0 * s4[i])
        .collect()
}

pub fn rmi(c: &[f64], n: usize) -> Series {
    let len = c.len();
    let mut mom = vec![0.0; len];
    for i in n..len {
        mom[i] = c[i] - c[i - n];
    }
    let mut out = vec![f64::NAN; len];
    for i in (2 * n)..len {
        let (mut up, mut dn) = (0.0, 0.0);
        for j in (i - n + 1)..=i {
            let d = mom[j] - mom[j - 1];
            if d > 0.0 {
                up += d;
            } else {
                dn += -d;
            }
        }
        out[i] = if up + dn == 0.0 { 50.0 } else { up / (up + dn) * 100.0 };
    }
    out
}

pub fn tii(c: &[f64], n: usize) -> Series {
    let len = c.len();
    let mut out = vec![f64::NAN; len];
    for i in n..len {
        let (mut up, mut dn) = (0.0, 0.0);
        for j in (i - n + 1)..=i {
            let d = c[j] - c[j - 1];
            if d > 0.0 {
                up += d;
            } else {
                dn += -d;
            }
        }
        out[i] = if up + dn == 0.0 { 50.0 } else { up / (up + dn) * 100.0 };
    }
    out
}

pub fn vwma(c: &[f64], v: &[f64], n: usize) -> Series {
    let len = c.len();
    let mut out = vec![f64::NAN; len];
    for i in n.saturating_sub(1)..len {
        let start = i + 1 - n;
        let mut num = 0.0;
        let mut den = 0.0;
        for j in start..=i {
            num += c[j] * v[j];
            den += v[j];
        }
        out[i] = if den != 0.0 { num / den } else { f64::NAN };
    }
    out
}

pub fn pivot_r1(h: &[f64], l: &[f64], c: &[f64]) -> Series {
    let p = pivot_p(h, l, c);
    (0..h.len()).map(|i| 2.0 * p[i] - l[i]).collect()
}

pub fn pivot_s1(h: &[f64], l: &[f64], c: &[f64]) -> Series {
    let p = pivot_p(h, l, c);
    (0..h.len()).map(|i| 2.0 * p[i] - h[i]).collect()
}

pub fn linreg_slope(c: &[f64], n: usize) -> Series {
    let len = c.len();
    let mut out = vec![f64::NAN; len];
    for i in 0..len {
        if i + 1 >= n {
            let ys = &c[i + 1 - n..=i];
            let xm = (n - 1) as f64 / 2.0;
            let ym = ys.iter().sum::<f64>() / n as f64;
            let (mut sxy, mut sxx) = (0.0, 0.0);
            for (k, v) in ys.iter().enumerate() {
                let dx = k as f64 - xm;
                sxy += dx * (v - ym);
                sxx += dx * dx;
            }
            out[i] = if sxx != 0.0 { sxy / sxx } else { 0.0 };
        }
    }
    out
}

pub fn price_channel_upper(h: &[f64], n: usize) -> Series {
    highest(h, n)
}

pub fn price_channel_lower(l: &[f64], n: usize) -> Series {
    lowest(l, n)
}

pub fn slope(c: &[f64], n: usize) -> Series {
    let s = sma(c, n);
    let mut out = vec![f64::NAN; c.len()];
    for i in n..c.len() {
        if !s[i].is_nan() && !s[i - n].is_nan() {
            out[i] = (s[i] - s[i - n]) / n as f64;
        }
    }
    out
}

pub fn sma_envelope_upper(c: &[f64], n: usize, pct: f64) -> Series {
    let m = sma(c, n);
    (0..c.len()).map(|i| m[i] * (1.0 + pct / 100.0)).collect()
}

pub fn sma_envelope_lower(c: &[f64], n: usize, pct: f64) -> Series {
    let m = sma(c, n);
    (0..c.len()).map(|i| m[i] * (1.0 - pct / 100.0)).collect()
}

pub fn atr_bands_upper(h: &[f64], l: &[f64], c: &[f64], n: usize, mult: f64) -> Series {
    let m = sma(c, n);
    let a = rma(&tr_of(h, l, c), n);
    (0..c.len()).map(|i| m[i] + mult * a[i]).collect()
}

pub fn atr_bands_lower(h: &[f64], l: &[f64], c: &[f64], n: usize, mult: f64) -> Series {
    let m = sma(c, n);
    let a = rma(&tr_of(h, l, c), n);
    (0..c.len()).map(|i| m[i] - mult * a[i]).collect()
}

pub fn pivot_r2(h: &[f64], l: &[f64], c: &[f64]) -> Series {
    let p = pivot_p(h, l, c);
    (0..h.len()).map(|i| p[i] + (h[i] - l[i])).collect()
}

pub fn pivot_s2(h: &[f64], l: &[f64], c: &[f64]) -> Series {
    let p = pivot_p(h, l, c);
    (0..h.len()).map(|i| p[i] - (h[i] - l[i])).collect()
}

pub fn chande_forecast(c: &[f64], n: usize) -> Series {
    let lr = crate::linreg(c, n).0;
    (0..c.len())
        .map(|i| if c[i] != 0.0 && !lr[i].is_nan() { (c[i] - lr[i]) / c[i] * 100.0 } else { f64::NAN })
        .collect()
}

pub fn laguerre_rsi(c: &[f64], gamma: f64) -> Series {
    let len = c.len();
    let mut out = vec![f64::NAN; len];
    let (mut l0, mut l1, mut l2, mut l3) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..len {
        let p = c[i];
        let l0n = (1.0 - gamma) * p + gamma * l0;
        let l1n = -gamma * l0n + l0 + gamma * l1;
        let l2n = -gamma * l1n + l1 + gamma * l2;
        let l3n = -gamma * l2n + l2 + gamma * l3;
        l0 = l0n;
        l1 = l1n;
        l2 = l2n;
        l3 = l3n;
        let cu = (if l0 >= l1 { l0 - l1 } else { 0.0 })
            + (if l1 >= l2 { l1 - l2 } else { 0.0 })
            + (if l2 >= l3 { l2 - l3 } else { 0.0 });
        let cd = (if l0 < l1 { l1 - l0 } else { 0.0 })
            + (if l1 < l2 { l2 - l1 } else { 0.0 })
            + (if l2 < l3 { l3 - l2 } else { 0.0 });
        out[i] = if cu + cd == 0.0 { 0.0 } else { cu / (cu + cd) };
    }
    out
}

pub fn up_down_volume(c: &[f64], v: &[f64], n: usize) -> Series {
    let len = c.len();
    let mut out = vec![f64::NAN; len];
    for i in n..len {
        let mut up = 0.0;
        let mut total = 0.0;
        for j in (i - n + 1)..=i {
            total += v[j];
            if c[j] > c[j - 1] {
                up += v[j];
            }
        }
        out[i] = if total != 0.0 { up / total * 100.0 } else { f64::NAN };
    }
    out
}

pub fn fisher(h: &[f64], l: &[f64], n: usize) -> Series {
    let len = h.len();
    let hh = highest(h, n);
    let ll = lowest(l, n);
    let mut out = vec![f64::NAN; len];
    let mut prev_v = 0.0f64;
    let mut prev_f = 0.0f64;
    for i in 0..len {
        if hh[i].is_nan() || ll[i].is_nan() {
            continue;
        }
        let d = hh[i] - ll[i];
        let mut v = if d != 0.0 {
            2.0 * (((h[i] + l[i]) / 2.0 - ll[i]) / d - 0.5)
        } else {
            0.0
        };
        v = 0.33 * v + 0.67 * prev_v;
        prev_v = v;
        let vc = v.clamp(-0.999, 0.999);
        let f = 0.5 * ((1.0 + vc) / (1.0 - vc)).ln() + 0.5 * prev_f;
        prev_f = f;
        out[i] = f;
    }
    out
}

pub fn smi(h: &[f64], l: &[f64], c: &[f64], n: usize, smooth: usize) -> Series {
    let len = h.len();
    let hh = highest(h, n);
    let ll = lowest(l, n);
    let mut raw = vec![0.0; len];
    for i in 0..len {
        if hh[i].is_nan() || ll[i].is_nan() {
            continue;
        }
        let mid = (hh[i] + ll[i]) / 2.0;
        let d = (hh[i] - ll[i]) / 2.0;
        raw[i] = if d != 0.0 { (c[i] - mid) / d * 100.0 } else { 0.0 };
    }
    let s1 = ema(&raw, smooth);
    ema(&s1, smooth)
}

fn shift_series(s: &[f64], n: usize) -> Series {
    let mut out = vec![f64::NAN; s.len()];
    for i in n..s.len() {
        out[i] = s[i - n];
    }
    out
}

pub fn alligator_jaw(c: &[f64]) -> Series {
    shift_series(&rma(c, 13), 8)
}

pub fn alligator_teeth(c: &[f64]) -> Series {
    shift_series(&rma(c, 8), 5)
}

pub fn alligator_lips(c: &[f64]) -> Series {
    shift_series(&rma(c, 5), 3)
}

pub fn gator_upper(c: &[f64]) -> Series {
    let j = alligator_jaw(c);
    let t = alligator_teeth(c);
    (0..c.len()).map(|i| (j[i] - t[i]).abs()).collect()
}

pub fn gator_lower(c: &[f64]) -> Series {
    let t = alligator_teeth(c);
    let l = alligator_lips(c);
    (0..c.len()).map(|i| (t[i] - l[i]).abs()).collect()
}

pub fn nvi(c: &[f64], v: &[f64]) -> Series {
    let len = c.len();
    let mut acc = 1000.0;
    let mut out = vec![f64::NAN; len];
    if len == 0 {
        return out;
    }
    out[0] = acc;
    for i in 1..len {
        if v[i] < v[i - 1] && c[i - 1] != 0.0 {
            acc += acc * (c[i] - c[i - 1]) / c[i - 1];
        }
        out[i] = acc;
    }
    out
}

pub fn pvi(c: &[f64], v: &[f64]) -> Series {
    let len = c.len();
    let mut acc = 1000.0;
    let mut out = vec![f64::NAN; len];
    if len == 0 {
        return out;
    }
    out[0] = acc;
    for i in 1..len {
        if v[i] > v[i - 1] && c[i - 1] != 0.0 {
            acc += acc * (c[i] - c[i - 1]) / c[i - 1];
        }
        out[i] = acc;
    }
    out
}

pub fn eom(h: &[f64], l: &[f64], v: &[f64], n: usize) -> Series {
    let len = h.len();
    let mut raw = vec![0.0; len];
    for i in 1..len {
        let dm = ((h[i] + l[i]) / 2.0) - ((h[i - 1] + l[i - 1]) / 2.0);
        let br = (v[i] / 1e6).max(1e-9) / (h[i] - l[i]).max(1e-9);
        raw[i] = dm / br;
    }
    sma(&raw, n)
}

pub fn vhf(c: &[f64], n: usize) -> Series {
    let len = c.len();
    let hh = highest(c, n);
    let ll = lowest(c, n);
    let mut out = vec![f64::NAN; len];
    for i in n..len {
        let mut sum = 0.0;
        for j in (i - n + 1)..=i {
            sum += (c[j] - c[j - 1]).abs();
        }
        out[i] = if sum != 0.0 { (hh[i] - ll[i]) / sum } else { 0.0 };
    }
    out
}

fn tr_of(h: &[f64], l: &[f64], c: &[f64]) -> Series {
    let len = h.len();
    let mut out = vec![f64::NAN; len];
    for i in 0..len {
        if i == 0 {
            out[i] = h[i] - l[i];
            continue;
        }
        let pc = c[i - 1];
        out[i] = (h[i] - l[i]).max((h[i] - pc).abs()).max((l[i] - pc).abs());
    }
    out
}

pub fn chandelier_long(h: &[f64], l: &[f64], c: &[f64], n: usize, mult: f64) -> Series {
    let hh = highest(h, n);
    let a = rma(&tr_of(h, l, c), n);
    (0..c.len()).map(|i| hh[i] - mult * a[i]).collect()
}

pub fn chandelier_short(h: &[f64], l: &[f64], c: &[f64], n: usize, mult: f64) -> Series {
    let ll = lowest(l, n);
    let a = rma(&tr_of(h, l, c), n);
    (0..c.len()).map(|i| ll[i] + mult * a[i]).collect()
}

pub fn twiggs_money_flow(h: &[f64], l: &[f64], c: &[f64], v: &[f64], n: usize) -> Series {
    let len = h.len();
    let mut ad = vec![0.0; len];
    for i in 0..len {
        let r = h[i] - l[i];
        ad[i] = if r != 0.0 { ((c[i] - l[i]) - (h[i] - c[i])) / r * v[i] } else { 0.0 };
    }
    let mut out = vec![f64::NAN; len];
    for i in n.saturating_sub(1)..len {
        let start = i + 1 - n;
        let mut num = 0.0;
        let mut den = 0.0;
        for j in start..=i {
            num += ad[j];
            den += v[j];
        }
        out[i] = if den != 0.0 { num / den } else { f64::NAN };
    }
    out
}

pub fn volume_osc(v: &[f64], fast: usize, slow: usize) -> Series {
    let f = sma(v, fast);
    let s = sma(v, slow);
    (0..v.len())
        .map(|i| if s[i].is_nan() || s[i] == 0.0 { f64::NAN } else { (f[i] - s[i]) / s[i] * 100.0 })
        .collect()
}

pub fn pvt(c: &[f64], v: &[f64]) -> Series {
    let len = c.len();
    let mut acc = 0.0;
    let mut out = vec![f64::NAN; len];
    if len == 0 {
        return out;
    }
    out[0] = 0.0;
    for i in 1..len {
        if c[i - 1] != 0.0 {
            acc += (c[i] - c[i - 1]) / c[i - 1] * v[i];
        }
        out[i] = acc;
    }
    out
}
