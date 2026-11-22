//! # sqx-optimizer
//!
//! Automated strategy discovery: random generation and genetic evolution,
//! matching the Builder's two build modes. Strategies are generated from the
//! building-block catalog, backtested, and ranked by a configurable fitness.

mod walkforward;

pub use walkforward::*;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use sqx_backtest::{backtest, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_metrics::{compute as compute_metrics, Metrics};
use sqx_strategy::{
    block_catalog, BlockSpec, Comparator, Condition, Join, MoneyManagement, Operand, Param,
    ProfitTarget, Signal, SignalKind, StopLoss, Strategy, BINARY_COMPARATORS, UNARY_COMPARATORS,
};

/// Fitness measure used to rank strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fitness {
    ProfitFactor,
    ReturnDD,
    NetProfit,
    SQN,
}

/// Generator configuration.
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// Number of conditions per entry signal.
    pub min_conditions: usize,
    pub max_conditions: usize,
    /// Stop-loss range in points (0 = no stop-loss).
    pub sl_points: (f64, f64),
    /// Profit-target range in points (0 = no target).
    pub pt_points: (f64, f64),
    /// Fixed contract size used during generation.
    pub fixed_size: f64,
    /// Trade long / short sides.
    pub long: bool,
    pub short: bool,
    /// Per-indicator parameter ranges: (indicator, param, min, max).
    pub param_ranges: Vec<(String, String, f64, f64)>,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            min_conditions: 1,
            max_conditions: 3,
            sl_points: (30.0, 200.0),
            pt_points: (30.0, 300.0),
            fixed_size: 0.1,
            long: true,
            short: true,
            param_ranges: Vec::new(),
        }
    }
}

/// The fitness score of a strategy; strategies with too few trades score NAN.
pub fn fitness(m: &Metrics, kind: Fitness, min_trades: usize) -> f64 {
    if m.number_of_trades < min_trades as f64 {
        return f64::NAN;
    }
    match kind {
        Fitness::ProfitFactor => m.profit_factor,
        Fitness::ReturnDD => m.return_dd_ratio,
        Fitness::NetProfit => m.net_profit,
        Fitness::SQN => m.sqn,
    }
}

/// Generate one random strategy.
pub fn random_strategy(rng: &mut StdRng, name: &str, config: &GeneratorConfig) -> Strategy {
    let catalog = block_catalog();
    let mut s = Strategy::new(name);
    s.long = config.long;
    s.short = config.short;
    s.money_management = MoneyManagement::FixedSize { size: config.fixed_size };

    // Long entry signal: n random conditions.
    let n = rng.gen_range(config.min_conditions..=config.max_conditions);
    let long_conditions: Vec<Condition> = (0..n).map(|_| random_condition(rng, &catalog, config)).collect();
    s.signals.push(Signal::new(SignalKind::LongEntry, long_conditions, Join::And));

    // Short entry mirrors long (negated family), or is generated independently.
    if config.short {
        let n = rng.gen_range(config.min_conditions..=config.max_conditions);
        let short_conditions: Vec<Condition> = (0..n).map(|_| random_condition(rng, &catalog, config)).collect();
        s.signals.push(Signal::new(SignalKind::ShortEntry, short_conditions, Join::And));
    }

    // Long/short exit: close on opposite signal (a random condition) or none.
    if rng.gen_bool(0.5) {
        let c = random_condition(rng, &catalog, config);
        s.signals.push(Signal::new(SignalKind::LongExit, vec![c.clone()], Join::And));
        s.signals.push(Signal::new(SignalKind::ShortExit, vec![c], Join::And));
    }

    // Stop loss / profit target.
    let sl = rng.gen_range(config.sl_points.0..=config.sl_points.1);
    if sl > 0.0 {
        s.stop_loss = Some(StopLoss::Points(sl));
    }
    let pt = rng.gen_range(config.pt_points.0..=config.pt_points.1);
    if pt > 0.0 {
        s.profit_target = Some(ProfitTarget::Points(pt));
    }
    s
}

/// Generate a random condition from the catalog.
fn params_for(spec: &BlockSpec, rng: &mut StdRng, ranges: &[(String, String, f64, f64)]) -> Vec<Param> {
    spec.params
        .iter()
        .map(|(name, def)| {
            let v = match ranges.iter().find(|(ind, p, _, _)| ind == &spec.base && p == name) {
                Some((_, _, lo, hi)) if (hi - lo).abs() > 1e-12 => {
                    let sampled = rng.gen_range(*lo..=*hi);
                    if lo.fract() == 0.0 && hi.fract() == 0.0 {
                        sampled.round()
                    } else {
                        sampled
                    }
                }
                Some((_, _, lo, _)) => *lo,
                None => *def,
            };
            Param::new(name, v)
        })
        .collect()
}

fn random_condition(rng: &mut StdRng, catalog: &[BlockSpec], config: &GeneratorConfig) -> Condition {
    let spec = catalog.choose(rng).unwrap();
    let params: Vec<Param> = params_for(&spec, rng, &config.param_ranges);
    if let Some(cmp) = spec.cmp {
        return Condition::new(Operand::Indicator(spec.base.clone(), params), cmp, Operand::Const(0.0));
    }
    if rng.gen_bool(0.5) {
        let cmp = *BINARY_COMPARATORS.choose(rng).unwrap();
        let field = *sqx_strategy::PRICE_FIELDS.choose(rng).unwrap();
        let shift = rng.gen_range(0..3);
        Condition::new(Operand::Price(field, shift), cmp, Operand::Indicator(spec.base.clone(), params))
    } else if spec.is_oscillator {
        let level = oscillator_level(rng, &spec.base);
        let cmp = *[Comparator::Gt, Comparator::Lt, Comparator::CrossAbove, Comparator::CrossBelow].choose(rng).unwrap();
        Condition::new(Operand::Indicator(spec.base.clone(), params), cmp, Operand::Const(level))
    } else {
        let cmp = *UNARY_COMPARATORS.choose(rng).unwrap();
        Condition::new(Operand::Indicator(spec.base.clone(), params), cmp, Operand::Const(0.0))
    }
}

/// Sample a sensible level for an oscillator.
fn oscillator_level(rng: &mut StdRng, name: &str) -> f64 {
    match name {
        "CCI" => (rng.gen_range(-150.0..150.0) as f64).round(),
        "DeMarker" | "RSI" | "StochK" | "StochD" | "WilliamsR" => {
            (rng.gen_range(20.0..80.0) as f64).round()
        }
        "AroonUp" | "AroonDown" => (rng.gen_range(20.0..80.0) as f64).round(),
        _ => 0.0,
    }
}

/// Backtest a batch of strategies and return (strategy, metrics) pairs.
pub fn backtest_batch(
    strategies: &[Strategy],
    bars: &[Bar],
    instrument: &Instrument,
    config: &BacktestConfig,
    periods_per_year: f64,
) -> Vec<(Strategy, Metrics)> {
    strategies
        .iter()
        .map(|s| {
            let result = backtest(s, bars, instrument, config);
            let metrics = compute_metrics(&result, bars.len(), periods_per_year);
            (s.clone(), metrics)
        })
        .collect()
}

/// Rank results by fitness (descending); NAN fitness is dropped.
pub fn rank(mut results: Vec<(Strategy, Metrics)>, kind: Fitness, min_trades: usize) -> Vec<(Strategy, Metrics)> {
    results.retain(|(_, m)| fitness(m, kind, min_trades).is_finite());
    results.sort_by(|a, b| {
        fitness(&b.1, kind, min_trades)
            .partial_cmp(&fitness(&a.1, kind, min_trades))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

/// Genetic-evolution configuration.
#[derive(Debug, Clone)]
pub struct GeneticConfig {
    pub population_size: usize,
    pub generations: usize,
    pub crossover_prob: f64,
    pub mutation_prob: f64,
    pub elite_count: usize,
    pub min_trades: usize,
}

impl Default for GeneticConfig {
    fn default() -> Self {
        Self {
            population_size: 50,
            generations: 20,
            crossover_prob: 0.8,
            mutation_prob: 0.3,
            elite_count: 5,
            min_trades: 10,
        }
    }
}

/// Run genetic evolution and return the ranked final population.
#[allow(clippy::too_many_arguments)]
pub fn genetic_evolve(
    seed: u64,
    gen_config: &GeneratorConfig,
    ga: &GeneticConfig,
    kind: Fitness,
    bars: &[Bar],
    instrument: &Instrument,
    backtest_config: &BacktestConfig,
    periods_per_year: f64,
) -> Vec<(Strategy, Metrics)> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut population: Vec<Strategy> = (0..ga.population_size)
        .map(|i| random_strategy(&mut rng, &format!("gen0-{i}"), gen_config))
        .collect();

    for _gen in 0..ga.generations {
        let mut scored = backtest_batch(&population, bars, instrument, backtest_config, periods_per_year);
        scored.retain(|(_, m)| fitness(m, kind, ga.min_trades).is_finite());
        scored.sort_by(|a, b| {
            fitness(&b.1, kind, ga.min_trades)
                .partial_cmp(&fitness(&a.1, kind, ga.min_trades))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if scored.is_empty() {
            break;
        }

        let mut next: Vec<Strategy> = scored
            .iter()
            .take(ga.elite_count.min(scored.len()))
            .map(|(s, _)| s.clone())
            .collect();

        while next.len() < ga.population_size {
            let parent_a = tournament(&scored, kind, ga.min_trades, &mut rng);
            let parent_b = tournament(&scored, kind, ga.min_trades, &mut rng);
            let mut child = if rng.gen_bool(ga.crossover_prob) {
                crossover(&parent_a, &parent_b, &mut rng)
            } else {
                parent_a.clone()
            };
            if rng.gen_bool(ga.mutation_prob) {
                mutate(&mut child, gen_config, &mut rng);
            }
            next.push(child);
        }
        population = next;
    }

    let scored = backtest_batch(&population, bars, instrument, backtest_config, periods_per_year);
    rank(scored, kind, ga.min_trades)
}

fn tournament(
    scored: &[(Strategy, Metrics)],
    kind: Fitness,
    min_trades: usize,
    rng: &mut StdRng,
) -> Strategy {
    let k = 3.min(scored.len());
    let mut best: Option<&(Strategy, Metrics)> = None;
    for _ in 0..k {
        let cand = scored.choose(rng).unwrap();
        if best.map_or(true, |b| fitness(&cand.1, kind, min_trades) > fitness(&b.1, kind, min_trades)) {
            best = Some(cand);
        }
    }
    best.unwrap().0.clone()
}

/// One-point crossover of two strategies: swap exit/exit signals and blend conditions.
fn crossover(a: &Strategy, b: &Strategy, rng: &mut StdRng) -> Strategy {
    let mut child = a.clone();
    // Blend LongEntry conditions: take a prefix from a and suffix from b.
    if !a.signals.is_empty() && !b.signals.is_empty() {
        let sig_a = a.signals.iter().find(|s| s.kind == SignalKind::LongEntry);
        let sig_b = b.signals.iter().find(|s| s.kind == SignalKind::LongEntry);
        if let (Some(sa), Some(sb)) = (sig_a, sig_b) {
            let cut_a = sa.conditions.len();
            let cut_b = sb.conditions.len();
            let cut = rng.gen_range(0..=cut_a.max(cut_b));
            let mut conds: Vec<Condition> = Vec::new();
            conds.extend(sa.conditions.iter().take(cut).cloned());
            conds.extend(sb.conditions.iter().skip(cut).cloned());
            if let Some(cs) = child.signals.iter_mut().find(|s| s.kind == SignalKind::LongEntry) {
                if !conds.is_empty() {
                    cs.conditions = conds;
                }
            }
        }
    }
    child.name = format!("offspring-{}", rng.gen_range(0..1_000_000u32));
    child
}

/// Mutate a strategy in place.
fn mutate(s: &mut Strategy, config: &GeneratorConfig, rng: &mut StdRng) {
    let catalog = block_catalog();
    let roll = rng.gen_range(0..4);
    match roll {
        // Replace a random condition.
        0 => {
            if let Some(sig) = s.signals.iter_mut().find(|sig| !sig.conditions.is_empty()) {
                let idx = rng.gen_range(0..sig.conditions.len());
                sig.conditions[idx] = random_condition(rng, &catalog, config);
            }
        }
        // Add a condition to a signal.
        1 => {
            if let Some(sig) = s.signals.iter_mut().next() {
                if sig.conditions.len() < config.max_conditions {
                    sig.conditions.push(random_condition(rng, &catalog, config));
                }
            }
        }
        // Remove a condition.
        2 => {
            if let Some(sig) = s.signals.iter_mut().find(|sig| sig.conditions.len() > 1) {
                sig.conditions.pop();
            }
        }
        // Change stop-loss / target.
        _ => {
            s.stop_loss = Some(StopLoss::Points(rng.gen_range(config.sl_points.0..=config.sl_points.1)));
            s.profit_target = Some(ProfitTarget::Points(rng.gen_range(config.pt_points.0..=config.pt_points.1)));
        }
    }
}
