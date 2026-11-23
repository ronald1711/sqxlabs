//! Cross-checks against the `ta` Rust package pinned by Cargo.lock on identical
//! synthetic data. Agreement covers these exercised formulas and conventions;
//! it does not establish correctness of every registered indicator.
//!
//! Only indicators whose conventions genuinely match are asserted equal.
//! Differences identified for the compared package version are documented below
//! rather than asserted equal:
//!
//! * EMA       - ta seeds with the first value; we seed with an SMA (the
//!               TradingView / Wilder warm-up). MACD/PPO/TSI inherit this.
//! * ATR       - ta smooths with EMA; we use Wilder's RMA (the classic ATR).
//!               Keltner and Chandelier inherit this ATR difference.
//! * RSI       - ta is the Cutler/EMA variant and seeds 0.1/0.1; we use
//!               Wilder. On a flat market ta decays to NaN while we return a
//!               neutral 50 (see regressions2.rs).
//! * SlowStoch - ta smooths %K with an EMA; we use an SMA (the common %D).
//! * Keltner   - ta uses EMA(typical price) for the middle; we use EMA(close).
//! * CCI       - ta has a bug: it feeds the bar (close) into the mean
//!               absolute deviation instead of the typical price, so its
//!               denominator is wrong. Ours uses TP, per the definition.

use sqx_core::Bar;
use sqx_indicators as ind;
use ta::Next;

fn make_series(n: usize) -> (Vec<Bar>, Vec<ta::DataItem>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut bars = Vec::with_capacity(n);
    let mut items = Vec::with_capacity(n);
    let (mut o, mut h, mut l, mut c, mut v) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    let mut close = 100.0;
    for i in 0..n {
        let open = close;
        close = 100.0 + (i as f64 * 0.7).sin() * 6.0 + i as f64 * 0.05;
        let high = open.max(close) + 1.0 + (i % 3) as f64 * 0.3;
        let low = open.min(close) - 1.0 - (i % 5) as f64 * 0.2;
        let volume = 100.0 + ((i * 37) % 50) as f64;
        o.push(open); h.push(high); l.push(low); c.push(close); v.push(volume);
        bars.push(Bar::new(i as i64 * 60_000, open, high, low, close, volume));
        items.push(ta::DataItem::builder().open(open).high(high).low(low).close(close).volume(volume).build().unwrap());
    }
    (bars, items, o, h, l, c, v)
}

fn cmp(tag: &str, mine: &[f64], theirs: &[f64], start: usize, tol: f64) {
    assert_eq!(mine.len(), theirs.len(), "{tag}: length mismatch");
    for i in start..mine.len() {
        assert!(
            (mine[i] - theirs[i]).abs() <= tol,
            "{tag}[{i}]: sqx={} ta={}",
            mine[i],
            theirs[i]
        );
    }
}

#[test]
fn sma_matches() {
    let (_, _, _, _, _, c, _) = make_series(120);
    let mut t = ta::indicators::SimpleMovingAverage::new(20).unwrap();
    let theirs: Vec<f64> = c.iter().map(|&x| t.next(x)).collect();
    let mine = ind::sma(&c, 20);
    cmp("SMA(20)", &mine, &theirs, 19, 1e-9);
}

#[test]
fn true_range_matches() {
    let (bars, items, ..) = make_series(120);
    let mut t = ta::indicators::TrueRange::new();
    let theirs: Vec<f64> = items.iter().map(|it| t.next(it)).collect();
    let mine = ind::true_range(&bars);
    cmp("TrueRange", &mine, &theirs, 0, 1e-9);
}

#[test]
fn obv_matches() {
    let (bars, items, ..) = make_series(120);
    let c: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let v: Vec<f64> = bars.iter().map(|b| b.volume).collect();
    let mut t = ta::indicators::OnBalanceVolume::new();
    let theirs: Vec<f64> = items.iter().map(|it| t.next(it)).collect();
    let mine = ind::obv(&c, &v);
    cmp("OBV", &mine, &theirs, 0, 1e-9);
}

#[test]
fn roc_matches_after_warmup() {
    let (_, _, _, _, _, c, _) = make_series(120);
    let mut t = ta::indicators::RateOfChange::new(10).unwrap();
    let theirs: Vec<f64> = c.iter().map(|&x| t.next(x)).collect();
    let mine = ind::roc(&c, 10);
    cmp("ROC(10)", &mine, &theirs, 10, 1e-9);
}

#[test]
fn fast_stochastic_matches() {
    let (bars, items, ..) = make_series(120);
    let h: Vec<f64> = bars.iter().map(|b| b.high).collect();
    let l: Vec<f64> = bars.iter().map(|b| b.low).collect();
    let c: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let mut t = ta::indicators::FastStochastic::new(14).unwrap();
    let theirs: Vec<f64> = items.iter().map(|it| t.next(it)).collect();
    let mine = ind::stoch_k(&h, &l, &c, 14);
    cmp("StochK(14)", &mine, &theirs, 13, 1e-9);
}

#[test]
fn bollinger_matches() {
    let (_, _, _, _, _, c, _) = make_series(120);
    let mut t = ta::indicators::BollingerBands::new(20, 2.0).unwrap();
    let theirs_upper: Vec<f64> = c.iter().map(|&x| t.next(x).upper).collect();
    let mut t2 = ta::indicators::BollingerBands::new(20, 2.0).unwrap();
    let theirs_lower: Vec<f64> = c.iter().map(|&x| t2.next(x).lower).collect();
    let (upper, _mid, lower) = ind::bollinger(&c, 20, 2.0);
    cmp("BB.upper(20,2)", &upper, &theirs_upper, 19, 1e-6);
    cmp("BB.lower(20,2)", &lower, &theirs_lower, 19, 1e-6);
}

#[test]
fn maximum_minimum_match() {
    let (_, _, _, h, l, _, _) = make_series(120);
    let mut tmax = ta::indicators::Maximum::new(20).unwrap();
    let theirs_max: Vec<f64> = h.iter().map(|&x| tmax.next(x)).collect();
    let mine_max = ind::highest(&h, 20);
    cmp("Highest(20)", &mine_max, &theirs_max, 19, 1e-9);
    let mut tmin = ta::indicators::Minimum::new(20).unwrap();
    let theirs_min: Vec<f64> = l.iter().map(|&x| tmin.next(x)).collect();
    let mine_min = ind::lowest(&l, 20);
    cmp("Lowest(20)", &mine_min, &theirs_min, 19, 1e-9);
}

#[test]
fn efficiency_ratio_matches_after_warmup() {
    let (_, _, _, _, _, c, _) = make_series(120);
    let mut t = ta::indicators::EfficiencyRatio::new(14).unwrap();
    let theirs: Vec<f64> = c.iter().map(|&x| t.next(x)).collect();
    let mine = ind::kaufman_er(&c, 14);
    cmp("EfficiencyRatio(14)", &mine, &theirs, 14, 1e-9);
}

#[test]
fn mfi_matches_after_warmup() {
    let (bars, items, ..) = make_series(120);
    let h: Vec<f64> = bars.iter().map(|b| b.high).collect();
    let l: Vec<f64> = bars.iter().map(|b| b.low).collect();
    let c: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let v: Vec<f64> = bars.iter().map(|b| b.volume).collect();
    let mut t = ta::indicators::MoneyFlowIndex::new(14).unwrap();
    let theirs: Vec<f64> = items.iter().map(|it| t.next(it)).collect();
    let mine = ind::mfi(&h, &l, &c, &v, 14);
    // ta 0.5 emits partial-window values from its streaming initializer. Only
    // index 14 has fourteen classified flows; earlier Sqx values must be NaN.
    // The hand-calculated fixture and ta-lib comparison independently check this.
    assert!(mine[..14].iter().all(|v|v.is_nan()));
    cmp("MFI(14)", &mine, &theirs, 14, 1e-9);
}

#[test]
fn standard_deviation_matches() {
    let (_, _, _, _, _, c, _) = make_series(120);
    let mut t = ta::indicators::StandardDeviation::new(20).unwrap();
    let theirs: Vec<f64> = c.iter().map(|&x| t.next(x)).collect();
    let mine = ind::stddev(&c, 20);
    cmp("StdDev(20)", &mine, &theirs, 19, 1e-6);
}
