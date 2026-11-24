use sqx_pipeline::{Pipeline,Task,TaskKind,run_observed};
use sqx_core::{Bar,Instrument};
use sqx_backtest::BacktestConfig;
use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Operand,Comparator,Join};
use sqx_optimizer::GeneratorConfig;

#[test]
fn robustness_pipeline_preserves_reports_and_every_scenario_checkpoint(){
    let bars:Vec<_>=(0..200).map(|i|{let p=1.0+i as f64*0.0001;Bar::new(i*60000,p,p+0.0001,p-0.0001,p,1.0)}).collect();
    let mut strategy=Strategy::new("fixture");strategy.long=true;strategy.short=false;
    strategy.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));
    let scenarios=vec![(1.0,0.0,0.0),(-1.0,0.0,0.0)];
    let pipeline=Pipeline{name:"robustness".into(),tasks:vec![Task{kind:TaskKind::Robustness{strategy:strategy.clone(),scenarios:scenarios.clone(),mc_runs:3,mc_seed:42,skip_probability:0.1,retest_runs:3,retest_seed:7,windows:2,candidates:2,oos_fraction:0.25}}]};
    let config=BacktestConfig::default();let instrument=Instrument::fx("EURUSD");let generator=GeneratorConfig::default();
    let mut events=Vec::new();
    let report=run_observed(&pipeline,&bars,&instrument,&config,525600.0,&generator,42,&mut |event|{events.push(serde_json::to_value(event).unwrap());Ok(())}).unwrap();
    let study=report.robustness.unwrap();assert!(report.errors.iter().any(|e|e.contains("What-if scenario 1")));assert!(report.databank.is_empty());
    let base=sqx_backtest::backtest(&strategy,&bars,&instrument,&config);
    assert_eq!(serde_json::to_value(&study.base).unwrap(),serde_json::to_value(&base).unwrap());
    assert_eq!(serde_json::to_value(&study.what_if).unwrap(),serde_json::to_value(sqx_robustness::what_if(&strategy,&bars,&instrument,&config,&scenarios)).unwrap());
    assert_eq!(serde_json::to_value(&study.monte_carlo).unwrap(),serde_json::to_value(sqx_robustness::monte_carlo_trade_manipulation(&base.trades,3,0.1,42)).unwrap());
    assert_eq!(serde_json::to_value(&study.retest).unwrap(),serde_json::to_value(sqx_robustness::monte_carlo_retest(&strategy,&bars,&instrument,&config,3,7)).unwrap());
    assert_eq!(serde_json::to_value(&study.walk_forward).unwrap(),serde_json::to_value(sqx_optimizer::walk_forward(&bars,&instrument,&config,525600.0,&generator,2,0.25,2,42)).unwrap());
    assert_eq!(events.iter().filter(|e|e["kind"]=="robustness_base_outcome").count(),1);
    assert_eq!(events.iter().filter(|e|e["kind"]=="robustness_scenario").count(),16); // 2 costs + 3 MC + 3 retests, each start/outcome.
    for boundary in 0..events.len(){
        let mut observed=0;
        let stopped=run_observed(&pipeline,&bars,&instrument,&config,525600.0,&generator,42,&mut |event|{
            assert_eq!(serde_json::to_value(&event).unwrap(),events[observed]);
            let fail=observed==boundary;observed+=1;
            if fail {Err("archive unavailable".into())}else{Ok(())}
        });
        assert_eq!(stopped.unwrap_err(),"archive unavailable");assert_eq!(observed,boundary+1);
    }
    let mixed=Pipeline{name:"invalid mix".into(),tasks:vec![pipeline.tasks[0].clone(),Task{kind:TaskKind::Build{count:1,keep:1}}]};
    let mut calls=0;
    let report=run_observed(&mixed,&bars,&instrument,&config,525600.0,&generator,42,&mut |_|{calls+=1;Ok(())}).unwrap();
    assert_eq!(calls,0);assert!(!report.errors.is_empty());
}

#[test]
fn sensitivity_preserves_only_requested_stages_and_stops_at_every_boundary(){
    let bars:Vec<_>=(0..60).map(|i|sqx_core::Bar::new(i*60000,1.0,1.001,0.999,1.0,1.0)).collect();
    let mut strategy=Strategy::new("web sensitivity");
    strategy.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Const(1.0),Comparator::Gt,Operand::Const(0.0))],Join::And));
    let pipeline=Pipeline{name:"sensitivity".into(),tasks:vec![Task{kind:TaskKind::Sensitivity{strategy,scenarios:vec![(0.0,0.0,0.0)],mc_runs:2,mc_seed:42,skip_probability:0.0}}]};
    let instrument=Instrument::fx("EURUSD");let config=BacktestConfig::default();let generator=GeneratorConfig::default();
    let mut events=Vec::new();
    let report=run_observed(&pipeline,&bars,&instrument,&config,525600.0,&generator,42,&mut |event|{events.push(serde_json::to_value(event).unwrap());Ok(())}).unwrap();
    let study=report.robustness.unwrap();assert!(study.retest.is_none());assert!(study.walk_forward.is_none());
    assert_eq!(study.what_if.len(),1);assert_eq!(study.monte_carlo.unwrap().samples.len(),2);
    assert!(!events.iter().any(|e|e["kind"]=="walk_forward"));
    for boundary in 0..events.len(){
        let mut observed=0;
        let result=run_observed(&pipeline,&bars,&instrument,&config,525600.0,&generator,42,&mut |event|{
            assert_eq!(serde_json::to_value(event).unwrap(),events[observed]);
            let stop=observed==boundary;observed+=1;if stop{Err("storage failed".into())}else{Ok(())}
        });
        assert_eq!(result.unwrap_err(),"storage failed");assert_eq!(observed,boundary+1);
    }
}
