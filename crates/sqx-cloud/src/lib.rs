//! AlgoCloud-style signal service: keep a set of strategies and evaluate them
//! on the latest bar of a market to produce a daily action.

use sqx_core::Bar;
use sqx_strategy::{SignalKind, Strategy};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalAction {
    Long,
    Short,
    Flat,
}

impl SignalAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            SignalAction::Long => "LONG",
            SignalAction::Short => "SHORT",
            SignalAction::Flat => "FLAT",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignalRow {
    pub strategy: String,
    pub symbol: String,
    pub action: SignalAction,
    pub price: f64,
}

#[derive(Debug, Clone)]
pub struct CloudStrategy {
    pub name: String,
    pub strategy: Strategy,
}

#[derive(Debug, Clone, Default)]
pub struct CloudService {
    pub strategies: Vec<CloudStrategy>,
}

impl CloudService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: &str, strategy: Strategy) {
        self.strategies.push(CloudStrategy { name: name.to_string(), strategy });
    }

    /// Load every `*.json` strategy file in a directory.
    pub fn load_dir(&mut self, dir: &Path) -> Result<usize, String> {
        let mut n = 0;
        let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in entries {
            let p = entry.map_err(|e| e.to_string())?.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&p) {
                if let Ok(s) = serde_json::from_str::<Strategy>(&text) {
                    let name = s.name.clone();
                    self.add(&name, s);
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// Evaluate every strategy on the last bar and return its action.
    pub fn signals(&self, symbol: &str, bars: &[Bar]) -> Vec<SignalRow> {
        let price = bars.last().map(|b| b.close).unwrap_or(f64::NAN);
        let mut out = Vec::new();
        for cs in &self.strategies {
            out.push(SignalRow {
                strategy: cs.name.clone(),
                symbol: symbol.to_string(),
                action: last_action(&cs.strategy, bars),
                price,
            });
        }
        out
    }
}

fn last_action(s: &Strategy, bars: &[Bar]) -> SignalAction {
    if let Some(sig) = s.signals.iter().find(|x| x.kind == SignalKind::LongEntry) {
        if sqx_backtest::compile_signal(sig, bars).last().copied().unwrap_or(false) {
            return SignalAction::Long;
        }
    }
    if let Some(sig) = s.signals.iter().find(|x| x.kind == SignalKind::ShortEntry) {
        if sqx_backtest::compile_signal(sig, bars).last().copied().unwrap_or(false) {
            return SignalAction::Short;
        }
    }
    SignalAction::Flat
}

pub fn render_table(rows: &[SignalRow]) -> String {
    let mut out = String::new();
    out.push_str("| Strategy | Symbol | Action | Price |\n|---|---|---|---:|\n");
    for r in rows {
        out.push_str(&format!("| {} | {} | {} | {:.5} |\n", r.strategy, r.symbol, r.action.as_str(), r.price));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_when_no_signal() {
        let s = Strategy::new("empty");
        let bars = vec![Bar::new(0, 1.0, 1.0, 1.0, 1.0, 1.0)];
        assert_eq!(last_action(&s, &bars), SignalAction::Flat);
    }
}
