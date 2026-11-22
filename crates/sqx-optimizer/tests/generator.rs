use rand::{rngs::StdRng, SeedableRng};
use sqx_optimizer::{random_strategy, GeneratorConfig};
use sqx_strategy::Operand;

#[test]
fn param_ranges_are_respected() {
    let mut cfg = GeneratorConfig::default();
    cfg.param_ranges = vec![("RSI".to_string(), "period".to_string(), 5.0, 9.0)];
    let mut rng = StdRng::seed_from_u64(7);
    let mut found = 0;
    for _ in 0..300 {
        let s = random_strategy(&mut rng, "t", &cfg);
        for sig in &s.signals {
            for c in &sig.conditions {
                for op in [&c.left, &c.right] {
                    if let Operand::Indicator(name, params) = op {
                        if name == "RSI" {
                            let p = params
                                .iter()
                                .find(|p| p.name == "period")
                                .map(|p| p.value)
                                .unwrap_or(14.0);
                            assert!(p >= 5.0 && p <= 9.0, "RSI period {p} out of range");
                            found += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(found > 0, "no RSI conditions generated in 300 strategies");
}
