use sqx_core::{Bar,Instrument};
use sqx_backtest::BacktestConfig;
use sqx_optimizer::{walk_forward_with_criteria,GeneratorConfig,WalkForwardCriteria};
fn bars()->Vec<Bar>{(0..1500).map(|i|{
    let p=1.1+(i as f64*0.3).sin()*0.01;
    Bar::new(i*60000,p,p+0.003,p-0.003,p+0.0002,10.0)
}).collect()}
#[test]
fn windows_carry_capital_and_do_not_discard_failed_oos_evidence(){
    let data=bars();let cfg=BacktestConfig::default();
    let gen=GeneratorConfig{min_conditions:1,max_conditions:1,sl_points:(10.0,20.0),pt_points:(10.0,20.0),..Default::default()};
    let criteria=WalkForwardCriteria{min_is_trades:1,min_oos_trades_per_window:usize::MAX};
    let report=walk_forward_with_criteria(&data,&Instrument::fx("EURUSD"),&cfg,525600.0,&gen,3,0.3,80,42,&criteria);
    assert!(report.errors.is_empty(),"{:?}",report.errors);
    assert!(report.windows>=2,"fixture needs multiple selected strategies: {:?}",report.window_results);
    assert_eq!(report.candidates_attempted,240);
    assert!(!report.evidence_sufficient);
    let mut balance=cfg.initial_deposit;
    for window in &report.window_results {
        assert_eq!(window.candidate_evaluations.len(),80);
        assert_eq!(window.candidate_evaluations.iter().filter(|c|c.selected).count(),usize::from(window.selected_strategy.is_some()));
        for (index,candidate) in window.candidate_evaluations.iter().enumerate(){
            assert_eq!(candidate.candidate_index,index);
            assert_eq!(candidate.eligible,candidate.exclusion_reason.is_none());
            if candidate.selected {assert_eq!(window.selected_candidate_index,Some(index));assert_eq!(window.selected_strategy.as_ref(),Some(&candidate.strategy));}
        }
        assert_eq!(window.starting_equity,balance);
        balance=window.final_equity;
        assert!(!window.evidence_sufficient);
        assert!(window.train_start<window.test_start && window.test_start<window.test_end);
    }
    assert!((report.oos_net_profit-(balance-cfg.initial_deposit)).abs()<1e-7);
    assert!(report.oos_trades>0,"insufficient evidence must not erase losing/test trades");
    let repeat=walk_forward_with_criteria(&data,&Instrument::fx("EURUSD"),&cfg,525600.0,&gen,3,0.3,80,42,&criteria);
    assert_eq!(report.oos_net_profit.to_bits(),repeat.oos_net_profit.to_bits());
    let mut changed=data.clone();for bar in &mut changed[1400..]{bar.high+=0.02;bar.low-=0.02;}
    let altered=walk_forward_with_criteria(&changed,&Instrument::fx("EURUSD"),&cfg,525600.0,&gen,3,0.3,80,42,&criteria);
    assert_eq!(report.window_results[0].selected_strategy,altered.window_results[0].selected_strategy);
    assert_eq!(report.window_results[0].final_equity,altered.window_results[0].final_equity);
}
#[test]
fn supplied_family_selection_is_causal_and_never_generates_unrelated_rules(){
 use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Operand,PriceField,Comparator,Join,MoneyManagement,ExitType};
 let mut original=Strategy::new("fixed family");original.long=true;original.short=false;original.money_management=MoneyManagement::FixedSize{size:0.01};
 original.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Price(PriceField::Close,0),Comparator::Gt,Operand::Const(0.0))],Join::And));
 original.exits.push(ExitType::ExitAfterBars(2));
 let mut alternative=original.clone();alternative.name="same entry, five-bar exit".into();alternative.exits=vec![ExitType::ExitAfterBars(5)];
 let family=vec![original,alternative];let data=bars();let cfg=BacktestConfig::default();let criteria=WalkForwardCriteria{min_is_trades:1,min_oos_trades_per_window:1};
 let report=sqx_optimizer::walk_forward_candidates(&data,&Instrument::fx("EURUSD"),&cfg,525600.0,&family,3,0.3,42,&criteria);
 assert!(report.errors.is_empty());assert_eq!(report.candidates_attempted,6);assert!(report.windows>0);
 for window in &report.window_results {if let Some(selected)=&window.selected_strategy{assert!(family.contains(selected));}}
 let mut changed=data.clone();for bar in &mut changed[350..500]{bar.high+=0.1;bar.low-=0.1;bar.close=bar.open;}
 let altered=sqx_optimizer::walk_forward_candidates(&changed,&Instrument::fx("EURUSD"),&cfg,525600.0,&family,3,0.3,42,&criteria);
 assert_eq!(report.window_results[0].selected_strategy,altered.window_results[0].selected_strategy);
}

#[test]
fn invalid_training_candidates_remain_visible_and_cannot_pass_evidence(){
    use sqx_strategy::Strategy;
    let mut invalid=Strategy::new("missing session cutoff");
    invalid.trading_options.exit_at_end_of_day=true;
    invalid.trading_options.session_close_utc_minute=None;
    let report=sqx_optimizer::walk_forward_candidates(&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),525600.0,&[invalid.clone()],3,0.3,42,&WalkForwardCriteria::default());
    assert_eq!(report.candidates_attempted,3);
    assert_eq!(report.errors.len(),3);
    assert!(!report.evidence_sufficient);
    assert_eq!(report.windows,0);
    for window in report.window_results {
        assert!(window.selected_strategy.is_none());
        assert_eq!(window.candidate_evaluations.len(),1);
        let evaluation=&window.candidate_evaluations[0];
        assert!(evaluation.metrics.is_none());assert!(!evaluation.eligible);assert!(!evaluation.selected);
        assert_eq!(evaluation.exclusion_reason.as_deref(),Some("execution_failed"));
        assert_eq!(evaluation.strategy,invalid);assert!(!evaluation.errors.is_empty());
        assert_eq!(window.candidate_failures.len(),1);
        let failure=&window.candidate_failures[0];
        assert_eq!(failure.candidate_index,0);assert_eq!(failure.strategy,invalid);
        assert!(failure.errors[0].contains("session_close_utc_minute"));
    }
}

#[test]
fn training_selection_matches_independent_runs_with_carried_capital(){
    use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Operand,PriceField,Comparator,Join,MoneyManagement,ExitType};
    let mut family=Vec::new();
    for duration in [2,5,9] {
        let mut s=Strategy::new(&format!("exit-{duration}"));s.long=true;s.short=false;
        s.money_management=MoneyManagement::RiskFixedPctOfAccount{risk:2.0,stop_loss_points:100.0};
        s.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Price(PriceField::Close,0),Comparator::Gt,Operand::Const(0.0))],Join::And));
        s.exits=vec![ExitType::ExitAfterBars(duration)];family.push(s);
    }
    family.push(family[0].clone()); // duplicate identities must select the first stable occurrence
    let data=bars();let instrument=Instrument::fx("EURUSD");let cfg=BacktestConfig::default();
    let criteria=WalkForwardCriteria{min_is_trades:1,min_oos_trades_per_window:1};
    let report=sqx_optimizer::walk_forward_candidates(&data,&instrument,&cfg,525600.0,&family,3,0.3,42,&criteria);
    assert!(report.errors.is_empty(),"{:?}",report.errors);assert_eq!(report.windows,3);
    assert_ne!(report.window_results[1].starting_equity,cfg.initial_deposit);
    for window in &report.window_results {
        let start=window.index*500;let split=start+350;
        let mut current=cfg.clone();current.initial_deposit=window.starting_equity;
        assert_eq!(window.candidate_evaluations.len(),family.len());
        let scored=family.iter().enumerate().map(|(index,s)|{
            let run=sqx_backtest::backtest_window(s,&data[..split],start,&instrument,&current);
            assert!(run.errors.is_empty());
            let metrics=sqx_metrics::compute(&run,350,525600.0);
            let evidence=&window.candidate_evaluations[index];
            assert_eq!(evidence.strategy,*s);
            assert_eq!(serde_json::to_value(evidence.metrics.unwrap()).unwrap(),serde_json::to_value(metrics).unwrap());
            assert_eq!(evidence.eligible,sqx_optimizer::fitness(&metrics,sqx_optimizer::Fitness::ProfitFactor,1).is_finite());
            (s.clone(),metrics)
        }).collect();
        let ranked=sqx_optimizer::rank(scored,sqx_optimizer::Fitness::ProfitFactor,1);
        assert_eq!(window.selected_strategy.as_ref(),Some(&ranked[0].0));
        assert_eq!(window.selected_candidate_index,family.iter().position(|s|s==&ranked[0].0));
        let oos=sqx_backtest::backtest_window(&ranked[0].0,&data[..start+500],split,&instrument,&current);
        assert_eq!(window.final_equity.to_bits(),oos.final_equity.to_bits());
    }
}

#[test]
fn unsuccessful_selection_keeps_valid_zero_trade_candidates(){
    let family=vec![sqx_strategy::Strategy::new("no signals"),sqx_strategy::Strategy::new("another empty strategy")];
    let report=sqx_optimizer::walk_forward_candidates(&bars(),&Instrument::fx("EURUSD"),&BacktestConfig::default(),525600.0,&family,3,0.3,42,&WalkForwardCriteria::default());
    assert!(report.errors.is_empty());assert_eq!(report.candidates_attempted,6);
    for window in report.window_results {
        assert_eq!(window.candidate_evaluations.len(),2);assert!(window.selected_candidate_index.is_none());
        for candidate in window.candidate_evaluations {
            assert_eq!(candidate.strategy,family[candidate.candidate_index]);
            assert_eq!(candidate.metrics.unwrap().number_of_trades,0.0);
            assert!(candidate.errors.is_empty());assert!(!candidate.eligible);assert!(!candidate.selected);
            assert_eq!(candidate.exclusion_reason.as_deref(),Some("insufficient_in_sample_trades"));
        }
    }
}
