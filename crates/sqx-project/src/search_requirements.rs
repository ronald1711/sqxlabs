//! Explicit gates for exploratory search results.
use serde::{Deserialize, Serialize};
use sqx_metrics::Metrics;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirements {
    pub min_trades: u32,
    pub min_net_profit: f64,
    pub min_profit_factor: f64,
    pub max_drawdown_pct: f64,
    pub spread_stress: Option<SpreadStress>,
    pub monte_carlo: Option<MonteCarlo>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpreadStress {
    pub extra_spread: f64,
    pub max_profit_drop: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonteCarlo {
    pub runs: usize,
    pub skip_probability: f64,
    pub min_positive_pct: f64,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Decision {
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
        {
            return Err("Invalid search requirements");
        }
        Ok(())
    }

    fn assess(&self, metrics: &Metrics, execution_failed: bool) -> [Check; 4] {
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

    #[test]
    fn strong_metrics_do_not_compensate_for_missing_evidence_or_failed_gates() {
        let requirements = Requirements {
            min_trades: 20,
            min_net_profit: 0.0,
            min_profit_factor: 1.0,
            max_drawdown_pct: 5.0,
            spread_stress: None,
            monte_carlo: None,
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
}

#[derive(Debug, Serialize)]
pub struct Assessment {
    pub candidate_index: usize,
    name: String,
    pub passed: bool,
    checks: Vec<Check>,
    errors: Vec<String>,
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
                Assessment {
                    candidate_index: row.candidate_index,
                    name: row.evaluation.strategy.name.clone(),
                    passed: checks
                        .iter()
                        .all(|check| check.decision == Decision::Passed),
                    checks,
                    errors,
                }
            })
            .collect()
    }
}
