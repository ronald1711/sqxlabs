//! Synthetic arithmetic evidence for the provisional profile, not broker validation.
use sqx_backtest::{backtest, AccountConfig, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_strategy::{Comparator, Condition, Join, MoneyManagement, Operand, Signal, SignalKind, Strategy};

#[test]
fn eur_profile_cost_scenarios_match_hand_calculated_long_and_short_balances() {
    let profile: serde_json::Value = serde_json::from_str(include_str!("../../../examples/research-profiles/eurusd-eur-provisional.json")).unwrap();
    // Flat bid 1.1000; 0.1 lot = 10,000 EUR. Round-trip losses in USD are
    // 2, 3 and 6 respectively (stress includes one pip on EACH fill).
    // Convert at the closing execution price, then deduct EUR commission.
    let expected = [
        (1.1002, 1.1000, 1.1000, 1.1002, 2.0),
        (1.1003, 1.1000, 1.1000, 1.1003, 3.0),
        (1.1005, 1.0999, 1.0999, 1.1005, 6.0),
    ];
    assert_eq!(profile["sensitivity_scenarios"].as_array().unwrap().len(), expected.len());
    let bars: Vec<_> = (0..3).map(|i| Bar::new(i * 60000, 1.1, 1.1, 1.1, 1.1, 1.0)).collect();
    for (scenario, &(long_open, long_close, short_open, short_close, usd_loss)) in profile["sensitivity_scenarios"].as_array().unwrap().iter().zip(&expected) {
        for long in [true, false] {
            for commission in [0.0, 6.0] {
                let mut strategy = Strategy::new("EUR profile arithmetic");
                strategy.long = long;
                strategy.short = !long;
                strategy.money_management = MoneyManagement::FixedSize { size: 0.1 };
                strategy.signals.push(Signal::new(if long { SignalKind::LongEntry } else { SignalKind::ShortEntry }, vec![Condition::new(Operand::Const(1.0), Comparator::Gt, Operand::Const(0.0))], Join::And));
                let config = BacktestConfig {
                    initial_deposit: profile["research_assumptions"]["initial_deposit_eur"].as_f64().unwrap(),
                    spread_points: scenario["spread_fx_pips"].as_f64().unwrap(),
                    slippage_points: scenario["extra_slippage_fx_pips"].as_f64().unwrap(),
                    commission_per_lot: commission,
                    account: AccountConfig { currency: "EUR".into(), max_leverage: 30.0, ..Default::default() },
                    ..Default::default()
                };
                let result = backtest(&strategy, &bars, &Instrument::fx("EURUSD"), &config);
                assert!(result.errors.is_empty(), "{:?}", result.errors);
                assert_eq!(result.trades.len(), 1);
                let trade = &result.trades[0];
                let (open, close) = if long { (long_open, long_close) } else { (short_open, short_close) };
                assert!((trade.open_price - open).abs() < 1e-12);
                assert!((trade.close_price - close).abs() < 1e-12);
                assert_eq!(trade.open_time, 60000);
                assert_eq!(trade.close_time, 120000);
                let net = -usd_loss / close - commission * 0.1;
                assert!((trade.pnl - net).abs() < 1e-8);
                assert!((result.final_equity - (10000.0 + net)).abs() < 1e-8);
                assert_eq!(serde_json::to_vec(&result).unwrap(), serde_json::to_vec(&backtest(&strategy, &bars, &Instrument::fx("EURUSD"), &config)).unwrap());
            }
        }
    }
}
