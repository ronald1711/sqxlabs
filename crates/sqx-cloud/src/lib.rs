//! AlgoCloud-style signal service: keep a set of strategies and evaluate them
//! on the latest bar of a market to produce a daily action.

use sqx_core::Bar;
use sqx_strategy::Strategy;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalAction {
    Long,
    Short,
    Flat,
    Unavailable,
}

impl SignalAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            SignalAction::Long => "LONG",
            SignalAction::Short => "SHORT",
            SignalAction::Flat => "FLAT",
            SignalAction::Unavailable => "UNAVAILABLE",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignalRow {
    pub strategy: String,
    pub symbol: String,
    pub action: SignalAction,
    pub price: f64,
    pub errors:Vec<String>,
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
        let mut paths=std::fs::read_dir(dir).map_err(|e|e.to_string())?.map(|e|e.map(|e|e.path())).collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        paths.sort();let mut staged=Vec::new();
        for path in paths.into_iter().filter(|p|p.extension().and_then(|x|x.to_str())==Some("json")) {
            let text=std::fs::read_to_string(&path).map_err(|e|format!("{}: {e}",path.display()))?;
            let strategy:Strategy=serde_json::from_str(&text).map_err(|e|format!("{}: {e}",path.display()))?;
            strategy.validate_execution_settings().map_err(|e|format!("{}: {e}",path.display()))?;
            staged.push(CloudStrategy{name:strategy.name.clone(),strategy});
        }
        let count=staged.len();self.strategies.extend(staged);Ok(count)
    }

    /// Preview entry conditions on caller-validated closed candles. This is not
    /// position-aware execution; unsupported confirmation yields Unavailable.
    pub fn signals(&self, symbol: &str, bars: &[Bar]) -> Vec<SignalRow> {
        let price = bars.last().map(|b| b.close).unwrap_or(f64::NAN);
        let mut out = Vec::new();
        for cs in &self.strategies {
            let (action,errors)=match last_action(&cs.strategy,bars){Ok(action)=>(action,Vec::new()),Err(error)=>(SignalAction::Unavailable,vec![error])};
            out.push(SignalRow {
                strategy: cs.name.clone(),
                symbol: symbol.to_string(),
                action,errors,
                price,
            });
        }
        out
    }
}

fn last_action(s:&Strategy,bars:&[Bar])->Result<SignalAction,String>{
    s.validate_execution_settings()?;
    if bars.is_empty(){return Err("No closed source candles supplied".into());}
    if s.confirmation.is_some(){return Err("Higher-timeframe confirmation is not supported by the raw entry preview; use the backtest execution path".into());}
    let mut long=false;let mut short=false;
    for signal in &s.signals {
        let values=sqx_backtest::compile_signal_checked(signal,bars)?;
        match signal.kind {
            sqx_strategy::SignalKind::LongEntry=>long=values.last()==Some(&true),
            sqx_strategy::SignalKind::ShortEntry=>short=values.last()==Some(&true),
            _=>{},
        }
    }
    if s.long && long{return Ok(SignalAction::Long);}
    if s.short && short{return Ok(SignalAction::Short);}
    Ok(SignalAction::Flat)
}

pub fn render_table(rows: &[SignalRow]) -> String {
    let mut out = String::from("Raw entry indications only; FLAT means no entry signal, not a position close. Prices are candle closes, not executable quotes.\n\n");
    out.push_str("| Strategy | Symbol | Action | Price |\n|---|---|---|---:|\n");
    for r in rows {
        out.push_str(&format!("| {} | {} | {} | {:.5} |\n", r.strategy, r.symbol, r.action.as_str(), r.price));
        for error in &r.errors {out.push_str(&format!("Unavailable: {error}\n"));}
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
        assert_eq!(last_action(&s, &bars).unwrap(), SignalAction::Flat);
    }
    fn entry_strategy()->Strategy{
        use sqx_strategy::{Signal,SignalKind,Condition,Operand,Comparator,Join};
        let mut s=Strategy::new("entry");s.long=true;
        s.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition{left:Operand::Const(1.0),cmp:Comparator::Gt,right:Operand::Const(0.0)}],Join::And));s
    }
    #[test]
    fn preview_honors_disabled_directions_and_exposes_unsupported_confirmation(){
        let bars=[Bar::new(0,1.0,1.0,1.0,1.0,1.0)];let mut s=entry_strategy();
        assert_eq!(last_action(&s,&bars).unwrap(),SignalAction::Long);
        s.long=false;assert_eq!(last_action(&s,&bars).unwrap(),SignalAction::Flat);
        s.confirmation=Some(sqx_strategy::Confirmation{timeframe:sqx_core::Timeframe::M5,signal:s.signals[0].clone()});
        let mut service=CloudService::new();service.add("confirmed",s);
        let rows=service.signals("EURUSD",&bars);assert_eq!(rows[0].action,SignalAction::Unavailable);assert!(!rows[0].errors.is_empty());
    }
    #[test]
    fn failed_indicator_evaluation_is_unavailable_instead_of_flat(){
        let mut s=entry_strategy();s.signals[0].conditions[0].left=sqx_strategy::Operand::Price(sqx_strategy::PriceField::Typical,0);
        let mut service=CloudService::new();service.add("overflow",s);
        let rows=service.signals("EURUSD",&[Bar::new(0,1e308,1e308,1e308,1e308,1.0)]);
        assert_eq!(rows[0].action,SignalAction::Unavailable);assert!(rows[0].errors[0].contains("nonfinite signal operand"));
        assert_eq!(service.signals("EURUSD",&[])[0].action,SignalAction::Unavailable);
    }
    #[test]
    fn directory_loading_is_atomic_and_deterministically_ordered(){
        let root=std::env::temp_dir().join(format!("sqx-cloud-load-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir_all(&root).unwrap();
        let mut a=entry_strategy();a.name="A".into();std::fs::write(root.join("a.json"),serde_json::to_vec(&a).unwrap()).unwrap();std::fs::write(root.join("b.json"),"broken").unwrap();
        let mut service=CloudService::new();assert!(service.load_dir(&root).is_err());assert!(service.strategies.is_empty());
        let mut b=a.clone();b.name="B".into();std::fs::write(root.join("b.json"),serde_json::to_vec(&b).unwrap()).unwrap();assert_eq!(service.load_dir(&root).unwrap(),2);assert_eq!(service.strategies.iter().map(|s|s.name.as_str()).collect::<Vec<_>>(),vec!["A","B"]);
        std::fs::remove_dir_all(root).unwrap();
    }

}
