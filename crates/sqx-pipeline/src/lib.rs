//! # sqx-pipeline
//!
//! Custom projects: an ordered pipeline of tasks that chain Build, Retest and
//! Optimize (with GoToTask loops), mirroring StrategyQuant's Custom Projects.

use rand::rngs::StdRng;
use rand::SeedableRng;

use sqx_backtest::{backtest, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_optimizer::{backtest_batch, rank, random_strategy, walk_forward, Fitness, GeneratorConfig};
use sqx_robustness::monte_carlo_trade_manipulation;
use sqx_strategy::Strategy;

/// One pipeline step.
#[derive(Debug, Clone)]
pub enum TaskKind {
    Build { count: usize, keep: usize },
    Retest { mc_runs: usize, min_positive_pct: f64 },
    Optimize { windows: usize, candidates: usize },
    GoToTask { index: usize },
}

/// A named task.
#[derive(Debug, Clone)]
pub struct Task {
    pub kind: TaskKind,
}

/// A custom project pipeline.
#[derive(Debug, Clone, Default)]
pub struct Pipeline {
    pub name: String,
    pub tasks: Vec<Task>,
}

/// The outcome of running a pipeline.
#[derive(Debug, Clone, Default)]
pub struct PipelineReport {
    pub databank: Vec<Strategy>,
    pub log: Vec<String>,
}

/// Run a pipeline over the given data.
pub fn run(
    pipeline: &Pipeline,
    bars: &[Bar],
    instrument: &Instrument,
    config: &BacktestConfig,
    periods_per_year: f64,
    gen: &GeneratorConfig,
    seed: u64,
) -> PipelineReport {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut databank: Vec<Strategy> = Vec::new();
    let mut log = Vec::new();
    let mut pc = 0usize;
    let mut steps = 0usize;
    while pc < pipeline.tasks.len() && steps < 200 {
        steps += 1;
        match &pipeline.tasks[pc].kind {
            TaskKind::Build { count, keep } => {
                let cands: Vec<Strategy> = (0..*count)
                    .map(|i| random_strategy(&mut rng, &format!("b{steps}-{i}"), gen))
                    .collect();
                let scored = backtest_batch(&cands, bars, instrument, config, periods_per_year);
                let ranked = rank(scored, Fitness::ProfitFactor, 5);
                databank = ranked.into_iter().take(*keep).map(|(s, _)| s).collect();
                log.push(format!("Build: {} candidates -> {} kept", count, databank.len()));
                pc += 1;
            }
            TaskKind::Retest { mc_runs, min_positive_pct } => {
                let mut kept = Vec::new();
                for s in databank.drain(..) {
                    let r = backtest(&s, bars, instrument, config);
                    let mc = monte_carlo_trade_manipulation(&r.trades, *mc_runs, 0.1, seed);
                    let positive_pct = 100.0 - mc.losing_runs_pct;
                    if positive_pct >= *min_positive_pct {
                        kept.push(s);
                    }
                }
                log.push(format!("Retest: {} survivors", kept.len()));
                databank = kept;
                pc += 1;
            }
            TaskKind::Optimize { windows, candidates } => {
                let mut kept = Vec::new();
                for s in databank.drain(..) {
                    let rep = walk_forward(bars, instrument, config, periods_per_year, gen, *windows, 0.25, *candidates, seed);
                    if rep.oos_net_profit > 0.0 {
                        kept.push(s);
                    }
                }
                log.push(format!("Optimize: {} survivors", kept.len()));
                databank = kept;
                pc += 1;
            }
            TaskKind::GoToTask { index } => {
                log.push(format!("GoToTask {} (databank {})", index, databank.len()));
                pc = *index;
            }
        }
    }
    log.push(format!("Pipeline finished after {} steps; {} strategies in databank", steps, databank.len()));
    PipelineReport { databank, log }
}

/// A default demo pipeline: Build -> Retest -> Optimize.
pub fn default_pipeline(count: usize, keep: usize) -> Pipeline {
    Pipeline {
        name: "Build -> Retest -> Optimize".to_string(),
        tasks: vec![
            Task { kind: TaskKind::Build { count, keep } },
            Task { kind: TaskKind::Retest { mc_runs: 500, min_positive_pct: 60.0 } },
            Task { kind: TaskKind::Optimize { windows: 4, candidates: 20 } },
        ],
    }
}
