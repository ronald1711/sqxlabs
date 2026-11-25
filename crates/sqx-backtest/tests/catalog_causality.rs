//! Prefix invariance of the actual strategy indicator dispatch.
//! This checks causality and alignment, not mathematical formula correctness.
use sqx_core::Bar;

#[test]
fn default_catalog_outputs_do_not_change_when_future_candles_arrive() {
    let bars: Vec<_> = (0..320).map(|i| {
        let open = 1.1 + (i as f64 * 0.17).sin() * 0.02;
        let close = open + (i as f64 * 0.31).cos() * 0.003;
        Bar::new(i * 60000, open, open.max(close) + 0.001, open.min(close) - 0.001, close, 10.0 + (i % 19) as f64)
    }).collect();
    let catalog = sqx_strategy::indicator_catalog();
    assert!(!catalog.is_empty());
    for indicator in catalog {
        let params = indicator.default_params();
        let complete = sqx_backtest::compute(indicator.name, &params, &bars).expect("catalog indicator must be registered");
        assert_eq!(complete.len(), bars.len(), "{} output alignment", indicator.name);
        assert!(complete.iter().any(|v| v.is_finite()), "{} must have defined coverage in this fixture", indicator.name);
        for end in 1..bars.len() {
            let prefix = sqx_backtest::compute(indicator.name, &params, &bars[..end]).unwrap();
            assert_eq!(prefix.len(), end, "{} truncated alignment", indicator.name);
            for (index, (&before, &after)) in prefix.iter().zip(&complete).enumerate() {
                assert!((before.is_nan() && after.is_nan()) || before.to_bits() == after.to_bits(),
                    "{} changed historical value {index} after bar {end}: {before} -> {after}", indicator.name);
            }
        }
    }
}
