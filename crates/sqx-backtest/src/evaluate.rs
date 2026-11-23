//! Compiles a Strategy into per-bar boolean signal series.

use std::collections::HashMap;

use sqx_core::Bar;
use sqx_strategy::{Comparator, Condition, Join, Operand, PriceField, Signal, SignalKind, Strategy};

use crate::registry;

/// A cache of computed series, keyed by an operand description.
struct SeriesCache<'a> {
    bars: &'a [Bar],
    cache: HashMap<String, Vec<f64>>,
    errors: Vec<String>,
}

impl<'a> SeriesCache<'a> {
    fn new(bars: &'a [Bar]) -> Self {
        Self { bars, cache: HashMap::new(), errors: Vec::new() }
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
                match registry::compute(name, params, self.bars) {
                    Some(series)=>series,
                    None=>{self.errors.push(format!("Indicator {name} has no computation implementation"));vec![f64::NAN;n]}
                }
            }
        };
        if let Some(index)=series.iter().position(|v|v.is_infinite()){self.errors.push(format!("nonfinite signal operand {key} at bar {index}"));}
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
                Comparator::Gt => (0..n).map(|i| a[i].is_finite() && b[i].is_finite() && a[i] > b[i]).collect(),
                Comparator::Gte => (0..n).map(|i| a[i].is_finite() && b[i].is_finite() && a[i] >= b[i]).collect(),
                Comparator::Lt => (0..n).map(|i| a[i].is_finite() && b[i].is_finite() && a[i] < b[i]).collect(),
                Comparator::Lte => (0..n).map(|i| a[i].is_finite() && b[i].is_finite() && a[i] <= b[i]).collect(),
                // Equality compares raw f64, so an unguarded NaN would make
                // `Neq` true for every warm-up bar and fabricate entries.
                Comparator::Eq => (0..n)
                    .map(|i| a[i].is_finite() && b[i].is_finite() && a[i] == b[i])
                    .collect(),
                Comparator::Neq => (0..n)
                    .map(|i| a[i].is_finite() && b[i].is_finite() && a[i] != b[i])
                    .collect(),
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

#[derive(Debug,Clone,PartialEq,serde::Serialize)]
pub struct ConditionEvidence {
    pub condition: Condition,
    pub left: Option<f64>,pub right: Option<f64>,
    pub previous_left: Option<f64>,pub previous_right: Option<f64>,
    pub matched: bool,
}
#[derive(Debug,Clone,PartialEq,serde::Serialize)]
pub struct SignalEvidence {
    pub kind:SignalKind,pub bar_index:usize,pub join:Join,pub fuzzy:Option<f64>,
    pub matched:bool,pub conditions:Vec<ConditionEvidence>,
}
struct ConditionSeries {condition:Condition,left:Vec<f64>,right:Option<Vec<f64>>,matched:Vec<bool>}
struct SignalSeries {kind:SignalKind,join:Join,fuzzy:Option<f64>,matched:Vec<bool>,conditions:Vec<ConditionSeries>}

/// The compiled, per-bar signal series of a strategy.
pub struct CompiledSignals {
    evidence: Vec<SignalSeries>,
    pub errors: Vec<String>,
    pub long_entry: Vec<bool>,
    pub long_exit: Vec<bool>,
    pub short_entry: Vec<bool>,
    pub short_exit: Vec<bool>,
}

impl CompiledSignals {
    pub fn explain(&self,kind:SignalKind,index:usize)->Option<SignalEvidence>{
        let signal=self.evidence.iter().find(|s|s.kind==kind)?;
        let matched=*signal.matched.get(index)?;
        let finite=|values:&[f64],i:usize|values.get(i).copied().filter(|v|v.is_finite());
        let conditions=signal.conditions.iter().map(|c|{
            let previous=matches!(c.condition.cmp,Comparator::CrossAbove|Comparator::CrossBelow|Comparator::Rising|Comparator::Falling);
            ConditionEvidence{condition:c.condition.clone(),left:finite(&c.left,index),right:c.right.as_ref().and_then(|v|finite(v,index)),previous_left:if previous{index.checked_sub(1).and_then(|i|finite(&c.left,i))}else{None},previous_right:if previous{index.checked_sub(1).and_then(|i|c.right.as_ref().and_then(|v|finite(v,i)))}else{None},matched:c.matched[index]}
        }).collect();
        Some(SignalEvidence{kind,bar_index:index,join:signal.join,fuzzy:signal.fuzzy,matched,conditions})
    }
}

/// Compile a single signal into per-bar booleans; numerical errors fail closed.
/// Research callers should use compile_signal_checked to retain error diagnostics.
pub fn compile_signal(sig: &Signal, bars: &[Bar]) -> Vec<bool> {
    compile_signal_checked(sig,bars).unwrap_or_else(|_|vec![false;bars.len()])
}

/// Checked compilation for research engines; NaN warmup remains undefined, while
/// infinities indicate invalid numerical evidence rather than an ordinary false signal.
pub fn compile_signal_checked(sig:&Signal,bars:&[Bar])->Result<Vec<bool>,String>{
    sig.validate_execution_settings()?;
    let mut cache=SeriesCache::new(bars);let values=eval_signal(&mut cache,sig);
    if cache.errors.is_empty(){Ok(values)}else{Err(cache.errors.join("; "))}
}

/// Compile a strategy into per-bar boolean signals.
pub fn compile(strategy: &Strategy, bars: &[Bar]) -> CompiledSignals {
    let mut cache = SeriesCache::new(bars);
    let mut out = CompiledSignals {
        errors: Vec::new(),evidence:Vec::new(),
        long_entry: vec![false; bars.len()],
        long_exit: vec![false; bars.len()],
        short_entry: vec![false; bars.len()],
        short_exit: vec![false; bars.len()],
    };
    for sig in &strategy.signals {
        let series = eval_signal(&mut cache, sig);
        let conditions=sig.conditions.iter().map(|condition|{
            let left=cache.resolve(&condition.left);
            let right=if matches!(condition.cmp,Comparator::Rising|Comparator::Falling){None}else{Some(cache.resolve(&condition.right))};
            let matched=eval_condition(&mut cache,condition);
            ConditionSeries{condition:condition.clone(),left,right,matched}
        }).collect();
        out.evidence.push(SignalSeries{kind:sig.kind,join:sig.join,fuzzy:sig.fuzzy,matched:series.clone(),conditions});
        match sig.kind {
            SignalKind::LongEntry => out.long_entry = series,
            SignalKind::LongExit => out.long_exit = series,
            SignalKind::ShortEntry => out.short_entry = series,
            SignalKind::ShortExit => out.short_exit = series,
        }
    }
    out.errors=cache.errors;
    out
}
