//! Compiles a Strategy into per-bar boolean signal series.

use std::collections::HashMap;

use sqx_core::Bar;
use sqx_strategy::{Comparator, Condition, Join, Operand, PriceField, Signal, SignalKind, Strategy};

use crate::registry;

/// A cache of computed series, keyed by an operand description.
struct SeriesCache<'a> {
    bars: &'a [Bar],
    cache: HashMap<String, Vec<f64>>,
}

impl<'a> SeriesCache<'a> {
    fn new(bars: &'a [Bar]) -> Self {
        Self { bars, cache: HashMap::new() }
    }

    fn resolve(&mut self, op: &Operand) -> Vec<f64> {
        let key = format!("{op:?}");
        if let Some(s) = self.cache.get(&key) {
            return s.clone();
        }
        let n = self.bars.len();
        let series = match op {
            Operand::Const(v) => vec![*v; n],
            Operand::Price(field, shift) => {
                let base: Vec<f64> = match field {
                    PriceField::Open => self.bars.iter().map(|b| b.open).collect(),
                    PriceField::High => self.bars.iter().map(|b| b.high).collect(),
                    PriceField::Low => self.bars.iter().map(|b| b.low).collect(),
                    PriceField::Close => self.bars.iter().map(|b| b.close).collect(),
                    PriceField::Volume => self.bars.iter().map(|b| b.volume).collect(),
                    PriceField::Typical => self.bars.iter().map(|b| b.typical()).collect(),
                };
                sqx_indicators::shift(&base, (*shift).max(0) as usize)
            }
            Operand::Indicator(name, params) => {
                registry::compute(name, params, self.bars).unwrap_or_else(|| vec![f64::NAN; n])
            }
        };
        self.cache.insert(key, series.clone());
        series
    }
}

fn eval_condition(cache: &mut SeriesCache, cond: &Condition) -> Vec<bool> {
    let n = cache.bars.len();
    match cond.cmp {
        Comparator::Rising => {
            let a = cache.resolve(&cond.left);
            sqx_indicators::changes_up(&a)
        }
        Comparator::Falling => {
            let a = cache.resolve(&cond.left);
            sqx_indicators::changes_down(&a)
        }
        cmp => {
            let a = cache.resolve(&cond.left);
            let b = cache.resolve(&cond.right);
            match cmp {
                Comparator::Gt => (0..n).map(|i| !a[i].is_nan() && !b[i].is_nan() && a[i] > b[i]).collect(),
                Comparator::Gte => (0..n).map(|i| !a[i].is_nan() && !b[i].is_nan() && a[i] >= b[i]).collect(),
                Comparator::Lt => (0..n).map(|i| !a[i].is_nan() && !b[i].is_nan() && a[i] < b[i]).collect(),
                Comparator::Lte => (0..n).map(|i| !a[i].is_nan() && !b[i].is_nan() && a[i] <= b[i]).collect(),
                Comparator::Eq => (0..n).map(|i| a[i] == b[i]).collect(),
                Comparator::Neq => (0..n).map(|i| a[i] != b[i]).collect(),
                Comparator::CrossAbove => sqx_indicators::crosses_above(&a, &b),
                Comparator::CrossBelow => sqx_indicators::crosses_below(&a, &b),
                Comparator::Rising | Comparator::Falling => unreachable!(),
            }
        }
    }
}

fn eval_signal(cache: &mut SeriesCache, sig: &Signal) -> Vec<bool> {
    if sig.conditions.is_empty() {
        return vec![false; cache.bars.len()];
    }
    let per: Vec<Vec<bool>> = sig.conditions.iter().map(|c| eval_condition(cache, c)).collect();
    let n = cache.bars.len();
    let base: Vec<bool> = match sig.join {
        Join::And => (0..n).map(|i| per.iter().all(|s| s[i])).collect(),
        Join::Or => (0..n).map(|i| per.iter().any(|s| s[i])).collect(),
    };
    match sig.fuzzy {
        Some(f) => (0..n)
            .map(|i| {
                let trues = per.iter().filter(|s| s[i]).count();
                (trues as f64 / per.len() as f64) >= f
            })
            .collect(),
        None => base,
    }
}

/// The compiled, per-bar signal series of a strategy.
pub struct CompiledSignals {
    pub long_entry: Vec<bool>,
    pub long_exit: Vec<bool>,
    pub short_entry: Vec<bool>,
    pub short_exit: Vec<bool>,
}

/// Compile a strategy into per-bar boolean signals.
pub fn compile(strategy: &Strategy, bars: &[Bar]) -> CompiledSignals {
    let mut cache = SeriesCache::new(bars);
    let mut out = CompiledSignals {
        long_entry: vec![false; bars.len()],
        long_exit: vec![false; bars.len()],
        short_entry: vec![false; bars.len()],
        short_exit: vec![false; bars.len()],
    };
    for sig in &strategy.signals {
        let series = eval_signal(&mut cache, sig);
        match sig.kind {
            SignalKind::LongEntry => out.long_entry = series,
            SignalKind::LongExit => out.long_exit = series,
            SignalKind::ShortEntry => out.short_entry = series,
            SignalKind::ShortExit => out.short_exit = series,
        }
    }
    out
}
