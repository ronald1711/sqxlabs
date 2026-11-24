mod audit_log;
mod pipeline_archive;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use rand::SeedableRng;

use sqx_backtest::{backtest, backtest_ticks, BacktestConfig, BacktestResult};
use sqx_core::{Bar, Instrument, Timeframe};
use sqx_data::{DataStore, QuestdbClient, QuestdbConfig};
use sqx_metrics::{compute as compute_metrics, render as render_metrics};
use sqx_optimizer::{backtest_batch, genetic_evolve_observed, rank, random_strategy, walk_forward, Fitness, GeneticConfig, GeneratorConfig};
use sqx_project::{Databank, MetricSummary, Project, StrategyRecord};
use sqx_export::{export_report as export_strategy, Engine};
use sqx_portfolio::{backtest_all, best_subset, combine, equal_weights};
use sqx_robustness::{monte_carlo_retest, monte_carlo_trade_manipulation, what_if};
use sqx_strategy::{render as render_pseudocode, Strategy};

#[derive(Parser)]
#[command(name = "sqx", version, about = "StrategyQuant open-source port command line")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Args)]
struct SourceOptions {
    /// Verified normalization bundle for these exact candles; embeds raw clock provenance.
    #[arg(long)]
    normalization_bundle:Option<PathBuf>,
    /// Declared candle timeframe (M1, M5, H1, D1, MN1, ...).
    #[arg(long)]
    timeframe:Timeframe,
    /// Explicit UTC market calendar JSON.
    #[arg(long)]
    calendar:Option<PathBuf>,
    /// Acknowledge unresolved gaps for exploratory research only.
    #[arg(long)]
    allow_data_gaps:bool,
}
impl SourceOptions {
    fn audit(&self,bars:&[Bar])->Result<(Option<i64>,serde_json::Value)>{
        let calendar:Option<sqx_data::quality::MarketCalendar>=self.calendar.as_ref().map(|path|->Result<_>{Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)}).transpose()?;
        let cutoff=sqx_data::quality::now_ms();
        let quality=sqx_data::quality::audit(bars,self.timeframe,cutoff,calendar.as_ref());
        if !quality.errors.is_empty(){anyhow::bail!("{}; data_quality: {}",quality.errors.join("; "),serde_json::to_string(&quality)?);}
        if !self.allow_data_gaps && (quality.missing_open_intervals>0 || quality.unclassified_intervals>0 || quality.irregular_steps>0){
            anyhow::bail!("Unresolved data gaps; supply --calendar or explicitly --allow-data-gaps for exploratory research. data_quality: {}",serde_json::to_string(&quality)?);
        }
        let duration=if self.timeframe==Timeframe::MN1 {None}else{self.timeframe.minutes().and_then(|n|n.checked_mul(60000))};
        let normalization=if let Some(path)=&self.normalization_bundle {
            let bundle=sqx_data::normalization::Bundle::load(path).map_err(anyhow::Error::msg)?;
            let verified=bundle.verify_bars(bars,duration).map_err(anyhow::Error::msg)?;
            Some(serde_json::json!({"verification":verified,"bundle":bundle}))
        }else{None};
        Ok((duration,serde_json::json!({"normalization":normalization,"timeframe":self.timeframe,"calendar":calendar,"as_of_ms":cutoff,"data_quality":quality,"data_gaps_acknowledged":self.allow_data_gaps,"scope":"Exploratory research; no independent execution parity or untouched holdout certified."})))
    }
}

#[derive(clap::Args)]
struct ResearchOptions {
    #[command(flatten)]
    source:SourceOptions,
    /// Instrument represented by the source data; never inferred from the filename.
    #[arg(long)]
    symbol:String,
    /// Full BacktestConfig JSON, including costs and account settings.
    #[arg(long)]
    config:Option<PathBuf>,
}
impl ResearchOptions {
    fn prepare_frozen(&self,bars:&[Bar])->Result<(Instrument,BacktestConfig,serde_json::Value)>{
        let (instrument,config,mut context)=self.prepare(bars)?;
        // Current-time audit above rejects unfinished data. Freeze its proven data
        // horizon, not the wall clock, so identical requests retain identical identity.
        context.as_object_mut().expect("source context object").remove("as_of_ms");
        context["validated_through_ms"]=serde_json::json!(bars.last().and_then(|bar|sqx_data::quality::candle_end(bar.time,self.source.timeframe)));
        Ok((instrument,config,context))
    }
    fn prepare(&self,bars:&[Bar])->Result<(Instrument,BacktestConfig,serde_json::Value)>{
        if self.symbol.trim().is_empty(){anyhow::bail!("symbol must be nonempty");}
        let (duration,mut context)=self.source.audit(bars)?;
        let mut config:BacktestConfig=match &self.config {Some(path)=>serde_json::from_str(&std::fs::read_to_string(path)?)?,None=>BacktestConfig::default()};
        if config.bar_duration_ms.is_some() && config.bar_duration_ms!=duration {anyhow::bail!("bar_duration_ms conflicts with source timeframe {}",self.source.timeframe);}
        config.bar_duration_ms=duration;
        let instrument=instrument(&self.symbol);
        sqx_backtest::validate_backtest_inputs(bars,&instrument,&config).map_err(anyhow::Error::msg)?;
        context["symbol"]=serde_json::json!(self.symbol);context["config"]=serde_json::json!(config);
        Ok((instrument,config,context))
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CrosscheckManifest { schema:u32, markets:Vec<CrosscheckMarket> }
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CrosscheckMarket {
    symbol:String,data:PathBuf,timeframe:Timeframe,
    #[serde(default)] normalization_bundle:Option<PathBuf>,
    #[serde(default)] calendar:Option<PathBuf>,
    #[serde(default)] config:Option<PathBuf>,
    #[serde(default)] allow_data_gaps:bool,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DataManifest { schema:u32, datasets:Vec<DataManifestEntry> }
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DataManifestEntry {
    symbol:String,data:PathBuf,timeframe:Timeframe,
    #[serde(default)] calendar:Option<PathBuf>,
}
fn load_data_manifest(path:&std::path::Path)->Result<DataStore>{
    let manifest:DataManifest=serde_json::from_str(&std::fs::read_to_string(path)?)?;
    if manifest.schema!=1 || manifest.datasets.is_empty(){anyhow::bail!("data manifest requires schema 1 and nonempty datasets");}
    let parent=path.parent().unwrap_or_else(||std::path::Path::new("."));
    let resolve=|path:PathBuf|if path.is_absolute(){path}else{parent.join(path)};
    let cutoff=sqx_data::quality::now_ms();let mut staged=Vec::new();let mut seen=std::collections::HashSet::new();
    for entry in manifest.datasets {
        let symbol=entry.symbol.trim().to_string();
        if symbol.is_empty(){anyhow::bail!("manifest symbol must be nonempty");}
        if !seen.insert((symbol.to_uppercase(),entry.timeframe)){anyhow::bail!("duplicate symbol/timeframe in data manifest");}
        let bars=load_bars(&resolve(entry.data))?;
        let calendar:Option<sqx_data::quality::MarketCalendar>=entry.calendar.map(|path|->Result<_>{Ok(serde_json::from_str(&std::fs::read_to_string(resolve(path))?)?)}).transpose()?;
        let quality=sqx_data::quality::audit(&bars,entry.timeframe,cutoff,calendar.as_ref());
        if !quality.errors.is_empty(){anyhow::bail!("{symbol} {}: {}",entry.timeframe,quality.errors.join("; "));}
        // Gaps remain inspectable in Data Manager; research still requires its own disclosure.
        staged.push(sqx_data::SymbolSeries{symbol,timeframe:entry.timeframe,calendar,normalization:None,bars});
    }
    Ok(DataStore{series:staged})
}

type PreparedMarket=(Vec<Bar>,Instrument,BacktestConfig,serde_json::Value);
fn execute_crosscheck(s:&Strategy,prepared:Vec<PreparedMarket>,min_trades:usize)->Result<serde_json::Value>{
            let mut executions=Vec::new();
            let requested=prepared.len();let mut rows=Vec::new();let mut profitable=0;let mut eligible=0;let mut invalid=0;
            for (bars,instrument,config,context) in prepared {
                let pyp=resolve_periods_per_year(bars.len(),0.0,&bars);
                let result=backtest(s,&bars,&instrument,&config);
                executions.push(serde_json::to_value(&result)?);
                if !result.errors.is_empty(){
                    invalid+=1;rows.push(serde_json::json!({"context":context,"status":"invalid","errors":result.errors}));continue;
                }
                let metrics=compute_metrics(&result,bars.len(),pyp);
                let enough=metrics.number_of_trades>=min_trades as f64;
                if enough {eligible+=1;if metrics.net_profit>0.0{profitable+=1;}}
                rows.push(serde_json::json!({"context":context,"status":if enough{"eligible"}else{"insufficient_trades"},"metrics":metrics}));
            }
            let report=serde_json::json!({"markets":rows,"requested_markets":requested,"invalid_markets":invalid,"eligible_markets":eligible,"profitable_markets":profitable,"minimum_trades":min_trades,"scope":"Exploratory cross-market comparison; no untouched holdout or execution parity certified."});
    Ok(serde_json::json!({"report":report,"executions":executions}))
}

#[derive(Subcommand)]
enum Cmd {
    /// List frozen AI trial checkpoints/results without contacting a provider.
    ListAssistantTrials {
        #[arg(long, default_value = "data/research")]
        archive_dir: PathBuf,
    },
    /// Compare a saved AI trial, or restart one checkpoint with --resume.
    ReplayAssistantTrial {
        #[arg(long)]
        resume: bool,
        #[arg(long, default_value = "data/research")]
        archive_dir: PathBuf,
        #[arg(long)]
        run_id: String,
    },
    /// List saved CLI checkpoints and their completed results without running them.
    ListBacktests {
        #[arg(long, default_value = "data/research")]
        archive_dir: PathBuf,
    },
    /// Replay a full CLI backtest archive using this exact executable.
    ReplayBacktest {
        /// Restart a saved pre-execution checkpoint and archive its result.
        #[arg(long)]
        resume: bool,
        #[arg(long, default_value = "data/research")]
        archive_dir: PathBuf,
        #[arg(long)]
        run_id: String,
    },
    /// Reproduce a recorded cross-check, or restart its frozen checkpoint.
    ReplayCrosscheck {
        #[arg(long)] resume:bool,
        #[arg(long, default_value="data/research")] archive_dir:PathBuf,
        #[arg(long)] run_id:String,
    },
    /// Backtest a single strategy over CSV data.
    Backtest {
        /// Save full inputs and execution evidence here (default: data/research).
        #[arg(long, default_value = "data/research")]
        archive_dir: Option<PathBuf>,
        /// Path to a strategy JSON file.
        #[arg(long)]
        strategy: PathBuf,
        /// Path to a CSV file (timestamp,open,high,low,close,volume).
        #[arg(long)]
        data: PathBuf,
        #[command(flatten)]
        source: SourceOptions,
        /// Optional tick CSV (timestamp,bid,ask) for tick-precision exits.
        #[arg(long)]
        ticks: Option<PathBuf>,
        /// Verified clock-normalization bundle for the exact supplied ticks.
        #[arg(long, requires = "ticks")]
        tick_normalization_bundle: Option<PathBuf>,
        /// Optional legacy tick duration; must match the declared source timeframe.
        #[arg(long, requires = "ticks")]
        tick_bar_ms: Option<i64>,
        /// Explicitly allow candle execution where ticks are missing (hybrid mode).
        #[arg(long, requires = "ticks")]
        allow_tick_fallback: bool,
        /// Account model JSON (currency, margin, volume, financing and historical FX rates).
        #[arg(long)]
        account_config: Option<PathBuf>,
        /// Symbol name (defaults to EURUSD).
        #[arg(long, default_value = "EURUSD")]
        symbol: String,
        /// Initial deposit.
        #[arg(long, default_value_t = 10_000.0)]
        deposit: f64,
        /// Spread in SqxLabs points (FX pips: EURUSD 0.0001, USDJPY 0.01); tick runs use observed bid/ask.
        #[arg(long, default_value_t = 0.0)]
        spread: f64,
        /// Slippage per fill in FX pips; one 0.001 USDJPY broker point is --slippage 0.1.
        #[arg(long, default_value_t = 0.0)]
        slippage: f64,
        /// Commission per lot (round-trip).
        #[arg(long, default_value_t = 0.0)]
        commission: f64,
        /// Bars per year for annualization (0 = infer from data).
        #[arg(long, default_value_t = 0.0)]
        periods_per_year: f64,
    },
    /// QuantAnalyzer-style analysis report for one strategy.
    Analyze {
        #[command(flatten)]
        source: SourceOptions,
        /// Full BacktestConfig JSON, including costs and account settings.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Path to a strategy JSON file.
        #[arg(long)]
        strategy: PathBuf,
        /// Path to a CSV file (timestamp,open,high,low,close,volume).
        #[arg(long)]
        data: PathBuf,
        /// Symbol name (defaults to EURUSD).
        #[arg(long, default_value = "EURUSD")]
        symbol: String,
        /// Bars per year for annualization (0 = infer from data).
        #[arg(long, default_value_t = 0.0)]
        periods_per_year: f64,
    },
    /// AlgoCloud-style daily signals: evaluate stored strategies on the last bar.
    Cloud {
        #[command(flatten)]
        source:SourceOptions,
        /// Directory with strategy JSON files.
        #[arg(long)]
        strategies: PathBuf,
        /// Path to a CSV file (timestamp,open,high,low,close,volume).
        #[arg(long)]
        data: PathBuf,
        /// Symbol name (defaults to EURUSD).
        #[arg(long)]
        symbol: String,
    },
    /// LLM assistant: drive the engine from an OpenAI-compatible endpoint.
    Assistant {
        #[command(flatten)]
        research:ResearchOptions,
        /// Path to a CSV file.
        #[arg(long)]
        data: PathBuf,
        /// Base URL (default: env SQX_LLM_BASE_URL or Groq).
        #[arg(long, default_value = "")]
        base_url: String,
        /// Model id (default: env SQX_LLM_MODEL or llama-3.3-70b-versatile).
        #[arg(long, default_value = "")]
        model: String,
        /// API key (default: env SQX_LLM_API_KEY).
        #[arg(long, default_value = "")]
        api_key: String,
        /// The question / instruction for the assistant.
        #[arg(long)]
        ask: String,
    },
    /// Generate random strategies, backtest and rank them.
    Generate {
        #[command(flatten)]
        research:ResearchOptions,
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
        #[command(flatten)]
        research:ResearchOptions,
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
        /// Create a new durable JSONL log of frozen inputs and every evaluation.
        #[arg(long)]
        audit_log: Option<PathBuf>,
    },
    /// Replay a complete evolution log with the matching executable.
    VerifyEvolution {
        #[arg(long)]
        audit_log: PathBuf,
    },
    /// Freeze a durable evolution job without running a generation yet.
    StartEvolution {
        #[command(flatten)]
        research:ResearchOptions,
        #[arg(long)] data:PathBuf,
        #[arg(long,default_value="data/research")] store:PathBuf,
        #[arg(long)] request_key:String,
        #[arg(long,default_value_t=42)] seed:u64,
        #[arg(long,default_value_t=50)] population:usize,
        #[arg(long,default_value_t=20)] generations:usize,
        #[arg(long,default_value_t=5)] min_trades:usize,
    },
    /// Continue a frozen job for a bounded number of generation evaluations.
    ResumeEvolution {
        #[arg(long,default_value="data/research")] store:PathBuf,
        #[arg(long)] job_id:String,
        #[arg(long,default_value_t=1)] steps:usize,
        /// Committed generation count from the last status response.
        #[arg(long)] expected_sequence:usize,
    },
    /// Inspect committed evolution progress and the latest ranked population.
    EvolutionStatus {
        #[arg(long,default_value="data/research")] store:PathBuf,
        #[arg(long)] job_id:String,
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
        #[command(flatten)]
        research:ResearchOptions,
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
        #[command(flatten)]
        research:ResearchOptions,
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
        #[arg(long, conflicts_with_all=["data_dir","questdb"], requires_all=["symbol","data_timeframe"])]
        data: Option<PathBuf>,
        /// Directory of *.csv files (symbol = file stem).
        #[arg(long, conflicts_with="questdb", requires="data_timeframe")]
        data_dir: Option<PathBuf>,
        /// Schema-1 dataset manifest with explicit per-file instrument/timeframe/calendar.
        #[arg(long, conflicts_with_all=["data","data_dir","questdb","data_timeframe"])]
        data_manifest:Option<PathBuf>,
        /// Declared interval shared by the supplied CSV files.
        #[arg(long)]
        data_timeframe:Option<Timeframe>,
        #[arg(long, requires="data")]
        symbol:Option<String>,
        /// Open the dashboard in the default browser.
        #[arg(long)]
        open: bool,
        /// Load data from QuestDB instead of CSV.
        #[arg(long)]
        questdb: bool,
        /// Comma-separated symbols to preload from QuestDB (the Data Manager
        /// tab can also list and load more on demand).
        #[arg(long, default_value = "EURUSD,GBPUSD,USDJPY")]
        questdb_symbols: String,
        /// Timeframe for QuestDB bars (M1/M5/H1/D1/...).
        #[arg(long, default_value = "M5")]
        questdb_timeframe: String,
        /// Most-recent bars per symbol to load from QuestDB.
        #[arg(long, default_value_t = 5000)]
        questdb_limit: usize,
    },
    /// Query a QuestDB instance for candles (reads the `candles` table).
    Questdb {
        #[command(subcommand)]
        sub: QuestdbCmd,
    },
    /// Check the environment: toolchain, data files, port, MT5 runtime.
    Doctor,
    /// Build a portfolio from generated strategies (equal weights, best subset).
    Portfolio {
        #[command(flatten)]
        research:ResearchOptions,
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
    /// Multi-market cross-check over an explicit source manifest.
    Crosscheck {
        #[arg(long, default_value = "data/research")]
        archive_dir: PathBuf,
        #[arg(long)]
        strategy: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long, default_value_t = 5)]
        min_trades: usize,
    },
    /// Inspect immutable pipeline events without running calculations.
    PipelineHistory {
        #[arg(long, default_value="data/research")] archive_dir:PathBuf,
        #[arg(long)] run_id:String,
    },
    /// Export a whole pipeline backup as JSON on stdout (no executable included).
    ExportPipeline {
        #[arg(long, default_value="data/research")] archive_dir:PathBuf,
        #[arg(long)] run_id:String,
    },
    /// Validate and restore a whole pipeline backup without executing it.
    ImportPipeline {
        #[arg(long, default_value="data/research")] archive_dir:PathBuf,
        #[arg(long)] input:PathBuf,
    },
    /// Replay an exact-engine pipeline, optionally finishing its incomplete prefix.
    ReplayPipeline {
        #[arg(long, default_value="data/research")] archive_dir:PathBuf,
        /// ID of the pipeline start record, not its completion record.
        #[arg(long)] run_id:String,
        #[arg(long)] resume:bool,
    },
    /// Run a custom-project pipeline: Build -> Retest -> Optimize.
    Pipeline {
        /// Immutable research archive (inputs, engine, checkpoint and full report).
        #[arg(long, default_value="data/research")]
        archive_dir: PathBuf,
        #[command(flatten)]
        research:ResearchOptions,
        #[arg(long, default_value_t = 42)]
        seed:u64,
        #[arg(long)]
        data: PathBuf,
        #[arg(long, default_value_t = 200)]
        count: usize,
        #[arg(long, default_value_t = 20)]
        keep: usize,
    },
    /// Full robustness cross-check set for one strategy.
    Robustness {
        #[command(flatten)]
        research:ResearchOptions,
        #[arg(long)]
        strategy: PathBuf,
        #[arg(long)]
        data: PathBuf,
        #[arg(long, default_value_t = 1000)]
        mc_runs: usize,
        #[arg(long, default_value_t = 300)]
        retest_runs: usize,
        #[arg(long, default_value_t = 6)]
        windows: usize,
    },
}

#[derive(Subcommand)]
enum QuestdbCmd {
    /// List distinct symbols in the candles table.
    Symbols,
    /// Fetch bars and print a summary; optionally write a CSV.
    Bars {
        /// Symbol to fetch (e.g. EURUSD).
        #[arg(long)]
        symbol: String,
        /// Timeframe (M1/M5/H1/D1/...).
        #[arg(long, default_value = "M5")]
        timeframe: String,
        /// Most-recent bars to return.
        #[arg(long, default_value_t = 5000)]
        limit: usize,
        /// Optional start bound (unix seconds).
        #[arg(long)]
        from: Option<i64>,
        /// Optional end bound (unix seconds).
        #[arg(long)]
        to: Option<i64>,
        /// Write the bars as CSV to this path.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::ListAssistantTrials { archive_dir } => {
            let archive=sqx_project::archive::Archive::new(archive_dir);
            let engine=sqx_project::archive::content_id(&std::fs::read(std::env::current_exe()?)?);
            let report=sqx_project::assistant_trials::list(&archive,&engine).map_err(anyhow::Error::msg)?;
            println!("{report}");
            Ok(())
        }
        Cmd::ReplayAssistantTrial { archive_dir, run_id, resume } => {
            let archive=sqx_project::archive::Archive::new(archive_dir);
            let engine=sqx_project::archive::content_id(&std::fs::read(std::env::current_exe()?)?);
            let report=sqx_project::assistant_trials::execute(&archive,&run_id,&engine,resume).map_err(anyhow::Error::msg)?;
            println!("Assistant trial: {report}");
            if report["matches"]==false {anyhow::bail!("replayed assistant execution differs from the recorded result");}
            if resume && report["execution_valid"]==false {anyhow::bail!("restarted assistant trial has execution errors; its result was archived");}
            Ok(())
        }
        Cmd::ListBacktests { archive_dir } => {
            let archive=sqx_project::archive::Archive::new(&archive_dir);
            let engine=sqx_project::archive::content_id(&std::fs::read(std::env::current_exe()?)?);
            let records=archive.list().map_err(anyhow::Error::msg)?.into_iter().map(|id|archive.load(&id)).collect::<std::result::Result<Vec<_>,_>>().map_err(anyhow::Error::msg)?;
            let mut rows=Vec::new();
            for started in records.iter().filter(|b|b.run.request["research_kind"]=="cli_backtest_started") {
                let mut completed=Vec::new();
                let mut expected_request=started.run.request.clone();expected_request["research_kind"]=serde_json::json!("cli_backtest");expected_request["started_run_id"]=serde_json::json!(started.run_id);
                for result in records.iter().filter(|b|b.run.request["research_kind"]=="cli_backtest"&&b.run.request["started_run_id"]==started.run_id) {
                    if result.run.request!=expected_request||result.run.dataset_id!=started.run.dataset_id||result.run.strategy_id!=started.run.strategy_id||result.run.engine_id!=started.run.engine_id {
                        anyhow::bail!("completed result does not match its checkpoint identity");
                    }
                    completed.push(serde_json::json!({"run_id":result.run_id,"execution_valid":result.run.result["errors"].as_array().is_some_and(|e|e.is_empty())}));
                }
                rows.push(serde_json::json!({"checkpoint_id":started.run_id,"name":started.strategy["name"],"symbol":started.dataset["symbol"],"state":if completed.is_empty(){"result_not_recorded"}else{"result_recorded"},"matching_engine":started.run.engine_id==engine,"results":completed}));
            }
            println!("{}",serde_json::json!({"checkpoints":rows,"scope":"Archive records only; absence of a result does not distinguish running, interrupted or failed work. Resume requires the matching executable."}));
            Ok(())
        }
        Cmd::ReplayBacktest { archive_dir, run_id, resume } => {
            let archive=sqx_project::archive::Archive::new(&archive_dir);
            let bundle=archive.load(&run_id).map_err(anyhow::Error::msg)?;
            if bundle.run.request["research_kind"]!=if resume {"cli_backtest_started"}else{"cli_backtest"} {anyhow::bail!("record type does not match replay/resume mode");}
            let engine=sqx_project::archive::content_id(&std::fs::read(std::env::current_exe()?)?);
            if engine!=bundle.run.engine_id {anyhow::bail!("engine executable differs from the recorded run; use the matching build");}
            let strategy:Strategy=serde_json::from_value(bundle.strategy.clone())?;
            let bars:Vec<Bar>=serde_json::from_value(bundle.dataset["bars"].clone())?;
            let ticks:Option<Vec<sqx_core::Tick>>=serde_json::from_value(bundle.dataset["ticks"].clone())?;
            let instrument:Instrument=serde_json::from_value(bundle.dataset["instrument"].clone())?;
            let config:BacktestConfig=serde_json::from_value(bundle.run.request["config"].clone())?;
            let provenance=&bundle.run.request["source_context"]["normalization"];
            if !provenance.is_null() {
                let normalization:sqx_data::normalization::Bundle=serde_json::from_value(provenance["bundle"].clone())?;
                let verified=normalization.verify_bars(&bars,config.bar_duration_ms).map_err(anyhow::Error::msg)?;
                if serde_json::to_value(verified)?!=provenance["verification"] {anyhow::bail!("archived normalization verification metadata differs from its bundle");}
            }
            let provenance=&bundle.run.request["source_context"]["tick_normalization"];
            if !provenance.is_null() {
                let normalization:sqx_data::normalization::Bundle=serde_json::from_value(provenance["bundle"].clone())?;
                let verified=normalization.verify_ticks(ticks.as_deref().ok_or_else(||anyhow!("archived tick provenance has no ticks"))?).map_err(anyhow::Error::msg)?;
                if serde_json::to_value(verified)?!=provenance["verification"] {anyhow::bail!("archived tick normalization verification metadata differs from its bundle");}
            }
            let result=match ticks {Some(t)=>backtest_ticks(&strategy,&bars,&t,&instrument,&config),None=>backtest(&strategy,&bars,&instrument,&config)};
            if resume {
                let mut request=bundle.run.request.clone();request["research_kind"]=serde_json::json!("cli_backtest");request["started_run_id"]=serde_json::json!(run_id);
                let id=archive.save(engine,bundle.dataset,bundle.strategy,request,serde_json::to_value(&result)?).map_err(anyhow::Error::msg)?;
                println!("Backtest archive: {}",serde_json::json!({"run_id":id,"root":archive_dir,"replay_available":true,"replay_command":"replay-backtest"}));
                if !result.errors.is_empty(){anyhow::bail!(result.errors.join("; "));}
                return Ok(());
            }
            let matches=serde_json::to_value(&result)?==bundle.run.result;
            println!("Backtest replay: {}",serde_json::json!({"run_id":run_id,"matches":matches,"execution_valid":result.errors.is_empty(),"scope":"Exact recorded execution comparison; not independent broker validation."}));
            if !matches {anyhow::bail!("replayed execution differs from the recorded result");}
            Ok(())
        }
        Cmd::Backtest { archive_dir, strategy, data, source, ticks, tick_normalization_bundle, account_config, tick_bar_ms, allow_tick_fallback, symbol, deposit, spread, slippage, commission, periods_per_year } => {
            let s: Strategy = load_strategy(&strategy)?;
            let bars = load_bars(&data)?;
            let instrument = instrument(&symbol);
            let pyp = resolve_periods_per_year(bars.len(), periods_per_year, &bars);
            let (duration,mut context)=source.audit(&bars)?;
            if tick_bar_ms.is_some() && tick_bar_ms!=duration {anyhow::bail!("tick-bar-ms conflicts with source timeframe {}",source.timeframe);}
            let config = BacktestConfig {
                initial_deposit: deposit,
                spread_points: spread,
                slippage_points: slippage,
                commission_per_lot: commission,
                bar_duration_ms: duration, allow_tick_fallback, intrabar_policy: sqx_backtest::IntrabarPolicy::AdverseFirst,
                account: match account_config {
                    Some(path) => serde_json::from_str(&std::fs::read_to_string(path)?)?,
                    None => Default::default(),
                },
            };
            let tick_data=match &ticks {
                Some(path)=>Some(sqx_data::read_ticks_csv(path.to_str().ok_or_else(||anyhow!("tick path is not UTF-8"))?).map_err(anyhow::Error::msg)?),
                None=>None,
            };
            if let Some(path)=tick_normalization_bundle {
                let bundle=sqx_data::normalization::Bundle::load(&path).map_err(anyhow::Error::msg)?;
                let verified=bundle.verify_ticks(tick_data.as_deref().ok_or_else(||anyhow!("tick normalization requires ticks"))?).map_err(anyhow::Error::msg)?;
                context["tick_normalization"]=serde_json::json!({"verification":verified,"bundle":bundle});
            }
            let checkpoint=if let Some(root)=&archive_dir {
                let archive=sqx_project::archive::Archive::new(root);
                let engine=archive.retain_current_engine().map_err(|e|anyhow!("Research engine could not be archived: {e}"))?;
                let dataset=serde_json::json!({"symbol":symbol,"timeframe":source.timeframe,"calendar":context["calendar"],"instrument":instrument,"bars":bars,"ticks":tick_data});
                let strategy=serde_json::to_value(&s)?;
                let mut request=serde_json::json!({"research_kind":"cli_backtest_started","config":config,"periods_per_year":pyp,"source_context":context});
                let started=archive.save(engine.clone(),dataset.clone(),strategy.clone(),request.clone(),serde_json::json!({"status":"execution_not_recorded"})).map_err(|e|anyhow!("Backtest evidence could not be archived: {e}"))?;
                println!("Backtest checkpoint: {}",serde_json::json!({"run_id":started,"root":root,"resume_command":"replay-backtest --resume"}));
                std::io::Write::flush(&mut std::io::stdout())?;
                request["research_kind"]=serde_json::json!("cli_backtest");request["started_run_id"]=serde_json::json!(started);
                Some((archive,engine,dataset,strategy,request))
            }else{None};
            let result=match &tick_data {
                Some(t)=>backtest_ticks(&s,&bars,t,&instrument,&config),
                None=>backtest(&s,&bars,&instrument,&config),
            };
            if let Some((archive,engine,dataset,strategy,request))=checkpoint {
                let id=archive.save(engine,dataset,strategy,request,serde_json::to_value(&result)?).map_err(|e|anyhow!("Backtest evidence could not be archived: {e}"))?;
                println!("Backtest archive: {}",serde_json::json!({"run_id":id,"root":archive_dir,"replay_available":true,"replay_command":"replay-backtest"}));
            }
            if !result.errors.is_empty() { anyhow::bail!(result.errors.join("; ")); }
            let m = compute_metrics(&result, bars.len(), pyp);
            println!("{}", render_pseudocode(&s));
            println!();
            println!("{}", render_metrics(&m));
            println!("Final equity: {:.2}", result.final_equity);
            println!("Trades: {}", result.trades.len());
            context["symbol"]=serde_json::json!(symbol);context["config"]=serde_json::json!(config);context["execution"]=serde_json::json!(result.execution);
            println!("Research context: {context}");
            Ok(())
        }
        Cmd::Analyze { strategy, data, source, config, symbol, periods_per_year } => {
            let s: Strategy = load_strategy(&strategy)?;
            let bars = load_bars(&data)?;
            let instrument = instrument(&symbol);
            let pyp = resolve_periods_per_year(bars.len(), periods_per_year, &bars);
            let (duration,mut context)=source.audit(&bars)?;
            let mut config:BacktestConfig=match config {Some(path)=>serde_json::from_str(&std::fs::read_to_string(path)?)?,None=>BacktestConfig::default()};
            if config.bar_duration_ms.is_some() && config.bar_duration_ms!=duration {anyhow::bail!("bar_duration_ms conflicts with source timeframe {}",source.timeframe);}
            config.bar_duration_ms=duration;
            let result = backtest(&s, &bars, &instrument, &config);
            if !result.errors.is_empty(){anyhow::bail!(result.errors.join("; "));}
            let report = sqx_analyzer::analyze(&result, &bars, pyp);
            if !report.errors.is_empty(){return Err(anyhow!(report.errors.join("; ")));}
            println!("{}", sqx_analyzer::render_markdown(&report));
            context["symbol"]=serde_json::json!(symbol);context["config"]=serde_json::json!(config);context["execution"]=serde_json::json!(result.execution);
            println!("Research context: {context}");
            Ok(())
        }
        Cmd::Cloud { strategies, data, symbol, source } => {
            if symbol.trim().is_empty(){anyhow::bail!("symbol must be nonempty");}
            let bars=load_bars(&data)?;let (_,mut context)=source.audit(&bars)?;
            let mut svc=sqx_cloud::CloudService::new();
            let n=svc.load_dir(&strategies).map_err(anyhow::Error::msg)?;
            if n==0 {anyhow::bail!("no strategies loaded");}
            let rows=svc.signals(&symbol,&bars);
            println!("Loaded {n} strategies");println!("{}",sqx_cloud::render_table(&rows));
            context["symbol"]=serde_json::json!(symbol);context["scope"]=serde_json::json!("Raw entry preview on closed candles; not position-aware execution or an executable trading instruction.");
            println!("Research context: {context}");
            if rows.iter().any(|row|!row.errors.is_empty()){anyhow::bail!("one or more entry previews unavailable; inspect reported errors");}
            Ok(())
        }
        Cmd::Assistant { data, research, base_url, model, api_key, ask } => {
            let bars = load_bars(&data)?;
            let (instrument,simulation,context)=research.prepare(&bars)?;
            println!("Research context: {context}");
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
            let base_url = if base_url.is_empty() {
                std::env::var("SQX_LLM_BASE_URL").ok().unwrap_or_else(|| {
                    let addr = "127.0.0.1:11434".parse().unwrap();
                    if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(250)).is_ok() {
                        "http://127.0.0.1:11434/v1".to_string()
                    } else {
                        "https://api.groq.com/openai/v1".to_string()
                    }
                })
            } else {
                base_url
            };
            let model = if model.is_empty() {
                std::env::var("SQX_LLM_MODEL").ok().unwrap_or_else(|| {
                    if base_url.contains("11434") {
                        "llama3.1".to_string()
                    } else {
                        "llama-3.3-70b-versatile".to_string()
                    }
                })
            } else {
                model
            };
            let api_key = if api_key.is_empty() {
                std::env::var("SQX_LLM_API_KEY").unwrap_or_default()
            } else {
                api_key
            };
            if api_key.is_empty() && !base_url.contains("127.0.0.1") && !base_url.contains("localhost") {
                eprintln!("warning: no API key (pass --api-key or set SQX_LLM_API_KEY)");
            }
            let cfg = sqx_assistant::LlmConfig { base_url, model, api_key, max_rounds: 6 };
            let archive=sqx_project::archive::Archive::new("data/research");
            let engine=archive.retain_current_engine().map_err(|e|anyhow!("Research engine could not be archived: {e}"))?;
            let mut a = sqx_assistant::Assistant::new(cfg, &research.symbol, bars, instrument, pyp).with_backtest_config(simulation)
                .with_trial_archive(archive.clone(),engine.clone(),context.clone());
            let answer=a.run(&ask);
            println!("Assistant candidate evidence: {}",serde_json::json!({"research_context":a.research_context(),"candidate_evaluations":a.candidate_evaluations(),"detail_evaluations":a.detail_evaluations()}));
            if !a.candidate_evaluations().is_empty() {
                let dataset=serde_json::json!({"symbol":research.symbol,"timeframe":research.source.timeframe,"instrument":a.instrument,"calendar":context["calendar"],"bars":a.bars});
                let request=serde_json::json!({"research_kind":"assistant_candidates","research_context":a.research_context(),"source_context":context});
                let result=serde_json::json!({"candidate_evaluations":a.candidate_evaluations(),"detail_evaluations":a.detail_evaluations(),"trial_executions":a.trial_executions(),"session_completed":answer.is_ok(),"scope":"Generation and details summaries with full executed trial results. Per-trial checkpoints preserve inputs/results; provider-conversation resume and independent validation are unavailable."});
                let id=archive.save(engine,dataset,serde_json::json!({"name":"AI candidate research","kind":"assistant_candidates"}),request,result)
                    .map_err(|error|anyhow!("Assistant evidence could not be archived: {error}"))?;
                println!("Assistant archive: {}",serde_json::json!({"run_id":id,"root":"data/research","replay_available":true,"replay_command":"replay-crosscheck"}));
            }
            match answer {
                Ok(answer) => {
                    println!("{answer}");
                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!(e)),
            }
        }
        Cmd::Generate { data, research, count, seed, top, min_trades, out } => {
            let bars = load_bars(&data)?;
            let (instrument,config,mut context)=research.prepare(&bars)?;
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
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
            context["seed"]=serde_json::json!(seed);context["candidates_attempted"]=serde_json::json!(count);
            println!("Research context: {context}");
            Ok(())
        }
        Cmd::Evolve { data, research, seed, population, generations, min_trades, audit_log } => {
            let bars = load_bars(&data)?;
            let (instrument,config,mut context)=research.prepare_frozen(&bars)?;
            context["seed"]=serde_json::json!(seed);
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
            let gen = GeneratorConfig::default();
            let ga = GeneticConfig { population_size: population, generations, min_trades, ..Default::default() };
            let mut audit=match audit_log {Some(path)=>{
                let executable=std::fs::read(std::env::current_exe()?)?;
                let mut log=audit_log::AuditLog::create(&path)?;
                log.append(&serde_json::json!({"kind":"started","schema":1,"engine_id":sqx_project::archive::content_id(&executable),"bars":bars,"instrument":instrument,"backtest_config":config,"periods_per_year":pyp,"generator":gen,"seed":seed,"fitness":"ProfitFactor","genetic":{"population_size":ga.population_size,"generations":ga.generations,"min_trades":ga.min_trades,"elite_count":ga.elite_count,"crossover_prob":ga.crossover_prob,"mutation_prob":ga.mutation_prob},"research_context":context,"metric_encoding":"Nonfinite floating-point metrics are JSON null; eligibility and errors are explicit."}))?;
                Some(log)
            },None=>None};
            let mut attempted=0usize;let mut invalid=0usize;let mut eligible=0usize;let mut examples=Vec::new();
            let ranked = genetic_evolve_observed(seed, &gen, &ga, Fitness::ProfitFactor, &bars, &instrument, &config, pyp,|generation,index,evaluation,qualifies|{
                if let Some(log)=audit.as_mut(){log.append(&serde_json::json!({"kind":"evaluation","generation":generation,"candidate_index":index,"strategy":evaluation.strategy,"metrics":evaluation.metrics,"errors":evaluation.errors,"eligible":qualifies})).map_err(|e|format!("Cannot persist evaluation: {e}"))?;}
                attempted+=1;if qualifies{eligible+=1;}
                if !evaluation.errors.is_empty(){invalid+=1;if examples.len()<10{examples.push(format!("Generation {generation}, candidate {index} ({}): {}",evaluation.strategy.name,evaluation.errors.join("; ")));}}
                println!("Research context: {context}");
            Ok(())
            });
            let ranked=match ranked {Ok(ranked)=>ranked,Err(error)=>{
                if let Some(log)=audit.as_mut(){let _=log.append(&serde_json::json!({"kind":"failed","error":error,"recorded_attempts":attempted}));}
                return Err(anyhow!(error));
            }};
            if let Some(log)=audit.as_mut(){log.append(&serde_json::json!({"kind":"completed","attempted":attempted,"invalid":invalid,"eligible":eligible,"ranked":ranked.iter().map(|(strategy,metrics)|serde_json::json!({"strategy":strategy,"metrics":metrics})).collect::<Vec<_>>()}))?;}
            println!("Evaluation attempts: {attempted}; invalid: {invalid}; eligible for ranking: {eligible}; valid but below ranking criteria: {}. Repeated elites count as repeated evaluations, not independent trials.",attempted-invalid-eligible);
            for error in &examples{eprintln!("{error}");}
            if invalid>examples.len(){eprintln!("{} further invalid evaluations; showing the first 10 diagnostics.",invalid-examples.len());}
            for (i, (s, m)) in ranked.iter().take(10).enumerate() {
                println!("{}. {}  PF={:.2}  Net={:.2}  Trades={:.0}", i + 1, s.name, m.profit_factor, m.net_profit, m.number_of_trades);
            }
            println!("Research context: {context}");
            Ok(())
        }
        Cmd::VerifyEvolution { audit_log } => {
            let attempted=audit_log::verify(&audit_log).map_err(anyhow::Error::msg)?;
            println!("Replay verified: {attempted} candidate evaluations and the final ranking match the log. This does not authenticate its origin or establish independent validation.");
            Ok(())
        }
        Cmd::StartEvolution {data,store,request_key,research,seed,population,generations,min_trades}=>{
            let bars=load_bars(&data)?;let pyp=resolve_periods_per_year(bars.len(),0.0,&bars);
            let (instrument,config,mut context)=research.prepare_frozen(&bars)?;context["source"]=serde_json::json!("cli");context["seed"]=serde_json::json!(seed);
            let ga=GeneticConfig{population_size:population,generations,min_trades,..Default::default()};
            let state=sqx_optimizer::EvolutionState::new(seed,&GeneratorConfig::default(),&ga,Fitness::ProfitFactor,&bars,&instrument,&config,pyp).map_err(anyhow::Error::msg)?;
            let engine=sqx_project::archive::Archive::new(&store).retain_current_engine()
                .map_err(|e|anyhow!("Research engine could not be archived: {e}"))?;
            let jobs=sqx_project::evolution::EvolutionJobs::new(store);
            let id=jobs.create_with_context(&engine,&request_key,&state,context).map_err(anyhow::Error::msg)?;
            println!("{}",serde_json::to_string(&jobs.status(&id).map_err(anyhow::Error::msg)?)?);Ok(())
        }
        Cmd::ResumeEvolution {store,job_id,steps,expected_sequence}=>{
            if steps==0 || steps>1001{return Err(anyhow!("steps must be between 1 and 1001"));}
            let jobs=sqx_project::evolution::EvolutionJobs::new(store);
            let engine=sqx_project::archive::content_id(&std::fs::read(std::env::current_exe()?)?);
            for offset in 0..steps {
                let expected=expected_sequence.checked_add(offset).ok_or_else(||anyhow!("Expected sequence overflow"))?;
                let status=jobs.status(&job_id).map_err(anyhow::Error::msg)?;
                if status.committed_generations!=expected{return Err(anyhow!("Evolution progress changed; inspect status before requesting more work"));}
                if status.complete{break;}
                jobs.advance(&job_id,&engine,expected).map_err(anyhow::Error::msg)?;
            }
            println!("{}",serde_json::to_string(&jobs.status(&job_id).map_err(anyhow::Error::msg)?)?);Ok(())
        }
        Cmd::EvolutionStatus {store,job_id}=>{
            let jobs=sqx_project::evolution::EvolutionJobs::new(store);
            let (status,latest)=jobs.snapshot(&job_id).map_err(anyhow::Error::msg)?;
            println!("{}",serde_json::json!({"status":status,"metadata":jobs.metadata(&job_id).map_err(anyhow::Error::msg)?,"engine_retention":jobs.engine_retention(&job_id).map_err(anyhow::Error::msg)?,"latest_ranked":latest.map(|record|record.ranked),"interpretation":"Exploratory search; repeated generations are not independent validation."}));Ok(())
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
            let report = export_strategy(&s, eng);
            for limitation in &report.limitations { eprintln!("{limitation}"); }
            let code = report.code.ok_or_else(|| anyhow!("Export blocked: {}", report.blockers.join(" ")))?;
            match out {
                Some(p) => std::fs::write(&p, &code).map_err(|e| anyhow!(e))?,
                None => println!("{code}"),
            }
            Ok(())
        }
        Cmd::Montecarlo { research, strategy, data, runs, skip, seed } => {
            let s: Strategy = load_strategy(&strategy)?;
            let bars = load_bars(&data)?;
            let (inst,cfg,mut context)=research.prepare(&bars)?;
            let result = backtest(&s, &bars, &inst, &cfg);
            if !result.errors.is_empty(){anyhow::bail!(result.errors.join("; "));}
            let report = monte_carlo_trade_manipulation(&result.trades, runs, skip, seed);
            println!("{}", sqx_robustness::render(&report));
            context["seed"]=serde_json::json!(seed);context["requested_runs"]=serde_json::json!(runs);context["skip_probability"]=serde_json::json!(skip);context["monte_carlo_errors"]=serde_json::json!(report.errors);
            println!("Research context: {context}");
            if !report.errors.is_empty(){anyhow::bail!(report.errors.join("; "));}
            Ok(())
        }
        Cmd::Optimize { data, research, windows, oos, candidates, seed } => {
            let bars = load_bars(&data)?;
            let (inst,config,mut context)=research.prepare(&bars)?;
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
            let gen = GeneratorConfig::default();
            let report = walk_forward(&bars, &inst, &config, pyp, &gen, windows, oos, candidates, seed);
            println!("Walk-forward optimization ({} windows)", report.windows);
            println!("  Candidates tried  : {}", report.candidates_attempted);
            println!("  Trade evidence    : {} (not a robustness certificate)", report.evidence_sufficient);
            for error in &report.errors { eprintln!("  Validation issue: {error}"); }
            println!("  OOS trades        : {}", report.oos_trades);
            println!("  OOS net profit    : {:.2}", report.oos_net_profit);
            println!("  OOS profit factor : {:.2}", report.oos_profit_factor);
            println!("  OOS winning %     : {:.1}", report.oos_winning_pct);
            println!("  OOS max DD %      : {:.2}", report.oos_max_dd_pct);
            context["seed"]=serde_json::json!(seed);context["walk_forward"]=serde_json::json!(report);
            println!("Research context: {context}");
            if !report.errors.is_empty(){anyhow::bail!(report.errors.join("; "));}
            Ok(())
        }
        Cmd::Serve { addr, data, data_dir, data_manifest, data_timeframe, symbol, open, questdb, questdb_symbols, questdb_timeframe, questdb_limit } => {
            if data_timeframe.is_some() && data.is_none() && data_dir.is_none(){anyhow::bail!("data-timeframe requires a CSV data source");}
            let mut store = DataStore::new();
            // With --questdb the client is handed to the dashboard, which lists
            // symbols and loads them on demand from the Data Manager tab.
            let questdb_client = if questdb {
                let cfg = QuestdbConfig::from_env();
                let client = QuestdbClient::new(&cfg);
                if !questdb_symbols.trim().is_empty() {
                    let tf = parse_timeframe(&questdb_timeframe)?;
                    for sym in questdb_symbols.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                        match client.fetch_bars(sym, tf, None, None, Some(questdb_limit)) {
                            Ok(bars) => {
                                println!("Loaded {} bars for {} ({})", bars.len(), sym, tf);
                                store.add(sym, tf, bars);
                            }
                            Err(e) => eprintln!("QuestDB {sym}: {e}"),
                        }
                    }
                }
                Some(client)
            } else {
                None
            };
            if questdb_client.is_none() {
                if let Some(manifest)=data_manifest {
                    store=load_data_manifest(&manifest)?;
                    for series in &store.series {println!("Loaded {} bars for {} ({})",series.bars.len(),series.symbol,series.timeframe);}
                } else if let Some(dir) = data_dir {
                    let loaded = store.import_dir(dir.to_str().unwrap(), data_timeframe.expect("CLI requires CSV timeframe")).map_err(|e| anyhow!(e))?;
                    for (s, n) in &loaded {
                        println!("Loaded {} bars for {}", n, s);
                    }
                } else if let Some(file) = data {
                    let symbol=symbol.as_deref().expect("CLI requires CSV symbol").trim();
                    if symbol.is_empty(){anyhow::bail!("symbol must be nonempty");}
                    let n = store.import_csv(file.to_str().unwrap(), symbol, data_timeframe.expect("CLI requires CSV timeframe")).map_err(|e| anyhow!(e))?;
                    println!("Loaded {} bars for {}", n, symbol);
                } else {
                    for sym in ["EURUSD", "GBPUSD"] {
                        store.add(sym, Timeframe::M1, sqx_data::synthetic_bars(sym, 2000));
                    }
                    println!("No CSV source given: loaded synthetic demo data for EURUSD and GBPUSD.");
                }
            }
            if store.is_empty() && questdb_client.is_none() {
                return Err(anyhow!("no data loaded"));
            }
            if open {
                let url = format!("http://{addr}");
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(600));
                    #[cfg(target_os = "macos")]
                    let _ = std::process::Command::new("open").arg(&url).status();
                    #[cfg(target_os = "linux")]
                    let _ = std::process::Command::new("xdg-open").arg(&url).status();
                    #[cfg(target_os = "windows")]
                    let _ = std::process::Command::new("cmd").args(["/C", "start", &url]).status();
                });
            }
            sqx_web::serve(&addr, store, questdb_client).map_err(|e| anyhow!(e))?;
            Ok(())
        }
        Cmd::Questdb { sub } => {
            let client = QuestdbClient::new(&QuestdbConfig::from_env());
            match sub {
                QuestdbCmd::Symbols => {
                    let syms = client.list_symbols().map_err(|e| anyhow!(e))?;
                    for s in &syms {
                        println!("{s}");
                    }
                    if syms.is_empty() {
                        println!("(no symbols)");
                    }
                    Ok(())
                }
                QuestdbCmd::Bars { symbol, timeframe, limit, from, to, out } => {
                    let tf = parse_timeframe(&timeframe)?;
                    let start_ms = from.map(|s| s * 1000);
                    let end_ms = to.map(|s| s * 1000);
                    let bars = client
                        .fetch_bars(&symbol, tf, start_ms, end_ms, Some(limit))
                        .map_err(|e| anyhow!(e))?;
                    println!(
                        "{} {}: {} bars ({} .. {})",
                        symbol,
                        tf,
                        bars.len(),
                        bars.first().map(|b| b.time).unwrap_or(0),
                        bars.last().map(|b| b.time).unwrap_or(0)
                    );
                    if let Some(first) = bars.first() {
                        println!(
                            "  first: time={} o={:.5} h={:.5} l={:.5} c={:.5} v={:.0}",
                            first.time, first.open, first.high, first.low, first.close, first.volume
                        );
                    }
                    if let Some(last) = bars.last() {
                        println!(
                            "  last : time={} o={:.5} h={:.5} l={:.5} c={:.5} v={:.0}",
                            last.time, last.open, last.high, last.low, last.close, last.volume
                        );
                    }
                    if let Some(p) = out {
                        write_bars_csv(&p, &bars).map_err(|e| anyhow!(e))?;
                        println!("Wrote {} bars to {}", bars.len(), p.display());
                    }
                    Ok(())
                }
            }
        }
        Cmd::Doctor => {
            println!("SqxLabs doctor");
            let rustc = std::process::Command::new("rustc")
                .arg("--version")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "rustc not found".to_string());
            println!("  rustc: {rustc}");
            for p in [
                "data",
                "data/mt5",
                "data/projects",
                "data/databank.json",
                "data/profiles.json",
                "examples/rsi-mean-reversion.json",
            ] {
                let s = if std::path::Path::new(p).exists() { "ok" } else { "missing" };
                println!("  {p}: {s}");
            }
            match std::net::TcpListener::bind("127.0.0.1:8080") {
                Ok(_) => println!("  port 8080: free"),
                Err(e) => println!("  port 8080: {e}"),
            }
            let mt5 = std::net::TcpStream::connect_timeout(
                &"127.0.0.1:8002".parse().unwrap(),
                std::time::Duration::from_millis(250),
            )
            .is_ok();
            println!(
                "  MT5 runtime (127.0.0.1:8002): {}",
                if mt5 { "running" } else { "not running (optional)" }
            );
            Ok(())
        }
        Cmd::Portfolio { research, data, count, seed, top, min_k, max_k } => {
            let bars = load_bars(&data)?;
            let (inst,cfg,mut context)=research.prepare(&bars)?;
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
            let gen = GeneratorConfig::default();
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let strategies: Vec<Strategy> = (0..count).map(|i| random_strategy(&mut rng, &format!("p{i:04}"), &gen)).collect();
            let names: Vec<String> = strategies.iter().map(|s| s.name.clone()).collect();
            let results: Vec<BacktestResult> = backtest_all(&strategies, &bars, &inst, &cfg);
            context["seed"]=serde_json::json!(seed);context["candidates_attempted"]=serde_json::json!(count);
            context["invalid_candidates"]=serde_json::json!(results.iter().enumerate().filter(|(_,r)|!r.errors.is_empty()).map(|(i,r)|serde_json::json!({"candidate_index":i,"errors":r.errors})).collect::<Vec<_>>());
            context["portfolio_scope"]=serde_json::json!("Weighted independent account simulations; not shared margin or joint account execution.");
            println!("Research context: {context}");
            let mets: Vec<sqx_metrics::Metrics> = results.iter().map(|r| compute_metrics(r, bars.len(), pyp)).collect();
            let mut order: Vec<usize> = (0..results.len()).collect();
            order.sort_by(|&a, &b| mets[b].profit_factor.partial_cmp(&mets[a].profit_factor).unwrap_or(std::cmp::Ordering::Equal));
            order.retain(|&i| results[i].errors.is_empty() && mets[i].number_of_trades >= 5.0 && mets[i].profit_factor.is_finite());
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
            if !pf.errors.is_empty(){return Err(anyhow!(pf.errors.join("; ")));}
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
        Cmd::ReplayCrosscheck {archive_dir,run_id,resume}=>{
            let archive=sqx_project::archive::Archive::new(&archive_dir);
            let bundle=archive.load(&run_id).map_err(anyhow::Error::msg)?;
            if bundle.run.request["research_kind"]!=if resume{"cli_crosscheck_started"}else{"cli_crosscheck"}{anyhow::bail!("record type does not match replay/resume mode");}
            let engine=sqx_project::archive::content_id(&std::fs::read(std::env::current_exe()?)?);
            if engine!=bundle.run.engine_id {anyhow::bail!("engine executable differs from the recorded run; use the matching build");}
            let strategy:Strategy=serde_json::from_value(bundle.strategy.clone())?;
            let min_trades:usize=serde_json::from_value(bundle.run.request["minimum_trades"].clone())?;
            if min_trades==0 {anyhow::bail!("minimum trades must be positive");}
            let markets=bundle.dataset["markets"].as_array().ok_or_else(||anyhow!("missing archived markets"))?;
            if markets.is_empty(){anyhow::bail!("empty archived markets");}
            let mut prepared=Vec::new();
            for market in markets {
                let bars:Vec<Bar>=serde_json::from_value(market["bars"].clone())?;
                let instrument:Instrument=serde_json::from_value(market["instrument"].clone())?;
                let config:BacktestConfig=serde_json::from_value(market["config"].clone())?;
                let context=market["source_context"].clone();
                let provenance=&context["normalization"];
                if !provenance.is_null(){
                    let normalization:sqx_data::normalization::Bundle=serde_json::from_value(provenance["bundle"].clone())?;
                    let verified=normalization.verify_bars(&bars,config.bar_duration_ms).map_err(anyhow::Error::msg)?;
                    if serde_json::to_value(verified)?!=provenance["verification"]{anyhow::bail!("archived normalization verification metadata differs from its bundle");}
                }
                prepared.push((bars,instrument,config,context));
            }
            let result=execute_crosscheck(&strategy,prepared,min_trades)?;
            if resume {
                let mut request=bundle.run.request.clone();request["research_kind"]=serde_json::json!("cli_crosscheck");request["started_run_id"]=serde_json::json!(run_id);
                let id=archive.save(engine,bundle.dataset,bundle.strategy,request,result.clone()).map_err(anyhow::Error::msg)?;
                println!("Crosscheck archive: {}",serde_json::json!({"run_id":id,"root":archive_dir,"replay_available":true,"replay_command":"replay-crosscheck"}));
                if result["report"]["invalid_markets"]!=0 {anyhow::bail!("resumed crosscheck contains invalid simulations");}
            }else{
                let matches=result==bundle.run.result;
                println!("Crosscheck replay: {}",serde_json::json!({"run_id":run_id,"matches":matches,"execution_valid":result["report"]["invalid_markets"]==0,"scope":"Exact archived execution comparison; not independent broker validation."}));
                if !matches{anyhow::bail!("replayed crosscheck differs from the recorded result");}
            }
            Ok(())
        }
        Cmd::Crosscheck { archive_dir, strategy, manifest, min_trades } => {
            if min_trades==0 {anyhow::bail!("min-trades must be positive");}
            let s:Strategy=load_strategy(&strategy)?;
            let input:CrosscheckManifest=serde_json::from_str(&std::fs::read_to_string(&manifest)?)?;
            if input.schema!=1 || input.markets.is_empty(){anyhow::bail!("crosscheck requires schema 1 and a nonempty markets list");}
            let parent=manifest.parent().unwrap_or_else(||std::path::Path::new("."));
            let resolve=|path:PathBuf|if path.is_absolute(){path}else{parent.join(path)};
            let mut seen=std::collections::HashSet::new();let mut prepared=Vec::new();
            // Validate every source first: a bad final file must not leave a partial comparison.
            for market in input.markets {
                if !seen.insert((market.symbol.trim().to_uppercase(),market.timeframe)){anyhow::bail!("duplicate symbol/timeframe in crosscheck manifest");}
                let bars=load_bars(&resolve(market.data))?;
                let options=ResearchOptions{source:SourceOptions{normalization_bundle:market.normalization_bundle.map(&resolve),timeframe:market.timeframe,calendar:market.calendar.map(&resolve),allow_data_gaps:market.allow_data_gaps},symbol:market.symbol,config:market.config.map(&resolve)};
                let (instrument,config,context)=options.prepare(&bars)?;
                prepared.push((bars,instrument,config,context));
            }
            let archive=sqx_project::archive::Archive::new(&archive_dir);
            let engine=archive.retain_current_engine().map_err(|e|anyhow!("Research engine could not be archived: {e}"))?;
            let dataset=serde_json::json!({"markets":prepared.iter().map(|(bars,instrument,config,context)|serde_json::json!({"bars":bars,"instrument":instrument,"config":config,"source_context":context})).collect::<Vec<_>>()});
            let strategy_value=serde_json::to_value(&s)?;
            let mut request=serde_json::json!({"research_kind":"cli_crosscheck_started","minimum_trades":min_trades});
            let started=archive.save(engine.clone(),dataset.clone(),strategy_value.clone(),request.clone(),serde_json::json!({"status":"execution_not_recorded"})).map_err(anyhow::Error::msg)?;
            println!("Crosscheck checkpoint: {}",serde_json::json!({"run_id":started,"root":archive_dir}));
            std::io::Write::flush(&mut std::io::stdout())?;
            let result=execute_crosscheck(&s,prepared,min_trades)?;
            let report=&result["report"];
            let invalid=report["invalid_markets"].as_u64().expect("count");
            request["research_kind"]=serde_json::json!("cli_crosscheck");request["started_run_id"]=serde_json::json!(started);
            let id=archive.save(engine,dataset,strategy_value,request,result.clone()).map_err(anyhow::Error::msg)?;
            println!("Crosscheck archive: {}",serde_json::json!({"run_id":id,"root":archive_dir,"replay_available":false}));
            println!("Research context: {report}");
            println!("Profitable on {}/{} eligible markets out of {} requested; invalid: {invalid}",report["profitable_markets"],report["eligible_markets"],report["requested_markets"]);
            if invalid>0 {anyhow::bail!("crosscheck contains invalid simulations; inspect per-market errors");}
            Ok(())
        }
        Cmd::PipelineHistory{archive_dir,run_id}=>{
            let archive=sqx_project::archive::Archive::new(archive_dir);
            println!("{}",pipeline_archive::inspect(&archive,&run_id).map_err(anyhow::Error::msg)?);
            Ok(())
        },
        Cmd::ExportPipeline{archive_dir,run_id}=>{
            let archive=sqx_project::archive::Archive::new(archive_dir);
            println!("{}",pipeline_archive::export(&archive,&run_id).map_err(anyhow::Error::msg)?);
            Ok(())
        },
        Cmd::ImportPipeline{archive_dir,input}=>{
            let archive=sqx_project::archive::Archive::new(archive_dir);
            let value=serde_json::from_slice(&std::fs::read(input)?)?;
            println!("{}",pipeline_archive::import(&archive,value).map_err(anyhow::Error::msg)?);
            Ok(())
        },
        Cmd::ReplayPipeline{archive_dir,run_id,resume}=>{
            let archive=sqx_project::archive::Archive::new(archive_dir);
            let recovered=pipeline_archive::recover(&archive,&run_id,resume).map_err(anyhow::Error::msg)?;
            println!("{}",serde_json::json!({"started_run_id":run_id,"completion_id":recovered.completion_id,
                "replayed_events":recovered.replayed,"appended_events":recovered.appended,
                "matches":true,"execution_valid":recovered.report.errors.is_empty(),"errors":recovered.report.errors,
                "scope":"Recorded calculations reproduced; not independent validation or untouched holdout evidence."}));
            Ok(())
        },
        Cmd::Pipeline { archive_dir, data, research, seed, count, keep } => {
            let bars = load_bars(&data)?;
            let (inst,cfg,mut context)=research.prepare(&bars)?;
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
            let gen = GeneratorConfig::default();
            let pipeline = sqx_pipeline::default_pipeline(count, keep);
            let archive=sqx_project::archive::Archive::new(&archive_dir);
            let engine=archive.retain_current_engine().map_err(anyhow::Error::msg)?;
            let dataset=serde_json::json!({"bars":bars,"instrument":inst,"timeframe":context["timeframe"],"source_context":context});
            let specification=serde_json::json!({"pipeline":pipeline,"generator":gen});
            let request=serde_json::json!({"research_kind":"cli_pipeline_started","attempt_id":format!("{:032x}",rand::random::<u128>()),"config":cfg,"seed":seed,"periods_per_year":pyp});
            let started=archive.save(engine.clone(),dataset.clone(),specification.clone(),request.clone(),serde_json::json!({"status":"execution_not_recorded"})).map_err(anyhow::Error::msg)?;
            println!("Pipeline checkpoint: {}",serde_json::json!({"run_id":started,"root":archive_dir}));
            std::io::Write::flush(&mut std::io::stdout())?;
            println!("Pipeline: {}", pipeline.name);
            let recovered=pipeline_archive::recover(&archive,&started,true)
                .map_err(|e|anyhow!("Pipeline stopped; inspect its archived prefix: {e}"))?;
            let report=recovered.report;
            println!("Pipeline archive: {}",serde_json::json!({"run_id":recovered.completion_id,"root":archive_dir,"replay_available":true}));
            context["seed"]=serde_json::json!(seed);context["pipeline"]=serde_json::json!({"name":pipeline.name,"requested_candidates":count,"keep":keep,"build_evaluations":report.builds.len(),"retest_evaluations":report.retests.len(),"optimization_evaluations":report.optimizations.len(),"errors":report.errors});
            println!("Research context: {context}");
            if !report.errors.is_empty(){return Err(anyhow!(report.errors.join("; ")));}
            for line in &report.log {
                println!("  {}", line);
            }
            Ok(())
        }
        Cmd::Robustness { research, strategy, data, mc_runs, retest_runs, windows } => {
            let s: Strategy = load_strategy(&strategy)?;
            let bars = load_bars(&data)?;
            let (inst,cfg,mut context)=research.prepare(&bars)?;
            let pyp = resolve_periods_per_year(bars.len(), 0.0, &bars);
            let base = backtest(&s, &bars, &inst, &cfg);
            if !base.errors.is_empty(){return Err(anyhow!(base.errors.join("; ")));}
            let base_m = compute_metrics(&base, bars.len(), pyp);
            println!("Base: net {:.2} PF {:.2} DD {:.2}% trades {:.0}", base_m.net_profit, base_m.profit_factor, base_m.max_drawdown_pct, base_m.number_of_trades);
            let scenarios = [(1.0, 0.0, 0.0), (2.0, 0.0, 0.0), (2.0, 1.0, 0.0), (3.0, 1.0, 5.0), (5.0, 2.0, 10.0)];
            let rows = what_if(&s, &bars, &inst, &cfg, &scenarios);
            println!("What-if:");
            for r in &rows {
                if !r.errors.is_empty(){println!("  Invalid scenario: {}",r.errors.join("; "));continue;}
                println!("  spread={:.0} slip={:.0} comm={:.0} -> net {:.2} PF {:.2}", r.spread, r.slippage, r.commission, r.net_profit, r.profit_factor);
            }
            let mc = monte_carlo_trade_manipulation(&base.trades, mc_runs, 0.1, 42);
            println!("{}",sqx_robustness::render(&mc));
            let rt = monte_carlo_retest(&s, &bars, &inst, &cfg, retest_runs, 7);
            println!("MC retest assumptions: seed {}; {}",rt.seed,rt.warmup_policy);
            println!("MC retest coverage: requested {}, skipped {}, invalid {}; issues: {:?}",rt.requested_runs,rt.skipped_runs,rt.invalid_runs,rt.errors);
            println!("MC retest: runs {} net5% {:.2} net50% {:.2} net95% {:.2} losing {:.1}%", rt.runs, rt.net_profit_5pct, rt.net_profit_50pct, rt.net_profit_95pct, rt.losing_runs_pct);
            let gen = GeneratorConfig::default();
            let wf = walk_forward(&bars, &inst, &cfg, pyp, &gen, windows, 0.25, 20, 42);
            println!("Walk-forward: {} windows, OOS net {:.2} PF {:.2}, evidence sufficient: {}, attempted candidates: {}, issues: {:?}", wf.windows, wf.oos_net_profit, wf.oos_profit_factor, wf.evidence_sufficient, wf.candidates_attempted, wf.errors);
            context["monte_carlo"]=serde_json::json!({"seed":42,"requested_runs":mc_runs,"errors":mc.errors});
            context["mc_retest"]=serde_json::json!({"seed":7,"requested_runs":rt.requested_runs,"skipped_runs":rt.skipped_runs,"invalid_runs":rt.invalid_runs,"errors":rt.errors});
            context["walk_forward"]=serde_json::json!(wf);
            let mut errors=Vec::new();
            for (index,row) in rows.iter().enumerate(){
                errors.extend(row.errors.iter().map(|error|format!("What-if scenario {index}: {error}")));
            }
            errors.extend(mc.errors.iter().map(|error|format!("Monte Carlo: {error}")));
            errors.extend(rt.errors.iter().map(|error|format!("MC retest: {error}")));
            errors.extend(wf.errors.iter().map(|error|format!("Walk-forward: {error}")));
            context["errors"]=serde_json::json!(errors);
            println!("Research context: {context}");
            if !errors.is_empty(){anyhow::bail!(errors.join("; "));}
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
    let bars=sqx_data::read_csv_bars_str(&text).map_err(anyhow::Error::msg)
        .with_context(||format!("parsing {}",path.display()))?;
    if bars.is_empty() {
        return Err(anyhow!("no bars loaded from {}", path.display()));
    }
    Ok(bars)
}

/// Parse a timeframe label ("M5", "H1", "1h", "1d", ...) into the enum.
fn parse_timeframe(s: &str) -> Result<Timeframe> {
    use Timeframe::*;
    let t = s.trim();
    let up = t.to_ascii_uppercase();
    let low = t.to_ascii_lowercase();
    Ok(match (up.as_str(), low.as_str()) {
        ("M1", _) | (_, "1m") => M1,
        ("M2", _) | (_, "2m") => M2,
        ("M3", _) | (_, "3m") => M3,
        ("M5", _) | (_, "5m") => M5,
        ("M10", _) | (_, "10m") => M10,
        ("M15", _) | (_, "15m") => M15,
        ("M30", _) | (_, "30m") => M30,
        ("H1", _) | (_, "1h") => H1,
        ("H2", _) | (_, "2h") => H2,
        ("H4", _) | (_, "4h") => H4,
        ("H6", _) | (_, "6h") => H6,
        ("H8", _) | (_, "8h") => H8,
        ("H12", _) | (_, "12h") => H12,
        ("D1", _) | (_, "1d") => D1,
        ("W1", _) | (_, "1w") | (_, "7d") => W1,
        ("MN1", _) | (_, "1mn") | (_, "1mth") | (_, "1mon") => MN1,
        _ => return Err(anyhow!("unknown timeframe '{t}' (use M1/M5/H1/H4/D1/W1/MN1 or 1m/5m/1h/1d)")),
    })
}

/// Write bars as CSV (header timestamp,open,high,low,close,volume; unix seconds).
fn write_bars_csv(path: &PathBuf, bars: &[Bar]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| anyhow!(e))?;
        }
    }
    let mut out = String::from("timestamp,open,high,low,close,volume\n");
    for b in bars {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            b.time / 1000,
            b.open,
            b.high,
            b.low,
            b.close,
            b.volume
        ));
    }
    std::fs::write(path, out).map_err(|e| anyhow!(e))
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
