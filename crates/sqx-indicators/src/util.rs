/// Rolling maximum over the trailing `n` values (including the current).
/// Leading `n-1` values are `NAN`.
pub fn highest(src: &[f64], n: usize) -> Vec<f64> {
    if n == 0 {
        return vec![f64::NAN; src.len()];
    }
    let mut out = vec![f64::NAN; src.len()];
    for i in 0..src.len() {
        if i + 1 >= n {
            let start = i + 1 - n;
            let mut m = f64::NEG_INFINITY;
            for v in &src[start..=i] {
                if *v > m { m = *v; }
            }
            out[i] = m;
        }
    }
    out
}

/// Rolling minimum over the trailing `n` values (including the current).
pub fn lowest(src: &[f64], n: usize) -> Vec<f64> {
    if n == 0 {
        return vec![f64::NAN; src.len()];
    }
    let mut out = vec![f64::NAN; src.len()];
    for i in 0..src.len() {
        if i + 1 >= n {
            let start = i + 1 - n;
            let mut m = f64::INFINITY;
            for v in &src[start..=i] {
                if *v < m { m = *v; }
            }
            out[i] = m;
        }
    }
    out
}

/// Rolling sum over the trailing `n` values.
pub fn sum(src: &[f64], n: usize) -> Vec<f64> {
    if n == 0 {
        return vec![f64::NAN; src.len()];
    }
    let mut out = vec![f64::NAN; src.len()];
    let mut acc = 0.0;
    for i in 0..src.len() {
        acc += src[i];
        if i >= n {
            acc -= src[i - n];
        }
        if i + 1 >= n {
            out[i] = acc;
        }
    }
    out
}

/// Population standard deviation over the trailing `n` values.
pub fn stddev(src: &[f64], n: usize) -> Vec<f64> {
    if n < 2 {
        return vec![f64::NAN; src.len()];
    }
    let mut out = vec![f64::NAN; src.len()];
    for i in 0..src.len() {
        if i + 1 >= n {
            let start = i + 1 - n;
            let window = &src[start..=i];
            let mean = window.iter().sum::<f64>() / n as f64;
            let var = window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
            out[i] = var.sqrt();
        }
    }
    out
}

/// Shift a series right by `shift` bars (fills the head with `NAN`).
pub fn shift(src: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; src.len()];
    for i in n..src.len() {
        out[i] = src[i - n];
    }
    out
}
