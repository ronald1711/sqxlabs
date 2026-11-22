//! Cross-checks every registered indicator building block against synthetic
//! bars: each one must (a) be registered, (b) be aligned with the input, and
//! (c) produce finite values once past its warm-up window. This catches the
//! class of bug where a NaN during warm-up poisons the whole series (e.g. ADX).

use sqx_core::Bar;

/// Deterministic, non-flat OHLCV series: a rising drift plus two sines plus
/// pseudo-random noise, with strictly positive volume and high > low.
fn make_bars(n: usize) -> Vec<Bar> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state >> 33) as f64 / (1u64 << 31) as f64 // [0, 1)
    };
    let mut bars = Vec::with_capacity(n);
    let mut close = 100.0;
    for i in 0..n {
        let t = i as f64;
        let open = close;
        let wave = (t / 12.0).sin() * 2.0 + (t / 5.0).sin() * 0.8 + (t / 2.0).cos() * 0.3;
        close = 100.0 + 0.015 * t + wave * 3.0 + (next() - 0.5) * 1.2;
        let high = open.max(close) + 0.4 + next() * 0.5;
        let low = open.min(close) - 0.4 - next() * 0.5;
        let volume = 500.0 + next() * 4000.0 + (i as f64 % 17.0) * 20.0;
        bars.push(Bar::new((i as i64) * 3600, open, high, low, close, volume));
    }
    bars
}

#[test]
fn every_catalog_indicator_is_registered_and_finite_after_warmup() {
    let bars = make_bars(400);
    let catalog = sqx_strategy::indicator_catalog();
    let tail_start = bars.len() - bars.len() / 4; // last 25%
    assert!(catalog.len() > 100, "catalog unexpectedly small: {}", catalog.len());

    let mut failures: Vec<String> = Vec::new();
    for spec in catalog {
        let params = spec.default_params();
        let series = match sqx_backtest::compute(spec.name, &params, &bars) {
            Some(s) => s,
            None => {
                failures.push(format!("{}: not registered (compute returned None)", spec.name));
                continue;
            }
        };
        assert_eq!(
            series.len(),
            bars.len(),
            "{}: series length mismatch",
            spec.name
        );
        if !series.iter().any(|v| v.is_finite()) {
            failures.push(format!("{}: all-NaN (no finite value)", spec.name));
            continue;
        }
        if let Some(bad) = series[tail_start..].iter().position(|v| !v.is_finite()) {
            failures.push(format!(
                "{}: non-finite in tail at index {}",
                spec.name,
                tail_start + bad
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} indicator(s) failed finite-after-warmup:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn adx_is_finite_and_bounded() {
    let bars = make_bars(400);
    let adx = sqx_backtest::compute("ADX", &[], &bars).expect("ADX should be registered");
    let tail = &adx[300..];
    assert!(
        tail.iter().all(|v| v.is_finite()),
        "ADX should be finite after warm-up, got NaN in tail"
    );
    assert!(
        tail.iter().all(|v| (0.0..=100.0).contains(v)),
        "ADX should be in [0,100], got values outside the range"
    );
}

#[test]
fn donchian_breakout_is_possible() {
    let bars = make_bars(400);
    let upper = sqx_backtest::compute("DonchianUpper", &[], &bars).expect("DonchianUpper registered");
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let mut breakouts = 0usize;
    for i in 20..bars.len() {
        if closes[i] > upper[i] {
            breakouts += 1;
        }
    }
    assert!(
        breakouts > 0,
        "close should exceed the (prior-bar) Donchian upper at least once"
    );
}
