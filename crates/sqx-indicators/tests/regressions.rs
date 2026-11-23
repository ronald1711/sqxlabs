//! Regression tests for the indicator corrections made after the audit.
//!
//! The SuperTrend and Parabolic SAR values were produced by an independent
//! implementation written from the published definitions, then compared element
//! by element. The ADX/+DI values reflect the corrected Wilder seed (skipping the
//! degenerate TR[0]/DM[0], matching TA-Lib's ATR convention); they are
//! cross-validated against ta-lib in talib_crosscheck.rs. Editing the indicator
//! code until these tests pass again is the intended use; editing the constants
//! is not.

use sqx_core::Bar;
use sqx_indicators as ind;

/// Synthetic OHLC with a clean up-leg and down-leg. Integer-valued so that the
/// series is exactly representable and the reference output is reproducible.
const H: [f64; 80] = [
    100.0, 104.0, 108.0, 108.0, 111.0, 111.0, 114.0, 118.0, 118.0, 121.0, 121.0, 124.0, 128.0, 128.0, 131.0, 131.0, 134.0, 138.0, 138.0, 141.0, 141.0, 140.0, 140.0, 140.0, 135.0, 135.0, 130.0, 130.0, 130.0, 125.0, 125.0, 120.0, 120.0, 120.0, 115.0, 115.0, 110.0, 110.0, 110.0, 105.0, 105.0, 104.0, 108.0, 108.0, 111.0, 111.0, 114.0, 118.0, 118.0, 121.0, 121.0, 124.0, 128.0, 128.0, 131.0, 131.0, 134.0, 138.0, 138.0, 141.0, 141.0, 140.0, 140.0, 140.0, 135.0, 135.0, 130.0, 130.0, 130.0, 125.0, 125.0, 120.0, 120.0, 120.0, 115.0, 115.0, 110.0, 110.0, 110.0, 105.0,
];

const L: [f64; 80] = [
    96.0, 96.0, 100.0, 103.0, 103.0, 106.0, 106.0, 110.0, 113.0, 113.0, 116.0, 116.0, 120.0, 123.0, 123.0, 126.0, 126.0, 130.0, 133.0, 133.0, 136.0, 136.0, 136.0, 131.0, 131.0, 126.0, 126.0, 126.0, 121.0, 121.0, 116.0, 116.0, 116.0, 111.0, 111.0, 106.0, 106.0, 106.0, 101.0, 101.0, 96.0, 96.0, 100.0, 103.0, 103.0, 106.0, 106.0, 110.0, 113.0, 113.0, 116.0, 116.0, 120.0, 123.0, 123.0, 126.0, 126.0, 130.0, 133.0, 133.0, 136.0, 136.0, 136.0, 131.0, 131.0, 126.0, 126.0, 126.0, 121.0, 121.0, 116.0, 116.0, 116.0, 111.0, 111.0, 106.0, 106.0, 106.0, 101.0, 101.0,
];

const C: [f64; 80] = [
    98.0, 102.0, 106.0, 105.0, 109.0, 108.0, 112.0, 116.0, 115.0, 119.0, 118.0, 122.0, 126.0, 125.0, 129.0, 128.0, 132.0, 136.0, 135.0, 139.0, 138.0, 138.0, 138.0, 133.0, 133.0, 128.0, 128.0, 128.0, 123.0, 123.0, 118.0, 118.0, 118.0, 113.0, 113.0, 108.0, 108.0, 108.0, 103.0, 103.0, 98.0, 102.0, 106.0, 105.0, 109.0, 108.0, 112.0, 116.0, 115.0, 119.0, 118.0, 122.0, 126.0, 125.0, 129.0, 128.0, 132.0, 136.0, 135.0, 139.0, 138.0, 138.0, 138.0, 133.0, 133.0, 128.0, 128.0, 128.0, 123.0, 123.0, 118.0, 118.0, 118.0, 113.0, 113.0, 108.0, 108.0, 108.0, 103.0, 103.0,
];

const EXPECTED_ST_LINE: [f64; 80] = [
    f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, 138.9000, 138.9000,
    138.9000, 138.9000, 138.9000, 138.9000, 138.9000, 138.9000, 138.9000, 116.2735, 118.3462, 118.6615, 119.3954, 119.3954,
    119.3954, 119.3954, 119.3954, 119.3954, 119.3954, 119.3954, 139.6613, 136.4451, 135.8006, 134.2206, 131.0485, 129.4436,
    126.2493, 125.6244, 124.0619, 120.9057, 119.3152, 119.3152, 119.3152, 119.3152, 119.3152, 119.3152, 119.3152, 119.3152,
    119.3152, 119.3152, 119.3152, 99.9589, 103.5630, 105.6067, 106.6960, 108.7264, 109.8038, 113.4234, 115.4811, 116.5830,
    118.6247, 118.9122, 119.6210, 119.6210, 119.6210, 119.6210, 119.6210, 119.6210, 119.6210, 119.6210, 139.5641, 136.3577,
    135.7220, 134.1498, 130.9848, 129.3863, 126.1977, 125.5779, 124.0201, 120.8681,
];

const EXPECTED_PSAR: [f64; 80] = [
    f64::NAN, 96.0000, 96.0000, 96.0000, 96.7200, 97.8624, 98.9134, 100.4221, 102.5314, 104.3876, 106.7134, 108.7135, 111.1593, 114.1907, 116.6763, 119.5411, 121.8329, 124.2663, 126.0000, 128.4000, 130.9200, 132.9360, 134.5488, 141.0000, 140.8000, 140.6040, 140.0198, 139.4590, 138.9207, 137.8454, 136.8347, 135.1679, 133.6345, 132.2237, 130.1014, 128.1912, 125.5283, 123.1849, 121.1227, 118.3055, 115.8828, 112.7015, 110.0293, 108.0000, 96.0000, 96.3000, 96.5940, 97.2902, 98.5328, 99.7009, 101.4048, 102.9724, 105.0752, 107.8261, 110.2470, 113.1524, 115.6511, 118.5869, 122.0813, 124.9466, 128.1573, 130.7258, 132.7807, 141.0000, 140.8000, 140.6040, 140.0198, 139.4590, 138.9207, 137.8454, 136.8347, 135.1679, 133.6345, 132.2237, 130.1014, 128.1912, 125.5283, 123.1849, 121.1227, 118.3055,
];

const EXPECTED_DIPLUS: [f64; 80] = [
    f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN,
    f64::NAN, f64::NAN, 31.9588, 30.2780, 30.8781, 32.4486, 30.7488, 31.3077, 29.6552, 28.3653, 27.0960, 24.4455,
    23.3522, 21.0689, 20.1269, 19.2024, 17.2792, 16.4888, 14.8435, 14.1669, 13.5040, 12.1288, 11.5651, 10.3946,
    9.9143, 9.4443, 8.4713, 8.0732, 7.2479, 6.6018, 10.4029, 9.8238, 12.2462, 11.5649, 13.8337, 16.9475,
    16.0192, 17.8717, 16.8912, 18.6712, 21.3372, 20.1813, 21.6599, 20.4828, 21.9398, 24.3087, 23.0017, 24.2310,
    22.9228, 21.9040, 20.9034, 18.8204, 17.9636, 16.1788, 15.4444, 14.7246, 13.2304, 12.6176, 11.3444, 10.8217,
    10.3101, 9.2505, 8.8168, 7.9174, 7.5488, 7.1884, 6.4431, 6.1384,
];

const EXPECTED_ADX: [f64; 80] = [
    f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN,
    f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN,
    f64::NAN, f64::NAN, f64::NAN, 80.9014, 75.9070, 71.2692, 66.7551, 62.5634, 58.6712, 56.2162, 53.9365, 52.7387,
    51.6264, 50.5935, 50.4203, 50.2595, 50.7350, 51.1767, 50.0690, 49.0405, 47.0644, 45.2295, 42.5815, 40.0019,
    37.6066, 36.0854, 34.6729, 34.0116, 34.1171, 34.2152, 34.7908, 35.3252, 36.2697, 37.6428, 38.9179, 40.4359,
    41.8456, 43.1545, 44.3700, 43.3173, 42.3397, 39.7553, 37.3555, 35.1271, 33.5768, 32.1373, 31.8903, 31.6610,
    31.4481, 32.1693, 32.8390, 34.1833, 35.4316, 36.5907, 38.2806, 39.8498,
];

fn bars() -> Vec<Bar> {
    (0..80)
        .map(|i| Bar::new(i as i64 * 60_000, C[i], H[i], L[i], C[i], 100.0))
        .collect()
}

fn assert_close(got: &[f64], want: &[f64], tol: f64, label: &str) {
    assert_eq!(got.len(), want.len(), "{label}: length mismatch");
    for i in 0..want.len() {
        if want[i].is_nan() {
            assert!(got[i].is_nan(), "{label}[{i}]: expected NaN, got {}", got[i]);
        } else {
            assert!(
                (got[i] - want[i]).abs() <= tol,
                "{label}[{i}]: expected {}, got {}",
                want[i],
                got[i]
            );
        }
    }
}

// ---------------------------------------------------------------- SuperTrend

#[test]
fn supertrend_matches_reference_implementation() {
    let (_, line) = ind::supertrend(&bars(), 10, 3.0);
    assert_close(&line, &EXPECTED_ST_LINE, 1e-3, "supertrend");
}

#[test]
fn supertrend_direction_flips_at_the_reference_bars() {
    let (trend, _) = ind::supertrend(&bars(), 10, 3.0);
    let flips: Vec<usize> = (1..trend.len())
        .filter(|&i| trend[i].is_finite() && trend[i - 1].is_finite() && trend[i] != trend[i - 1])
        .collect();
    assert_eq!(flips, vec![19, 30, 51, 70], "SuperTrend must flip on every reversal");
}

/// The band is a ratchet: while the trend is up it may only rise, and while the
/// trend is down it may only fall. A violation means the band is leaking toward
/// the raw ATR band and the trend can never turn.
#[test]
fn supertrend_line_is_a_ratchet_within_a_trend() {
    let (trend, line) = ind::supertrend(&bars(), 10, 3.0);
    // ATR(10) warms up over the first ten bars (its seed skips TR[0]).
    for i in 10..line.len() {
        assert!(trend[i].is_finite(), "bar {i} should be past the warm-up");
        if trend[i] == 1.0 && trend[i - 1] == 1.0 {
            assert!(
                line[i] >= line[i - 1],
                "bar {i}: up-trend line fell from {} to {}",
                line[i - 1],
                line[i]
            );
        }
        if trend[i] == -1.0 && trend[i - 1] == -1.0 {
            assert!(
                line[i] <= line[i - 1],
                "bar {i}: down-trend line rose from {} to {}",
                line[i - 1],
                line[i]
            );
        }
    }
}

/// The specific failure that motivated the fix: a straight up-leg followed by a
/// straight down-leg used to produce zero flips, i.e. a permanent long signal.
#[test]
fn supertrend_flips_on_a_clean_reversal() {
    let mut ohlc = Vec::new();
    let mut p = 100.0;
    for _ in 0..20 {
        ohlc.push((p, p + 2.0, p - 1.0, p + 1.5));
        p += 1.0;
    }
    for _ in 0..25 {
        ohlc.push((p, p + 1.0, p - 3.0, p - 2.5));
        p -= 2.0;
    }
    let b: Vec<Bar> = ohlc
        .iter()
        .enumerate()
        .map(|(i, v)| Bar::new(i as i64 * 60_000, v.0, v.1, v.2, v.3, 100.0))
        .collect();
    let (trend, _) = ind::supertrend(&b, 10, 3.0);
    let flips: Vec<usize> = (1..trend.len())
        .filter(|&i| trend[i].is_finite() && trend[i - 1].is_finite() && trend[i] != trend[i - 1])
        .collect();
    assert_eq!(flips.len(), 1, "expected exactly one flip, got {flips:?}");
    assert_eq!(trend[24], -1.0, "must be short after the reversal");
    assert_eq!(trend[23], 1.0, "must be long before the reversal");
}

// ------------------------------------------------------------- Parabolic SAR

#[test]
fn parabolic_sar_matches_reference_implementation() {
    let s = ind::parabolic_sar(&bars(), 0.02, 0.2);
    assert_close(&s, &EXPECTED_PSAR, 1e-3, "psar");
}

/// Wilder's clamp: the SAR may never sit inside the range of the previous two
/// bars. Without it the stop lands inside price and can never be hit.
#[test]
fn parabolic_sar_never_penetrates_the_previous_two_bars() {
    let b = bars();
    let s = ind::parabolic_sar(&b, 0.02, 0.2);
    for i in 2..s.len() {
        if !s[i].is_finite() {
            continue;
        }
        // A SAR strictly between the prior low and prior high is inside the bar.
        assert!(
            !(s[i] > b[i - 1].low && s[i] < b[i - 1].high),
            "bar {i}: SAR {} sits inside the previous bar [{}, {}]",
            s[i],
            b[i - 1].low,
            b[i - 1].high
        );
    }
}

#[test]
fn parabolic_sar_has_a_one_bar_warmup() {
    let s = ind::parabolic_sar(&bars(), 0.02, 0.2);
    assert!(s[0].is_nan(), "a single bar carries no direction");
    assert!(s[1].is_finite(), "SAR must be defined from the second bar");
}

// ----------------------------------------------------------------------- ADX

#[test]
fn adx_matches_reference_implementation() {
    let b = bars();
    let (dip, _, adx) = ind::adx(&b, 14);
    assert_close(&dip, &EXPECTED_DIPLUS, 1e-3, "DI+");
    assert_close(&adx, &EXPECTED_ADX, 1e-3, "ADX");
}

/// A perfectly flat market has zero true range, so every DI is a 0/0 form. That
/// must yield zero, not NaN - a NaN here silently kills the whole ADX series and
/// every strategy condition built on it.
#[test]
fn adx_is_finite_and_neutral_on_a_flat_market() {
    let b: Vec<Bar> = (0..40)
        .map(|i| Bar::new(i as i64 * 60_000, 100.0, 100.0, 100.0, 100.0, 100.0))
        .collect();
    let (dip, dim, adx) = ind::adx(&b, 14);
    for i in 30..40 {
        assert_eq!(dip[i], 0.0, "DI+ at flat bar {i}");
        assert_eq!(dim[i], 0.0, "DI- at flat bar {i}");
        assert_eq!(adx[i], 0.0, "ADX at flat bar {i}");
    }
}

/// A single flat stretch in the middle of a moving market must not poison the
/// values that follow it.
#[test]
fn adx_recovers_after_a_flat_stretch() {
    let mut b = bars();
    for i in 40..45 {
        b[i] = Bar::new(i as i64 * 60_000, 100.0, 100.0, 100.0, 100.0, 100.0);
    }
    let (_, _, adx) = ind::adx(&b, 14);
    assert!(
        adx[45..].iter().all(|v| v.is_finite()),
        "ADX must stay finite after a flat stretch: {:?}",
        &adx[45..]
    );
}
