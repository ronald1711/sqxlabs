//! Regression oracles for the optional segmented walk-forward gate.
//!
//! Frozen fixture: one candidate, two windows of 80 one-minute bars with a 30%
//! out-of-sample tail (split at 56 and 136). Each in-sample run is eligible and
//! each out-of-sample window trades exactly twice. Starting capital is 1,000 and
//! the window-ordered out-of-sample nets are -20 and +70, so the carried capital
//! runs 1,000 -> 980 -> 1,050 and the joint total is 50. The single joint
//! threshold (50 passes, 51 rejects) is the only profit requirement; a negative
//! but valid window is therefore not a rejection on its own.
use sqx_backtest::BacktestConfig;
use sqx_core::{Bar, Instrument};
use sqx_optimizer::BatchEvaluation;
use sqx_pipeline::BuildEvaluation;
use sqx_project::search_requirements::{Requirements, WalkForward};
use sqx_strategy::{
    Comparator, Condition, ExitType, Join, MoneyManagement, Operand, Signal, SignalKind, Strategy,
};

const PYP: f64 = 525_600.0;

/// Long-only, always enters, exits after twelve bars, fixed 0.1 lot. On a
/// USD-quoted EURUSD pair with 0.1 lot one pip is worth $1, so a window's net is
/// its captured price movement in pips.
fn frozen_candidate() -> Strategy {
    let mut strategy = Strategy::new("frozen walk-forward oracle candidate");
    strategy.long = true;
    strategy.short = false;
    strategy.money_management = MoneyManagement::FixedSize { size: 0.1 };
    strategy.exits = vec![ExitType::ExitAfterBars(12)];
    strategy.signals.push(Signal::new(
        SignalKind::LongEntry,
        vec![Condition::new(
            Operand::Const(1.0),
            Comparator::Gt,
            Operand::Const(0.0),
        )],
        Join::And,
    ));
    strategy
}

fn initial_deposit() -> f64 {
    1_000.0
}

fn config() -> BacktestConfig {
    BacktestConfig {
        initial_deposit: initial_deposit(),
        ..BacktestConfig::default()
    }
}

/// Build 160 flat candles from a piecewise-constant price plan. A step at index
/// `i` takes effect for every candle from `i` on, so a trade's result is the sum
/// of the steps it spans.
fn bars_from_steps(steps: &[(usize, f64)]) -> Vec<Bar> {
    let mut bars = Vec::with_capacity(160);
    let mut price = 1.1000f64;
    let mut next = 0;
    for index in 0..160 {
        while next < steps.len() && steps[next].0 <= index {
            price = steps[next].1;
            next += 1;
        }
        bars.push(Bar::new(index as i64 * 60_000, price, price, price, price, 1.0));
    }
    bars
}

/// Two eligible in-sample runs (mixed winners and losers), a declining first
/// out-of-sample window (-20) and a rising second one (+70), two trades each.
fn oracle_steps() -> Vec<(usize, f64)> {
    vec![
        // In-sample window 0: +10, -20, +10, -20, +20 pips over five trades.
        (7, 1.1010),
        (20, 1.0990),
        (33, 1.1000),
        (46, 1.0980),
        (54, 1.1000),
        // Out-of-sample window 0: -10 then -10 pips over two trades. The first
        // out-of-sample fill happens on the bar at the split, so the step is at 56.
        (56, 1.1013),
        (63, 1.1003),
        (76, 1.0993),
        // In-sample window 1: +10, -20, +10, -20, +26 pips over five trades.
        (87, 1.1003),
        (100, 1.0983),
        (113, 1.0993),
        (126, 1.0973),
        (134, 1.0999),
        // Out-of-sample window 1: +50 then +20 pips over two trades.
        (143, 1.1049),
        (154, 1.1069),
    ]
}

/// Same fixture, but the second in-sample run wins every trade, so its profit
/// factor is not finite and no candidate is eligible there.
fn steps_with_only_winning_second_is() -> Vec<(usize, f64)> {
    vec![
        (7, 1.1010),
        (20, 1.0990),
        (33, 1.1000),
        (46, 1.0980),
        (54, 1.1000),
        (56, 1.1013),
        (63, 1.1003),
        (76, 1.0993),
        (87, 1.1003),
        (100, 1.1013),
        (113, 1.1023),
        (126, 1.1033),
        (134, 1.1043),
        (143, 1.1093),
        (154, 1.1113),
    ]
}

fn build(strategy: &Strategy, bars: &[Bar]) -> BuildEvaluation {
    let execution = sqx_backtest::backtest(strategy, bars, &Instrument::fx("EURUSD"), &config());
    let metrics = sqx_metrics::compute(&execution, bars.len(), PYP);
    BuildEvaluation {
        execution,
        step: 0,
        candidate_index: 0,
        selected: false,
        evaluation: BatchEvaluation {
            strategy: strategy.clone(),
            metrics,
            errors: Vec::new(),
        },
    }
}

fn spec(min_total_oos_net_profit: f64) -> WalkForward {
    WalkForward {
        windows: 2,
        oos_fraction: 0.30,
        min_is_trades: 2,
        min_oos_trades_per_window: 2,
        min_total_oos_net_profit,
    }
}

fn requirements(walk_forward: Option<WalkForward>) -> Requirements {
    Requirements {
        min_trades: 1,
        min_net_profit: -1e9,
        min_profit_factor: 0.0,
        max_drawdown_pct: 1e12,
        spread_stress: None,
        monte_carlo: None,
        walk_forward,
    }
}

fn assess(bars: &[Bar], strategy: &Strategy, walk_forward: Option<WalkForward>) -> serde_json::Value {
    let build = build(strategy, bars);
    let assessments = requirements(walk_forward).assess_builds(
        std::slice::from_ref(&build),
        bars,
        &Instrument::fx("EURUSD"),
        &config(),
        PYP,
        42,
    );
    serde_json::to_value(&assessments[0]).unwrap()
}

fn walk_forward_check(assessment: &serde_json::Value) -> &serde_json::Value {
    assessment["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["metric"] == "walk_forward_total_oos_net_profit")
        .expect("walk-forward check present when the option is enabled")
}

fn near(value: Option<f64>, expected: f64) -> bool {
    value.is_some_and(|value| (value - expected).abs() < 1e-6)
}

#[test]
fn joint_threshold_of_fifty_passes_and_fifty_one_rejects_with_a_negative_window() {
    let bars = bars_from_steps(&oracle_steps());
    let strategy = frozen_candidate();

    let passed = assess(&bars, &strategy, Some(spec(50.0)));
    assert_eq!(passed["passed"], true, "assessment: {passed}");
    assert_eq!(walk_forward_check(&passed)["decision"], "passed");
    assert!(near(walk_forward_check(&passed)["measured"].as_f64(), 50.0));
    assert!(near(walk_forward_check(&passed)["threshold"].as_f64(), 50.0));

    let detail = &passed["walk_forward"];
    assert_eq!(detail["method"], "fixed_candidate_segmented_v1");
    assert!(near(detail["oos_total_net_profit"].as_f64(), 50.0));
    assert!(near(detail["min_total_oos_net_profit"].as_f64(), 50.0));
    // 160 bars, 2 windows, 30% out-of-sample: [0,56)+[56,80) and [80,136)+[136,160).
    let windows = detail["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0]["oos_start"], 56);
    assert_eq!(windows[0]["oos_end"], 80);
    assert_eq!(windows[1]["oos_start"], 136);
    assert_eq!(windows[1]["oos_end"], 160);
    // Exactly two out-of-sample trades per window, each window individually valid.
    assert_eq!(windows[0]["oos_trades"], 2);
    assert_eq!(windows[1]["oos_trades"], 2);
    assert_eq!(windows[0]["decision"], "passed");
    assert_eq!(windows[1]["decision"], "passed");
    // The first window genuinely loses yet the joint total clears the threshold.
    assert!(near(windows[0]["oos_net_profit"].as_f64(), -20.0));
    assert!(near(windows[1]["oos_net_profit"].as_f64(), 70.0));
    // Carried capital: 1,000 -> 980 -> 1,050, reconciled against the joint total.
    assert!(near(windows[0]["starting_equity"].as_f64(), 1_000.0));
    assert!(near(windows[0]["oos_final_equity"].as_f64(), 980.0));
    assert!(near(windows[1]["starting_equity"].as_f64(), 980.0));
    assert!(near(windows[1]["oos_final_equity"].as_f64(), 1_050.0));
    let total = windows[0]["oos_net_profit"].as_f64().unwrap()
        + windows[1]["oos_net_profit"].as_f64().unwrap();
    assert!(near(Some(total), 50.0));

    // One unit more on the same evidence is rejected; the measurement stays visible.
    let rejected = assess(&bars, &strategy, Some(spec(51.0)));
    assert_eq!(walk_forward_check(&rejected)["decision"], "rejected");
    assert!(near(walk_forward_check(&rejected)["measured"].as_f64(), 50.0));
    assert!(near(walk_forward_check(&rejected)["threshold"].as_f64(), 51.0));
    assert_eq!(rejected["passed"], false);
    let rejected_windows = &rejected["walk_forward"]["windows"];
    assert!(near(rejected_windows[0]["oos_net_profit"].as_f64(), -20.0));
    assert!(near(rejected_windows[1]["oos_net_profit"].as_f64(), 70.0));
}

#[test]
fn one_window_without_an_eligible_run_is_insufficient_evidence_and_hides_the_total() {
    // Threshold far below the realized total: the joint result is not the cause.
    let bars = bars_from_steps(&oracle_steps());
    let baseline = assess(&bars, &frozen_candidate(), Some(spec(-1e9)));
    assert_eq!(walk_forward_check(&baseline)["decision"], "passed");

    // The second in-sample run has no losing trade, so its profit factor is not
    // finite and no run is selected there: that window's outcome is missing.
    let bars = bars_from_steps(&steps_with_only_winning_second_is());
    let assessment = assess(&bars, &frozen_candidate(), Some(spec(-1e9)));
    assert_eq!(walk_forward_check(&assessment)["decision"], "insufficient_evidence");
    assert_eq!(walk_forward_check(&assessment)["measured"], serde_json::Value::Null);
    assert_eq!(assessment["passed"], false);
    assert!(assessment["walk_forward"]["oos_total_net_profit"].is_null());
    let windows = assessment["walk_forward"]["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 2);
    // The first window's evidence is intact and stays visible...
    assert_eq!(windows[0]["decision"], "passed");
    assert!(near(windows[0]["oos_net_profit"].as_f64(), -20.0));
    // ...while the second window has no selected run and therefore no outcome.
    assert_eq!(windows[1]["decision"], "insufficient_evidence");
    assert_eq!(windows[1]["is_eligible"], false);
    assert_eq!(windows[1]["is_exclusion_reason"], "nonfinite_profit_factor");
    assert!(windows[1]["oos_net_profit"].is_null());
    assert_eq!(windows[1]["reason"], "nonfinite_profit_factor");
    assert_eq!(windows[1]["oos_decision"], "not_run");
}

#[test]
fn lone_negative_window_below_the_joint_threshold_still_rejects() {
    let mut steps = oracle_steps();
    // Deepen the first window's decline: -60 + 70 = 10, below the 50 threshold.
    steps[6] = (63, 1.0973);
    steps[7] = (76, 1.0953);
    let bars = bars_from_steps(&steps);
    let assessment = assess(&bars, &frozen_candidate(), Some(spec(50.0)));
    assert_eq!(walk_forward_check(&assessment)["decision"], "rejected");
    assert!(near(walk_forward_check(&assessment)["measured"].as_f64(), 10.0));
    let windows = assessment["walk_forward"]["windows"].as_array().unwrap();
    assert_eq!(windows[0]["decision"], "passed");
    assert_eq!(windows[1]["decision"], "passed");
}

#[test]
fn disabled_option_keeps_the_legacy_assessment_shape() {
    // Studies recorded before the option existed must stay readable: no extra
    // check and no walk-forward archive appear for them.
    let bars = bars_from_steps(&oracle_steps());
    let assessment = assess(&bars, &frozen_candidate(), None);
    assert!(assessment.get("walk_forward").is_none());
    let metrics: Vec<&str> = assessment["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|check| check["metric"].as_str().unwrap())
        .collect();
    assert_eq!(metrics, vec!["trades", "net_profit", "profit_factor", "drawdown_pct"]);
    assert_eq!(assessment["passed"], true);
}
