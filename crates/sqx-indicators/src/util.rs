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
            let window = &src[start..=i];
            // Poison on NaN so an upstream indicator's warm-up passes through
            // instead of being skipped (NaN compares less than everything, which
            // made highest() return a garbage value from a mostly-NaN window).
            if window.iter().any(|v| v.is_nan()) {
                continue;
            }
            let mut m = f64::NEG_INFINITY;
            for v in window {
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
            let window = &src[start..=i];
            if window.iter().any(|v| v.is_nan()) {
                continue;
            }
            let mut m = f64::INFINITY;
            for v in window {
                if *v < m { m = *v; }
            }
            out[i] = m;
        }
    }
    out
}

/// Rolling maximum over the *previous* `n` values (excluding the current bar).
/// Leading `n` values are `NAN`. Used for Donchian-style breakout bands, where
/// the current bar must be able to exceed the channel to signal a breakout.
pub fn highest_prior(src: &[f64], n: usize) -> Vec<f64> {
    if n == 0 {
        return vec![f64::NAN; src.len()];
    }
    let mut out = vec![f64::NAN; src.len()];
    for i in 0..src.len() {
        if i >= n {
            let start = i - n;
            let window = &src[start..i];
            if window.iter().any(|v| v.is_nan()) {
                continue;
            }
            let mut m = f64::NEG_INFINITY;
            for v in window {
                if *v > m { m = *v; }
            }
            out[i] = m;
        }
    }
    out
}

/// Rolling minimum over the *previous* `n` values (excluding the current bar).
/// Leading `n` values are `NAN`. Companion to [`highest_prior`].
pub fn lowest_prior(src: &[f64], n: usize) -> Vec<f64> {
    if n == 0 {
        return vec![f64::NAN; src.len()];
    }
    let mut out = vec![f64::NAN; src.len()];
    for i in 0..src.len() {
        if i >= n {
            let start = i - n;
            let window = &src[start..i];
            if window.iter().any(|v| v.is_nan()) {
                continue;
            }
            let mut m = f64::INFINITY;
            for v in window {
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
    for i in 0..src.len() {
        if i + 1 >= n {
            let start = i + 1 - n;
            let window = &src[start..=i];
            if window.iter().any(|v| v.is_nan()) {
                continue;
            }
            out[i] = window.iter().sum();
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
