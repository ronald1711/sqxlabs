use rand::{SeedableRng,RngCore,rngs::StdRng};
use sqx_optimizer::{GeneratorConfig,try_random_strategy};
#[test]
fn invalid_ranges_are_rejected_without_consuming_rng(){
 let mut configs=Vec::new();
 let mut c=GeneratorConfig::default();c.sl_points=(20.0,10.0);configs.push(c);
 let mut c=GeneratorConfig::default();c.min_conditions=4;c.max_conditions=1;configs.push(c);
 let mut c=GeneratorConfig::default();c.fixed_size=f64::NAN;configs.push(c);
 let mut c=GeneratorConfig::default();c.long=false;c.short=false;configs.push(c);
 let mut c=GeneratorConfig::default();c.param_ranges=vec![("RSI".into(),"period".into(),20.0,10.0)];configs.push(c);
 let mut c=GeneratorConfig::default();c.param_ranges=vec![("NotAnIndicator".into(),"period".into(),1.0,2.0)];configs.push(c);
 for config in configs {
  let mut rng=StdRng::seed_from_u64(42);let mut expected=rng.clone();
  assert!(try_random_strategy(&mut rng,"invalid",&config).is_err());
  assert_eq!(rng.next_u64(),expected.next_u64());
 }
}
#[test]
fn valid_fixed_ranges_are_seed_reproducible(){
 let mut config=GeneratorConfig::default();config.sl_points=(10.0,10.0);config.param_ranges=vec![("RSI".into(),"period".into(),14.0,14.0)];
 let a=try_random_strategy(&mut StdRng::seed_from_u64(42),"valid",&config).unwrap();
 let b=try_random_strategy(&mut StdRng::seed_from_u64(42),"valid",&config).unwrap();assert_eq!(a,b);
 config.param_ranges.push(config.param_ranges[0].clone());assert!(config.validate().is_err());
}
#[test]
fn evolution_rejects_invalid_probability_and_empty_population(){
 use sqx_optimizer::{genetic_evolve,GeneticConfig,Fitness};
 let bars=vec![sqx_core::Bar::new(0,1.0,1.1,0.9,1.0,1.0)];
 for ga in [GeneticConfig{mutation_prob:f64::NAN,..Default::default()},GeneticConfig{population_size:0,..Default::default()},GeneticConfig{crossover_prob:1.1,..Default::default()}] {
  assert!(genetic_evolve(42,&GeneratorConfig::default(),&ga,Fitness::NetProfit,&bars,&sqx_core::Instrument::fx("EURUSD"),&sqx_backtest::BacktestConfig::default(),252.0).is_err());
 }
}
#[test]
fn exit_optimization_family_preserves_rules_and_is_seeded(){
 use sqx_strategy::{Strategy,StopLoss,ProfitTarget};
 let mut original=Strategy::new("original");original.stop_loss=Some(StopLoss::Points(100.0));original.profit_target=Some(ProfitTarget::Atr{period:14,mult:2.0});
 let family=sqx_optimizer::exit_parameter_family(&original,10,42).unwrap();
 assert_eq!(family.len(),10);assert_eq!(family[0],original);
 assert_eq!(family,sqx_optimizer::exit_parameter_family(&original,10,42).unwrap());
 for variant in family {
  let Some(StopLoss::Points(distance))=variant.stop_loss else{panic!("stop type changed")};assert!((80.0..=120.0).contains(&distance));
  let Some(ProfitTarget::Atr{period,mult})=variant.profit_target else{panic!("target type changed")};assert_eq!(period,14);assert!((1.6..=2.4).contains(&mult));
  let mut restored=variant;restored.name=original.name.clone();restored.stop_loss=original.stop_loss;restored.profit_target=original.profit_target;assert_eq!(restored,original);
 }
 let no_exits=Strategy::new("unchanged");assert_eq!(sqx_optimizer::exit_parameter_family(&no_exits,10,42).unwrap(),vec![no_exits]);
}
#[test]
fn explicit_parameter_family_applies_ranges_without_changing_rule_structure(){
 use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Operand,Comparator,Join,Param};
 let mut original=Strategy::new("RSI family");original.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Indicator("RSI".into(),vec![Param::new("period",14.0)]),Comparator::Lt,Operand::Const(30.0))],Join::And));
 let ranges=vec![("RSI".into(),"period".into(),5.0,7.0)];
 let family=sqx_optimizer::parameter_family(&original,10,42,&ranges).unwrap();
 assert_eq!(family.len(),10);assert_eq!(family[0],original);
 assert_eq!(family,sqx_optimizer::parameter_family(&original,10,42,&ranges).unwrap());
 for mut member in family.into_iter().skip(1){
  let Operand::Indicator(_,params)=&mut member.signals[0].conditions[0].left else{panic!("rule changed")};
  assert!((5.0..=7.0).contains(&params[0].value));assert_eq!(params[0].value.fract(),0.0);
  params[0].value=14.0;member.name=original.name.clone();assert_eq!(member,original);
 }
 assert!(sqx_optimizer::parameter_family(&original,10,42,&[("SMA".into(),"period".into(),5.0,7.0)]).is_err());
}

#[test]
fn period_range_endpoints_cannot_produce_invalid_execution_settings(){
    for (lo,hi) in [(0.0,10.0),(-2.0,3.0),(2.5,9.0),(2.0,9.5)] {
        let config=sqx_optimizer::GeneratorConfig{param_ranges:vec![("SMA".into(),"period".into(),lo,hi)],..Default::default()};
        assert!(config.validate().is_err());
    }
}

#[test]
fn parallel_batch_retains_each_failed_candidate_in_input_order(){
    let candidates:Vec<_>=(0..12).map(|i|{
        let mut s=sqx_strategy::Strategy::new(&format!("candidate-{i}"));
        if i%2==0{s.money_management=sqx_strategy::MoneyManagement::FixedSize{size:-1.0};}s
    }).collect();
    let bars:Vec<_>=(0..20).map(|i|sqx_core::Bar::new(i*60000,1.0,1.1,0.9,1.0,1.0)).collect();
    let rows=sqx_optimizer::backtest_batch_detailed(&candidates,&bars,&sqx_core::Instrument::fx("EURUSD"),&sqx_backtest::BacktestConfig::default(),252.0);
    assert_eq!(rows.len(),candidates.len());
    for (index,row) in rows.iter().enumerate(){
        assert_eq!(row.strategy,candidates[index]);assert_eq!(!row.errors.is_empty(),index%2==0);
        if index%2==0{assert!(row.metrics.net_profit.is_nan());}else{assert_eq!(row.metrics.net_profit,0.0);}
    }
}
