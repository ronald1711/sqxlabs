use sqx_optimizer::*;
use sqx_core::{Bar,Instrument};
use sqx_backtest::BacktestConfig;
#[test]
fn no_eligible_population_is_observed_once_without_duplicate_final_evaluation(){
 let bars=vec![Bar::new(0,1.0,1.1,0.9,1.0,1.0),Bar::new(60000,1.0,1.1,0.9,1.0,1.0)];
 let ga=GeneticConfig{population_size:12,elite_count:2,generations:3,..Default::default()};
 let mut rows=Vec::new();
 let ranked=genetic_evolve_observed(42,&GeneratorConfig::default(),&ga,Fitness::NetProfit,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,|generation,index,evaluation,qualifies|{rows.push((generation,index,evaluation.strategy.name.clone(),qualifies,evaluation.errors.clone()));Ok(())}).unwrap();
 assert!(ranked.is_empty());assert_eq!(rows.len(),12);
 for (index,row) in rows.iter().enumerate(){assert_eq!(row.0,0);assert_eq!(row.1,index);assert_eq!(row.2,format!("gen0-{index}"));assert!(!row.3);assert!(row.4.is_empty());}
}
#[test]
fn generations_have_stable_evidence_and_observer_failure_aborts(){
 let bars:Vec<_>=(0..1500).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;Bar::new(i*60000,p,p+0.003,p-0.003,p+0.0002,10.0)}).collect();
 let ga=GeneticConfig{population_size:20,elite_count:2,generations:2,min_trades:1,..Default::default()};
 let mut traces=Vec::new();
 for _ in 0..2 {
  let mut trace=Vec::new();
  let ranked=genetic_evolve_observed(42,&GeneratorConfig::default(),&ga,Fitness::NetProfit,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),525600.0,|generation,index,evaluation,qualifies|{trace.push((generation,index,evaluation.strategy.clone(),evaluation.metrics.net_profit.to_bits(),evaluation.errors.clone(),qualifies));Ok(())}).unwrap();
  assert!(!ranked.is_empty());assert_eq!(trace.len(),60);assert_eq!(trace.last().unwrap().0,2);traces.push(trace);
 }
 assert_eq!(traces[0],traces[1]);
 let mut observed=0;
 let result=genetic_evolve_observed(42,&GeneratorConfig::default(),&ga,Fitness::NetProfit,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),525600.0,|_,_,_,_|{observed+=1;Err("evidence sink unavailable".into())});
 assert_eq!(result.unwrap_err(),"evidence sink unavailable");assert_eq!(observed,1);
}

#[test]
fn serialized_generation_checkpoints_resume_with_identical_future_evidence(){
 let bars:Vec<_>=(0..1500).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;Bar::new(i*60000,p,p+0.003,p-0.003,p+0.0002,10.0)}).collect();
 let ga=GeneticConfig{population_size:20,elite_count:2,generations:3,min_trades:1,..Default::default()};
 let initial=EvolutionState::new(42,&GeneratorConfig::default(),&ga,Fitness::NetProfit,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),525600.0).unwrap();
 let mut reference=initial.clone();let mut restored=initial;
 loop {
  let mut left=Vec::new();let mut right=Vec::new();
  let a=reference.advance(|generation,index,evaluation,eligible|{left.push(serde_json::json!({"generation":generation,"index":index,"strategy":evaluation.strategy,"metrics":evaluation.metrics,"errors":evaluation.errors,"eligible":eligible}));Ok(())}).unwrap();
  restored=serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
  let b=restored.advance(|generation,index,evaluation,eligible|{right.push(serde_json::json!({"generation":generation,"index":index,"strategy":evaluation.strategy,"metrics":evaluation.metrics,"errors":evaluation.errors,"eligible":eligible}));Ok(())}).unwrap();
  assert_eq!(left,right);assert_eq!(a.complete,b.complete);assert_eq!(serde_json::to_value(&a.ranked).unwrap(),serde_json::to_value(&b.ranked).unwrap());
  assert_eq!(serde_json::to_vec(&reference).unwrap(),serde_json::to_vec(&restored).unwrap());
  if a.complete{break;}
 }
 assert_eq!(restored.generation(),3);assert!(restored.advance(|_,_,_,_|Ok(())).is_err());
}
#[test]
fn failed_observer_leaves_checkpoint_unchanged_and_legacy_random_stream_is_preserved(){
 use rand::{SeedableRng,RngCore};
 let mut legacy=rand::rngs::StdRng::seed_from_u64(123);let mut explicit=rand_chacha::ChaCha12Rng::seed_from_u64(123);
 for index in 0..100{assert_eq!(random_strategy(&mut legacy,&format!("s{index}"),&GeneratorConfig::default()),random_strategy(&mut explicit,&format!("s{index}"),&GeneratorConfig::default()));}
 assert_eq!(legacy.next_u64(),explicit.next_u64());
 let bars=vec![Bar::new(0,1.0,1.1,0.9,1.0,1.0),Bar::new(60000,1.0,1.1,0.9,1.0,1.0)];
 let mut state=EvolutionState::new(42,&GeneratorConfig::default(),&GeneticConfig::default(),Fitness::NetProfit,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0).unwrap();
 let before=serde_json::to_vec(&state).unwrap();assert!(state.advance(|_,_,_,_|Err("sink failed".into())).is_err());assert_eq!(serde_json::to_vec(&state).unwrap(),before);
 let mut malformed=serde_json::to_value(&state).unwrap();malformed["generation"]=serde_json::json!(1001);
 let mut malformed:EvolutionState=serde_json::from_value(malformed).unwrap();assert!(malformed.advance(|_,_,_,_|Ok(())).is_err());
}
