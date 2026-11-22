use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use rand::SeedableRng;

use sqx_backtest::{backtest, BacktestConfig, BacktestResult};
use sqx_core::{Bar, Instrument, Timeframe};
use sqx_data::DataStore;
use sqx_metrics::{compute as compute_metrics, render as render_metrics};
use sqx_optimizer::{backtest_batch, genetic_evolve, rank, random_strategy, walk_forward, Fitness, GeneticConfig, GeneratorConfig};
use sqx_project::{Databank, MetricSummary, Project, StrategyRecord};
use sqx_export::{export as export_strategy, Engine};
use sqx_portfolio::{backtest_all, best_subset, combine, equal_weights};
use sqx_robustness::monte_carlo_trade_manipulation;
use sqx_strategy::{render as render_pseudocode, Strategy};

#[derive(Parser)]
#[command(name = "sqx", version, about = "StrategyQuant open-source port command line")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Backtest a single strategy over CSV data.
    Backtest {
        /// Path to a strategy JSON file.
        #[arg(long)]
        strategy: PathBuf,
        /// Path to a CSV file (timestamp,open,high,low,close,volume).
        #[arg(long)]
        data: PathBuf,
        /// Symbol name (defaults to EURUSD).
        #[arg(long, default_value = "EURUSD")]
        symbol: String,
        /// Initial deposit.
        #[arg(long, default_value_t = 10_000.0)]
        deposit: f64,
        /// Spread in points.
        #[arg(long, default_value_t = 0.0)]
        spread: f64,
        /// Slippage in points.
        #[arg(long, default_value_t = 0.0)]
        slippage: f64,
        /// Commission per lot (round-trip).
        #[arg(long, default_value_t = 0.0)]
        commission: f64,
        /// Bars per year for annualization (0 = infer from data).
        #[arg(long, default_value_t = 0.0)]
        periods_per_year: f64,
    },
    /// Generate random strategies, backtest and rank them.
    Generate {
        #[arg(long)]
        data: PathBuf,
        /// Number of strategies to generate.
        #[arg(long, default_value_t = 200)]
        count: usize,
        /// Random seed.
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// How many top strategies to print.
        #[arg(long, default_value_t = 10)]
        top: usize,
        /// Minimum trades a strategy must have to be ranked.
        #[arg(long, default_value_t = 5)]
        min_trades: usize,
        /// Save the ranked databank to this JSON file.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run genetic evolution.
    Evolve {
        #[arg(long)]
        data: PathBuf,
        /// Random seed.
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Population size.
        #[arg(long, default_value_t = 50)]
        population: usize,
        /// Number of generations.
        #[arg(long, default_value_t = 20)]
        generations: usize,
        /// Minimum trades to be ranked.
        #[arg(long, default_value_t = 5)]
        min_trades: usize,
    },
    /// List the indicator building-block catalog.
    Indicators,
    /// Render a strategy as pseudo-code.
    Pseudocode {
        #[arg(long)]
        strategy: PathBuf,
    },
    /// Export a strategy to platform source code.
    Export {
        #[arg(long)]
        strategy: PathBuf,
        /// mql4 | mql5 | pine | pseudocode
        #[arg(long)]
        engine: String,
        /// Write to this file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Monte Carlo trade-manipulation robustness test.
    Montecarlo {
        #[arg(long)]
        strategy: PathBuf,
        #[arg(long)]
        data: PathBuf,
        #[arg(long, default_value_t = 1000)]
        runs: usize,
        #[arg(long, default_value_t = 0.1)]
        skip: f64,
        #[arg(long, default_value_t = 42)]
        seed: u64,
    },
    /// Run walk-forward optimization.
    Optimize {
        #[arg(long)]
        data: PathBuf,
        #[arg(long, default_value_t = 6)]
        windows: usize,
        #[arg(long, default_value_t = 0.25)]
        oos: f64,
        #[arg(long, default_value_t = 40)]
        candidates: usize,
        #[arg(long, default_value_t = 42)]
        seed: u64,
    },
    /// Start the web dashboard.
    Serve {
        /// Listen address, e.g. 127.0.0.1:8080.
        #[arg(long, default_value = "127.0.0.1:8080")]
        addr: String,
        /// Single CSV file (symbol from --symbol).
        #[arg(long)]
        data: Option<PathBuf>,
        /// Directory of *.csv files (symbol = file stem).
        #[arg(long)]
        data_dir: Option<PathBuf>,
        #[arg(long, default_value = "EURUSD")]
        symbol: String,
    },
    /// Build a portfolio from generated strategies (equal weights, best subset).
    Portfolio {
        #[arg(long)]
        data: PathBuf,
        #[arg(long, default_value_t = 300)]
        count: usize,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long, default_value_t = 12)]
        top: usize,
        #[arg(long, default_value_t = 2)]
        min_k: usize,
        #[arg(long, default_value_t = 4)]
        max_k: usize,
    },
    /// Multi-market cross-check: backtest one strategy on every symbol in a folder.
    Crosscheck {
        #[arg(long)]
        strategy: PathBuf,
        #[arg(long)]
        data_dir: PathBuf,
        #[arg(long, default_value_t = 5)]
        min_trades: usize,
    },
    /// Run a custom-project pipeline: Build -> Retest -> Optimize.
    Pipeline {
        #[arg(long)]
        data: PathBuf,
        #[arg(long, default_value_t = 200)]
        count: usize,
        #[arg(long, default_value_t = 20)]
        keep: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Backtest { strategy, data, symbol, deposit, spread, slippage, commission, periods_per_year } => {
            let s: Strategy = load_strategy(&strategy)?;
            let bars = load_bars(&data)?;
            let instrument = instrument(&symbol);
            let pyp = resolve_periods_per_year(bars.len(), periods_per_year, &bars);
            let config = BacktestConfig {
                initial_deposit: deposit,
                spread_points: spread,
                slippage_points: slippage,
                commission_per_lot: commission,
            };
            let result = backtest(&s, &bars, &instrument, &config);
            let m = compute_metrics(&result, bars.len(), pyp);
            println!("{}", render_pseudocode(&s));
            println!();
            println!("{}", render_metrics(&m));
            println!("Final equity: {:.2}", result.final_equity);
            println!("Trades: {}", result.trades.len());
            Ok(())
        }
        Cmd::Generate { data, count, seed, top, min_trades, out } => {
            let bars = load_bars(&data)?;
            let instrument = instrument("EURUSD");
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
            let config = BacktestConfig::default();
            let gen = GeneratorConfig::default();
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let strategies: Vec<Strategy> = (0..count)
                .map(|i| random_strategy(&mut rng, &format!("s{i:04}"), &gen))
                .collect();
            let scored = backtest_batch(&strategies, &bars, &instrument, &config, pyp);
            let ranked = rank(scored, Fitness::ProfitFactor, min_trades);
            let top_n = ranked.iter().take(top).collect::<Vec<_>>();
            for (i, (s, m)) in top_n.iter().enumerate() {
                println!("{}. {}  PF={:.2}  Net={:.2}  Trades={:.0}  DD%={:.2}", i + 1, s.name, m.profit_factor, m.net_profit, m.number_of_trades, m.max_drawdown_pct);
            }
            if let Some(out_path) = out {
                let mut bank = Databank { name: "Results".into(), strategies: Vec::new() };
                for (s, m) in ranked.iter().take(100) {
                    bank.strategies.push(StrategyRecord { strategy: s.clone(), metrics: MetricSummary::from(m) });
                }
                let project = Project { name: "Generator".into(), version: "0.1.0".into(), databanks: vec![bank] };
                sqx_project::save(out_path.to_str().unwrap(), &project).map_err(|e| anyhow!(e))?;
                println!("Saved {} strategies to {}", ranked.len().min(100), out_path.display());
            }
            Ok(())
        }
        Cmd::Evolve { data, seed, population, generations, min_trades } => {
            let bars = load_bars(&data)?;
            let instrument = instrument("EURUSD");
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
            let config = BacktestConfig::default();
            let gen = GeneratorConfig::default();
            let ga = GeneticConfig { population_size: population, generations, min_trades, ..Default::default() };
            let ranked = genetic_evolve(seed, &gen, &ga, Fitness::ProfitFactor, &bars, &instrument, &config, pyp);
            for (i, (s, m)) in ranked.iter().take(10).enumerate() {
                println!("{}. {}  PF={:.2}  Net={:.2}  Trades={:.0}", i + 1, s.name, m.profit_factor, m.net_profit, m.number_of_trades);
            }
            Ok(())
        }
        Cmd::Indicators => {
            for spec in sqx_strategy::indicator_catalog() {
                let params: Vec<String> = spec.params.iter().map(|(n, v)| format!("{n}={v}")).collect();
                println!("{:<14} {:<22} {}", spec.name, spec.label, params.join(", "));
            }
            Ok(())
        }
        Cmd::Pseudocode { strategy } => {
            let s: Strategy = load_strategy(&strategy)?;
            println!("{}", render_pseudocode(&s));
            Ok(())
        }
        Cmd::Export { strategy, engine, out } => {
            let s: Strategy = load_strategy(&strategy)?;
            let eng = Engine::from_label(&engine)
                .ok_or_else(|| anyhow!("unknown engine '{engine}' (use mql4|mql5|pine|pseudocode)"))?;
            let code = export_strategy(&s, eng);
            match out {
                Some(p) => std::fs::write(&p, &code).map_err(|e| anyhow!(e))?,
                None => println!("{code}"),
            }
            Ok(())
        }
        Cmd::Montecarlo { strategy, data, runs, skip, seed } => {
            let s: Strategy = load_strategy(&strategy)?;
            let bars = load_bars(&data)?;
            let inst = instrument("EURUSD");
            let result = backtest(&s, &bars, &inst, &BacktestConfig::default());
            let report = monte_carlo_trade_manipulation(&result.trades, runs, skip, seed);
            print!("{}", sqx_robustness::render(&report));
            Ok(())
        }
        Cmd::Optimize { data, windows, oos, candidates, seed } => {
            let bars = load_bars(&data)?;
            let inst = instrument("EURUSD");
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
            let gen = GeneratorConfig::default();
            let report = walk_forward(&bars, &inst, &BacktestConfig::default(), pyp, &gen, windows, oos, candidates, seed);
            println!("Walk-forward optimization ({} windows)", report.windows);
            println!("  OOS trades        : {}", report.oos_trades);
            println!("  OOS net profit    : {:.2}", report.oos_net_profit);
            println!("  OOS profit factor : {:.2}", report.oos_profit_factor);
            println!("  OOS winning %     : {:.1}", report.oos_winning_pct);
            println!("  OOS max DD %      : {:.2}", report.oos_max_dd_pct);
            Ok(())
        }
        Cmd::Serve { addr, data, data_dir, symbol } => {
            let mut store = DataStore::new();
            if let Some(dir) = data_dir {
                let loaded = store.import_dir(dir.to_str().unwrap(), Timeframe::M1).map_err(|e| anyhow!(e))?;
                for (s, n) in &loaded {
                    println!("Loaded {} bars for {}", n, s);
                }
            } else if let Some(file) = data {
                let n = store.import_csv(file.to_str().unwrap(), &symbol, Timeframe::M1).map_err(|e| anyhow!(e))?;
                println!("Loaded {} bars for {}", n, symbol);
            } else {
                return Err(anyhow!("serve requires --data or --data-dir"));
            }
            if store.is_empty() {
                return Err(anyhow!("no data loaded"));
            }
            sqx_web::serve(&addr, store).map_err(|e| anyhow!(e))?;
            Ok(())
        }
        Cmd::Portfolio { data, count, seed, top, min_k, max_k } => {
            let bars = load_bars(&data)?;
            let inst = instrument("EURUSD");
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
            let cfg = BacktestConfig::default();
            let gen = GeneratorConfig::default();
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let strategies: Vec<Strategy> = (0..count).map(|i| random_strategy(&mut rng, &format!("p{i:04}"), &gen)).collect();
            let names: Vec<String> = strategies.iter().map(|s| s.name.clone()).collect();
            let results: Vec<BacktestResult> = backtest_all(&strategies, &bars, &inst, &cfg);
            let mets: Vec<sqx_metrics::Metrics> = results.iter().map(|r| compute_metrics(r, bars.len(), pyp)).collect();
            let mut order: Vec<usize> = (0..results.len()).collect();
            order.sort_by(|&a, &b| mets[b].profit_factor.partial_cmp(&mets[a].profit_factor).unwrap_or(std::cmp::Ordering::Equal));
            order.retain(|&i| mets[i].number_of_trades >= 5.0 && mets[i].profit_factor.is_finite());
            order.truncate(top);
            if order.len() < 2 {
                println!("Not enough qualifying strategies to build a portfolio");
                return Ok(());
            }
            let sel: Vec<BacktestResult> = order.iter().map(|&i| results[i].clone()).collect();
            let sel_names: Vec<String> = order.iter().map(|&i| names[i].clone()).collect();
            let subset = best_subset(&sel, min_k, max_k, cfg.initial_deposit);
            if subset.len() < 2 {
                println!("No subset found");
                return Ok(());
            }
            let comp: Vec<BacktestResult> = subset.iter().map(|&i| sel[i].clone()).collect();
            let comp_names: Vec<String> = subset.iter().map(|&i| sel_names[i].clone()).collect();
            let weights = equal_weights(comp.len());
            let pf = combine(&comp_names, &comp, &weights, cfg.initial_deposit, bars.len(), pyp);
            println!("Portfolio of {} strategies (equal weights)", comp.len());
            println!("  Net profit {:.2} | PF {:.2} | Max DD {:.2}% | Return/DD {:.2} | Sharpe {:.2}", pf.metrics.net_profit, pf.metrics.profit_factor, pf.metrics.max_drawdown_pct, pf.metrics.return_dd_ratio, pf.metrics.sharpe_ratio);
            println!("  Components:");
            for c in &pf.components {
                println!("    {:<10} w={:.2} net={:.2} dd={:.2}% trades={}", c.name, c.weight, c.net_profit, c.max_drawdown_pct, c.trades);
            }
            println!("  Correlation:");
            for row in &pf.correlation {
                let s: Vec<String> = row.iter().map(|v| format!("{:.2}", v)).collect();
                println!("    {}", s.join("  "));
            }
            Ok(())
        }
        Cmd::Crosscheck { strategy, data_dir, min_trades } => {
            let s: Strategy = load_strategy(&strategy)?;
            let mut store = DataStore::new();
            store.import_dir(data_dir.to_str().unwrap(), Timeframe::M1).map_err(|e| anyhow!(e))?;
            if store.is_empty() {
                return Err(anyhow!("no data in {}", data_dir.display()));
            }
            let cfg = BacktestConfig::default();
            println!("Multi-market cross-check for {}", s.name);
            let mut profitable = 0;
            let mut total = 0;
            for series in &store.series {
                let inst = instrument(&series.symbol);
                let pyp = resolve_periods_per_year(series.bars.len(), 0.0, &series.bars);
                let r = backtest(&s, &series.bars, &inst, &cfg);
                let m = compute_metrics(&r, series.bars.len(), pyp);
                let enough = m.number_of_trades >= min_trades as f64;
                if enough {
                    total += 1;
                    if m.net_profit > 0.0 {
                        profitable += 1;
                    }
                }
                println!(
                    "  {:<16} net={:>10.2} PF={:>7.2} DD%={:>6.2} trades={:>4}{}",
                    series.symbol, m.net_profit, m.profit_factor, m.max_drawdown_pct, m.number_of_trades,
                    if enough { "" } else { "  (too few trades)" }
                );
            }
            println!("  Profitable on {}/{} markets (>= {} trades)", profitable, total, min_trades);
            Ok(())
        }
        Cmd::Pipeline { data, count, keep } => {
            let bars = load_bars(&data)?;
            let inst = instrument("EURUSD");
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
            let cfg = BacktestConfig::default();
            let gen = GeneratorConfig::default();
            let pipeline = sqx_pipeline::default_pipeline(count, keep);
            println!("Pipeline: {}", pipeline.name);
            let report = sqx_pipeline::run(&pipeline, &bars, &inst, &cfg, pyp, &gen, 42);
            for line in &report.log {
                println!("  {}", line);
            }
            Ok(())
        }
    }
}

fn instrument(symbol: &str) -> Instrument {
    if symbol.contains("JPY") {
        Instrument::fx(symbol)
    } else {
        Instrument::fx(symbol)
    }
}

fn load_strategy(path: &PathBuf) -> Result<Strategy> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| anyhow!("parsing strategy JSON: {e}"))
}

/// Parse bars from a CSV with header: timestamp,open,high,low,close,volume.
fn load_bars(path: &PathBuf) -> Result<Vec<Bar>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut bars = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if idx == 0 && line.starts_with("timestamp") {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 6 {
            continue;
        }
        let time = cols[0].trim().parse::<i64>().map_err(|e| anyhow!("bad timestamp at line {}: {e}", idx + 1))?;
        let open = cols[1].trim().parse::<f64>().map_err(|e| anyhow!("bad open at line {}: {e}", idx + 1))?;
        let high = cols[2].trim().parse::<f64>().map_err(|e| anyhow!("bad high at line {}: {e}", idx + 1))?;
        let low = cols[3].trim().parse::<f64>().map_err(|e| anyhow!("bad low at line {}: {e}", idx + 1))?;
        let close = cols[4].trim().parse::<f64>().map_err(|e| anyhow!("bad close at line {}: {e}", idx + 1))?;
        let volume = cols[5].trim().parse::<f64>().unwrap_or(0.0);
        bars.push(Bar::new(time * 1000, open, high, low, close, volume));
    }
    if bars.is_empty() {
        return Err(anyhow!("no bars loaded from {}", path.display()));
    }
    Ok(bars)
}

/// Resolve bars-per-year: explicit override, else infer from the median bar interval.
fn resolve_periods_per_year(len: usize, explicit: f64, bars: &[Bar]) -> f64 {
    if explicit > 0.0 {
        return explicit;
    }
    let interval_secs = median_interval_secs(bars);
    if interval_secs > 0.0 {
        let per_year = 365.25 * 24.0 * 3600.0 / interval_secs;
        return per_year.min(len as f64).max(1.0);
    }
    252.0
}

fn median_interval_secs(bars: &[Bar]) -> f64 {
    if bars.len() < 2 {
        return 0.0;
    }
    let mut diffs: Vec<i64> = bars.windows(2).map(|w| (w[1].time - w[0].time) / 1000).collect();
    diffs.sort_unstable();
    diffs[diffs.len() / 2] as f64
}
