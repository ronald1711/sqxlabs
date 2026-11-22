//! # sqx-export
//!
//! Clean-room code exporters: translate a strategy's rule tree into platform
//! source code. Mirrors StrategyQuant's "export to platform" feature
//! (MetaTrader, EasyLanguage, NinjaTrader, PseudoCode), here for MQL4, MQL5 and
//! PineScript v5, plus the built-in pseudo-code renderer.

mod mql;
mod pine;

pub use mql::{render_mql4, render_mql5};
pub use pine::render_pine;

use sqx_strategy::Strategy;

/// Target export engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Mql4,
    Mql5,
    PineScript,
    PseudoCode,
}

impl Engine {
    pub fn label(&self) -> &'static str {
        match self {
            Engine::Mql4 => "mql4",
            Engine::Mql5 => "mql5",
            Engine::PineScript => "pine",
            Engine::PseudoCode => "pseudocode",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        match s {
            "mql4" => Some(Engine::Mql4),
            "mql5" => Some(Engine::Mql5),
            "pine" | "pinescript" => Some(Engine::PineScript),
            "pseudocode" | "pseudo" => Some(Engine::PseudoCode),
            _ => None,
        }
    }
}

/// Export a strategy to the given engine's source code.
pub fn export(strategy: &Strategy, engine: Engine) -> String {
    match engine {
        Engine::Mql4 => render_mql4(strategy),
        Engine::Mql5 => render_mql5(strategy),
        Engine::PineScript => render_pine(strategy),
        Engine::PseudoCode => sqx_strategy::render(strategy),
    }
}
