//! A further batch of standard indicators.

use crate::{ma::*, roc, sum, Series};

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
