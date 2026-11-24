use sqx_backtest::compute;
use sqx_core::Bar;
use sqx_strategy::Param;

#[test]
fn stochastic_registry_uses_fast_k14_and_parameterized_d_smoothing() {
    // Constant extrema 0/10 make raw K equal to ten times close. At indices
    // 13..17 those values are 60,80,0,20,40. D is their trailing mean,
    // not a second smoothed K or a shortened K lookback.
    let bars: Vec<_> = (0..18).map(|i| {
        let close = (i % 5) as f64 * 2.0;
        Bar::new(i * 60_000, close, 10.0, 0.0, close, 1.0)
    }).collect();
    let k = compute("StochK", &[], &bars).unwrap();
    assert!(k[..13].iter().all(|v|v.is_nan()));
    assert_eq!(&k[13..], &[60.0,80.0,0.0,20.0,40.0]);
    for (params, first, expected) in [
        (vec![], 15, vec![140.0/3.0,100.0/3.0,20.0]),
        (vec![Param::new("period",2.0)], 14, vec![70.0,40.0,10.0,30.0]),
    ] {
        let d = compute("StochD", &params, &bars).unwrap();
        assert!(d[..first].iter().all(|v|v.is_nan()));
        for (got,want) in d[first..].iter().zip(expected) { assert!((got-want).abs()<1e-10); }
    }
}
