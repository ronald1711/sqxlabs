//! Candidate and subset evidence before exploratory portfolio selection.
use super::*;

#[derive(Debug,Clone,serde::Serialize,serde::Deserialize)]
pub struct SearchConfig{pub min_trades:usize,pub shortlist_limit:usize,pub min_subset:usize,pub max_subset:usize}
impl Default for SearchConfig{fn default()->Self{Self{min_trades:5,shortlist_limit:12,min_subset:2,max_subset:4}}}
#[derive(Debug,Clone,serde::Serialize)]
pub struct CandidateExecution{pub strategy:Strategy,pub result:BacktestResult,pub metrics:Metrics,pub errors:Vec<String>}
#[derive(Debug,Clone,Default,serde::Serialize)]
pub struct SearchReport{
 pub candidates:Vec<CandidateExecution>,pub eligible:usize,pub shortlist:Vec<usize>,
 pub subset:Option<SubsetSearchReport>,pub selected:Vec<usize>,pub combined:Option<PortfolioResult>,pub errors:Vec<String>,
}
#[derive(Debug,serde::Serialize)]
#[serde(tag="kind",rename_all="snake_case")]
pub enum SearchEvent<'a>{
 CandidateStarted{index:usize,strategy:&'a Strategy},
 CandidateOutcome{index:usize,evaluation:&'a CandidateExecution},
 Shortlist{eligible:usize,indices:&'a [usize]},
 Subset{event:SubsetEvent<'a>},
 CombinationStarted{indices:&'a [usize],weights:&'a [f64]},
 CombinationOutcome{result:&'a PortfolioResult},
}
#[allow(clippy::too_many_arguments)]
pub fn search_observed(strategies:&[Strategy],bars:&[Bar],instrument:&Instrument,config:&BacktestConfig,pyp:f64,selection:&SearchConfig,
 observer:&mut impl FnMut(SearchEvent<'_>)->Result<(),String>)->Result<SearchReport,String>{
 search_using(strategies,bars,instrument,config,pyp,selection,observer,backtest)
}
#[allow(clippy::too_many_arguments)]
fn search_using(strategies:&[Strategy],bars:&[Bar],instrument:&Instrument,config:&BacktestConfig,pyp:f64,selection:&SearchConfig,
 observer:&mut impl FnMut(SearchEvent<'_>)->Result<(),String>,mut evaluate:impl FnMut(&Strategy,&[Bar],&Instrument,&BacktestConfig)->BacktestResult)->Result<SearchReport,String>{
 let mut report=SearchReport::default();
 if strategies.is_empty() || strategies.len()>2000 || selection.shortlist_limit==0 || selection.shortlist_limit>12 || selection.min_subset==0 || selection.min_subset>selection.max_subset || selection.max_subset>selection.shortlist_limit || !pyp.is_finite() || pyp<=0.0 {
  report.errors.push("Portfolio search requires 1..2000 candidates, a shortlist of 1..12, valid subset bounds and positive annualization".into());return Ok(report);
 }
 if let Err(error)=sqx_backtest::validate_backtest_inputs(bars,instrument,config){report.errors.push(error);return Ok(report);}
 for (index,strategy) in strategies.iter().enumerate(){
  observer(SearchEvent::CandidateStarted{index,strategy})?;
  let result=evaluate(strategy,bars,instrument,config);let mut errors=result.errors.clone();
  if let Err(error)=sqx_metrics::validate_observations(&result){if !errors.contains(&error){errors.push(error);}}
  let evaluation=CandidateExecution{strategy:strategy.clone(),metrics:compute(&result,bars.len(),pyp),result,errors};
  observer(SearchEvent::CandidateOutcome{index,evaluation:&evaluation})?;
  report.errors.extend(evaluation.errors.iter().map(|error|format!("Candidate {index}: {error}")));report.candidates.push(evaluation);
 }
 if !report.errors.is_empty(){return Ok(report);}
 let mut order:Vec<usize>=(0..report.candidates.len()).collect();
 order.sort_by(|&a,&b|report.candidates[b].metrics.profit_factor.partial_cmp(&report.candidates[a].metrics.profit_factor).unwrap_or(std::cmp::Ordering::Equal));
 order.retain(|&i|report.candidates[i].metrics.number_of_trades>=selection.min_trades as f64 && report.candidates[i].metrics.profit_factor.is_finite());
 report.eligible=order.len();order.truncate(selection.shortlist_limit);report.shortlist=order;
 observer(SearchEvent::Shortlist{eligible:report.eligible,indices:&report.shortlist})?;
 if report.shortlist.len()<selection.min_subset{report.errors.push("Not enough qualifying strategies to form a portfolio".into());return Ok(report);}
 let shortlisted:Vec<_>=report.shortlist.iter().map(|&i|report.candidates[i].result.clone()).collect();
 let subset=best_subset_observed(&shortlisted,selection.min_subset,selection.max_subset,config.initial_deposit,&mut |event|observer(SearchEvent::Subset{event}))?;
 report.errors.extend(subset.errors.iter().cloned());report.selected=subset.selected.iter().map(|&i|report.shortlist[i]).collect();report.subset=Some(subset);
 if !report.errors.is_empty(){return Ok(report);}
 let components:Vec<_>=report.selected.iter().map(|&i|report.candidates[i].result.clone()).collect();
 let names:Vec<_>=report.selected.iter().map(|&i|report.candidates[i].strategy.name.clone()).collect();
 let weights=equal_weights(components.len());observer(SearchEvent::CombinationStarted{indices:&report.selected,weights:&weights})?;
 let combined=combine(&names,&components,&weights,config.initial_deposit,bars.len(),pyp);
 observer(SearchEvent::CombinationOutcome{result:&combined})?;
 report.errors.extend(combined.errors.iter().cloned());report.combined=Some(combined);Ok(report)
}

#[cfg(test)]
mod tests{
 use super::*;
 #[test]
 fn malformed_candidate_outputs_are_preserved_without_subset_selection(){
  let bars:Vec<_>=(0..4).map(|i|Bar::new(i*60000,1.0,1.1,0.9,1.0,1.0)).collect();let config=BacktestConfig::default();let instrument=Instrument::fx("EURUSD");
  for missing_errors in [false,true]{
   let mut events=Vec::new();let report=search_using(&[Strategy::new("bad"),Strategy::new("valid")],&bars,&instrument,&config,252.0,&SearchConfig::default(),&mut |event|{events.push(serde_json::to_value(event).unwrap());Ok(())},|strategy,bars,instrument,config|{
    let mut result=backtest(strategy,bars,instrument,config);if strategy.name=="bad"{result.final_equity=f64::NAN;if !missing_errors{result.errors.push("invalid result".into());}}result
   }).unwrap();
   assert_eq!(report.candidates.len(),2);assert!(!report.candidates[0].errors.is_empty());assert!(report.candidates[1].errors.is_empty());assert!(report.selected.is_empty());assert!(report.subset.is_none());assert!(!report.errors.is_empty());assert_eq!(events.len(),4);
  }
 }
 #[test]
 fn full_search_preserves_rejected_candidates_and_stops_at_every_storage_boundary(){
  use sqx_strategy::{Signal,SignalKind,Condition,Operand,Comparator,Join};
  let bars:Vec<_>=(0..40).map(|i|{let p=1.1+(i as f64*0.7).sin()*0.01;Bar::new(i*60000,p,p+0.001,p-0.001,p,1.0)}).collect();
  let mut active=Strategy::new("first");
  for kind in [SignalKind::LongEntry,SignalKind::LongExit]{active.signals.push(Signal::new(kind,vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));}
  let mut second=active.clone();second.name="second".into();
  let strategies=[Strategy::new("rejected: no trades"),active,second];
  let instrument=Instrument::fx("EURUSD");let config=BacktestConfig::default();
  let selection=SearchConfig{min_trades:5,shortlist_limit:3,min_subset:2,max_subset:2};
  let mut events=Vec::new();
  let report=search_observed(&strategies,&bars,&instrument,&config,252.0,&selection,&mut |event|{events.push(serde_json::to_value(event).unwrap());Ok(())}).unwrap();
  assert!(report.errors.is_empty(),"{:?}",report.errors);
  assert_eq!(report.candidates.len(),3);assert_eq!(report.candidates[0].metrics.number_of_trades,0.0);
  assert_eq!(report.eligible,2);assert_eq!(report.shortlist,vec![1,2]);assert_eq!(report.selected,vec![1,2]);
  assert_eq!(report.subset.as_ref().unwrap().attempted,1);
  let combined=report.combined.as_ref().unwrap();assert!(combined.errors.is_empty());
  // Equal weights of identical executions must preserve every equity sample.
  assert_eq!(combined.equity.len(),report.candidates[1].result.equity.len());
  for (actual,expected) in combined.equity.iter().zip(&report.candidates[1].result.equity){assert_eq!(actual.0,expected.0);assert!((actual.1-expected.1).abs()<1e-9);}
  assert_eq!(events.iter().map(|e|e["kind"].as_str().unwrap()).collect::<Vec<_>>(),vec!["candidate_started","candidate_outcome","candidate_started","candidate_outcome","candidate_started","candidate_outcome","shortlist","subset","subset","combination_started","combination_outcome"]);
  for fail_at in 0..events.len(){
   let mut seen=Vec::new();let evaluated=std::cell::Cell::new(0);
   let result=search_using(&strategies,&bars,&instrument,&config,252.0,&selection,&mut |event|{
    seen.push(serde_json::to_value(event).unwrap());if seen.len()==fail_at+1{Err("storage unavailable".into())}else{Ok(())}
   },|strategy,bars,instrument,config|{evaluated.set(evaluated.get()+1);backtest(strategy,bars,instrument,config)});
   assert_eq!(result.unwrap_err(),"storage unavailable");assert_eq!(seen,events[..=fail_at]);
   assert_eq!(evaluated.get(),((fail_at+1)/2).min(3),"no evaluation after a failed checkpoint {fail_at}");
  }
 }

}
