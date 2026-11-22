use sqx_indicators as ind;

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn assert_series(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len());
    for (g, wv) in got.iter().zip(want) {
        if wv.is_nan() {
            assert!(g.is_nan(), "expected NaN, got {g}");
        } else {
            assert!(approx(*g, *wv), "expected {wv}, got {g}");
        }
    }
}

#[test]
fn sma_reference() {
    assert_series(&ind::sma(&[1.0, 2.0, 3.0, 4.0, 5.0], 3), &[f64::NAN, f64::NAN, 2.0, 3.0, 4.0]);
}

#[test]
fn ema_reference() {
    // k = 0.5, seeded with SMA of first 3 = 2.0
    assert_series(&ind::ema(&[1.0, 2.0, 3.0, 4.0, 5.0], 3), &[f64::NAN, f64::NAN, 2.0, 3.0, 4.0]);
}

#[test]
fn rsi_all_gains_is_100() {
    let r = ind::rsi(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2);
    assert!(r[..2].iter().all(|v| v.is_nan()));
    for v in &r[2..] {
        assert!(approx(*v, 100.0), "expected 100, got {v}");
    }
}

#[test]
fn rsi_all_losses_is_0() {
    let r = ind::rsi(&[6.0, 5.0, 4.0, 3.0, 2.0, 1.0], 2);
    for v in &r[2..] {
        assert!(approx(*v, 0.0), "expected 0, got {v}");
    }
}

#[test]
fn highest_lowest() {
    let h = ind::highest(&[3.0, 1.0, 5.0, 2.0, 4.0], 3);
    assert_series(&h, &[f64::NAN, f64::NAN, 5.0, 5.0, 5.0]);
    let l = ind::lowest(&[3.0, 1.0, 5.0, 2.0, 4.0], 3);
    assert_series(&l, &[f64::NAN, f64::NAN, 1.0, 1.0, 2.0]);
}

#[test]
fn momentum_reference() {
    let m = ind::momentum(&[1.0, 2.0, 3.0, 5.0], 2);
    assert_series(&m, &[f64::NAN, f64::NAN, 2.0, 3.0]);
}

#[test]
fn crosses_above_reference() {
    let a = vec![1.0, 2.0, 3.0, 2.0, 4.0];
    let b = vec![2.0, 2.0, 2.0, 2.0, 2.0];
    let c = ind::crosses_above(&a, &b);
    assert_eq!(c, vec![false, false, true, false, true]);
}
