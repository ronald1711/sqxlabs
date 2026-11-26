//! Integration regressions for the optional segmented walk-forward search gate:
//! the joint out-of-sample net-profit threshold, per-window validity/trade
//! evidence, status priority, archive fields and the fail-closed not-run gate.
use serde_json::json;
use sqx_backtest::BacktestConfig;
use sqx_core::{Bar, Instrument};
use sqx_optimizer::BatchEvaluation;
use sqx_pipeline::BuildEvaluation;
use sqx_project::search_requirements::{Requirements, WalkForward};
use sqx_strategy::{
    Comparator, Condition, ExitType, Join, MoneyManagement, Operand, Signal, SignalKind, Strategy,
};

const PYP: f64 = 525_600.0;

/// Long-only, always enters, exits after one bar, fixed 0.1 lot. On EURUSD/USD
/// one pip is worth $1, so every trade's net is the open-to-open pip delta.
fn always_long() -> Strategy {
    let mut strategy = Strategy::new("frozen walk-forward candidate");
    strategy.long = true;
    strategy.short = false;
    strategy.money_management = MoneyManagement::FixedSize { size: 0.1 };
    strategy.exits = vec![ExitType::ExitAfterBars(1)];
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

fn bars_with_deltas(deltas: &[f64]) -> Vec<Bar> {
    let mut open = 1.1000f64;
    deltas
        .iter()
        .enumerate()
        .map(|(i, &delta)| {
            let price = open;
            open += delta;
            Bar::new(i as i64 * 60_000, price, price, price, price, 1.0)
        })
        .collect()
}

/// 160 one-minute bars with K=2, fraction=.30 -> (0,56,80) and (80,136,160).
/// In-sample regions mix winners and losers (finite profit factor); window 0's
/// out-of-sample declines (negative net) while window 1's rises (positive net).
fn fixture_bars() -> Vec<Bar> {
    let mut deltas = vec![0.0f64; 160];
    for i in 0..160 {
        deltas[i] = match i {
            0..28 => 0.0010,   // IS0 rising
            28..56 => -0.0010, // IS0 falling
            56..80 => -0.0010, // OOS0 decline
            80..108 => 0.0010, // IS1 rising
            108..136 => -0.0010, // IS1 falling
            _ => 0.0010,       // OOS1 rise
        };
    }
    bars_with_deltas(&deltas)
}

fn build(strategy: &Strategy, bars: &[Bar]) -> BuildEvaluation {
    let execution = sqx_backtest::backtest(strategy, bars, &Instrument::fx("EURUSD"), &BacktestConfig::default());
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

/// Run the four basic gates plus the optional walk-forward gate, returning the
/// assessment as JSON (the shape the web layer and recovery view consume).
fn assess(bars: &[Bar], strategy: &Strategy, walk_forward: WalkForward, min_net_profit: f64) -> serde_json::Value {
    let build = build(strategy, bars);
    let requirements = Requirements {
        min_trades: 1,
        min_net_profit,
        min_profit_factor: 0.0,
        max_drawdown_pct: 1e12,
        spread_stress: None,
        monte_carlo: None,
        walk_forward: Some(walk_forward),
    };
    let assessments = requirements.assess_builds(
        std::slice::from_ref(&build),
        bars,
        &Instrument::fx("EURUSD"),
        &BacktestConfig::default(),
        PYP,
        42,
    );
    serde_json::to_value(&assessments[0]).unwrap()
}

fn walk_forward_spec() -> WalkForward {
    WalkForward {
        windows: 2,
        oos_fraction: 0.30,
        min_is_trades: 2,
        min_oos_trades_per_window: 2,
        min_total_oos_net_profit: 0.0,
    }
}

fn wf_check(assessment: &serde_json::Value) -> &serde_json::Value {
    &assessment["checks"].as_array().unwrap()[4]
}

#[test]
fn joint_threshold_passes_with_a_negative_window_and_rejects_above_it() {
    let bars = fixture_bars();
    let strategy = always_long();
    // A very low threshold definitely passes and exposes the realized totals.
    let passed = assess(&bars, &strategy, walk_forward_spec(), -1e9);
    assert_eq!(passed["passed"], true, "assessment: {passed}");
    assert_eq!(wf_check(&passed)["decision"], "passed");
    let detail = passed["walk_forward"].clone();
    assert_eq!(detail["method"], "fixed_candidate_segmented_v1");
    assert_eq!(detail["planned_windows"], 2);
    let windows = detail["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 2);
    let w0 = windows[0]["oos_net_profit"].as_f64().unwrap();
    let w1 = windows[1]["oos_net_profit"].as_f64().unwrap();
    // One window is genuinely negative yet the joint total still passes.
    assert!(w0 < 0.0, "window 0 out-of-sample net should be negative, got {w0}");
    assert!(w1 > 0.0, "window 1 out-of-sample net should be positive, got {w1}");
    let total = detail["oos_total_net_profit"].as_f64().unwrap();
    assert!((w0 + w1 - total).abs() < 1e-6, "joint total must be the window-ordered sum");
    assert_eq!(wf_check(&passed)["measured"].as_f64().unwrap(), total);
    assert_eq!(detail["min_total_oos_net_profit"].as_f64().unwrap(), 0.0);
    // The same evidence rejects only when the joint threshold exceeds the total.
    let mut spec = walk_forward_spec();
    spec.min_total_oos_net_profit = total + 0.01;
    let rejected = assess(&bars, &strategy, spec, -1e9);
    assert_eq!(wf_check(&rejected)["decision"], "rejected");
    assert_eq!(wf_check(&rejected)["measured"].as_f64().unwrap(), total);
    assert_eq!(rejected["passed"], false);
    // Equality at the threshold passes.
    let mut spec = walk_forward_spec();
    spec.min_total_oos_net_profit = total;
    let exact = assess(&bars, &strategy, spec, -1e9);
    assert_eq!(wf_check(&exact)["decision"], "passed");
}

#[test]
fn insufficient_oos_trades_is_insufficient_evidence() {
    let bars = fixture_bars();
    let strategy = always_long();
    let mut spec = walk_forward_spec();
    spec.min_oos_trades_per_window = 10_000;
    let assessment = assess(&bars, &strategy, spec, -1e9);
    assert_eq!(wf_check(&assessment)["decision"], "insufficient_evidence");
    assert_eq!(wf_check(&assessment)["measured"], json!(null));
    assert_eq!(assessment["passed"], false);
    // Available per-window measurements stay visible; no partial sum is reported.
    assert!(assessment["walk_forward"]["oos_total_net_profit"].is_null());
}

#[test]
fn missing_required_window_is_insufficient_evidence_and_never_selectable() {
    // Fewer than 100 bars is insufficient data: every planned window is absent.
    // The baseline keeps a finite profit factor so the mandatory gates pass first.
    let deltas: Vec<f64> = (0..60).map(|i| if i < 30 { 0.0010 } else { -0.0010 }).collect();
    let bars = bars_with_deltas(&deltas);
    let strategy = always_long();
    let assessment = assess(&bars, &strategy, walk_forward_spec(), -1e9);
    assert_eq!(wf_check(&assessment)["decision"], "insufficient_evidence");
    assert_eq!(wf_check(&assessment)["measured"], json!(null));
    assert_eq!(assessment["passed"], false);
    // All planned windows are archived with an explicit reason, never as zero profit.
    let windows = assessment["walk_forward"]["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 2);
    assert!(windows.iter().all(|window| window["oos_net_profit"].is_null()));
    assert!(windows.iter().all(|window| window["reason"].as_str().is_some()));
}

#[test]
fn failing_prior_gate_leaves_walk_forward_not_run() {
    let bars = fixture_bars();
    let strategy = always_long();
    // A mandatory basic gate fails first, so the walk-forward gate must not run.
    let assessment = assess(&bars, &strategy, walk_forward_spec(), 1e12);
    assert_eq!(wf_check(&assessment)["decision"], "not_run");
    assert_eq!(wf_check(&assessment)["measured"], json!(null));
    assert_eq!(assessment["passed"], false);
    assert!(assessment["walk_forward"].is_null(), "no walk-forward archive when the gate never ran");
}

#[test]
fn archive_records_inclusive_bar_times_and_capital() {
    let bars = fixture_bars();
    let strategy = always_long();
    let passed = assess(&bars, &strategy, walk_forward_spec(), -1e9);
    let detail = &passed["walk_forward"];
    assert_eq!(detail["allowed_start_index"], 0);
    assert_eq!(detail["allowed_end_index"], 160);
    assert_eq!(detail["allowed_start_time"], 0);
    assert_eq!(detail["allowed_end_time"], 159 * 60_000);
    // window_len=80, fraction=.30 -> (0,56,80) and (80,136,160).
    let windows = detail["windows"].as_array().unwrap();
    assert_eq!(windows[0]["is_start"], 0);
    assert_eq!(windows[0]["is_end"], 56);
    assert_eq!(windows[0]["oos_start"], 56);
    assert_eq!(windows[0]["oos_end"], 80);
    assert_eq!(windows[0]["is_first_bar_time"], 0);
    assert_eq!(windows[0]["is_last_bar_time"], 55 * 60_000);
    assert_eq!(windows[0]["oos_first_bar_time"], 56 * 60_000);
    assert_eq!(windows[0]["oos_last_bar_time"], 79 * 60_000);
    assert_eq!(windows[1]["is_start"], 80);
    assert_eq!(windows[1]["is_end"], 136);
    assert_eq!(windows[1]["oos_start"], 136);
    assert_eq!(windows[1]["oos_end"], 160);
    // Capital carries from the first window's out-of-sample ending equity.
    assert_eq!(windows[0]["starting_equity"].as_f64().unwrap(), 10_000.0);
    assert_eq!(
        windows[1]["starting_equity"].as_f64().unwrap(),
        windows[0]["oos_final_equity"].as_f64().unwrap()
    );
    // Both windows are eligible, sufficiently traded and individually passed;
    // each exposes its in-sample trades, profit factor and net profit.
    for window in windows {
        assert_eq!(window["decision"], "passed");
        assert!(window["is_eligible"].as_bool().unwrap());
        assert!(window["is_trades"].is_number());
        assert!(window["is_net_profit"].is_number());
        assert!(window["oos_trades"].as_u64().unwrap() >= 2);
    }
}

#[test]
fn short_data_archives_missing_windows_without_panicking() {
    // Ten bars over ten windows makes the split round onto the window end and
    // the last window's out-of-sample start equal bars.len() — an out-of-bounds
    // bar index under the old code. Fewer bars than windows makes the planned
    // split zero (the split-1 underflow). The optimizer rejects such sizes, so
    // the archive must record missing windows with null bounds/times and a
    // preserved diagnosis, never panic.
    let strategy = always_long();
    for bar_count in [5usize, 10] {
        let deltas: Vec<f64> = (0..bar_count)
            .map(|i| if i % 2 == 0 { 0.0010 } else { -0.0010 })
            .collect();
        let bars = bars_with_deltas(&deltas);
        let spec = WalkForward {
            windows: 10,
            oos_fraction: 0.30,
            min_is_trades: 2,
            min_oos_trades_per_window: 2,
            min_total_oos_net_profit: 0.0,
        };
        let assessment = assess(&bars, &strategy, spec, -1e9);
        assert_eq!(wf_check(&assessment)["decision"], "insufficient_evidence");
        assert_eq!(assessment["passed"], false);
        let windows = assessment["walk_forward"]["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 10);
        for window in windows {
            assert!(window["is_start"].is_null(), "window bounds must be missing, not fabricated: {window}");
            assert!(window["is_end"].is_null());
            assert!(window["oos_start"].is_null());
            assert!(window["oos_end"].is_null());
            assert!(window["is_first_bar_time"].is_null());
            assert!(window["is_last_bar_time"].is_null());
            assert!(window["oos_first_bar_time"].is_null());
            assert!(window["oos_last_bar_time"].is_null());
            assert_eq!(window["decision"], "insufficient_evidence");
            assert!(window["reason"].as_str().unwrap().contains("invalid walk-forward sizes"));
        }
        // The optimizer's diagnosis is preserved at both the assessment and the
        // study level, never silently dropped from the saved errors.
        let detail_errors = assessment["walk_forward"]["errors"].as_array().unwrap();
        assert!(detail_errors.iter().any(|e| e.as_str().unwrap().contains("invalid walk-forward sizes")));
        let study_errors = assessment["errors"].as_array().unwrap();
        assert!(study_errors.iter().any(|e| e.as_str().unwrap().contains("invalid walk-forward sizes")));
    }
}

#[test]
fn too_short_windows_keep_concrete_bounds_with_an_explicit_reason() {
    // 120 bars over 2 windows yields a 60-bar window whose 42-bar in-sample
    // half is too short (split 0..42 < 50). The sizes are otherwise valid, so
    // each missing window keeps its concrete bounds and reports "too short",
    // unlike the null bounds reserved for invalid sizes.
    let deltas: Vec<f64> = (0..120).map(|i| if i < 60 { 0.0010 } else { -0.0010 }).collect();
    let bars = bars_with_deltas(&deltas);
    let strategy = always_long();
    let spec = WalkForward {
        windows: 2,
        oos_fraction: 0.30,
        min_is_trades: 2,
        min_oos_trades_per_window: 2,
        min_total_oos_net_profit: 0.0,
    };
    let assessment = assess(&bars, &strategy, spec, -1e9);
    assert_eq!(wf_check(&assessment)["decision"], "insufficient_evidence");
    let windows = assessment["walk_forward"]["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 2);
    for (i, window) in windows.iter().enumerate() {
        let start = i * 60;
        assert_eq!(window["is_start"].as_u64().unwrap() as usize, start);
        assert_eq!(window["is_end"].as_u64().unwrap() as usize, start + 42);
        assert_eq!(window["oos_start"].as_u64().unwrap() as usize, start + 42);
        assert_eq!(window["oos_end"].as_u64().unwrap() as usize, start + 60);
        assert_eq!(window["decision"], "insufficient_evidence");
        assert!(window["reason"].as_str().unwrap().contains("too short"), "{window}");
        // Concrete bounds also carry their inclusive bar times.
        assert!(window["is_first_bar_time"].is_number());
        assert!(window["is_last_bar_time"].is_number());
        assert!(window["oos_first_bar_time"].is_number());
        assert!(window["oos_last_bar_time"].is_number());
    }
}
