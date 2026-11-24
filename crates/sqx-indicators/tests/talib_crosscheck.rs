//! Cross-checks against the `ta-lib` Rust package pinned by Cargo.lock.
//! This suite does not run a separate TA-Lib C executable. Agreement is evidence
//! for the exercised inputs/periods, not certification of the whole library.
//! Comparisons check nonempty reference output, full tail coverage and warmup;
//! explicitly documented exceptions cover multi-output alignment and first-bar TR.
//! ADX/DI below compare only their converged tail because their seeds differ.
//! Flat-market RSI/MFI neutral values are separate local conventions tested in
//! regression fixtures; these comparisons use non-flat input.

use sqx_core::Bar;
use sqx_indicators as ind;
use ta_lib::{Core, MAType};

fn make_series(n: usize) -> (Vec<Bar>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut bars = Vec::with_capacity(n);
    let (mut o, mut h, mut l, mut c, mut v) =
        (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    let mut close = 100.0;
    for i in 0..n {
        let open = close;
        close = 100.0 + (i as f64 * 0.71).sin() * 6.0 + (i as f64 * 0.23).cos() * 3.0 + i as f64 * 0.04;
        let high = open.max(close) + 1.0 + (i % 3) as f64 * 0.4;
        let low = open.min(close) - 1.0 - (i % 5) as f64 * 0.25;
        let volume = 100.0 + ((i * 37) % 97) as f64 + (i % 11) as f64 * 7.0;
        o.push(open); h.push(high); l.push(low); c.push(close); v.push(volume);
        bars.push(Bar::new(i as i64 * 60_000, open, high, low, close, volume));
    }
    (bars, o, h, l, c, v)
}

// Compare my series against ta-lib's output, which starts at beg_idx.
fn cmp(tag: &str, mine: &[f64], theirs: &[f64], beg: usize, count: usize, tol: f64) {
    assert!(count>0,"{tag}: reference returned no values; comparison would be vacuous");
    assert_eq!(beg+count,mine.len(),"{tag}: reference must cover the complete defined tail");
    assert!(theirs.len()>=count,"{tag}: reference buffer is too short");
    // STOCHF aligns both outputs to the later %D warmup; our %K is valid earlier.
    // TrueRange explicitly defines the first bar's TR as high-low in this crate.
    if tag!="StochK(14)" && tag!="TrueRange" {
        assert!(mine[..beg].iter().all(|v|v.is_nan()),"{tag}: fabricated values before the reference warmup ends");
    }

    for k in 0..count {
        let i = beg + k;
        let (a, b) = (mine[i], theirs[k]);
        if a.is_nan() || b.is_nan() {
            assert!(a.is_nan() && b.is_nan(), "{tag}[{i}]: sqx={a} ta={b}");
            continue;
        }
        assert!((a - b).abs() <= tol, "{tag}[{i}]: sqx={a} ta={b}");
    }
}

#[test]
fn sma_and_ema_match() {
    let (_, _, _, _, c, _) = make_series(200);
    let core = Core::new();
    for period in [3, 10, 20] {
        let mut o = vec![0.0; c.len()];
        let r = core.sma(0, c.len() - 1, &c, period, &mut o).unwrap();
        cmp(&format!("SMA({period})"), &ind::sma(&c, period as usize), &o, r.beg_idx, r.count, 1e-9);

        let mut o2 = vec![0.0; c.len()];
        let r2 = core.ema(0, c.len() - 1, &c, period, &mut o2).unwrap();
        cmp(&format!("EMA({period})"), &ind::ema(&c, period as usize), &o2, r2.beg_idx, r2.count, 1e-9);
    }
}

#[test]
fn rsi_matches_wilder() {
    let (_, _, _, _, c, _) = make_series(200);
    let core = Core::new();
    for period in [2, 14] {
        let mut o = vec![0.0; c.len()];
        let r = core.rsi(0, c.len() - 1, &c, period, &mut o).unwrap();
        cmp(&format!("RSI({period})"), &ind::rsi(&c, period as usize), &o, r.beg_idx, r.count, 1e-9);
    }
}

#[test]
fn atr_matches_wilder() {
    let (bars, _, h, l, c, _) = make_series(200);
    let core = Core::new();
    for period in [14, 20] {
        let mut o = vec![0.0; c.len()];
        let r = core.atr(0, c.len() - 1, &h, &l, &c, period, &mut o).unwrap();
        cmp(&format!("ATR({period})"), &ind::atr(&bars, period as usize), &o, r.beg_idx, r.count, 1e-9);
    }
}

// ADX / +DI / -DI are asserted in the TAIL only, not from the warm-up.
//
// ta-lib's ADX seeds its internal +DI/-DI with the SUM of the first period-1
// DM/TR values followed by a full-increment Wilder step, which differs from
// Wilder's original (average of the first `period` values) and from ta-lib's
// OWN ATR function. This crate uses the Wilder-faithful average seed - the same
// one its ATR uses (which matches ta-lib ATR exactly) - so the two diverge in
// the warm-up and converge once Wilder's smoothing forgets the seed.
#[test]
fn adx_and_di_converge_to_ta_lib_in_the_tail() {
    let (bars, _, h, l, c, _) = make_series(200);
    let core = Core::new();
    let (mine_p, mine_m, mine_adx) = ind::adx(&bars, 14);

    let mut op = vec![0.0; c.len()];
    let rp = core.plus_di(0, c.len() - 1, &h, &l, &c, 14, &mut op).unwrap();
    let mut om = vec![0.0; c.len()];
    let rm = core.minus_di(0, c.len() - 1, &h, &l, &c, 14, &mut om).unwrap();
    let mut oa = vec![0.0; c.len()];
    let ra = core.adx(0, c.len() - 1, &h, &l, &c, 14, &mut oa).unwrap();

    for i in 100..c.len() {
        assert!((mine_p[i] - op[i - rp.beg_idx]).abs() <= 1e-3, "+DI[{i}] sqx={} ta={}", mine_p[i], op[i - rp.beg_idx]);
        assert!((mine_m[i] - om[i - rm.beg_idx]).abs() <= 1e-3, "-DI[{i}] sqx={} ta={}", mine_m[i], om[i - rm.beg_idx]);
        // ADX applies a second Wilder smoothing on top of the DI, so its seed
        // difference decays even more slowly; allow a looser bound.
        assert!((mine_adx[i] - oa[i - ra.beg_idx]).abs() <= 2e-2, "ADX[{i}] sqx={} ta={}", mine_adx[i], oa[i - ra.beg_idx]);
    }
}

#[test]
fn dpo_matches() {
    let (_, _, _, _, c, _) = make_series(200);
    let core = Core::new();
    for period in [4, 20] {
        let mut o = vec![0.0; c.len()];
        let r = core.dpo(0, c.len() - 1, &c, period, &mut o).unwrap();
        cmp(&format!("DPO({period})"), &ind::dpo(&c, period as usize), &o, r.beg_idx, r.count, 1e-9);
    }
}

#[test]
fn cci_matches() {
    let (_, _, h, l, c, _) = make_series(200);
    let core = Core::new();
    for period in [14, 20] {
        let mut o = vec![0.0; c.len()];
        let r = core.cci(0, c.len() - 1, &h, &l, &c, period, &mut o).unwrap();
        cmp(&format!("CCI({period})"), &ind::cci(&h, &l, &c, period as usize), &o, r.beg_idx, r.count, 1e-9);
    }
}

#[test]
fn mfi_matches() {
    let (_, _, h, l, c, v) = make_series(200);
    let core = Core::new();
    for period in [14] {
        let mut o = vec![0.0; c.len()];
        let r = core.mfi(0, c.len() - 1, &h, &l, &c, &v, period, &mut o).unwrap();
        cmp(&format!("MFI({period})"), &ind::mfi(&h, &l, &c, &v, period as usize), &o, r.beg_idx, r.count, 1e-9);
    }
}

#[test]
fn stoch_matches() {
    let (_, _, h, l, c, _) = make_series(200);
    let core = Core::new();
    let mut ok = vec![0.0; c.len()];
    let mut od = vec![0.0; c.len()];
    let r = core.stoch(0, c.len() - 1, &h, &l, &c, 14, 1, MAType::SMA, 3, MAType::SMA, &mut ok, &mut od).unwrap();
    cmp("StochK(14)", &ind::stoch_k(&h, &l, &c, 14), &ok, r.beg_idx, r.count, 1e-9);
    cmp("StochD(3)", &ind::stoch_d(&ind::stoch_k(&h, &l, &c, 14), 3), &od, r.beg_idx, r.count, 1e-9);
}

#[test]
fn aroon_matches() {
    let (_, _, h, l, _, _) = make_series(200);
    let core = Core::new();
    for period in [14, 25] {
        let mut odown = vec![0.0; c_of(&h)];
        let mut oup = vec![0.0; c_of(&h)];
        let r = core.aroon(0, h.len() - 1, &h, &l, period, &mut odown, &mut oup).unwrap();
        let (mine_up, mine_down) = ind::aroon(&h, &l, period as usize);
        cmp(&format!("AroonUp({period})"), &mine_up, &oup, r.beg_idx, r.count, 1e-9);
        cmp(&format!("AroonDown({period})"), &mine_down, &odown, r.beg_idx, r.count, 1e-9);
    }
}

fn c_of(x: &[f64]) -> usize { x.len() }

// MACD is deliberately NOT asserted equal to ta-lib: ta-lib late-seeds the fast
// EMA (from bar slow-fast rather than bar 0) as a lockstep shortcut, so its
// early MACD values differ from the standard EMA(fast) - EMA(slow). Both
// converge in the far tail. This check only establishes internal consistency;
// reference.rs separately checks fixed rational values and first-signal warmup
// for this crate's explicitly SMA-seeded (2,3,2) convention.
#[test]
fn macd_components_are_internally_consistent() {
    let (_, _, _, _, c, _) = make_series(200);
    let line = ind::macd(&c, 12, 26);
    let sig = ind::macd_signal(&line, 9);
    let hist = ind::macd_hist(&line, &sig);
    // hist == line - signal, and line == ema(12) - ema(26).
    let e12 = ind::ema(&c, 12);
    let e26 = ind::ema(&c, 26);
    for i in 33..c.len() {
        assert!((hist[i] - (line[i] - sig[i])).abs() <= 1e-12, "hist[{i}] inconsistent");
        assert!((line[i] - (e12[i] - e26[i])).abs() <= 1e-12, "macd[{i}] != ema12 - ema26");
    }
}

#[test]
fn bollinger_matches() {
    let (_, _, _, _, c, _) = make_series(200);
    let core = Core::new();
    let mut ou = vec![0.0; c.len()];
    let mut om = vec![0.0; c.len()];
    let mut ol = vec![0.0; c.len()];
    let r = core.bbands(0, c.len() - 1, &c, 20, 2.0, 2.0, MAType::SMA, &mut ou, &mut om, &mut ol).unwrap();
    let (u, m, l) = ind::bollinger(&c, 20, 2.0);
    cmp("BB.upper", &u, &ou, r.beg_idx, r.count, 1e-6);
    cmp("BB.middle", &m, &om, r.beg_idx, r.count, 1e-9);
    cmp("BB.lower", &l, &ol, r.beg_idx, r.count, 1e-6);
}

// CMO is deliberately NOT asserted here: ta-lib smooths the up/down sums with
// Wilder's average, while this crate implements Chande's original rolling-window
// CMO. They are different variants and do not agree.
#[test]
fn trange_roc_stddev_obv_willr_ppo_match() {
    let (bars, _, h, l, c, v) = make_series(200);
    let core = Core::new();

    let mut ot = vec![0.0; c.len()];
    let rt = core.trange(0, c.len() - 1, &h, &l, &c, &mut ot).unwrap();
    cmp("TrueRange", &ind::true_range(&bars), &ot, rt.beg_idx, rt.count, 1e-9);

    let mut or = vec![0.0; c.len()];
    let rr = core.roc(0, c.len() - 1, &c, 10, &mut or).unwrap();
    cmp("ROC(10)", &ind::roc(&c, 10), &or, rr.beg_idx, rr.count, 1e-9);

    let mut os = vec![0.0; c.len()];
    let rs = core.stddev(0, c.len() - 1, &c, 20, 1.0, &mut os).unwrap();
    cmp("StdDev(20)", &ind::stddev(&c, 20), &os, rs.beg_idx, rs.count, 1e-6);

    let mut oo = vec![0.0; c.len()];
    let ro = core.obv(0, c.len() - 1, &c, &v, &mut oo).unwrap();
    cmp("OBV", &ind::obv(&c, &v), &oo, ro.beg_idx, ro.count, 1e-9);

    let mut ow = vec![0.0; c.len()];
    let rw = core.willr(0, c.len() - 1, &h, &l, &c, 14, &mut ow).unwrap();
    cmp("WilliamsR(14)", &ind::williams_r(&h, &l, &c, 14), &ow, rw.beg_idx, rw.count, 1e-9);

    let mut op = vec![0.0; c.len()];
    let rp = core.ppo(0, c.len() - 1, &c, 12, 26, MAType::EMA, &mut op).unwrap();
    cmp("PPO(12,26)", &ind::ppo(&c, 12, 26), &op, rp.beg_idx, rp.count, 1e-9);
}
