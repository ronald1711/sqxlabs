//! # sqx-project
//!
//! Project and databank persistence. The StrategyQuant .cfx container is a
//! flat ZIP with a config.xml task list; this open-source port uses the same
//! logical model (project -> databanks -> ranked strategies) serialized as
//! JSON for portability and easy inspection.

use serde::{Deserialize, Serialize};

use sqx_metrics::Metrics;
use sqx_strategy::Strategy;

/// A compact subset of metrics stored alongside each strategy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct MetricSummary {
    pub net_profit: f64,
    pub profit_factor: f64,
    pub return_dd: f64,
    pub number_of_trades: f64,
    pub winning_pct: f64,
    pub max_drawdown_pct: f64,
    pub sqn: f64,
    pub sharpe: f64,
}

impl From<&Metrics> for MetricSummary {
    fn from(m: &Metrics) -> Self {
        Self {
            net_profit: m.net_profit,
            profit_factor: m.profit_factor,
            return_dd: m.return_dd_ratio,
            number_of_trades: m.number_of_trades,
            winning_pct: m.winning_pct,
            max_drawdown_pct: m.max_drawdown_pct,
            sqn: m.sqn,
            sharpe: m.sharpe_ratio,
        }
    }
}

/// A strategy plus its performance summary — one databank row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRecord {
    pub strategy: Strategy,
    pub metrics: MetricSummary,
}

/// A databank: a named store of ranked strategies.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Databank {
    pub name: String,
    pub strategies: Vec<StrategyRecord>,
}

/// A project: a name plus its databanks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub version: String,
    pub databanks: Vec<Databank>,
}

impl Project {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            databanks: Vec::new(),
        }
    }
}

/// Serialize a project to JSON text.
pub fn to_json(project: &Project) -> Result<String, String> {
    serde_json::to_string_pretty(project).map_err(|e| e.to_string())
}

/// Parse a project from JSON text.
pub fn from_json(text: &str) -> Result<Project, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

/// Save a project to a file.
pub fn save(path: &str, project: &Project) -> Result<(), String> {
    let json = to_json(project)?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Load a project from a file.
pub fn load(path: &str) -> Result<Project, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    from_json(&text)
}

pub mod archive;
pub mod datasets;

pub mod experiment;

pub mod reconciliation;

pub mod evolution;

pub mod assistant_trials;

mod observation;

pub mod search_requirements;
pub mod pipeline_archive;

pub mod mcp_config;

pub mod mcp_probe;

pub mod mcp_logs;

pub mod mcp_runtime;
