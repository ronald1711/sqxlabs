use sqx_backtest::BacktestConfig;
use sqx_core::{Bar,Instrument};
use sqx_strategy::Strategy;
use sqx_robustness::{what_if,monte_carlo_retest,monte_carlo_trade_manipulation};
fn bars()->Vec<Bar>{(0..100).map(|i|Bar::new(1700000000000+i*60000,1.0,1.1,0.9,1.0,1.0)).collect()}
#[test]
fn invalid_cost_scenario_is_not_zero_profit(){
 let rows=what_if(&Strategy::new("fixture"),&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),&[(-1.0,0.0,0.0),(0.0,0.0,0.0)]);
 assert!(rows[0].net_profit.is_nan());assert!(!rows[0].errors.is_empty());
 assert_eq!(rows[1].net_profit,0.0);assert!(rows[1].errors.is_empty());
}
#[test]
fn failed_retests_are_counted_and_cannot_produce_valid_quantiles(){
 let cfg=BacktestConfig{spread_points:-1.0,..Default::default()};
 let report=monte_carlo_retest(&Strategy::new("fixture"),&bars(),&Instrument::fx("EURUSD"),&cfg,10,42);
 assert_eq!((report.requested_runs,report.runs,report.invalid_runs,report.skipped_runs),(10,0,10,0));
 assert!(!report.errors.is_empty());assert!(report.net_profit_50pct.is_nan());assert!(report.losing_runs_pct.is_nan());
}
#[test]
fn short_windows_are_reported_as_skipped_not_successful(){
 let report=monte_carlo_retest(&Strategy::new("fixture"),&bars()[..20],&Instrument::fx("EURUSD"),&BacktestConfig::default(),10,42);
 assert_eq!((report.runs,report.skipped_runs,report.invalid_runs),(0,10,0));assert!(!report.errors.is_empty());
}
#[test]
fn missing_trades_and_invalid_probabilities_are_explicitly_unavailable(){
 for probability in [0.1,-0.1,1.1,f64::NAN]{
  let report=monte_carlo_trade_manipulation(&[],100,probability,42);
  assert_eq!(report.runs,0);assert!(!report.errors.is_empty());assert!(report.net_profit_50pct.is_nan());
 }
}

#[test]
fn retest_warms_on_prefix_and_matches_hand_calculated_entry_timing(){
 use sqx_strategy::{Param,Signal,SignalKind,Condition,Operand,Comparator,Join,MoneyManagement};
 let bars:Vec<_>=(0..300).map(|i|{let p=1.0+i as f64*0.001;Bar::new(1700000000000+i*60000,p,p+0.0005,p-0.0005,p,1.0)}).collect();
 let mut strategy=Strategy::new("SMA warmup fixture");strategy.long=true;strategy.short=false;
 strategy.money_management=MoneyManagement::FixedSize{size:0.1};
 strategy.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Indicator("SMA".into(),vec![Param::new("period",10.0)]),Comparator::Gt,Operand::Const(0.0))],Join::And));
 let cfg=BacktestConfig::default();let inst=Instrument::fx("EURUSD");
 let report=monte_carlo_retest(&strategy,&bars,&inst,&cfg,100,42);
 assert_eq!(report.runs,100);assert_eq!(report.seed,42);assert!(report.errors.is_empty());
 assert!(report.start_offsets.iter().any(|offset|*offset>10));
 // SMA(10) first exists at close 9, so earliest entry is open 10.
 // A later flat-start scenario enters at its own start open, not before it.
 // One 0.1-lot EURUSD position then exits at the final close, with zero costs.
 let mut expected:Vec<_>=report.start_offsets.iter().map(|offset|{
  let entry=(*offset).max(10);(bars[299].close-bars[entry].open)*10_000.0
 }).collect();
 expected.sort_by(f64::total_cmp);
 for (actual,p) in [(report.net_profit_5pct,0.05),(report.net_profit_50pct,0.5),(report.net_profit_95pct,0.95)] {
  let target=expected[((expected.len()-1) as f64*p).round() as usize];
  assert!((actual-target).abs()<1e-8,"{actual} != {target}");
 }
 assert_eq!(report.max_dd_95pct,0.0);
 let repeat=monte_carlo_retest(&strategy,&bars,&inst,&cfg,100,42);
 assert_eq!(repeat.start_offsets,report.start_offsets);
 assert_eq!(repeat.net_profit_50pct.to_bits(),report.net_profit_50pct.to_bits());
}

fn trade(pnl:f64)->sqx_backtest::Trade {
 sqx_backtest::Trade{id:0,direction:sqx_core::Direction::Long,open_time:0,close_time:60000,open_price:1.0,close_price:1.0,size:0.1,pnl,pnl_points:0.0,commission:0.0,financing:0.0,bars:1,mae_points:0.0,mfe_points:0.0,exit_reason:sqx_backtest::ExitReason::EndOfData}
}

#[test]
fn manipulation_checkpoints_preserve_legacy_random_paths_and_stop_on_failure(){
 use rand::{Rng,SeedableRng,seq::SliceRandom};
 use sqx_robustness::{monte_carlo_trade_manipulation_observed,RobustnessEvent};
 let trades=[trade(10.0),trade(-20.0),trade(5.0)]; // IDs deliberately identical.
 let mut rng=rand::rngs::StdRng::seed_from_u64(123);
 let mut legacy_paths=Vec::new();
 for _ in 0..20 {
  let mut path:Vec<f64>=trades.iter().filter(|_|rng.gen_bool(0.7)).map(|t|t.pnl).collect();
  path.shuffle(&mut rng);legacy_paths.push(path);
 }
 for fail_at in 0..=40 {
  let mut events=0;let mut captured=Vec::new();
  let report=monte_carlo_trade_manipulation_observed(&trades,20,0.3,123,&mut |event|{
   let position=events;events+=1;
   match event {
    RobustnessEvent::TradeManipulationStarted{index,trade_indices}=>{
     assert_eq!(position,2*index);
     let path:Vec<_>=trade_indices.iter().map(|&i|trades[i].pnl).collect();
     assert_eq!(path,legacy_paths[index]);
     let mut unique=trade_indices.to_vec();unique.sort_unstable();unique.dedup();assert_eq!(unique.len(),trade_indices.len());
    },
    RobustnessEvent::TradeManipulationOutcome{index,sample,errors}=>{
     assert_eq!(position,2*index+1);assert!(errors.is_empty());
     let sample=sample.unwrap();assert_eq!(sample.net_profit,legacy_paths[index].iter().sum::<f64>());
     assert_eq!(sample.included_trades,legacy_paths[index].len());captured.push(sample.clone());
    },
    _=>panic!("wrong event"),
   }
   if position==fail_at{Err("write failed".into())}else{Ok(())}
  });
  assert_eq!(events,(fail_at+1).min(40));
  if fail_at<40 {assert_eq!(report.unwrap_err(),"write failed");}
  else {assert_eq!(report.unwrap().samples,captured);}
 }
 let mut events=0;
 let invalid=monte_carlo_trade_manipulation_observed(&[],10,0.3,123,&mut |_|{events+=1;Ok(())}).unwrap();
 assert_eq!(events,0);assert!(!invalid.errors.is_empty());
 let mut events=0;
 let overflow=monte_carlo_trade_manipulation_observed(&[trade(f64::MAX),trade(f64::MAX)],10,0.0,123,&mut |event|{
  events+=1;
  if let RobustnessEvent::TradeManipulationOutcome{sample,errors,..}=event{assert!(sample.is_none());assert!(!errors.is_empty());}
  Ok(())
 }).unwrap();
 assert_eq!(events,2);assert!(!overflow.errors.is_empty());
}
#[test]
fn zero_profit_and_skipped_all_paths_never_count_as_profitable(){
 for (trades,skip) in [(vec![trade(0.0)],0.0),(vec![trade(100.0)],1.0)] {
  let result=monte_carlo_trade_manipulation(&trades,100,skip,42);
  assert!(result.errors.is_empty());assert_eq!(result.positive_runs_pct,0.0);assert_eq!(result.flat_runs_pct,100.0);assert_eq!(result.losing_runs_pct,0.0);
  assert!(!result.meets_positive_threshold(60.0));
 }
 let winning=monte_carlo_trade_manipulation(&[trade(10.0)],100,0.0,42);
 assert_eq!(winning.positive_runs_pct,100.0);assert!(winning.meets_positive_threshold(100.0));
 assert!(!winning.meets_positive_threshold(f64::NAN));
}
#[test]
fn monte_carlo_sign_categories_partition_the_distribution(){
 let result=monte_carlo_trade_manipulation(&[trade(10.0),trade(-10.0)],1000,0.5,42);
 assert!(result.errors.is_empty());assert!(result.positive_runs_pct>0.0);assert!(result.flat_runs_pct>0.0);assert!(result.losing_runs_pct>0.0);
 assert!((result.positive_runs_pct+result.flat_runs_pct+result.losing_runs_pct-100.0).abs()<1e-10);
 let again=monte_carlo_trade_manipulation(&[trade(10.0),trade(-10.0)],1000,0.5,42);
 assert_eq!(result.positive_runs_pct.to_bits(),again.positive_runs_pct.to_bits());
}
#[test]
fn invalid_probability_and_overflow_are_not_usable_distributions(){
 let invalid=monte_carlo_trade_manipulation(&[trade(10.0)],10,f64::NAN,42);
 assert!(!invalid.errors.is_empty());assert!(!invalid.meets_positive_threshold(0.0));
 let overflow=monte_carlo_trade_manipulation(&[trade(f64::MAX),trade(f64::MAX)],10,0.0,42);
 assert!(!overflow.errors.is_empty());assert_eq!(overflow.runs,0);assert!(overflow.positive_runs_pct.is_nan());
}

#[test]
fn samples_preserve_generation_order_and_reconstruct_published_distribution(){
 let trades=[trade(10.0),trade(-10.0)];
 let report=monte_carlo_trade_manipulation(&trades,1000,0.5,42);
 assert_eq!(report.samples.len(),report.runs);
 for (index,sample) in report.samples.iter().enumerate(){
  assert_eq!(sample.run_index,index);
  match sample.included_trades {
   0=>{assert_eq!(sample.net_profit,0.0);assert_eq!(sample.max_drawdown,0.0);},
   1=>{assert!(sample.net_profit==10.0 || sample.net_profit== -10.0);assert_eq!(sample.max_drawdown,if sample.net_profit<0.0{10.0}else{0.0});},
   2=>{assert_eq!(sample.net_profit,0.0);assert_eq!(sample.max_drawdown,10.0);},
   other=>panic!("impossible retained trade count: {other}"),
  }
 }
 let mut nets:Vec<_>=report.samples.iter().map(|sample|sample.net_profit).collect();nets.sort_by(f64::total_cmp);
 let mut dds:Vec<_>=report.samples.iter().map(|sample|sample.max_drawdown).collect();dds.sort_by(f64::total_cmp);
 for (actual,p) in [(report.net_profit_5pct,0.05),(report.net_profit_50pct,0.5),(report.net_profit_95pct,0.95)]{assert_eq!(actual,nets[((nets.len()-1) as f64*p).round() as usize]);}
 assert_eq!(report.max_drawdown_95pct,dds[((dds.len()-1) as f64*0.95).round() as usize]);
 assert_eq!(report.samples,monte_carlo_trade_manipulation(&trades,1000,0.5,42).samples);
 assert!(monte_carlo_trade_manipulation(&[],1000,0.5,42).samples.is_empty());
}

#[test]
fn partial_random_start_coverage_cannot_publish_a_complete_distribution(){
 let report=monte_carlo_retest(&Strategy::new("boundary fixture"),&bars()[..50],&Instrument::fx("EURUSD"),&BacktestConfig::default(),100,42);
 assert!(report.runs>0);assert!(report.skipped_runs>0);assert_eq!(report.invalid_runs,0);
 assert_eq!(report.runs+report.skipped_runs,report.requested_runs);
 assert!(!report.errors.is_empty(),"a distribution with missing scenarios must be explicitly incomplete");
 assert!(report.net_profit_5pct.is_nan());assert!(report.net_profit_50pct.is_nan());assert!(report.net_profit_95pct.is_nan());
 assert!(report.max_dd_95pct.is_nan());assert!(report.losing_runs_pct.is_nan());
}
