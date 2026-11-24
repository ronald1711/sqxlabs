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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Fitness {
    ProfitFactor,
    ReturnDD,
    NetProfit,
    SQN,
}

/// Generator configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

impl GeneratorConfig {
    /// Validate externally supplied ranges before any RNG sampling or allocation.
    pub fn validate(&self) -> Result<(),String> {
        if self.min_conditions==0 || self.min_conditions>self.max_conditions || self.max_conditions>64{return Err("Condition counts must satisfy 1 <= min <= max <= 64".into());}
        if !self.long && !self.short{return Err("Enable at least one trading direction".into());}
        if !self.fixed_size.is_finite() || self.fixed_size<=0.0{return Err("Generated position size must be finite and positive".into());}
        for (label,(lo,hi)) in [("stop loss",self.sl_points),("profit target",self.pt_points)] {
            if !lo.is_finite() || !hi.is_finite() || lo<0.0 || hi<lo || !(hi-lo).is_finite(){return Err(format!("Invalid {label} range: require finite 0 <= min <= max"));}
        }
        let catalog=block_catalog();let mut seen=std::collections::BTreeSet::new();
        for (indicator,param,lo,hi) in &self.param_ranges {
            if !seen.insert((indicator,param)){return Err(format!("Duplicate parameter range: {indicator}.{param}"));}
            if !catalog.iter().any(|spec|&spec.base==indicator && spec.params.iter().any(|(name,_)|name==param)){return Err(format!("Unknown generator parameter: {indicator}.{param}"));}
            if !lo.is_finite() || !hi.is_finite() || hi<lo || !(hi-lo).is_finite(){return Err(format!("Invalid parameter range: {indicator}.{param}"));}
            if *lo<=0.0 {return Err(format!("Parameter range must be positive: {indicator}.{param}"));}
            if matches!(param.as_str(),"period"|"fast"|"slow"|"signal") && (lo.fract()!=0.0 || hi.fract()!=0.0 || *hi>=usize::MAX as f64) {
                return Err(format!("Period range must contain representable whole-number endpoints: {indicator}.{param}"));
            }
        }
        Ok(())
    }
}

/// Checked entry point for callers accepting untrusted generator settings.
pub fn try_random_strategy(rng:&mut impl Rng,name:&str,config:&GeneratorConfig)->Result<Strategy,String>{
    config.validate()?;
    Ok(random_strategy(rng,name,config))
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
pub fn random_strategy(rng: &mut impl Rng, name: &str, config: &GeneratorConfig) -> Strategy {
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
fn params_for(spec: &BlockSpec, rng: &mut impl Rng, ranges: &[(String, String, f64, f64)]) -> Vec<Param> {
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

fn random_condition(rng: &mut impl Rng, catalog: &[BlockSpec], config: &GeneratorConfig) -> Condition {
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
fn oscillator_level(rng: &mut impl Rng, name: &str) -> f64 {
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
#[derive(Debug,Clone,serde::Serialize)]
pub struct BatchEvaluation {
    pub strategy:Strategy,
    pub metrics:Metrics,
    pub errors:Vec<String>,
}

/// Compatibility metrics-only view. Use the detailed API to retain failed attempts.
pub fn backtest_batch(strategies:&[Strategy],bars:&[Bar],instrument:&Instrument,config:&BacktestConfig,periods_per_year:f64)->Vec<(Strategy,Metrics)>{
    backtest_batch_detailed(strategies,bars,instrument,config,periods_per_year).into_iter().map(|row|(row.strategy,row.metrics)).collect()
}

/// Ordered per-candidate results including execution failures, even in parallel batches.
pub fn backtest_batch_detailed(
    strategies: &[Strategy],
    bars: &[Bar],
    instrument: &Instrument,
    config: &BacktestConfig,
    periods_per_year: f64,
) -> Vec<BatchEvaluation> {
    let one = |s: &Strategy| {
        let result = backtest(s, bars, instrument, config);
        let metrics = compute_metrics(&result, bars.len(), periods_per_year);
        let mut errors=result.errors.clone();
        if let Err(error)=sqx_metrics::validate_observations(&result){errors.push(error);}
        BatchEvaluation{strategy:s.clone(),metrics,errors}
    };
    let n = strategies.len();
    if n < 8 {
        return strategies.iter().map(one).collect();
    }
    let threads = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(4)
        .min(8)
        .min(n);
    let chunk = (n + threads - 1) / threads;
    let mut out: Vec<BatchEvaluation> = Vec::with_capacity(n);
    std::thread::scope(|scope| {
        let handles: Vec<_> = strategies
            .chunks(chunk)
            .map(|part| scope.spawn(move || part.iter().map(one).collect::<Vec<_>>()))
            .collect();
        for h in handles {
            out.extend(h.join().unwrap());
        }
    });
    out
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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
) -> Result<Vec<(Strategy, Metrics)>,String> {
    genetic_evolve_observed(seed,gen_config,ga,kind,bars,instrument,backtest_config,periods_per_year,|_,_,_,_|Ok(()))
}

/// Observe every evaluation in deterministic population order, including invalid and
/// fitness-rejected candidates. Observer errors abort the search; no success is reported.
/// Generation zero is the initial population; the final population is also evaluated.
#[allow(clippy::too_many_arguments)]
pub fn genetic_evolve_observed<F>(
    seed:u64,gen_config:&GeneratorConfig,ga:&GeneticConfig,kind:Fitness,bars:&[Bar],
    instrument:&Instrument,backtest_config:&BacktestConfig,periods_per_year:f64,mut observe:F,
)->Result<Vec<(Strategy,Metrics)>,String>
where F:FnMut(usize,usize,&BatchEvaluation,bool)->Result<(),String> {
    let mut state=EvolutionState::new(seed,gen_config,ga,kind,bars,instrument,backtest_config,periods_per_year)?;
    loop {let step=state.advance(&mut observe)?;if step.complete{return Ok(step.ranked);}}
}

/// Serializable generation-boundary checkpoint, including frozen inputs and RNG state.
/// Persist only after `advance` succeeds; retain the returned evaluation evidence separately.
#[derive(Debug,Clone,serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionState {
    schema:u32,
    generator:GeneratorConfig,
    ga:GeneticConfig,
    kind:Fitness,
    bars:Vec<Bar>,
    instrument:Instrument,
    config:BacktestConfig,
    periods_per_year:f64,
    generation:usize,
    population:Vec<Strategy>,
    rng:rand_chacha::ChaCha12Rng,
    complete:bool,
}
pub struct EvolutionStep {pub generation:usize,pub ranked:Vec<(Strategy,Metrics)>,pub complete:bool}
impl EvolutionState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(seed:u64,gen_config:&GeneratorConfig,ga:&GeneticConfig,kind:Fitness,bars:&[Bar],instrument:&Instrument,backtest_config:&BacktestConfig,periods_per_year:f64)->Result<Self,String>{
    gen_config.validate()?;
    if ga.population_size==0 || ga.population_size>2000 || ga.generations>1000 || ga.elite_count>ga.population_size || ga.min_trades==0
        || !ga.crossover_prob.is_finite() || !(0.0..=1.0).contains(&ga.crossover_prob)
        || !ga.mutation_prob.is_finite() || !(0.0..=1.0).contains(&ga.mutation_prob) {
        return Err("Invalid genetic population, generation, elite, trade threshold or probability settings".into());
    }
    sqx_backtest::validate_backtest_inputs(bars,instrument,backtest_config)?;
        if !periods_per_year.is_finite() || periods_per_year<=0.0{return Err("Annualization must be finite and positive".into());}
        let mut rng=rand_chacha::ChaCha12Rng::seed_from_u64(seed);
        let population=(0..ga.population_size).map(|i|random_strategy(&mut rng,&format!("gen0-{i}"),gen_config)).collect();
        Ok(Self{schema:1,generator:gen_config.clone(),ga:ga.clone(),kind,bars:bars.to_vec(),instrument:instrument.clone(),config:backtest_config.clone(),periods_per_year,generation:0,population,rng,complete:false})
    }
    pub fn generation(&self)->usize{self.generation}
    pub fn population_size(&self)->usize{self.population.len()}
    pub fn instrument(&self)->&Instrument{&self.instrument}
    pub fn backtest_config(&self)->&BacktestConfig{&self.config}
    pub fn source_bars(&self)->&[Bar]{&self.bars}
    /// Only population, RNG position and progress may change between checkpoints.
    pub fn frozen_inputs_match(&self,other:&Self)->Result<bool,String>{
        let encode=|state:&Self|serde_json::to_vec(&(state.schema,&state.generator,&state.ga,&state.kind,&state.bars,&state.instrument,&state.config,state.periods_per_year)).map_err(|e|e.to_string());
        Ok(encode(self)?==encode(other)?)
    }
    pub fn genetic_config(&self)->&GeneticConfig{&self.ga}
    pub fn fitness(&self)->Fitness{self.kind}
    pub fn observation_range(&self)->Option<(i64,i64)>{Some((self.bars.first()?.time,self.bars.last()?.time))}
    pub fn is_complete(&self)->bool{self.complete}
    pub fn advance<F>(&mut self,mut observe:F)->Result<EvolutionStep,String>
    where F:FnMut(usize,usize,&BatchEvaluation,bool)->Result<(),String>{
        if self.complete{return Err("Evolution checkpoint is already complete".into());}
        if self.schema!=1 || self.generation>self.ga.generations || self.population.len()!=self.ga.population_size{return Err("Invalid evolution checkpoint structure".into());}
        self.generator.validate()?;
        if self.ga.population_size==0 || self.ga.population_size>2000 || self.ga.generations>1000 || self.ga.elite_count>self.ga.population_size || self.ga.min_trades==0 || !self.ga.crossover_prob.is_finite() || !(0.0..=1.0).contains(&self.ga.crossover_prob) || !self.ga.mutation_prob.is_finite() || !(0.0..=1.0).contains(&self.ga.mutation_prob) || !self.periods_per_year.is_finite() || self.periods_per_year<=0.0{return Err("Invalid evolution checkpoint settings".into());}
        sqx_backtest::validate_backtest_inputs(&self.bars,&self.instrument,&self.config)?;
        let generation=self.generation;
        let scored=evaluate_generation(&self.population,generation,&self.bars,&self.instrument,&self.config,self.periods_per_year,self.kind,self.ga.min_trades,&mut observe)?;
        if scored.is_empty() || generation==self.ga.generations {self.complete=true;return Ok(EvolutionStep{generation,ranked:scored,complete:true});}
        let mut rng=self.rng.clone();
        let mut next: Vec<Strategy> = scored
            .iter()
            .take(self.ga.elite_count.min(scored.len()))
            .map(|(s, _)| s.clone())
            .collect();

        while next.len() < self.ga.population_size {
            let parent_a = tournament(&scored, self.kind, self.ga.min_trades, &mut rng);
            let parent_b = tournament(&scored, self.kind, self.ga.min_trades, &mut rng);
            let mut child = if rng.gen_bool(self.ga.crossover_prob) {
                crossover(&parent_a, &parent_b, &mut rng)
            } else {
                parent_a.clone()
            };
            if rng.gen_bool(self.ga.mutation_prob) {
                mutate(&mut child, &self.generator, &mut rng);
            }
            next.push(child);
        }
        self.population=next;self.rng=rng;self.generation+=1;
        Ok(EvolutionStep{generation,ranked:scored,complete:false})
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_generation<F>(population:&[Strategy],generation:usize,bars:&[Bar],instrument:&Instrument,config:&BacktestConfig,pyp:f64,kind:Fitness,min_trades:usize,observe:&mut F)->Result<Vec<(Strategy,Metrics)>,String>
where F:FnMut(usize,usize,&BatchEvaluation,bool)->Result<(),String>{
    let evaluations=backtest_batch_detailed(population,bars,instrument,config,pyp);
    let mut eligible=Vec::new();
    for (index,evaluation) in evaluations.into_iter().enumerate(){
        let qualifies=evaluation.errors.is_empty() && fitness(&evaluation.metrics,kind,min_trades).is_finite();
        observe(generation,index,&evaluation,qualifies)?;
        if qualifies{eligible.push((evaluation.strategy,evaluation.metrics));}
    }
    Ok(rank(eligible,kind,min_trades))
}

fn tournament(
    scored: &[(Strategy, Metrics)],
    kind: Fitness,
    min_trades: usize,
    rng: &mut impl Rng,
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
fn crossover(a: &Strategy, b: &Strategy, rng: &mut impl Rng) -> Strategy {
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
fn mutate(s: &mut Strategy, config: &GeneratorConfig, rng: &mut impl Rng) {
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

/// Freeze a family that changes only this strategy's SL/PT distances (±20%).
/// The original is always first. Entry/exit signals, sizing, trading schedules,
/// direction flags and confirmation rules remain exactly as supplied.
pub fn exit_parameter_family(strategy:&Strategy,count:usize,seed:u64)->Result<Vec<Strategy>,String>{
    if !(1..=2000).contains(&count){return Err("Candidate family size must be between 1 and 2000".into());}
    let valid=|value:f64|value.is_finite() && value>0.0 && value<=f64::MAX/1.2;
    if strategy.stop_loss.is_some_and(|sl|match sl{StopLoss::Points(p)=>!valid(p),StopLoss::Atr{period,mult}=>period==0||!valid(mult)})
        || strategy.profit_target.is_some_and(|pt|match pt{ProfitTarget::Points(p)=>!valid(p),ProfitTarget::Atr{period,mult}=>period==0||!valid(mult)}) {
        return Err("Candidate SL/PT distances must be positive, finite and safely scalable".into());
    }
    let mut family=vec![strategy.clone()];
    if strategy.stop_loss.is_none() && strategy.profit_target.is_none(){return Ok(family);}
    let mut rng=StdRng::seed_from_u64(seed);
    for index in 1..count {
        let mut candidate=strategy.clone();candidate.name=format!("{} · exit variant {index}",strategy.name);
        candidate.stop_loss=candidate.stop_loss.map(|sl|{let f=rng.gen_range(0.8..=1.2);match sl{StopLoss::Points(p)=>StopLoss::Points(p*f),StopLoss::Atr{period,mult}=>StopLoss::Atr{period,mult:mult*f}}});
        candidate.profit_target=candidate.profit_target.map(|pt|{let f=rng.gen_range(0.8..=1.2);match pt{ProfitTarget::Points(p)=>ProfitTarget::Points(p*f),ProfitTarget::Atr{period,mult}=>ProfitTarget::Atr{period,mult:mult*f}}});
        family.push(candidate);
    }
    Ok(family)
}

/// Candidate-linked optimization of explicit indicator ranges plus SL/PT distances.
/// The first member is an unchanged control, even when it is outside requested ranges.
/// All occurrences of a named indicator parameter share the sampled value.
pub fn parameter_family(strategy:&Strategy,count:usize,seed:u64,ranges:&[(String,String,f64,f64)])->Result<Vec<Strategy>,String>{
    let validation=GeneratorConfig{param_ranges:ranges.to_vec(),..Default::default()};validation.validate()?;
    let present=|indicator:&str,param:&str|strategy.signals.iter().chain(strategy.confirmation.iter().map(|c|&c.signal))
        .flat_map(|signal|signal.conditions.iter()).flat_map(|condition|[&condition.left,&condition.right])
        .any(|operand|matches!(operand,Operand::Indicator(name,params) if name==indicator && params.iter().any(|p|p.name==param)));
    for (indicator,param,_,_) in ranges {
        if !present(indicator,param){return Err(format!("Candidate {} has no parameter {indicator}.{param}",strategy.name));}
    }
    let mut family=exit_parameter_family(strategy,count,seed)?;
    if ranges.is_empty(){return Ok(family);}
    while family.len()<count {let mut variant=strategy.clone();variant.name=format!("{} · parameter variant {}",strategy.name,family.len());family.push(variant);}
    let mut rng=StdRng::seed_from_u64(seed ^ 0x706172616d657465);
    for variant in family.iter_mut().skip(1) {
        for (indicator,param,lo,hi) in ranges {
            let value=if lo==hi {*lo}else{let sampled=rng.gen_range(*lo..=*hi);if lo.fract()==0.0 && hi.fract()==0.0{sampled.round()}else{sampled}};
            for signal in variant.signals.iter_mut().chain(variant.confirmation.iter_mut().map(|c|&mut c.signal)) {
                for condition in &mut signal.conditions {
                    for operand in [&mut condition.left,&mut condition.right] {
                        if let Operand::Indicator(name,params)=operand {
                            if name==indicator {for p in params {if &p.name==param {p.value=value;}}}
                        }
                    }
                }
            }
        }
    }
    Ok(family)
}
