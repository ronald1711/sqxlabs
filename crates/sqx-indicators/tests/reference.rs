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

#[test]
fn vortex_hand_calculation_requires_n_actual_movements() {
    // Independent arithmetic fixture following Botes & Siepman's original
    // spreadsheet definition: n consecutive movements require n+1 candles.
    // https://technical.traders.com/free/V28C01005BOTE.pdf (Excel sidebar)
    // Per-bar values after the first candle:
    // VM+ = [4,2,8,3], VM- = [1,5,0,5], TR = [3,4,7,4].
    let h=[11.0,13.0,12.0,16.0,15.0];
    let l=[9.0,10.0,8.0,12.0,11.0];
    let c=[10.0,12.0,9.0,15.0,12.0];
    let (plus,minus)=ind::vortex(&h,&l,&c,2);
    assert_series(&plus,&[f64::NAN,f64::NAN,6.0/7.0,10.0/11.0,1.0]);
    assert_series(&minus,&[f64::NAN,f64::NAN,6.0/7.0,5.0/11.0,5.0/11.0]);
    let (plus,minus)=ind::vortex(&h,&l,&c,3);
    assert_series(&plus,&[f64::NAN,f64::NAN,f64::NAN,1.0,13.0/15.0]);
    assert_series(&minus,&[f64::NAN,f64::NAN,f64::NAN,3.0/7.0,2.0/3.0]);
    let (plus,minus)=ind::vortex(&h,&l,&c,1);
    assert_series(&plus,&[f64::NAN,4.0/3.0,0.5,8.0/7.0,0.75]);
    assert_series(&minus,&[f64::NAN,1.0/3.0,1.25,0.0,1.25]);
    assert_eq!(ind::vortex(&[],&[],&[],14),(vec![],vec![]));
}


#[test]
fn mfi_hand_calculation_requires_n_classified_flows() {
    // Typical prices [10,12,9,15,12], volumes [1,2,3,4,5].
    // Classified flows are +24, -27, +60, -60; candle 0 is unclassified.
    // Formula reference: https://www.tradingview.com/support/solutions/43000502348-money-flow-mfi/
    let prices=[10.0,12.0,9.0,15.0,12.0];let volumes=[1.0,2.0,3.0,4.0,5.0];
    assert_series(&ind::mfi(&prices,&prices,&prices,&volumes,2),
        &[f64::NAN,f64::NAN,2400.0/51.0,6000.0/87.0,50.0]);
    assert_series(&ind::mfi(&prices,&prices,&prices,&volumes,1),
        &[f64::NAN,100.0,0.0,100.0,0.0]);
    assert!(ind::mfi(&[],&[],&[],&[],14).is_empty());
}

#[test]
fn cmo_matches_hand_calculated_rolling_changes_not_wilder_smoothing(){
    // Independent definition: Fidelity CMO indicator guide, accessed 2026-11-23.
    // https://www.fidelity.com/learning-center/trading-investing/technical-analysis/technical-indicator-guide/cmo
    // Changes +3,-2,+4,-1,0: consecutive three-change windows have
    // (up,down)=(7,2),(4,3),(4,1). No smoothed sums enter these fractions.
    let close=[10.0,13.0,11.0,15.0,14.0,14.0];
    let expected=[f64::NAN,f64::NAN,f64::NAN,500.0/9.0,100.0/7.0,60.0];
    assert_series(&ind::cmo(&close,3),&expected);
    let transformed:Vec<f64>=close.iter().map(|v|100.0+2.0*v).collect();
    assert_series(&ind::cmo(&transformed,3),&expected);
    assert_series(&ind::cmo(&[1.0,2.0,3.0,4.0],3),&[f64::NAN,f64::NAN,f64::NAN,100.0]);
    assert_series(&ind::cmo(&[4.0,3.0,2.0,1.0],3),&[f64::NAN,f64::NAN,f64::NAN,-100.0]);
    // Flat denominator handling is our explicit neutral-value convention.
    assert_series(&ind::cmo(&[4.0;4],3),&[f64::NAN,f64::NAN,f64::NAN,0.0]);
}

#[test]
fn cmo_requires_complete_change_windows_and_recovers_after_missing_input(){
    let close=[10.0,13.0,f64::NAN,15.0,14.0,16.0,19.0];
    let values=ind::cmo(&close,3);
    assert!(values[..6].iter().all(|v|v.is_nan()));
    // The final valid window contains -1,+2,+3: up=5, down=1.
    assert!(approx(values[6],200.0/3.0));
    for length in 0..=3 {assert!(ind::cmo(&close[..length],3).iter().all(|v|v.is_nan()));}
    assert!(ind::cmo(&close,0).iter().all(|v|v.is_nan()));
}

#[test]
fn tii_current_baseline_and_two_to_one_periods_have_independent_values(){
    // M.H. Pee, Stocks & Commodities June 2002, original description:
    // https://traders.com/documentation/feedbk_docs/2002/06/Abstracts_new/Pee/pee
    // n=2: SMA4 is 7 then 7.25. Last two deviations are (-1,+3),
    // then (+2.75,-2.25), yielding 75 and 55 respectively.
    assert_series(&ind::tii(&[4.0,8.0,6.0,10.0,5.0],2),&[f64::NAN,f64::NAN,f64::NAN,75.0,55.0]);
    // Original 30/60 mapping: SMA60=1.25. Last 30 deviations contain
    // 15*1.75 positive and 15*1.25 negative: 100*26.25/45 = 175/3.
    let mut closes=vec![1.0;30];closes.extend(vec![3.0;15]);closes.extend(vec![0.0;15]);
    let values=ind::tii(&closes,30);
    assert!(values[..59].iter().all(|v|v.is_nan()));assert!(approx(values[59],175.0/3.0));
    for period in [0,31,usize::MAX/2+1,usize::MAX] {
        let short=ind::tii(&closes,period);assert_eq!(short.len(),closes.len());assert!(short.iter().all(|v|v.is_nan()));
    }
}
