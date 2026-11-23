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

// ---------------------------------------------------------------------------
// Degenerate inputs.
//
// Real market data contains flat bars (halted sessions, limit-locked days,
// illiquid minutes) and zero-volume bars. These used to be untested, which is
// how a divide-by-zero in ADX survived a test file written specifically to
// catch NaN poisoning.
// ---------------------------------------------------------------------------

/// Indicators that are *legitimately* undefined on each degenerate input: each
/// one reduces to a 0/0 form with no meaningful neutral value (a Hurst exponent
/// of a constant series, a percent-B of a zero-width band, a volume-weighted
/// average of zero volume). Returning NaN there is correct.
///
/// Anything NOT in this list must stay finite. If a new indicator starts
/// returning NaN on degenerate data it will appear as a failure here, which is
/// the signal to decide deliberately whether that is correct.
const FLAT_UNDEFINED: &[&str] = &[
    "Acceleration",      // ROC of an all-zero series: percent change from 0
    "BBPercentB",        // upper == lower, zero-width band
    "ChaikinVolatility", // EMA of a zero range
    "Choppiness",        // sum(TR) == 0
    "FractalDimension",  // 2 - Hurst, undefined for a constant series
    "Hurst",             // standard deviation == 0
    "Jerk",              // ROC of an all-zero series
    "KeltnerPosition",   // ATR == 0
    "MassIndex",         // ratio of two zero EMAs
    "RVI",               // SMA(H-L) == 0
    "RangeExpansion",    // SMA(H-L) == 0
    "TSI",               // EMA(|momentum|) == 0
    "UltimateOsc",       // sum(TR) == 0
    "VortexMinus",       // sum(TR) == 0
    "VortexPlus",        // sum(TR) == 0
    "ZScore",            // standard deviation == 0
];

const ZERO_VOLUME_UNDEFINED: &[&str] = &[
    "CMF",          // divides by sum(volume)
    "TwiggsMF",     // divides by sum(volume)
    "UpDownVolume", // divides by sum(volume)
    "VWAP",         // divides by cumulative volume
    "VWMA",         // divides by sum(volume)
    "VolumeOsc",    // SMA(volume) == 0
    "VolumeRatio",  // SMA(volume) == 0
];

const ZERO_RANGE_UNDEFINED: &[&str] = &[
    "ChaikinVolatility",
    "MassIndex",
    "RVI",
    "RangeExpansion",
];

fn flat_bars(n: usize) -> Vec<Bar> {
    (0..n)
        .map(|i| Bar::new(i as i64 * 3600, 100.0, 100.0, 100.0, 100.0, 100.0))
        .collect()
}

/// Price moves, every bar has zero range.
fn zero_range_bars(n: usize) -> Vec<Bar> {
    (0..n)
        .map(|i| {
            let c = 100.0 + ((i * 37) % 23) as f64;
            Bar::new(i as i64 * 3600, c, c, c, c, 1000.0)
        })
        .collect()
}

/// Price moves, volume is zero.
fn zero_volume_bars(n: usize) -> Vec<Bar> {
    (0..n)
        .map(|i| {
            let c = 100.0 + ((i * 37) % 23) as f64;
            Bar::new(i as i64 * 3600, c, c + 2.0, c - 2.0, c, 0.0)
        })
        .collect()
}

fn unexpected_nan(bars: &[Bar], allowed: &[&str]) -> Vec<String> {
    let tail_start = bars.len() - bars.len() / 4;
    let mut failures = Vec::new();
    for spec in sqx_strategy::indicator_catalog() {
        if allowed.contains(&spec.name) {
            continue;
        }
        let Some(s) = sqx_backtest::compute(spec.name, &spec.default_params(), bars) else {
            failures.push(format!("{}: not registered", spec.name));
            continue;
        };
        if let Some(bad) = s[tail_start..].iter().position(|v| !v.is_finite()) {
            failures.push(format!("{}: non-finite at index {}", spec.name, tail_start + bad));
        }
    }
    failures.sort();
    failures
}

#[test]
fn indicators_stay_finite_on_flat_bars() {
    let failures = unexpected_nan(&flat_bars(200), FLAT_UNDEFINED);
    assert!(
        failures.is_empty(),
        "{} indicator(s) went non-finite on flat bars (add to FLAT_UNDEFINED only if 0/0 is genuinely undefined):
{}",
        failures.len(),
        failures.join("
")
    );
}

#[test]
fn indicators_stay_finite_on_zero_volume() {
    let failures = unexpected_nan(&zero_volume_bars(200), ZERO_VOLUME_UNDEFINED);
    assert!(
        failures.is_empty(),
        "{} indicator(s) went non-finite on zero-volume bars:
{}",
        failures.len(),
        failures.join("
")
    );
}

#[test]
fn indicators_stay_finite_on_zero_range_bars() {
    let failures = unexpected_nan(&zero_range_bars(200), ZERO_RANGE_UNDEFINED);
    assert!(
        failures.is_empty(),
        "{} indicator(s) went non-finite on zero-range bars:
{}",
        failures.len(),
        failures.join("
")
    );
}

/// The specific regression behind the ADX fix: a flat market used to produce
/// NaN for DI+/DI-/ADX because every DI was a 0/0 form.
#[test]
fn adx_is_finite_and_zero_on_flat_bars() {
    let bars = flat_bars(200);
    for name in ["DIPlus", "DIMinus", "ADX"] {
        let s = sqx_backtest::compute(name, &[], &bars).expect("registered");
        assert!(
            s[150..].iter().all(|v| v.is_finite()),
            "{name} went non-finite on flat bars"
        );
        assert!(
            s[150..].iter().all(|v| *v == 0.0),
            "{name} should be 0 when nothing moved, got {:?}",
            &s[150..160]
        );
    }
}

/// Causality is independent of formula agreement: appending or replacing future
/// candles must never revise a historical value or its undefined/warmup status.
#[test]
fn catalog_values_and_warmups_are_prefix_invariant() {
    let mut bars=make_bars(400);
    for (i,bar) in bars.iter_mut().enumerate(){bar.time=1_700_000_000_000+i as i64*900_000;}
    let mut failures=Vec::new();
    for spec in sqx_strategy::indicator_catalog() {
      for period_override in [None,Some(1.0),Some(2.0),Some(7.0),Some(31.0)] {
        let mut params=spec.default_params();
        if let Some(period)=period_override {
            let Some(parameter)=params.iter_mut().find(|p|p.name=="period") else {continue;};
            parameter.value=period;
        }
        let tag=format!("{} period={period_override:?}",spec.name);
        let full=match std::panic::catch_unwind(||sqx_backtest::compute(spec.name,&params,&bars).expect("catalog indicator must be registered")) {
            Ok(values)=>values,Err(_)=>{failures.push(format!("{tag}: panics on full input"));continue;}
        };
        for end in [0,1,2,3,5,9,13,14,19,20,21,26,29,30,50,99,199,299,399] {
            let prefix=match std::panic::catch_unwind(||sqx_backtest::compute(spec.name,&params,&bars[..end]).unwrap()) {
                Ok(values)=>values,Err(_)=>{failures.push(format!("{}: panics for prefix {end}",tag));continue;}
            };
            if prefix.len()!=end {failures.push(format!("{}: length {} for prefix {end}",tag,prefix.len()));continue;}
            if let Some(i)=(0..end).find(|&i|!same_value(prefix[i],full[i])) {
                failures.push(format!("{}: prefix {end}, index {i}: alone={}, with future={}",tag,prefix[i],full[i]));
            }
        }
        for end in [1,20,100,300] {
            let mut changed=bars.clone();
            for (i,bar) in changed[end..].iter_mut().enumerate(){
                bar.open=1000.0+i as f64;bar.close=1001.0+i as f64;bar.high=1010.0+i as f64;bar.low=990.0+i as f64;bar.volume=1.0;
            }
            let perturbed=sqx_backtest::compute(spec.name,&params,&changed).unwrap();
            if let Some(i)=(0..end).find(|&i|!same_value(perturbed[i],full[i])) {
                failures.push(format!("{}: future perturbation from {end} changed index {i}",tag));
            }
        }
    }
    }
    assert!(failures.is_empty(),"Historical indicator values changed:\n{}",failures.join("\n"));
}
fn same_value(a:f64,b:f64)->bool {(a.is_nan()&&b.is_nan())||a.to_bits()==b.to_bits()}
