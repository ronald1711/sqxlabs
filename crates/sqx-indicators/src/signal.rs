use crate::Series;

/// True where `a` crosses above `b` at the current bar.
pub fn crosses_above(a: &[f64], b: &[f64]) -> Vec<bool> {
    (0..a.len()).map(|i| {
        if i == 0 { return false; }
        !a[i].is_nan() && !b[i].is_nan() && !a[i - 1].is_nan() && !b[i - 1].is_nan()
            && a[i - 1] <= b[i - 1] && a[i] > b[i]
    }).collect()
}

/// True where `a` crosses below `b` at the current bar.
pub fn crosses_below(a: &[f64], b: &[f64]) -> Vec<bool> {
    (0..a.len()).map(|i| {
        if i == 0 { return false; }
        !a[i].is_nan() && !b[i].is_nan() && !a[i - 1].is_nan() && !b[i - 1].is_nan()
            && a[i - 1] >= b[i - 1] && a[i] < b[i]
    }).collect()
}

/// True where `a` crosses above a constant level.
pub fn crosses_above_level(a: &[f64], level: f64) -> Vec<bool> {
    (0..a.len()).map(|i| {
        if i == 0 { return false; }
        !a[i].is_nan() && !a[i - 1].is_nan() && a[i - 1] <= level && a[i] > level
    }).collect()
}

/// True where `a` crosses below a constant level.
pub fn crosses_below_level(a: &[f64], level: f64) -> Vec<bool> {
    (0..a.len()).map(|i| {
        if i == 0 { return false; }
        !a[i].is_nan() && !a[i - 1].is_nan() && a[i - 1] >= level && a[i] < level
    }).collect()
}

/// True where the value is rising over `n` bars (value[i] > value[i-n]).
pub fn is_rising(src: &[f64], n: usize) -> Vec<bool> {
    (0..src.len()).map(|i| {
        if i >= n && !src[i].is_nan() && !src[i - n].is_nan() { src[i] > src[i - n] } else { false }
    }).collect()
}

/// True where the value is falling over `n` bars.
pub fn is_falling(src: &[f64], n: usize) -> Vec<bool> {
    (0..src.len()).map(|i| {
        if i >= n && !src[i].is_nan() && !src[i - n].is_nan() { src[i] < src[i - n] } else { false }
    }).collect()
}

/// True where the value increased versus the previous bar.
pub fn changes_up(src: &[f64]) -> Vec<bool> {
    (0..src.len()).map(|i| {
        i > 0 && !src[i].is_nan() && !src[i - 1].is_nan() && src[i] > src[i - 1]
    }).collect()
}

/// True where the value decreased versus the previous bar.
pub fn changes_down(src: &[f64]) -> Vec<bool> {
    (0..src.len()).map(|i| {
        i > 0 && !src[i].is_nan() && !src[i - 1].is_nan() && src[i] < src[i - 1]
    }).collect()
}

/// True where the value is strictly above a constant level.
pub fn above(a: &[f64], level: f64) -> Vec<bool> {
    a.iter().map(|v| !v.is_nan() && *v > level).collect()
}

/// True where the value is strictly below a constant level.
pub fn below(a: &[f64], level: f64) -> Vec<bool> {
    a.iter().map(|v| !v.is_nan() && *v < level).collect()
}

/// Sum of true values in a bool series over the trailing `n` bars.
pub fn count_true(src: &[bool], n: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; src.len()];
    let mut acc = 0.0;
    for i in 0..src.len() {
        if src[i] { acc += 1.0; }
        if i >= n && src[i - n] { acc -= 1.0; }
        if i + 1 >= n { out[i] = acc; }
    }
    out
}

/// Keep this import used so the crate builds cleanly with `deny(unused)`.
#[allow(unused)]
type _SeriesAlias = Series;
