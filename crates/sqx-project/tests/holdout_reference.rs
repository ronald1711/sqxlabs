//! Hand-calculated EURUSD/USD fixture; expected fills and cash do not use engine helpers.
use serde_json::{json,Value};
use sqx_project::experiment::{Experiments,ExperimentData,ExperimentPlan};
use sqx_backtest::BacktestConfig;
use sqx_core::{Bar,Instrument};
use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Comparator,Operand,PriceField,Join,MoneyManagement,ExitType,Param};
fn near(value:&Value, expected:f64){let actual=value.as_f64().unwrap();assert!((actual-expected).abs()<1e-8,"{actual} != {expected}");}
#[test]
fn frozen_holdout_matches_hand_calculated_long_and_short_fills(){
    // M1 bid bars increase by two pips each minute. With a 3-close SMA,
    // close[5]=1.1010 > SMA(3,4,5)=1.1008, so the first holdout open
    // (minute 6) is eligible using development history only.
    let data=ExperimentData{bars:(0..10).map(|i|{let price=(11000+2*i) as f64/10000.0;
        Bar::new(i*60000,price,price,price,price,1.0)}).collect(),
        instrument:Instrument::fx("EURUSD"),provenance:json!({"source":"hand_calculated_fixture","timeframe":"M1"})};
    for short in [false,true] {
        let path=std::env::temp_dir().join(format!("sqx-holdout-reference-{}-{short}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
        let _cleanup=Cleanup(path.clone());let store=Experiments::new(&path);
        let mut config=BacktestConfig::default();config.spread_points=1.0;config.commission_per_lot=3.5;
        let plan=ExperimentPlan{schema:1,engine_id:"hand-reference-engine".into(),dataset_id:String::new(),validation_start:Some(4),requirements:None,holdout_start:6,
            config,min_development_trades:1,min_holdout_trades:2,periods_per_year:525600.0,prior_exposure_disclosed:false};
        let mut strategy=Strategy::new("frozen SMA3");strategy.long=!short;strategy.short=short;
        strategy.money_management=MoneyManagement::FixedSize{size:0.1};strategy.exits=vec![ExitType::ExitAfterBars(1)];
        strategy.signals.push(Signal::new(if short{SignalKind::ShortEntry}else{SignalKind::LongEntry},vec![Condition::new(
            Operand::Price(PriceField::Close,0),Comparator::Gt,Operand::Indicator("SMA".into(),vec![Param::new("period",3.0)]))],Join::And));
        let mut gated = plan.clone();
        gated.requirements = Some(serde_json::from_value(json!({
            "min_trades": 1, "min_net_profit": 1_000.0,
            "min_profit_factor": 1.0, "max_drawdown_pct": 5.0
        })).unwrap());
        let gated_id = store.create(&data, gated).unwrap();
        let gated_trial = store.trial(&gated_id, "hand-reference-engine", &strategy).unwrap();
        assert_eq!(gated_trial["report"]["assessment"]["passed"], false);
        assert_eq!(gated_trial["report"]["validation"]["checks"][1]["decision"], "rejected");
        assert!(store.reveal(&gated_id, "hand-reference-engine", 0).is_err());
        assert_eq!(store.status(&gated_id).unwrap()["state"], "development");
        let id=store.create(&data,plan.clone()).unwrap();
        let development=store.trial(&id,"hand-reference-engine",&strategy).unwrap();
        let validation = &development["report"]["validation"];
        let validation_trades = validation["execution"]["trades"].as_array().unwrap();
        assert_eq!(validation_trades.len(), 1);
        assert_eq!(validation_trades[0]["open_time"], 4 * 60000);
        assert_eq!(validation_trades[0]["close_time"], 5 * 60000);
        near(&validation["metrics"]["net_profit"], if short { -3.35 } else { 0.65 });
        let mut changed_validation = data.clone();
        let bar = &mut changed_validation.bars[5];
        bar.open += 0.001;
        bar.high += 0.001;
        bar.low += 0.001;
        bar.close += 0.001;
        let validation_id = store.create(&changed_validation, plan.clone()).unwrap();
        let updated = store.trial(&validation_id, "hand-reference-engine", &strategy).unwrap();
        assert_eq!(updated["report"]["execution"], development["report"]["execution"]);
        assert_ne!(updated["report"]["validation"]["metrics"], validation["metrics"]);
        // Mutating only future bars cannot change the registered development result.
        let mut changed=data.clone();for (index,bar) in changed.bars[6..].iter_mut().enumerate(){let delta=(index+1) as f64*0.001;bar.open+=delta;bar.high+=delta;bar.low+=delta;bar.close+=delta;}
        let alternate=store.create(&changed,plan).unwrap();
        assert_eq!(development,store.trial(&alternate,"hand-reference-engine",&strategy).unwrap());
        let report=store.reveal(&id,"hand-reference-engine",0).unwrap();
        assert_eq!(report["execution"]["errors"],json!([]));
        let trades=report["execution"]["trades"].as_array().unwrap();assert_eq!(trades.len(),2);
        // 0.1 lot = 10000 EUR: one pip is $1. Long earns 2-1 spread=$1;
        // short loses 2+1 spread=$3. Each round trip costs $0.35 commission.
        let expected=if short{[(1.1012,1.1015),(1.1016,1.1019)]}else{[(1.1013,1.1014),(1.1017,1.1018)]};
        for (index,(trade,(entry,exit))) in trades.iter().zip(expected).enumerate(){
            assert_eq!(trade["open_time"],(6+2*index)*60000);
            assert_eq!(trade["close_time"],(7+2*index)*60000);
            near(&trade["open_price"],entry);near(&trade["close_price"],exit);near(&trade["commission"],0.35);
        }
        near(&report["execution"]["initial_deposit"],10000.0);
        near(&report["execution"]["final_equity"],if short{9993.30}else{10001.30});
        near(&report["metrics"]["net_profit"],if short{-6.70}else{1.30});
        assert_eq!(report["evidence_sufficient"],true);
        let alternate_report=store.reveal(&alternate,"hand-reference-engine",0).unwrap();
        assert_ne!(alternate_report["execution"]["final_equity"],report["execution"]["final_equity"],
            "future mutation must affect holdout results for a nonvacuous causality check");
        // A restarted store returns the frozen outcome and rejects new development.
        let reopened=Experiments::new(&path);
        assert_eq!(reopened.reveal(&id,"hand-reference-engine",0).unwrap(),report);
        assert!(reopened.trial(&id,"hand-reference-engine",&strategy).is_err());
    }
}
