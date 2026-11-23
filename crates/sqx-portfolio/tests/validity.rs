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
