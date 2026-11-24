//! # sqx-export
//!
//! Clean-room code exporters: translate a strategy's rule tree into platform
//! source code. Mirrors StrategyQuant's "export to platform" feature
//! (MetaTrader, EasyLanguage, NinjaTrader, PseudoCode), here for MQL4, MQL5 and
//! PineScript v5, plus the built-in pseudo-code renderer.

mod easy;
mod mql;
mod ninja;
mod pine;

use easy::render_easylanguage;
use mql::{render_mql4, render_mql5};
use ninja::render_ninjascript;
use pine::render_pine;

use sqx_strategy::Strategy;

// Keep stochastic parameter rounding/defaults aligned with backtest registry.
fn stochastic_period(params: &[sqx_strategy::Param], default: usize) -> usize {
    params.iter().find(|p| p.name == "period")
        .map(|p| p.value.round() as usize).filter(|&n| n > 0).unwrap_or(default)
}

/// Target export engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Mql4,
    Mql5,
    PineScript,
    NinjaTrader,
    EasyLanguage,
    PseudoCode,
}

impl Engine {
    pub fn label(&self) -> &'static str {
        match self {
            Engine::Mql4 => "mql4",
            Engine::Mql5 => "mql5",
            Engine::PineScript => "pine",
            Engine::NinjaTrader => "ninjascript",
            Engine::EasyLanguage => "easylanguage",
            Engine::PseudoCode => "pseudocode",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        match s {
            "mql4" => Some(Engine::Mql4),
            "mql5" => Some(Engine::Mql5),
            "pine" | "pinescript" => Some(Engine::PineScript),
            "ninja" | "ninjascript" => Some(Engine::NinjaTrader),
            "el" | "easylanguage" | "tradestation" => Some(Engine::EasyLanguage),
            "pseudocode" | "pseudo" => Some(Engine::PseudoCode),
            _ => None,
        }
    }
}

/// Export a strategy to the given engine's source code.
fn render_source(strategy: &Strategy, engine: Engine) -> String {
    match engine {
        Engine::Mql4 => render_mql4(strategy),
        Engine::Mql5 => render_mql5(strategy),
        Engine::PineScript => render_pine(strategy),
        Engine::NinjaTrader => render_ninjascript(strategy),
        Engine::EasyLanguage => render_easylanguage(strategy),
        Engine::PseudoCode => sqx_strategy::render(strategy),
    }
}

/// An explicitly unverified reference export, never a deployment certificate.
#[derive(Debug)]
pub struct ExportReport {
    pub code: Option<String>,
    pub blockers: Vec<String>,
    pub limitations: Vec<String>,
    pub status: &'static str,
}

pub fn export_report(strategy: &Strategy, engine: Engine) -> ExportReport {
    let source = render_source(strategy, engine);
    if engine == Engine::PseudoCode {
        return ExportReport { code: Some(source), blockers: vec![], limitations: vec!["Human-readable rules; not executable platform code.".into()], status: "documentation" };
    }
    let mut blockers = Vec::new();
    let options = &strategy.trading_options;
    if options.exit_at_end_of_day || options.exit_on_friday {
        blockers.push("Scheduled daily/Friday exits are not implemented by this exporter.".into());
    }
    if options.limit_hours.is_some() || options.max_trades_per_day.is_some() {
        blockers.push("Entry hours and daily trade limits are not implemented by this exporter.".into());
    }
    if strategy.confirmation.is_some() { blockers.push("Higher-timeframe confirmation is not exported.".into()); }
    if !strategy.exits.is_empty() { blockers.push("Additional exits (including trailing, break-even and bar-count exits) are not exported.".into()); }
    if !matches!(strategy.money_management, sqx_strategy::MoneyManagement::FixedSize{..}) {
        blockers.push("This money-management model is not exported; no fallback sizing is permitted.".into());
    }
    if strategy.signals.iter().any(|s|s.fuzzy.is_some()) { blockers.push("Fuzzy signal evaluation is not exported.".into()); }
    if strategy.signals.iter().enumerate().any(|(i,s)|strategy.signals[..i].iter().any(|other|other.kind==s.kind)) {
        blockers.push("Multiple signal blocks of the same kind are not supported by this exporter.".into());
    }
    if strategy.stop_loss.is_some() || strategy.profit_target.is_some() {
        blockers.push("Stop/target order execution has not been implemented equivalently on this platform.".into());
    }
    if source.contains(":unmapped") { blockers.push("At least one indicator has no platform mapping; numeric or SMA substitution is forbidden.".into()); }
    let mut limitations = vec![
        "REFERENCE DRAFT ONLY: platform compilation and execution parity have not been verified. Do not deploy to live trading.".into(),
        "Account currency, contract units, margin, financing, costs and fill assumptions are not transferred from a backtest.".into(),
        "Indicator formulas, parameters, warmup and signal timing need independent platform comparison.".into(),
    ];
    limitations.push(match engine {
        Engine::Mql4 => "MQL4 routing lacks signal-exit handling and a once-per-bar execution guard.",
        Engine::Mql5 => "MQL5 order actions are placeholders; indicator calls still use MQL4-style value access rather than MQL5 handles/buffers.",
        Engine::PineScript => "Pine sizing currently converts fixed lots to an equity fraction; indicator tuple syntax and stop/target units require correction.",
        Engine::NinjaTrader => "NinjaScript uses a fixed 50-bar warmup and forced session-close exit; quantity units and indicator APIs require correction.",
        Engine::EasyLanguage => "EasyLanguage quantity units and platform position APIs require verification.",
        Engine::PseudoCode => unreachable!(),
    }.into());
    let status = if blockers.is_empty() { "reference_draft" } else { "blocked" };
    let code = blockers.is_empty().then(|| {
        // Keep Pine's version directive first. Diagnostics travel with downloaded files.
        let header = limitations.iter().map(|s|format!("// {s}\n")).collect::<String>();
        if let Some(rest)=source.strip_prefix("//@version=5\n") { format!("//@version=5\n{header}{rest}") }
        else { format!("{header}{source}") }
    });
    ExportReport { code, blockers, limitations, status }
}

/// Compatibility renderer. Unsupported strategies produce diagnostics only, never
/// a partly implemented executable. Prefer export_report for structured handling.
pub fn export(strategy: &Strategy, engine: Engine) -> String {
    let report = export_report(strategy, engine);
    report.code.unwrap_or_else(|| report.blockers.iter().map(|s|format!("// EXPORT BLOCKED: {s}\n")).collect())
}
