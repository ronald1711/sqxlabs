use sqx_pipeline::{Pipeline,Task,TaskKind,WalkForwardSpec,PipelineEvent,run_observed};
use sqx_optimizer::{GeneratorConfig,WalkForwardCriteria};
use sqx_core::{Bar,Instrument};
use sqx_backtest::BacktestConfig;

#[test]
fn matrix_preserves_each_specification_full_report_and_failed_write_boundary() {
    let bars:Vec<_>=(0..300).map(|i|{let p=1.1+(i as f64*0.3).sin()*0.01;Bar::new(i*60000,p,p+0.003,p-0.003,p+0.0002,10.0)}).collect();
    let instrument=Instrument::fx("EURUSD");let config=BacktestConfig::default();let gen=GeneratorConfig::default();
    let studies=vec![
        WalkForwardSpec{windows:2,candidates:2,oos_fraction:0.2,criteria:WalkForwardCriteria{min_is_trades:1,min_oos_trades_per_window:1}},
        WalkForwardSpec{windows:3,candidates:2,oos_fraction:0.4,criteria:WalkForwardCriteria{min_is_trades:1000,min_oos_trades_per_window:1000}},
    ];
    let pipeline=Pipeline{name:"matrix".into(),tasks:vec![Task{kind:TaskKind::WalkForwardStudy{studies:studies.clone()}}]};
    let mut events=Vec::new();
    let result=run_observed(&pipeline,&bars,&instrument,&config,525600.0,&gen,123,&mut |e|{events.push(serde_json::to_value(e).unwrap());Ok(())}).unwrap();
    assert_eq!(result.walk_forward_studies.len(),2);assert!(result.databank.is_empty());
    for (i,cell) in result.walk_forward_studies.iter().enumerate() {
        let s=&studies[i];
        let direct=sqx_optimizer::walk_forward_generated_observed(&bars,&instrument,&config,525600.0,&gen,s.windows,s.oos_fraction,s.candidates,123,&s.criteria,&mut |_|Ok(())).unwrap();
        assert_eq!(serde_json::to_value(&cell.report).unwrap(),serde_json::to_value(direct).unwrap());
        assert_eq!(serde_json::to_value(&cell.specification).unwrap(),serde_json::to_value(s).unwrap());
        assert_eq!(events.iter().filter(|e|e["kind"]=="walk_forward_study_started"&&e["study_index"]==i).count(),1);
        assert_eq!(events.iter().filter(|e|e["kind"]=="walk_forward_study_outcome"&&e["study_index"]==i).count(),1);
        assert_eq!(events.iter().filter(|e|e["kind"]=="walk_forward"&&e["candidate_index"]==i&&e["event"]["kind"]=="candidate_outcome").count(),cell.report.candidates_attempted);
    }
    assert!(!result.walk_forward_studies[1].report.evidence_sufficient);
    // Every persisted boundary must stop the operation immediately on failure.
    for boundary in 0..events.len() {
        let mut prefix=Vec::new();
        let err=run_observed(&pipeline,&bars,&instrument,&config,525600.0,&gen,123,&mut |e|{
            prefix.push(serde_json::to_value(e).unwrap());
            if prefix.len()==boundary+1 {Err("write unavailable".into())}else{Ok(())}
        }).unwrap_err();
        assert_eq!(err,"write unavailable");assert_eq!(prefix,events[..=boundary]);
    }
}

#[test]
fn invalid_or_mixed_matrix_is_rejected_before_execution() {
    let valid=WalkForwardSpec{windows:2,candidates:1,oos_fraction:0.3,criteria:WalkForwardCriteria::default()};
    let mut invalid=valid.clone();invalid.criteria.min_is_trades=0;
    for tasks in [vec![Task{kind:TaskKind::WalkForwardStudy{studies:vec![]}}],vec![Task{kind:TaskKind::WalkForwardStudy{studies:vec![valid.clone(),invalid]}}],vec![Task{kind:TaskKind::WalkForwardStudy{studies:vec![valid]}},Task{kind:TaskKind::Generate{count:1,min_trades:1}}]] {
        let result=run_observed(&Pipeline{name:"invalid".into(),tasks},&[],&Instrument::fx("EURUSD"),&BacktestConfig::default(),252.0,&GeneratorConfig::default(),1,&mut |_:PipelineEvent<'_>|panic!("execution before preflight")).unwrap();
        assert!(!result.errors.is_empty());assert!(result.walk_forward_studies.is_empty());
    }
}
