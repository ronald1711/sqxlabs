//! Explicit gates for exploratory search results.
use serde::{Deserialize, Serialize};
use sqx_core::Bar;
use sqx_metrics::Metrics;
use sqx_strategy::Strategy;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirements {
    pub min_trades: u32,
    pub min_net_profit: f64,
    pub min_profit_factor: f64,
    pub max_drawdown_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spread_stress: Option<SpreadStress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monte_carlo: Option<MonteCarlo>,
    /// Optional segmented walk-forward stability check over the search data.
    /// Absent means the check is not configured; old requests stay byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walk_forward: Option<WalkForward>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpreadStress {
    pub extra_spread: f64,
    pub max_profit_drop: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonteCarlo {
    pub runs: usize,
    pub skip_probability: f64,
    pub min_positive_pct: f64,
}

/// Frozen segmented walk-forward configuration for one candidate. Every planned
/// window must independently be present, technically valid and sufficiently
/// traded; the single net-profit threshold applies to the joint out-of-sample
/// total, never to an individual window.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalkForward {
    pub windows: usize,
    pub oos_fraction: f64,
    pub min_is_trades: usize,
    pub min_oos_trades_per_window: usize,
    /// Threshold on the joint out-of-sample net profit across all windows.
    pub min_total_oos_net_profit: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Passed,
    Rejected,
    InsufficientEvidence,
    TechnicalError,
    NotRun,
}

#[derive(Debug, Serialize)]
pub(super) struct Check {
    metric: &'static str,
    measured: Option<f64>,
    threshold: f64,
    comparison: &'static str,
    pub decision: Decision,
}

/// Archive of a completed segmented walk-forward check for one candidate. This
/// is a stability check over already-seen search data, never an independent
/// final test. Per-window rows record both step decisions and the window
/// decision so a replay can reproduce the exact selection boundary.
#[derive(Debug, Clone, Serialize)]
pub struct WalkForwardAssessment {
    pub method: &'static str,
    pub version: u32,
    pub scope: &'static str,
    pub candidate_index: usize,
    pub strategy_name: String,
    pub frozen_specification: WalkForward,
    pub allowed_start_index: usize,
    pub allowed_end_index: usize,
    pub allowed_start_time: i64,
    pub allowed_end_time: i64,
    pub planned_windows: usize,
    /// Joint out-of-sample net-profit threshold (account currency).
    pub min_total_oos_net_profit: f64,
    /// Joint out-of-sample net profit summed in window order; null until every
    /// planned window has fully comparable evidence.
    pub oos_total_net_profit: Option<f64>,
    pub windows: Vec<WalkForwardWindowAssessment>,
}

/// Half-open index bounds `[is_start, is_end)` and `[oos_start, oos_end)`.
/// Bar times are recorded as explicit inclusive first/last timestamps.
#[derive(Debug, Clone, Serialize)]
pub struct WalkForwardWindowAssessment {
    pub index: usize,
    pub is_start: usize,
    pub is_end: usize,
    pub is_first_bar_time: i64,
    pub is_last_bar_time: i64,
    pub oos_start: usize,
    pub oos_end: usize,
    pub oos_first_bar_time: i64,
    pub oos_last_bar_time: i64,
    pub starting_equity: f64,
    pub oos_final_equity: Option<f64>,
    pub is_trades: Option<f64>,
    pub is_net_profit: Option<f64>,
    pub is_profit_factor: Option<f64>,
    pub is_eligible: bool,
    pub is_exclusion_reason: Option<String>,
    pub oos_trades: Option<usize>,
    pub oos_net_profit: Option<f64>,
    pub thresholds: WalkForwardThresholds,
    pub is_decision: Decision,
    pub oos_decision: Decision,
    pub decision: Decision,
    pub reason: String,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalkForwardThresholds {
    pub min_is_trades: usize,
    pub min_oos_trades_per_window: usize,
    pub comparison: &'static str,
}

impl Requirements {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.min_trades == 0
            || !self.min_net_profit.is_finite()
            || !self.min_profit_factor.is_finite()
            || self.min_profit_factor < 0.0
            || !self.max_drawdown_pct.is_finite()
            || self.max_drawdown_pct < 0.0
            || self.monte_carlo.as_ref().is_some_and(|test| {
                !(20..=500).contains(&test.runs)
                    || !(0.0..=1.0).contains(&test.skip_probability)
                    || !(0.0 < test.min_positive_pct && test.min_positive_pct <= 100.0)
            })
            || self.spread_stress.as_ref().is_some_and(|stress| {
                !stress.extra_spread.is_finite()
                    || stress.extra_spread <= 0.0
                    || !stress.max_profit_drop.is_finite()
                    || stress.max_profit_drop < 0.0
            })
            || self.walk_forward.as_ref().is_some_and(|wf| {
                !(2..=10).contains(&wf.windows)
                    || !wf.oos_fraction.is_finite()
                    || !(0.05..=0.5).contains(&wf.oos_fraction)
                    || wf.min_is_trades == 0
                    || wf.min_oos_trades_per_window == 0
                    || !wf.min_total_oos_net_profit.is_finite()
            })
        {
            return Err("Invalid search requirements");
        }
        Ok(())
    }

    pub(crate) fn assess(&self, metrics: &Metrics, execution_failed: bool) -> [Check; 4] {
        let assess = |metric, value: f64, threshold, minimum| {
            let passed = if minimum {
                value >= threshold
            } else {
                value <= threshold
            };
            let decision = if execution_failed {
                Decision::TechnicalError
            } else if !value.is_finite() {
                Decision::InsufficientEvidence
            } else if passed {
                Decision::Passed
            } else if metric == "trades" {
                Decision::InsufficientEvidence
            } else {
                Decision::Rejected
            };
            Check {
                metric,
                measured: (!execution_failed && value.is_finite()).then_some(value),
                threshold,
                comparison: if minimum { "at_least" } else { "at_most" },
                decision,
            }
        };
        [
            assess(
                "trades",
                metrics.number_of_trades,
                f64::from(self.min_trades),
                true,
            ),
            assess("net_profit", metrics.net_profit, self.min_net_profit, true),
            assess(
                "profit_factor",
                metrics.profit_factor,
                self.min_profit_factor,
                true,
            ),
            assess(
                "drawdown_pct",
                metrics.max_drawdown_pct,
                self.max_drawdown_pct,
                false,
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strong_metrics_do_not_compensate_for_missing_evidence_or_failed_gates() {
        let requirements = Requirements {
            min_trades: 20,
            min_net_profit: 0.0,
            min_profit_factor: 1.0,
            max_drawdown_pct: 5.0,
            spread_stress: None,
            monte_carlo: None,
            walk_forward: None,
        };
        assert!(requirements.validate().is_ok());
        let metrics = Metrics {
            number_of_trades: 19.0,
            net_profit: 200.0,
            profit_factor: f64::NAN,
            max_drawdown_pct: 6.0,
            ..Metrics::default()
        };
        let checks = requirements.assess(&metrics, false);
        assert_eq!(
            checks.map(|check| check.decision),
            [
                Decision::InsufficientEvidence,
                Decision::Passed,
                Decision::InsufficientEvidence,
                Decision::Rejected,
            ]
        );
        assert!(requirements
            .assess(&metrics, true)
            .iter()
            .all(|check| check.decision == Decision::TechnicalError && check.measured.is_none()));
        let at_threshold = Metrics {
            number_of_trades: 20.0,
            net_profit: 0.0,
            profit_factor: 1.0,
            max_drawdown_pct: 5.0,
            ..Metrics::default()
        };
        assert!(requirements
            .assess(&at_threshold, false)
            .iter()
            .all(|check| check.decision == Decision::Passed));
    }
    #[test]
    fn one_extra_pip_costs_one_dollar_at_one_tenth_lot() {
        use sqx_core::{Bar, Instrument};
        use sqx_optimizer::BatchEvaluation;
        use sqx_pipeline::BuildEvaluation;
        use sqx_strategy::{
            Comparator, Condition, Join, MoneyManagement, Operand, Signal, SignalKind, Strategy,
        };

        let bars = [
            Bar::new(0, 1.1000, 1.1000, 1.1000, 1.1000, 1.0),
            Bar::new(60_000, 1.1000, 1.1000, 1.1000, 1.1000, 1.0),
            Bar::new(120_000, 1.1010, 1.1010, 1.1010, 1.1010, 1.0),
        ];
        let instrument = Instrument::fx("EURUSD");
        let config = sqx_backtest::BacktestConfig::default();
        let mut strategy = Strategy::new("One round trip");
        strategy.long = true;
        strategy.short = false;
        strategy.money_management = MoneyManagement::FixedSize { size: 0.1 };
        strategy.signals.push(Signal::new(
            SignalKind::LongEntry,
            vec![Condition::new(
                Operand::Const(1.0),
                Comparator::Gt,
                Operand::Const(0.0),
            )],
            Join::And,
        ));
        let execution = sqx_backtest::backtest(&strategy, &bars, &instrument, &config);
        assert!(execution.errors.is_empty(), "{:?}", execution.errors);
        assert_eq!(execution.trades.len(), 1);
        // The baseline buys at 1.1000 and sells at 1.1010 for $10.
        // One extra pip costs 0.0001 × 100,000 × 0.1 = $1.
        let build = BuildEvaluation {
            execution,
            step: 0,
            candidate_index: 0,
            selected: true,
            evaluation: BatchEvaluation {
                strategy,
                errors: vec![],
                // Supply eligible baseline metrics to isolate the spread gate.
                metrics: Metrics {
                    number_of_trades: 1.0,
                    net_profit: 10.0,
                    profit_factor: 2.0,
                    ..Metrics::default()
                },
            },
        };
        let mut requirements = Requirements {
            min_trades: 1,
            min_net_profit: 0.0,
            min_profit_factor: 1.0,
            max_drawdown_pct: 10.0,
            monte_carlo: None,
            spread_stress: Some(SpreadStress {
                extra_spread: 1.0,
                max_profit_drop: 0.5,
            }),
            walk_forward: None,
        };
        let assess = |requirements: &Requirements| {
            requirements.assess_builds(
                std::slice::from_ref(&build),
                &bars,
                &instrument,
                &config,
                252.0,
                42,
            )
        };
        let rejected = assess(&requirements);
        assert!(!rejected[0].passed);
        let stress = &rejected[0].checks[4];
        assert_eq!(stress.decision, Decision::Rejected);
        assert!((stress.measured.unwrap() - 1.0).abs() < 1e-8);
        requirements.spread_stress.as_mut().unwrap().max_profit_drop = 1.5;
        assert!(assess(&requirements)[0].passed);
        requirements.monte_carlo = Some(MonteCarlo {
            runs: 20,
            skip_probability: 1.0,
            min_positive_pct: 80.0,
        });
        let skipped = assess(&requirements);
        assert_eq!(skipped[0].checks[5].measured, Some(0.0));
        assert!(!skipped[0].passed);
        requirements.monte_carlo.as_mut().unwrap().skip_probability = 0.0;
        let retained = assess(&requirements);
        assert_eq!(retained[0].checks[5].measured, Some(100.0));
        assert!(retained[0].passed);
    }

    #[test]
    fn split_oracle_matches_architect_example() {
        // N=307, K=3, fraction=.30 -> (0,71,102),(102,173,204),(204,276,307).
        assert_eq!(
            plan_windows(307, 3, 0.30),
            vec![(0, 71, 102), (102, 173, 204), (204, 276, 307)]
        );
    }

    #[test]
    fn walk_forward_configuration_is_validated_before_start() {
        let valid = WalkForward {
            windows: 3,
            oos_fraction: 0.30,
            min_is_trades: 10,
            min_oos_trades_per_window: 10,
            min_total_oos_net_profit: 0.0,
        };
        let base = Requirements {
            min_trades: 10,
            min_net_profit: 0.0,
            min_profit_factor: 1.0,
            max_drawdown_pct: 10.0,
            spread_stress: None,
            monte_carlo: None,
            walk_forward: None,
        };
        let with = |walk_forward: Option<WalkForward>| Requirements {
            walk_forward,
            ..base.clone()
        };
        assert!(with(Some(valid.clone())).validate().is_ok());
        for invalid in [
            WalkForward { windows: 1, ..valid.clone() },
            WalkForward { windows: 11, ..valid.clone() },
            WalkForward { oos_fraction: 0.04, ..valid.clone() },
            WalkForward { oos_fraction: 0.51, ..valid.clone() },
            WalkForward { oos_fraction: f64::NAN, ..valid.clone() },
            WalkForward { min_is_trades: 0, ..valid.clone() },
            WalkForward { min_oos_trades_per_window: 0, ..valid.clone() },
            WalkForward { min_total_oos_net_profit: f64::NAN, ..valid.clone() },
            WalkForward { min_total_oos_net_profit: f64::INFINITY, ..valid.clone() },
        ] {
            assert!(with(Some(invalid)).validate().is_err(), "invalid config accepted");
        }
        // Unknown fields are rejected before start.
        let unknown = json!({
            "min_trades": 10, "min_net_profit": 0.0, "min_profit_factor": 1.0,
            "max_drawdown_pct": 10.0, "walk_forward": {
                "windows": 3, "oos_fraction": 0.30, "min_is_trades": 10,
                "min_oos_trades_per_window": 10, "min_total_oos_net_profit": 0.0,
                "unexpected": true
            }
        });
        assert!(serde_json::from_value::<Requirements>(unknown).is_err());
    }

    #[test]
    fn absent_walk_forward_serializes_byte_identically() {
        let requirements = Requirements {
            min_trades: 10,
            min_net_profit: 0.0,
            min_profit_factor: 1.0,
            max_drawdown_pct: 10.0,
            spread_stress: None,
            monte_carlo: None,
            walk_forward: None,
        };
        let json = serde_json::to_string(&requirements).unwrap();
        assert!(!json.contains("walk_forward"), "absent option must not serialize a field");
        // A legacy request without the field still parses and stays equivalent.
        let legacy = json!({
            "min_trades": 10, "min_net_profit": 0.0, "min_profit_factor": 1.0,
            "max_drawdown_pct": 10.0
        });
        let parsed: Requirements = serde_json::from_value(legacy).unwrap();
        assert!(parsed.walk_forward.is_none());
        assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
    }

    #[test]
    fn status_priority_orders_technical_then_insufficient_then_rejected_then_passed() {
        assert_eq!(worse(Decision::InsufficientEvidence, Decision::TechnicalError), Decision::TechnicalError);
        assert_eq!(worse(Decision::Rejected, Decision::InsufficientEvidence), Decision::InsufficientEvidence);
        assert_eq!(worse(Decision::Passed, Decision::Rejected), Decision::Rejected);
        assert_eq!(worse(Decision::Passed, Decision::Passed), Decision::Passed);
        assert_eq!(classify_report_error("invalid walk-forward sizes, OOS fraction or evidence thresholds"), Decision::InsufficientEvidence);
        assert_eq!(classify_report_error("account exhausted; remaining windows not traded"), Decision::InsufficientEvidence);
        assert_eq!(classify_report_error("window 1 is too short"), Decision::InsufficientEvidence);
        assert_eq!(classify_report_error("some engine failure"), Decision::TechnicalError);
    }
}

#[derive(Debug, Serialize)]
pub struct Assessment {
    pub candidate_index: usize,
    name: String,
    pub passed: bool,
    checks: Vec<Check>,
    errors: Vec<String>,
    /// Present only when the walk-forward check actually ran; a `not_run`
    /// decision from an earlier failing gate leaves this absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub walk_forward: Option<WalkForwardAssessment>,
}

impl Requirements {
    pub fn assess_builds(
        &self,
        builds: &[sqx_pipeline::BuildEvaluation],
        bars: &[sqx_core::Bar],
        instrument: &sqx_core::Instrument,
        config: &sqx_backtest::BacktestConfig,
        periods_per_year: f64,
        seed: u64,
    ) -> Vec<Assessment> {
        let stress_config = self.spread_stress.as_ref().map(|stress| {
            let mut config = config.clone();
            config.spread_points += stress.extra_spread;
            config
        });
        builds
            .iter()
            .map(|row| {
                let mut checks = self
                    .assess(&row.evaluation.metrics, !row.evaluation.errors.is_empty())
                    .into_iter()
                    .collect::<Vec<_>>();
                let mut errors = row.evaluation.errors.clone();
                if let Some((stress, config)) =
                    self.spread_stress.as_ref().zip(stress_config.as_ref())
                {
                    let mut check = Check {
                        metric: "spread_profit_drop",
                        measured: None,
                        threshold: stress.max_profit_drop,
                        comparison: "at_most",
                        decision: Decision::NotRun,
                    };
                    if checks
                        .iter()
                        .all(|check| check.decision == Decision::Passed)
                    {
                        let execution = sqx_backtest::backtest(
                            &row.evaluation.strategy,
                            bars,
                            instrument,
                            config,
                        );
                        check.decision = if !execution.errors.is_empty() {
                            errors.extend(execution.errors);
                            Decision::TechnicalError
                        } else if let Err(error) = sqx_metrics::validate_observations(&execution) {
                            errors.push(error);
                            Decision::TechnicalError
                        } else {
                            let metrics =
                                sqx_metrics::compute(&execution, bars.len(), periods_per_year);
                            let drop = row.evaluation.metrics.net_profit - metrics.net_profit;
                            check.measured = drop.is_finite().then_some(drop);
                            if !drop.is_finite()
                                || metrics.number_of_trades < f64::from(self.min_trades)
                            {
                                Decision::InsufficientEvidence
                            } else if drop <= stress.max_profit_drop {
                                Decision::Passed
                            } else {
                                Decision::Rejected
                            }
                        };
                    }
                    checks.push(check);
                }
                if let Some(test) = &self.monte_carlo {
                    let mut check = Check {
                        metric: "monte_carlo_positive_pct",
                        measured: None,
                        threshold: test.min_positive_pct,
                        comparison: "at_least",
                        decision: Decision::NotRun,
                    };
                    if checks
                        .iter()
                        .all(|check| check.decision == Decision::Passed)
                    {
                        let report = sqx_robustness::monte_carlo_trade_manipulation(
                            &row.execution.trades,
                            test.runs,
                            test.skip_probability,
                            seed,
                        );
                        check.decision = if !report.errors.is_empty() {
                            errors.extend(report.errors);
                            Decision::TechnicalError
                        } else if !report.positive_runs_pct.is_finite() || report.runs != test.runs
                        {
                            Decision::InsufficientEvidence
                        } else {
                            check.measured = Some(report.positive_runs_pct);
                            if report.positive_runs_pct >= test.min_positive_pct {
                                Decision::Passed
                            } else {
                                Decision::Rejected
                            }
                        };
                    }
                    checks.push(check);
                }
                let mut walk_forward_assessment = None;
                if let Some(wf) = &self.walk_forward {
                    let mut check = Check {
                        metric: "walk_forward_total_oos_net_profit",
                        measured: None,
                        threshold: wf.min_total_oos_net_profit,
                        comparison: "at_least",
                        decision: Decision::NotRun,
                    };
                    if checks
                        .iter()
                        .all(|check| check.decision == Decision::Passed)
                    {
                        let (assessment, decision, measured) = run_walk_forward(
                            wf,
                            &row.evaluation.strategy,
                            bars,
                            instrument,
                            config,
                            periods_per_year,
                            seed,
                            row.candidate_index,
                        );
                        check.decision = decision;
                        check.measured = measured;
                        errors.extend(
                            assessment
                                .windows
                                .iter()
                                .flat_map(|window| window.errors.iter().cloned()),
                        );
                        walk_forward_assessment = Some(assessment);
                    }
                    checks.push(check);
                }
                Assessment {
                    candidate_index: row.candidate_index,
                    name: row.evaluation.strategy.name.clone(),
                    passed: checks
                        .iter()
                        .all(|check| check.decision == Decision::Passed),
                    checks,
                    errors,
                    walk_forward: walk_forward_assessment,
                }
            })
            .collect()
    }
}

/// Higher severity wins when aggregating started walk-forward windows:
/// technical error > insufficient evidence > rejected > passed.
fn severity(decision: &Decision) -> u8 {
    match decision {
        Decision::TechnicalError => 4,
        Decision::InsufficientEvidence => 3,
        Decision::Rejected => 2,
        Decision::Passed => 1,
        Decision::NotRun => 0,
    }
}

fn worse(a: Decision, b: Decision) -> Decision {
    if severity(&b) > severity(&a) { b } else { a }
}

/// Mirrors the optimizer split exactly so planned rows agree with the emitted
/// family bounds even for windows the optimizer never executes.
fn plan_windows(bars: usize, windows: usize, oos_fraction: f64) -> Vec<(usize, usize, usize)> {
    let window_len = bars / windows;
    (0..windows)
        .map(|w| {
            let start = w * window_len;
            let end = if w + 1 == windows { bars } else { (w + 1) * window_len };
            let split = start + ((end - start) as f64 * (1.0 - oos_fraction)).round() as usize;
            (start, split, end)
        })
        .collect()
}

/// Data/evidence shortages are classified as insufficient evidence rather than
/// technical failure; only invalid inputs and engine/integrity errors are technical.
fn classify_report_error(error: &str) -> Decision {
    if error.starts_with("invalid walk-forward sizes")
        || error.starts_with("account exhausted")
        || (error.starts_with("window ") && error.contains("is too short"))
    {
        Decision::InsufficientEvidence
    } else {
        Decision::TechnicalError
    }
}

/// Intermediate per-window collection from the durable observer events.
struct CollectedWindow {
    index: usize,
    is_start: usize,
    is_end: usize,
    oos_start: usize,
    oos_end: usize,
    starting_equity: f64,
    is_trades: Option<f64>,
    is_net_profit: Option<f64>,
    is_profit_factor: Option<f64>,
    is_eligible: bool,
    is_exclusion_reason: Option<String>,
    is_errors: Vec<String>,
    oos_ran: bool,
    oos_execution: Option<sqx_backtest::BacktestResult>,
}

/// Run the frozen, segmented walk-forward check for a single candidate and
/// archive every planned window. The same strategy is evaluated identically in
/// each window; no parameters are reselected and no RNG selection occurs.
#[allow(clippy::too_many_arguments)]
fn run_walk_forward(
    spec: &WalkForward,
    strategy: &Strategy,
    bars: &[Bar],
    instrument: &sqx_core::Instrument,
    config: &sqx_backtest::BacktestConfig,
    periods_per_year: f64,
    seed: u64,
    candidate_index: usize,
) -> (WalkForwardAssessment, Decision, Option<f64>) {
    let planned = plan_windows(bars.len(), spec.windows, spec.oos_fraction);
    let criteria = sqx_optimizer::WalkForwardCriteria {
        min_is_trades: spec.min_is_trades,
        min_oos_trades_per_window: spec.min_oos_trades_per_window,
    };
    let mut collected: Vec<CollectedWindow> = Vec::new();
    let mut report_errors: Vec<String> = Vec::new();
    let result = sqx_optimizer::walk_forward_candidates_observed(
        bars,
        instrument,
        config,
        periods_per_year,
        std::slice::from_ref(strategy),
        spec.windows,
        spec.oos_fraction,
        seed,
        &criteria,
        &mut |event| {
            match event {
                sqx_optimizer::WalkForwardEvent::Family {
                    window,
                    train_start,
                    test_start,
                    test_end,
                    starting_equity,
                    ..
                } => {
                    collected.push(CollectedWindow {
                        index: window,
                        is_start: train_start,
                        is_end: test_start,
                        oos_start: test_start,
                        oos_end: test_end,
                        starting_equity,
                        is_trades: None,
                        is_net_profit: None,
                        is_profit_factor: None,
                        is_eligible: false,
                        is_exclusion_reason: None,
                        is_errors: Vec::new(),
                        oos_ran: false,
                        oos_execution: None,
                    });
                }
                sqx_optimizer::WalkForwardEvent::CandidateOutcome { evaluation, .. } => {
                    let current = collected.last_mut().expect("candidate outcome follows family");
                    current.is_trades = evaluation.metrics.map(|m| m.number_of_trades);
                    current.is_net_profit = evaluation.metrics.map(|m| m.net_profit);
                    current.is_profit_factor = evaluation
                        .metrics
                        .and_then(|m| m.profit_factor.is_finite().then_some(m.profit_factor));
                    current.is_eligible = evaluation.eligible;
                    current.is_exclusion_reason = evaluation.exclusion_reason.clone();
                    current.is_errors = evaluation.errors.clone();
                }
                sqx_optimizer::WalkForwardEvent::OutOfSampleStarted { .. } => {
                    collected.last_mut().expect("OOS start follows family").oos_ran = true;
                }
                sqx_optimizer::WalkForwardEvent::OutOfSampleOutcome { execution, .. } => {
                    collected
                        .last_mut()
                        .expect("OOS outcome follows family")
                        .oos_execution = Some(execution.clone());
                }
                sqx_optimizer::WalkForwardEvent::CandidateStarted { .. } => {}
            }
            Ok(())
        },
    );
    if let Err(error) = result {
        report_errors.push(error);
    } else {
        report_errors.extend(result.expect("handled").errors);
    }

    let thresholds = WalkForwardThresholds {
        min_is_trades: spec.min_is_trades,
        min_oos_trades_per_window: spec.min_oos_trades_per_window,
        comparison: "at_least",
    };

    let mut windows = Vec::with_capacity(planned.len());
    let mut all_oos_nets = Vec::with_capacity(planned.len());
    for (index, &(is_start, split, oos_end)) in planned.iter().enumerate() {
        match collected.iter().find(|window| window.index == index) {
            Some(executed) => {
                let (is_decision, oos_decision, window_decision, oos_final_equity, oos_trades, oos_net, oos_errors, reason) =
                    decide_window(executed, spec, periods_per_year, split, oos_end);
                if let Some(net) = oos_net {
                    all_oos_nets.push(net);
                }
                windows.push(WalkForwardWindowAssessment {
                    index,
                    is_start: executed.is_start,
                    is_end: executed.is_end,
                    is_first_bar_time: bars[executed.is_start].time,
                    is_last_bar_time: bars[executed.is_end - 1].time,
                    oos_start: executed.oos_start,
                    oos_end: executed.oos_end,
                    oos_first_bar_time: bars[executed.oos_start].time,
                    oos_last_bar_time: bars[executed.oos_end - 1].time,
                    starting_equity: executed.starting_equity,
                    oos_final_equity,
                    is_trades: executed.is_trades,
                    is_net_profit: executed.is_net_profit,
                    is_profit_factor: executed.is_profit_factor,
                    is_eligible: executed.is_eligible,
                    is_exclusion_reason: executed.is_exclusion_reason.clone(),
                    oos_trades,
                    oos_net_profit: oos_net,
                    thresholds: thresholds.clone(),
                    is_decision,
                    oos_decision,
                    decision: window_decision,
                    reason,
                    errors: oos_errors,
                });
            }
            None => {
                let (reason, is_too_short) = missing_window_reason(&report_errors, is_start, split, oos_end);
                let reason = if is_too_short {
                    format!("window {index} is too short")
                } else {
                    reason
                };
                windows.push(WalkForwardWindowAssessment {
                    index,
                    is_start,
                    is_end: split,
                    is_first_bar_time: bars[is_start].time,
                    is_last_bar_time: bars[split - 1].time,
                    oos_start: split,
                    oos_end,
                    oos_first_bar_time: bars[split].time,
                    oos_last_bar_time: bars[oos_end - 1].time,
                    starting_equity: 0.0,
                    oos_final_equity: None,
                    is_trades: None,
                    is_net_profit: None,
                    is_profit_factor: None,
                    is_eligible: false,
                    is_exclusion_reason: None,
                    oos_trades: None,
                    oos_net_profit: None,
                    thresholds: thresholds.clone(),
                    is_decision: Decision::NotRun,
                    oos_decision: Decision::NotRun,
                    decision: Decision::InsufficientEvidence,
                    reason,
                    errors: Vec::new(),
                });
            }
        }
    }

    // Technical error dominates; insufficient evidence follows; only with fully
    // valid, sufficient evidence is the joint total compared to the threshold.
    let all_valid = windows.len() == spec.windows
        && all_oos_nets.len() == spec.windows
        && windows.iter().all(|window| window.decision == Decision::Passed);
    let mut decision = Decision::Passed;
    for error in &report_errors {
        decision = worse(decision, classify_report_error(error));
    }
    for window in &windows {
        decision = worse(decision, window.decision.clone());
    }
    let joint_total = all_oos_nets.iter().sum::<f64>();
    if all_valid && decision == Decision::Passed {
        // Cross-check the summed per-window nets against the carried capital:
        // last out-of-sample equity minus the first deposit. No return sum.
        let first_starting = windows.first().map(|window| window.starting_equity).unwrap_or(0.0);
        let last_final = windows.last().and_then(|window| window.oos_final_equity);
        let reconciles = last_final.is_some_and(|last| {
            joint_total.is_finite()
                && (joint_total - (last - first_starting)).abs() <= 1e-6 * (1.0 + last.abs())
        });
        if !reconciles {
            decision = Decision::TechnicalError;
        } else if joint_total < spec.min_total_oos_net_profit {
            decision = Decision::Rejected;
        }
    }
    let measured = matches!(decision, Decision::Passed | Decision::Rejected)
        .then_some(joint_total);

    let assessment = WalkForwardAssessment {
        method: "fixed_candidate_segmented_v1",
        version: 1,
        scope: "Segmented walk-forward stability check over already-seen search data; one frozen candidate evaluated identically in every window. Not an independent final test.",
        candidate_index,
        strategy_name: strategy.name.clone(),
        frozen_specification: spec.clone(),
        allowed_start_index: 0,
        allowed_end_index: bars.len(),
        allowed_start_time: bars.first().map(|bar| bar.time).unwrap_or(0),
        allowed_end_time: bars.last().map(|bar| bar.time).unwrap_or(0),
        planned_windows: spec.windows,
        min_total_oos_net_profit: spec.min_total_oos_net_profit,
        oos_total_net_profit: measured,
        windows,
    };
    (assessment, decision, measured)
}

fn missing_window_reason(
    report_errors: &[String],
    is_start: usize,
    split: usize,
    oos_end: usize,
) -> (String, bool) {
    let is_too_short = split - is_start < 50 || oos_end - split < 10;
    if report_errors.iter().any(|e| e.starts_with("account exhausted")) {
        ("account exhausted; remaining windows not traded".into(), false)
    } else if is_too_short {
        ("window is too short".into(), true)
    } else if report_errors.iter().any(|e| e.starts_with("invalid walk-forward sizes")) {
        ("invalid walk-forward sizes, OOS fraction or evidence thresholds".into(), false)
    } else {
        ("window not executed".into(), false)
    }
}

#[allow(clippy::type_complexity)]
fn decide_window(
    executed: &CollectedWindow,
    spec: &WalkForward,
    periods_per_year: f64,
    split: usize,
    oos_end: usize,
) -> (
    Decision,
    Decision,
    Decision,
    Option<f64>,
    Option<usize>,
    Option<f64>,
    Vec<String>,
    String,
) {
    let is_decision = if !executed.is_errors.is_empty() {
        Decision::TechnicalError
    } else if !executed.is_eligible {
        Decision::InsufficientEvidence
    } else {
        Decision::Passed
    };
    let (oos_decision, oos_final_equity, oos_trades, oos_net, oos_errors, reason) =
        if !executed.oos_ran {
            let reason = executed
                .is_exclusion_reason
                .clone()
                .unwrap_or_else(|| "no eligible in-sample strategy".into());
            (Decision::NotRun, None, None, None, Vec::new(), reason)
        } else {
            let execution = executed.oos_execution.as_ref().expect("ran implies execution");
            if !execution.errors.is_empty() {
                (
                    Decision::TechnicalError,
                    Some(execution.final_equity),
                    Some(execution.trades.len()),
                    None,
                    execution.errors.clone(),
                    "out-of-sample execution failed".into(),
                )
            } else if let Err(error) = sqx_metrics::validate_observations(execution) {
                (
                    Decision::TechnicalError,
                    Some(execution.final_equity),
                    Some(execution.trades.len()),
                    None,
                    vec![error],
                    "out-of-sample observations failed validation".into(),
                )
            } else {
                let metrics =
                    sqx_metrics::compute(execution, oos_end - split, periods_per_year);
                let net = metrics.net_profit;
                let trades = execution.trades.len();
                let reconciled = execution.final_equity - executed.starting_equity;
                let reconciles = net.is_finite()
                    && reconciled.is_finite()
                    && (net - reconciled).abs() <= 1e-6 * (1.0 + reconciled.abs());
                if !reconciles {
                    (
                        Decision::TechnicalError,
                        Some(execution.final_equity),
                        Some(trades),
                        None,
                        vec!["out-of-sample net profit does not reconcile with ending equity".into()],
                        "out-of-sample net profit does not reconcile with ending equity".into(),
                    )
                } else if trades < spec.min_oos_trades_per_window {
                    (
                        Decision::InsufficientEvidence,
                        Some(execution.final_equity),
                        Some(trades),
                        Some(net),
                        Vec::new(),
                        "insufficient out-of-sample trades".into(),
                    )
                } else {
                    // A negative net in a valid, sufficiently traded window is
                    // not a rejection: only the joint total meets the threshold.
                    (
                        Decision::Passed,
                        Some(execution.final_equity),
                        Some(trades),
                        Some(net),
                        Vec::new(),
                        "passed".into(),
                    )
                }
            }
        };
    let window_decision = match (&is_decision, &oos_decision) {
        (Decision::TechnicalError, _) | (_, Decision::TechnicalError) => Decision::TechnicalError,
        (_, Decision::NotRun) => Decision::InsufficientEvidence,
        (_, Decision::InsufficientEvidence) => Decision::InsufficientEvidence,
        _ => Decision::Passed,
    };
    (
        is_decision,
        oos_decision,
        window_decision,
        oos_final_equity,
        oos_trades,
        oos_net,
        oos_errors,
        reason,
    )
}
