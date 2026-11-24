use sqx_backtest::BacktestConfig;
use sqx_core::{Bar,Instrument};
use sqx_strategy::{Strategy,StopLoss};
use sqx_robustness::{what_if_observed,monte_carlo_retest_observed,RobustnessEvent};

fn bars()->Vec<Bar>{(0..100).map(|i|Bar::new(i*60000,1.0,1.1,0.9,1.0,1.0)).collect()}

#[test]
fn what_if_checkpoints_retain_invalid_results_and_stop_on_every_write_failure(){
    let bars=bars();let strategy=Strategy::new("cost scenarios");let instrument=Instrument::fx("EURUSD");
    let config=BacktestConfig::default();let scenarios=[(-1.0,0.0,0.0),(2.0,1.0,3.0)];
    for fail_at in 0..=4 {
        let mut seen=0;let mut started=None;
        let result=what_if_observed(&strategy,&bars,&instrument,&config,&scenarios,&mut |event|{
            let position=seen;seen+=1;
            match event {
                RobustnessEvent::WhatIfStarted{index,config}=>{
                    assert_eq!(position,2*index);started=Some(config.clone());
                },
                RobustnessEvent::WhatIfOutcome{index,result,row}=>{
                    assert_eq!(position,2*index+1);
                    let independent_run=sqx_backtest::backtest(&strategy,&bars,&instrument,started.as_ref().unwrap());
                    assert_eq!(format!("{result:?}"),format!("{independent_run:?}"));
                    assert_eq!(row.errors.is_empty(),index!=0);
                },
                _=>panic!("unexpected event"),
            }
            if position==fail_at {Err("journal unavailable".into())} else {Ok(())}
        });
        assert_eq!(seen,(fail_at+1).min(4));
        if fail_at<4 {assert_eq!(result.unwrap_err(),"journal unavailable");}
        else {assert_eq!(result.unwrap().len(),2);}
    }
}

#[test]
fn retest_checkpoints_freeze_jitter_and_preserve_failed_and_skipped_trials(){
    let bars=bars();let instrument=Instrument::fx("EURUSD");
    let mut strategy=Strategy::new("retest");strategy.stop_loss=Some(StopLoss::Points(10.0));
    for invalid in [false,true] {
        let config=BacktestConfig{spread_points:if invalid{-1.0}else{0.0},..Default::default()};
        for fail_at in 0..=6 {
            let mut seen=0;let mut frozen=None;let mut offsets=Vec::new();
            let report=monte_carlo_retest_observed(&strategy,&bars,&instrument,&config,3,7,&mut |event|{
                let position=seen;seen+=1;
                match event {
                    RobustnessEvent::RetestStarted{index,start_offset,strategy}=>{
                        assert_eq!(position,2*index);offsets.push(start_offset);
                        frozen=Some((start_offset,strategy.clone()));
                    },
                    RobustnessEvent::RetestOutcome{index,result,errors}=>{
                        assert_eq!(position,2*index+1);assert_eq!(!errors.is_empty(),invalid);
                        let (offset,strategy)=frozen.as_ref().unwrap();
                        let repeated=sqx_backtest::backtest_window(strategy,&bars,*offset,&instrument,&config);
                        assert_eq!(format!("{result:?}"),format!("{repeated:?}"));
                    },
                    _=>panic!("unexpected event"),
                }
                if position==fail_at{Err("journal unavailable".into())}else{Ok(())}
            });
            assert_eq!(seen,(fail_at+1).min(6));
            if fail_at<6 {assert_eq!(report.unwrap_err(),"journal unavailable");}
            else {let report=report.unwrap();assert_eq!(report.start_offsets,offsets);assert_eq!(report.invalid_runs,if invalid{3}else{0});}
        }
    }
    let mut skipped=0;
    let report=monte_carlo_retest_observed(&strategy,&bars[..20],&instrument,&Default::default(),3,7,&mut |event|{
        match event{RobustnessEvent::RetestSkipped{index,available_bars,..}=>{assert_eq!(index,skipped);assert!(available_bars<50);skipped+=1;},_=>panic!("short window was evaluated")};Ok(())
    }).unwrap();
    assert_eq!((skipped,report.skipped_runs),(3,3));
    let stopped=monte_carlo_retest_observed(&strategy,&bars[..20],&instrument,&Default::default(),3,7,&mut |_|Err("skip write failed".into()));
    assert_eq!(stopped.unwrap_err(),"skip write failed");
}
