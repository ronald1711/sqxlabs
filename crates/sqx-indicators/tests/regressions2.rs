//! Regression tests for the second batch of indicator corrections.
//!
//! Expected values were produced by an independent implementation written from
//! the published definitions (Ehlers' Center of Gravity, the standard DPO,
//! M. H. Pee's Trend Intensity Index, linear-interpolated percentile), then checked
//! against this crate element by element. Editing the indicator code until these
//! tests pass is the intended use; editing the constants is not.

use sqx_indicators as ind;

fn approx(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn assert_series(got: &[f64], want: &[f64], tol: f64, label: &str) {
    assert_eq!(got.len(), want.len(), "{label}: length mismatch");
    for i in 0..want.len() {
        if want[i].is_nan() {
            assert!(got[i].is_nan(), "{label}[{i}]: expected NaN, got {}", got[i]);
        } else {
            assert!(
                approx(got[i], want[i], tol),
                "{label}[{i}]: expected {}, got {}",
                want[i],
                got[i]
            );
        }
    }
}

// ---------------------------------------------------------- Center of gravity

#[test]
fn center_of_gravity_matches_ehlers_reference() {
    let c: Vec<f64> = (1..=10).map(|v| v as f64).collect();
    let g = ind::center_of_gravity(&c, 5);
    let want = [
        f64::NAN, f64::NAN, f64::NAN, f64::NAN,
        -2.333333, -2.5, -2.6, -2.666667, -2.714286, -2.75,
    ];
    assert_series(&g, &want, 1e-5, "center_of_gravity");
}

// ------------------------------------------------------------------------ DPO

#[test]
fn dpo_is_close_displaced_back_minus_sma() {
    let c: Vec<f64> = (1..=12).map(|v| v as f64).collect();
    let d = ind::dpo(&c, 4);
    // shift = 3; on a unit ramp the DPO is a constant -1.5.
    let want = [
        f64::NAN, f64::NAN, f64::NAN,
        -1.5, -1.5, -1.5, -1.5, -1.5, -1.5, -1.5, -1.5, -1.5,
    ];
    assert_series(&d, &want, 1e-6, "dpo");
}

// ----------------------------------------------------------------------- TII

#[test]
fn tii_is_pees_price_deviation_indicator() {
    // M. H. Pee's TII ranges 0..100: a pure uptrend reads 100, a pure
    // downtrend 0, and a flat market (zero deviation) reads 50 - never a
    // negative value.
    let up: Vec<f64> = (1..=7).map(|v| v as f64).collect();
    let dn: Vec<f64> = (1..=7).rev().map(|v| v as f64).collect();
    let flat = vec![5.0; 7];
    let t_up = ind::tii(&up, 3);
    let t_dn = ind::tii(&dn, 3);
    let t_flat = ind::tii(&flat, 3);
    // Warm-up: SMA(2*3 = 6) is defined from index 5.
    for v in &t_up[5..] { assert!(approx(*v, 100.0, 1e-6), "up-trend TII = {v}"); }
    for v in &t_dn[5..] { assert!(approx(*v, 0.0, 1e-6), "down-trend TII = {v}"); }
    for v in &t_flat[5..] { assert!(approx(*v, 50.0, 1e-6), "flat TII = {v}"); }
}

#[test]
fn tii_matches_reference_implementation() {
    let c = [0.0, 1.0, 2.0, 1.0, 2.0, 3.0, 2.0, 1.0];
    let t = ind::tii(&c, 3);
    // Pee's TII: deviation window 3, SMA period 6 (2:1), current baseline.
    // The final bar's TII is 100 * 1.333 / (1.333 + 0.833) = 61.538.
    let want = [
        f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN,
        80.0, 100.0, 61.538461538,
    ];
    assert_series(&t, &want, 1e-6, "tii");
}

// --------------------------------------------------- percentile / median / MAD

#[test]
fn median_interpolates_on_even_windows() {
    let c = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let m = ind::median(&c, 4);
    let want = [f64::NAN, f64::NAN, f64::NAN, 2.5, 3.5, 4.5];
    assert_series(&m, &want, 1e-9, "median");
}

#[test]
fn percentile_interpolates_linearly() {
    let c = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let q = ind::percentile(&c, 4, 0.25);
    let want = [f64::NAN, f64::NAN, f64::NAN, 1.75, 2.75, 3.75];
    assert_series(&q, &want, 1e-9, "percentile(0.25)");
}

#[test]
fn percentile_rejects_nan_containing_windows() {
    // An undefined value has no rank: a window containing NaN must stay NaN,
    // not fabricate a percentile from the remaining values.
    let c = [f64::NAN, 1.0, 2.0, 3.0];
    let m = ind::median(&c, 3);
    assert!(m[..3].iter().all(|v| v.is_nan()), "median warm-up = {:?}", &m[..3]);
    assert!(approx(m[3], 2.0, 1e-9), "median[3] = {}", m[3]);
}

// ----------------------------------------------------------------------- CMO

#[test]
fn cmo_is_chandes_rolling_window() {
    // Chande's original CMO sums up/down moves over the trailing n bars; it is
    // NOT the Wilder-smoothed variant TA-Lib implements.
    let c = [10.0, 11.0, 12.0, 11.0, 10.0];
    let m = ind::cmo(&c, 3);
    let want = [
        f64::NAN, f64::NAN, f64::NAN,
        33.333333333, -33.333333333,
    ];
    assert_series(&m, &want, 1e-6, "cmo");
}

// --------------------------------------------------- RSI / MFI on a flat market

#[test]
fn rsi_is_neutral_on_a_flat_market() {
    let flat = vec![100.0; 20];
    let r = ind::rsi(&flat, 14);
    for v in &r[14..] {
        assert!(approx(*v, 50.0, 1e-9), "flat RSI should be 50, got {v}");
    }
}

#[test]
fn rsi_is_still_100_for_pure_gains_and_0_for_pure_losses() {
    let up = ind::rsi(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2);
    let down = ind::rsi(&[6.0, 5.0, 4.0, 3.0, 2.0, 1.0], 2);
    for v in &up[2..] { assert!(approx(*v, 100.0, 1e-9), "pure-gain RSI = {v}"); }
    for v in &down[2..] { assert!(approx(*v, 0.0, 1e-9), "pure-loss RSI = {v}"); }
}

#[test]
fn mfi_is_neutral_on_a_flat_market() {
    let h = vec![100.0; 20];
    let l = vec![100.0; 20];
    let c = vec![100.0; 20];
    let v = vec![1000.0; 20];
    let m = ind::mfi(&h, &l, &c, &v, 14);
    for x in &m[14..] {
        assert!(approx(*x, 50.0, 1e-9), "flat MFI should be 50, got {x}");
    }
}

// ------------------------------------------------------------- Aroon tie-break

#[test]
fn aroon_reads_the_most_recent_extreme_on_a_flat_stretch() {
    // Six identical bars (the window spans n+1 bars): there is no higher high
    // or lower low than the most recent bar, so both Aroon lines are 100.
    let h = vec![5.0; 6];
    let l = vec![1.0; 6];
    let (up, down) = ind::aroon(&h, &l, 5);
    assert!(approx(up[5], 100.0, 1e-9), "Aroon up = {}", up[5]);
    assert!(approx(down[5], 100.0, 1e-9), "Aroon down = {}", down[5]);
}

#[test]
fn aroon_tracks_the_newest_high_in_a_rising_market() {
    // Strictly rising highs: the newest bar is always the highest, so Aroon up
    // must be 100 on every bar past the n+1-bar warm-up.
    let h = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let l = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    let (up, _) = ind::aroon(&h, &l, 3);
    for v in &up[3..] {
        assert!(approx(*v, 100.0, 1e-9), "rising Aroon up = {v}");
    }
}

#[test]
fn aroon_is_zero_when_the_extreme_is_n_bars_back() {
    // The high peaks at index 0 then falls: with n=3, at index 3 the highest
    // is 3 bars back, so Aroon up must be 0 (the n+1-bar window reaches it).
    let h = [9.0, 4.0, 3.0, 2.0, 1.0];
    let l = [0.0, 1.0, 2.0, 3.0, 4.0];
    let (up, _) = ind::aroon(&h, &l, 3);
    assert!(approx(up[3], 0.0, 1e-9), "Aroon up = {}", up[3]);
}

// ------------------------------------------------------------- NaN policy in
// highest/lowest: a mostly-NaN window must return NaN, not a fabricated value.

#[test]
fn highest_and_lowest_poison_a_window_containing_nan() {
    let s = vec![f64::NAN, f64::NAN, 3.0, 1.0, 5.0];
    let h = ind::highest(&s, 3);
    let l = ind::lowest(&s, 3);
    // Every window up to and including index 3 still contains a NaN (the
    // window at i=3 is [NaN, 3, 1]), so it stays NaN. Only i=4 is clean.
    assert!(h[..4].iter().all(|v| v.is_nan()), "highest warm-up = {:?}", &h[..4]);
    assert!(l[..4].iter().all(|v| v.is_nan()), "lowest warm-up = {:?}", &l[..4]);
    assert!(approx(h[4], 5.0, 1e-9), "highest[4] = {}", h[4]);
    assert!(approx(l[4], 1.0, 1e-9), "lowest[4] = {}", l[4]);
}
