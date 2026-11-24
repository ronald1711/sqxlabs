use sqx_backtest::{backtest,BacktestConfig,BacktestResult};
use sqx_core::{Bar,Instrument};
use sqx_strategy::Strategy;
use sqx_portfolio::{combine,best_subset};
fn run()->BacktestResult {
 let bars:Vec<_>=(0..4).map(|i|Bar::new(i*60000,1.0,1.1,0.9,1.0,1.0)).collect();
 backtest(&Strategy::new("flat"),&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default())
}
#[test]
fn malformed_dimensions_and_weights_are_errors_not_partial_portfolios(){
 let runs=vec![run(),run()];let names=vec!["A".into(),"B".into()];
 for weights in [vec![1.0],vec![0.2,0.2],vec![-0.5,1.5],vec![f64::NAN,0.5]] {
  let result=combine(&names,&runs,&weights,10000.0,4,252.0);
  assert!(!result.errors.is_empty());assert!(result.metrics.net_profit.is_nan());assert!(result.equity.is_empty());
 }
 assert!(!combine(&[],&runs,&[0.5,0.5],10000.0,4,252.0).errors.is_empty());
}
#[test]
fn dates_and_capital_must_match_before_equity_is_combined(){
 let names=vec!["A".into(),"B".into()];let original=run();
 let mut shifted=original.clone();shifted.equity[1].0+=1;
 assert!(!combine(&names,&[original.clone(),shifted],&[0.5,0.5],10000.0,4,252.0).errors.is_empty());
 let mut shortened=original.clone();shortened.equity.pop();
 assert!(!combine(&names,&[original.clone(),shortened],&[0.5,0.5],10000.0,4,252.0).errors.is_empty());
 let mut deposit=original.clone();deposit.initial_deposit=5000.0;
 assert!(!combine(&names,&[original,deposit],&[0.5,0.5],10000.0,4,252.0).errors.is_empty());
}
#[test]
fn aligned_weighted_equity_matches_hand_calculation(){
 let mut a=run();let mut b=run();
 for i in 0..4 {a.equity[i].1=10000.0+100.0*i as f64;b.equity[i].1=10000.0-40.0*i as f64;}
 a.final_equity=10300.0;b.final_equity=9880.0;
 let result=combine(&["A".into(),"B".into()],&[a,b],&[0.25,0.75],10000.0,4,252.0);
 assert!(result.errors.is_empty());
 assert_eq!(result.equity.iter().map(|(_,v)|*v).collect::<Vec<_>>(),vec![10000.0,9995.0,9990.0,9985.0]);
}
#[test]
fn subset_respects_requested_size_when_all_scores_are_undefined(){
 let runs=vec![run();5];
 assert_eq!(best_subset(&runs,2,2,10000.0),vec![0,1]);
 assert_eq!(best_subset(&runs,4,4,10000.0).len(),4);
 assert!(best_subset(&runs,4,2,10000.0).is_empty());
 assert!(best_subset(&runs,0,2,10000.0).is_empty());
}

#[test]
fn unknown_and_different_account_currencies_require_explicit_conversion(){
 let a=run();assert_eq!(a.execution.account_currency.as_deref(),Some("USD"));
 let names=vec!["A".into(),"B".into()];
 for currency in [None,Some("EUR".to_owned())]{
  let mut b=a.clone();b.execution.account_currency=currency;
  let result=combine(&names,&[a.clone(),b],&[0.5,0.5],10000.0,4,252.0);
  assert!(!result.errors.is_empty());assert!(result.metrics.net_profit.is_nan());assert!(result.account_currency.is_none());
 }
 let valid=combine(&names,&[a.clone(),a],&[0.5,0.5],10000.0,4,252.0);
 assert!(valid.errors.is_empty());assert_eq!(valid.account_currency.as_deref(),Some("USD"));
}

#[test]
fn engine_records_non_default_currency_for_normal_and_short_runs(){
 let mut cfg=BacktestConfig::default();cfg.account.currency="EUR".into();
 cfg.account.conversion_rates.push(sqx_backtest::ConversionRate{time:0,from:"USD".into(),to:"EUR".into(),rate:0.9});
 let bars:Vec<_>=(0..4).map(|i|Bar::new(i*60000,1.0,1.1,0.9,1.0,1.0)).collect();
 for input in [&bars[..],&bars[..1]] {
  let result=backtest(&Strategy::new("EUR fixture"),input,&Instrument::fx("EURUSD"),&cfg);
  assert!(result.errors.is_empty(),"{:?}",result.errors);
  assert_eq!(result.execution.account_currency.as_deref(),Some("EUR"));
 }
}

#[test]
fn constant_returns_have_no_defined_correlation_even_on_the_diagonal(){
 let result=combine(&["flat A".into(),"flat B".into()],&[run(),run()],&[0.5,0.5],10000.0,4,252.0);
 assert!(result.errors.is_empty());assert!(result.correlation.iter().flatten().all(|c|c.is_nan()));
}
#[test]
fn correlation_includes_first_sample_return_and_preserves_opposite_paths(){
 let mut a=run();let mut b=run();
 a.equity=vec![(0,11000.0),(60000,9900.0)];a.final_equity=9900.0;
 b.equity=vec![(0,9000.0),(60000,9900.0)];b.final_equity=9900.0;
 let result=combine(&["A".into(),"B".into()],&[a,b],&[0.5,0.5],10000.0,2,252.0);
 // Returns are [0.1,-0.1] and [-0.1,0.1], so Pearson is exactly -1.
 assert!((result.correlation[0][1]+1.0).abs()<1e-12);
 assert!((result.correlation[0][0]-1.0).abs()<1e-12);
 assert_eq!(result.correlation[0][1],result.correlation[1][0]);
}
#[test]
fn nonpositive_equity_does_not_fabricate_zero_returns(){
 let mut a=run();a.equity[1].1=0.0;
 let result=combine(&["A".into(),"B".into()],&[a,run()],&[0.5,0.5],10000.0,4,252.0);
 assert!(result.errors.is_empty());assert!(result.correlation[0][0].is_nan());assert!(result.correlation[0][1].is_nan());
}

#[test]
fn overflowing_subset_arithmetic_cannot_select_a_portfolio(){
 let mut component=run();component.initial_deposit=1e308;component.final_equity=-1e308;
 for sample in &mut component.equity{sample.1=-1e308;}
 let combined=combine(&["A".into(),"B".into()],&[component.clone(),component.clone()],&[0.5,0.5],1e308,4,252.0);
 assert!(!combined.errors.is_empty());assert!(combined.equity.is_empty());
 assert!(best_subset(&[component.clone(),component],2,2,1e308).is_empty(),"nonfinite combined arithmetic must not select the first subset");
}

#[test]
fn subset_checkpoints_match_hand_calculations_and_stop_on_write_failure(){
 let mut a=run();let mut b=run();let c=run();
 for (sample,value) in a.equity.iter_mut().zip([10000.0,10100.0,10050.0,10200.0]){sample.1=value;}a.final_equity=10200.0;
 for (sample,value) in b.equity.iter_mut().zip([10000.0,9900.0,10100.0,10100.0]){sample.1=value;}b.final_equity=10100.0;
 let inputs=[a,b,c];let mut events=Vec::new();
 let result=sqx_portfolio::best_subset_observed(&inputs,2,2,10000.0,&mut |event|{events.push(serde_json::to_value(event).unwrap());Ok(())}).unwrap();
 assert_eq!(result.selected,vec![0,1]);assert_eq!(result.attempted,3);assert_eq!(result.invalid,0);assert!(result.errors.is_empty());assert_eq!(events.len(),6);
 for (index,(components,np,dd,score)) in [
  (vec![0,1],150.0,0.0,serde_json::json!("PositiveNoDrawdown")),
  (vec![0,2],100.0,25.0,serde_json::json!({"Ratio":4.0})),
  (vec![1,2],50.0,50.0,serde_json::json!({"Ratio":1.0})),
 ].into_iter().enumerate(){
  assert_eq!(events[2*index]["kind"],"started");assert_eq!(events[2*index]["components"],serde_json::json!(components));
  let outcome=&events[2*index+1];assert_eq!(outcome["kind"],"outcome");assert_eq!(outcome["components"],serde_json::json!(components));
  assert_eq!(outcome["net_profit"],np);assert_eq!(outcome["max_drawdown"],dd);assert_eq!(outcome["score"],score);assert_eq!(outcome["errors"],serde_json::json!([]));
 }
 for failure in 0..events.len(){
  let mut seen=0;let interrupted=sqx_portfolio::best_subset_observed(&inputs,2,2,10000.0,&mut |event|{
   assert_eq!(serde_json::to_value(event).unwrap(),events[seen]);let index=seen;seen+=1;if index==failure{Err("journal failed".into())}else{Ok(())}
  });assert_eq!(interrupted.unwrap_err(),"journal failed");assert_eq!(seen,failure+1);
 }
}

#[test]
fn subset_errors_are_explicit_and_invalid_inputs_never_start_trials(){
 let mut invalid=run();invalid.execution.account_currency=None;
 let report=sqx_portfolio::best_subset_observed(&[invalid],1,1,10000.0,&mut |_|panic!("invalid input must not execute a trial")).unwrap();
 assert_eq!(report.attempted,0);assert!(report.selected.is_empty());assert!(!report.errors.is_empty());
 let mut large=run();large.initial_deposit=1e308;large.final_equity=-1e308;for sample in &mut large.equity{sample.1=-1e308;}
 let mut outcomes=Vec::new();let report=sqx_portfolio::best_subset_observed(&[large.clone(),large],1,2,1e308,&mut |event|{outcomes.push(serde_json::to_value(event).unwrap());Ok(())}).unwrap();
 assert_eq!(report.attempted,3);assert_eq!(report.invalid,3);assert!(report.selected.is_empty());assert_eq!(report.errors.len(),3);
 for row in outcomes.iter().filter(|row|row["kind"]=="outcome"){
  assert_eq!(row["score"],"Unavailable");assert!(!row["errors"].as_array().unwrap().is_empty());assert!(row["net_profit"].is_null());
 }
}

#[test]
fn explicit_composition_keeps_all_twenty_components_and_stops_on_storage_failure(){
 let bars:Vec<_>=(0..4).map(|i|Bar::new(i*60000,1.0,1.1,0.9,1.0,1.0)).collect();
 let names:Vec<_>=(0..20).map(|i|format!("Display {i}")).collect();let strategies:Vec<_>=(0..20).map(|i|Strategy::new(&format!("Rule {i}"))).collect();
 let mut events=Vec::new();let config=BacktestConfig::default();let instrument=Instrument::fx("EURUSD");
 let report=sqx_portfolio::combine_observed(&names,&strategies,&bars,&instrument,&config,252.0,&mut |event|{events.push(serde_json::to_value(event).unwrap());Ok(())}).unwrap();
 assert!(report.errors.is_empty());assert_eq!(report.names,names);assert_eq!(report.weights,vec![0.05;20]);assert_eq!(report.components.len(),20);
 assert!(report.components.iter().all(|row|row.result.trades.is_empty()));let combined=report.combined.unwrap();assert_eq!(combined.components.len(),20);
 assert_eq!(combined.components.iter().map(|row|&row.name).collect::<Vec<_>>(),names.iter().collect::<Vec<_>>());assert!(combined.equity.iter().all(|row|row.1==10000.0));
 assert_eq!(events.len(),42);assert_eq!(events[40]["kind"],"combination_started");assert_eq!(events[40]["indices"],serde_json::json!((0..20).collect::<Vec<_>>()));assert_eq!(events[41]["kind"],"combination_outcome");
 assert!(!events.iter().any(|event|matches!(event["kind"].as_str(),Some("shortlist"|"subset"))));
 for failure in 0..events.len(){let mut observed=Vec::new();let result=sqx_portfolio::combine_observed(&names,&strategies,&bars,&instrument,&config,252.0,&mut |event|{observed.push(serde_json::to_value(event).unwrap());if observed.len()==failure+1{Err("storage failure".into())}else{Ok(())}});assert_eq!(result.unwrap_err(),"storage failure");assert_eq!(observed,events[..=failure]);}
}

#[test]
fn explicit_composition_preserves_failed_components_without_silently_excluding_them(){
 let bars:Vec<_>=(0..4).map(|i|Bar::new(i*60000,1.0,1.1,0.9,1.0,1.0)).collect();let mut invalid=Strategy::new("invalid");invalid.stop_loss=Some(sqx_strategy::StopLoss::Points(-1.0));
 let names=vec!["Invalid selection".into(),"Valid selection".into()];let mut events=Vec::new();
 let report=sqx_portfolio::combine_observed(&names,&[invalid,Strategy::new("valid")],&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,&mut |event|{events.push(serde_json::to_value(event).unwrap());Ok(())}).unwrap();
 assert_eq!(report.components.len(),2);assert!(!report.components[0].errors.is_empty());assert!(report.components[1].errors.is_empty());assert!(report.combined.is_none());assert_eq!(events.len(),4);
 let rejected=sqx_portfolio::combine_observed(&[],&[Strategy::new("a"),Strategy::new("b")],&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,&mut |_|panic!("invalid names must fail before execution")).unwrap();assert!(!rejected.errors.is_empty());assert!(rejected.components.is_empty());
}
