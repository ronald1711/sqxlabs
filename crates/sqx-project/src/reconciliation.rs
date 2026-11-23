//! Closed-trade comparison only: explicit correspondence, no guessed time matching.
//! This does not validate open positions, orders, partial fills or market-data parity.
use serde::{Deserialize, Serialize};
use sqx_core::Direction;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionTrade {
    pub direction: Direction,
    pub open_time: i64,
    pub close_time: i64,
    pub open_price: f64,
    pub close_price: f64,
    pub size: f64,
    /// Net realized result in the declared account currency, including costs.
    pub pnl: f64,
    pub commission: f64,
    /// Signed cost: positive means a charge, negative means a credit.
    pub financing: f64,
}
impl From<&sqx_backtest::Trade> for ExecutionTrade {
    fn from(t:&sqx_backtest::Trade)->Self {Self{direction:t.direction,open_time:t.open_time,close_time:t.close_time,open_price:t.open_price,close_price:t.close_price,size:t.size,pnl:t.pnl,commission:t.commission,financing:t.financing}}
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedTrade {
    pub ticket: String,
    /// Explicit mapping supplied by the importer/user; never inferred from proximity.
    pub expected_trade_id: Option<usize>,
    pub execution: ExecutionTrade,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tolerances {
    pub time_ms: u64,
    pub price: f64,
    pub size: f64,
    pub money: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeComparison {
    pub expected_trade_id: usize,
    pub ticket: String,
    pub expected: ExecutionTrade,
    pub observed: ExecutionTrade,
    pub differences: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reconciliation {
    pub tolerances: Tolerances,
    pub comparisons: Vec<TradeComparison>,
    pub missing_expected_ids: Vec<usize>,
    pub unexpected_tickets: Vec<String>,
    /// True only for a nonempty, fully mapped set with no out-of-tolerance fields.
    /// This is correspondence evidence, not a deployment or profitability certificate.
    pub matches_within_tolerance: bool,
}
fn validate(t:&ExecutionTrade)->Result<(),String>{
    if t.open_time>t.close_time{return Err("Trade closes before it opens".into());}
    if ![t.open_price,t.close_price,t.size,t.pnl,t.commission,t.financing].iter().all(|v|v.is_finite()) {return Err("Trade contains nonfinite values".into());}
    if t.open_price<=0.0 || t.close_price<=0.0 || t.size<=0.0{return Err("Trade prices and size must be positive".into());}
    Ok(())
}
/// Both sides must cover the same instrument, account currency and declared observation
/// period before calling. This function compares closed trades, not those declarations.
pub fn compare(expected:&BTreeMap<usize,ExecutionTrade>,observed:&[ObservedTrade],tolerance:&Tolerances)->Result<Reconciliation,String>{
    if ![tolerance.price,tolerance.size,tolerance.money].iter().all(|v|v.is_finite() && *v>=0.0){return Err("Tolerances must be finite and nonnegative".into());}
    for (id,t) in expected {validate(t).map_err(|e|format!("Expected trade {id}: {e}"))?;}
    let mut tickets=BTreeSet::new();let mut mapped=BTreeSet::new();
    let mut comparisons=Vec::new();let mut unexpected_tickets=Vec::new();
    for row in observed {
        if row.ticket.trim().is_empty() || !tickets.insert(row.ticket.clone()){return Err("Observed tickets must be nonempty and unique".into());}
        validate(&row.execution).map_err(|e|format!("Ticket {}: {e}",row.ticket))?;
        let Some(id)=row.expected_trade_id else {unexpected_tickets.push(row.ticket.clone());continue;};
        let target=expected.get(&id).ok_or_else(||format!("Ticket {} maps to unknown expected trade {id}",row.ticket))?;
        if !mapped.insert(id){return Err(format!("Multiple tickets map to expected trade {id}; aggregate partial fills explicitly before comparison"));}
        let actual=&row.execution;let mut differences=Vec::new();
        if actual.direction!=target.direction{differences.push("direction".into());}
        for (field,a,b) in [("open_time",actual.open_time,target.open_time),("close_time",actual.close_time,target.close_time)] {
            if a.abs_diff(b)>tolerance.time_ms{differences.push(field.into());}
        }
        for (field,a,b,limit) in [("open_price",actual.open_price,target.open_price,tolerance.price),("close_price",actual.close_price,target.close_price,tolerance.price),("size",actual.size,target.size,tolerance.size),("pnl",actual.pnl,target.pnl,tolerance.money),("commission",actual.commission,target.commission,tolerance.money),("financing",actual.financing,target.financing,tolerance.money)]{
            if (a-b).abs()>limit{differences.push(field.into());}
        }
        comparisons.push(TradeComparison{expected_trade_id:id,ticket:row.ticket.clone(),expected:target.clone(),observed:actual.clone(),differences});
    }
    comparisons.sort_by_key(|row|row.expected_trade_id);unexpected_tickets.sort();
    let missing_expected_ids:Vec<_>=expected.keys().filter(|id|!mapped.contains(id)).copied().collect();
    let matches_within_tolerance=!expected.is_empty() && missing_expected_ids.is_empty() && unexpected_tickets.is_empty() && comparisons.iter().all(|row|row.differences.is_empty());
    Ok(Reconciliation{tolerances:tolerance.clone(),comparisons,missing_expected_ids,unexpected_tickets,matches_within_tolerance})
}

/// Normalized market identity must be resolved explicitly by the importer. Broker aliases
/// are not guessed. Times are UTC milliseconds; the closed interval is intentional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonScope {
    pub market: String,
    pub quote_currency: String,
    pub account_currency: String,
    /// Underlying units per one reported lot on both sides.
    pub contract_size: f64,
    pub start_ms: i64,
    pub end_ms: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedReconciliation {
    pub expected_scope: ComparisonScope,
    pub observed_scope: ComparisonScope,
    pub report: Reconciliation,
}
fn validate_scope(scope:&ComparisonScope)->Result<(),String>{
    if [&scope.market,&scope.quote_currency,&scope.account_currency].iter().any(|s|s.trim().is_empty() || s.trim()!=s.as_str()) {return Err("Market and currencies must be nonempty normalized identifiers".into());}
    if !scope.contract_size.is_finite() || scope.contract_size<=0.0{return Err("Contract size must be finite and positive".into());}
    if scope.start_ms>=scope.end_ms{return Err("Observation period must have positive duration".into());}
    Ok(())
}
/// Use this entry point for imported reports. Both sets must fit entirely inside the
/// declared period. Cross-boundary/open positions require a separate reconciliation;
/// silently dropping them would invalidate the completeness claim.
pub fn compare_scoped(expected_scope:&ComparisonScope,expected:&BTreeMap<usize,ExecutionTrade>,observed_scope:&ComparisonScope,observed:&[ObservedTrade],tolerance:&Tolerances)->Result<ScopedReconciliation,String>{
    validate_scope(expected_scope)?;validate_scope(observed_scope)?;
    if expected_scope!=observed_scope{return Err("Market, quote/account currencies, contract size and observation period must match exactly; normalize explicitly before comparing".into());}
    for (label,trade) in expected.iter().map(|(id,t)|(format!("Expected trade {id}"),t)).chain(observed.iter().map(|t|(format!("Ticket {}",t.ticket),&t.execution))){
        if trade.open_time<expected_scope.start_ms || trade.close_time>expected_scope.end_ms{return Err(format!("{label} crosses the declared observation period"));}
    }
    let report=compare(expected,observed,tolerance)?;
    Ok(ScopedReconciliation{expected_scope:expected_scope.clone(),observed_scope:observed_scope.clone(),report})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedComparison {
    pub run_id: String,
    pub dataset_id: String,
    pub strategy_id: String,
    pub engine_id: String,
    /// These expected exits are reporting liquidations, not live signal orders.
    pub end_of_data_trade_ids: Vec<usize>,
    pub comparison: ScopedReconciliation,
}
/// Loads and verifies the content-addressed dashboard backtest before comparison.
/// No backtest is rerun and no current strategy/data/configuration is consulted.
pub fn compare_archived(archive:&crate::archive::Archive,run_id:&str,observed_scope:&ComparisonScope,observed:&[ObservedTrade],tolerances:&Tolerances)->Result<ArchivedComparison,String>{
    compare_bundle(archive.load(run_id)?,observed_scope,observed,tolerances)
}
fn compare_bundle(bundle:crate::archive::ResearchBundle,observed_scope:&ComparisonScope,observed:&[ObservedTrade],tolerances:&Tolerances)->Result<ArchivedComparison,String>{
    let kind=bundle.run.request.get("research_kind").and_then(|v|v.as_str()).unwrap_or("backtest");
    if kind!="backtest" || bundle.run.result.get("error").is_some(){return Err("Reconciliation requires a successful archived backtest".into());}
    let instrument:sqx_core::Instrument=serde_json::from_value(bundle.dataset["instrument"].clone()).map_err(|e|format!("Archived instrument: {e}"))?;
    let bars:Vec<sqx_core::Bar>=serde_json::from_value(bundle.dataset["bars"].clone()).map_err(|e|format!("Archived bars: {e}"))?;
    let timeframe:sqx_core::Timeframe=serde_json::from_value(bundle.dataset["timeframe"].clone()).map_err(|e|format!("Archived timeframe: {e}"))?;
    let first=bars.first().ok_or("Archived dataset is empty")?;
    let last=bars.last().unwrap();
    let end=timeframe.bucket_bounds(last.time).ok_or("Cannot determine archived observation end")?.1.checked_sub(1).ok_or("Archived end overflows")?;
    let currency=bundle.run.result["run_context"]["account_currency"].as_str().ok_or("Archived account currency is missing")?;
    let scope=ComparisonScope{market:instrument.name,quote_currency:instrument.quote_currency,account_currency:currency.into(),contract_size:instrument.contract_size,start_ms:first.time,end_ms:end};
    let rows=bundle.run.result["trade_details"].as_array().ok_or("Archived trade details are missing")?;
    if bundle.run.result["trades"].as_u64()!=Some(rows.len() as u64){return Err("Archived trade count does not match details".into());}
    let mut expected=BTreeMap::new();let mut end_of_data_trade_ids=Vec::new();
    for row in rows {
        let id=row["id"].as_u64().and_then(|v|usize::try_from(v).ok()).ok_or("Invalid archived trade ID")?;
        let mut fields=serde_json::Map::new();
        for key in ["direction","open_time","close_time","open_price","close_price","size","pnl","commission","financing"]{fields.insert(key.into(),row.get(key).ok_or_else(||format!("Archived trade {id} lacks {key}"))?.clone());}
        let trade:ExecutionTrade=serde_json::from_value(serde_json::Value::Object(fields)).map_err(|e|format!("Archived trade {id}: {e}"))?;
        if expected.insert(id,trade).is_some(){return Err("Archived trade IDs are not unique".into());}
        let reason=row["exit_reason"].as_str().ok_or("Archived exit reason is missing")?;
        if reason=="EndOfData"{end_of_data_trade_ids.push(id);}
    }
    let comparison=compare_scoped(&scope,&expected,observed_scope,observed,tolerances)?;
    Ok(ArchivedComparison{run_id:bundle.run_id,dataset_id:bundle.run.dataset_id,strategy_id:bundle.run.strategy_id,engine_id:bundle.run.engine_id,end_of_data_trade_ids,comparison})
}

/// Normalized import format. Source labels are user declarations, not authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperImport {
    pub schema: u32,
    pub source_label: String,
    pub scope: ComparisonScope,
    pub trades: Vec<ObservedTrade>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonRecord {
    pub schema: u32,
    pub reference: crate::archive::ResearchBundle,
    /// Exact UTF-8 import bytes, including whitespace and original field order.
    pub source_json: String,
    pub source_id: String,
    pub tolerances: Tolerances,
    pub result: ArchivedComparison,
}
fn parse_import(source:&str)->Result<PaperImport,String>{
    if source.len()>4*1024*1024{return Err("Paper import exceeds 4 MiB".into());}
    let import:PaperImport=serde_json::from_str(source).map_err(|e|format!("Invalid paper import: {e}"))?;
    if import.schema!=1{return Err("Unsupported paper import schema".into());}
    if import.source_label.trim().is_empty() || import.source_label.len()>512{return Err("Paper source label must contain 1..512 bytes".into());}
    if import.trades.len()>10000{return Err("Paper import exceeds 10000 closed trades".into());}
    Ok(import)
}
fn encoded<T:Serialize>(value:&T)->Result<Vec<u8>,String>{serde_json::to_vec(value).map_err(|e|e.to_string())}
fn verify_record(record:&ComparisonRecord)->Result<(),String>{
    use crate::archive::content_id;
    let reference=&record.reference;
    if record.schema!=1 || reference.run.schema!=1{return Err("Unsupported comparison/reference schema".into());}
    if content_id(&encoded(&reference.run)?)!=reference.run_id || content_id(&encoded(&reference.dataset)?)!=reference.run.dataset_id || content_id(&encoded(&reference.strategy)?)!=reference.run.strategy_id || content_id(record.source_json.as_bytes())!=record.source_id {
        return Err("Comparison source/reference integrity failure".into());
    }
    let import=parse_import(&record.source_json)?;
    let expected=compare_bundle(reference.clone(),&import.scope,&import.trades,&record.tolerances)?;
    if encoded(&expected)?!=encoded(&record.result)?{return Err("Comparison result does not match its frozen inputs".into());}
    Ok(())
}
/// Publishes the complete comparison atomically; repeating identical inputs returns the
/// same identifier. A comparison record is separate from a backtest/research attempt.
pub fn save_comparison(archive:&crate::archive::Archive,run_id:&str,source_json:&str,tolerances:&Tolerances)->Result<String,String>{
    let import=parse_import(source_json)?;
    let reference=archive.load(run_id)?;
    let result=compare_bundle(reference.clone(),&import.scope,&import.trades,tolerances)?;
    let record=ComparisonRecord{schema:1,reference,source_json:source_json.into(),source_id:crate::archive::content_id(source_json.as_bytes()),tolerances:tolerances.clone(),result};
    archive.write_blob("comparisons",&encoded(&record)?)
}
pub fn load_comparison(archive:&crate::archive::Archive,id:&str)->Result<ComparisonRecord,String>{
    let record:ComparisonRecord=serde_json::from_slice(&archive.read_blob("comparisons",id)?).map_err(|e|e.to_string())?;
    verify_record(&record)?;Ok(record)
}
/// Validates content and recomputes field comparisons before any publication. The embedded
/// reference makes the comparison independently portable; restoring its run is unnecessary.
pub fn restore_comparison(archive:&crate::archive::Archive,id:&str,record:&ComparisonRecord)->Result<String,String>{
    let bytes=encoded(record)?;
    if crate::archive::content_id(&bytes)!=id{return Err("Comparison backup failed integrity validation".into());}
    verify_record(record)?;
    archive.write_blob("comparisons",&bytes)
}

pub fn list_comparisons(archive:&crate::archive::Archive)->Result<Vec<String>,String>{
    let directory=archive.root().join("comparisons");
    if !directory.exists(){return Ok(Vec::new());}
    let mut ids=Vec::new();
    for entry in std::fs::read_dir(directory).map_err(|e|e.to_string())?{
        let path=entry.map_err(|e|e.to_string())?.path();
        if path.extension().and_then(|v|v.to_str())!=Some("json"){continue;}
        if let Some(id)=path.file_stem().and_then(|v|v.to_str()){
            load_comparison(archive,id)?;ids.push(id.to_string());
        }
    }
    ids.sort();Ok(ids)
}
