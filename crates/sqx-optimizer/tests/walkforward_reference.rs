//! Independent arithmetic oracle for a deliberately small, declared WF contract.
//! No backtest, indicator, metrics or ranking helper computes the expected values.
use sqx_core::{Bar,Instrument};
use sqx_backtest::BacktestConfig;
use sqx_optimizer::{walk_forward_candidates,WalkForwardCriteria};
use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Operand,PriceField,Comparator,Join,MoneyManagement,ExitType,Param};

fn family()->Vec<Strategy>{
    [true,false].into_iter().map(|long|{
        let mut s=Strategy::new(if long{"SMA3 long"}else{"SMA3 short"});s.long=long;s.short=!long;
        s.signals.push(Signal::new(if long{SignalKind::LongEntry}else{SignalKind::ShortEntry},vec![Condition::new(
            Operand::Price(PriceField::Close,0),if long{Comparator::Gt}else{Comparator::Lt},
            Operand::Indicator("SMA".into(),vec![Param::new("period",3.0)]))],Join::And));
        s.exits=vec![ExitType::ExitAfterBars(1)];
        s.money_management=MoneyManagement::RiskFixedPctOfAccount{risk:2.0,stop_loss_points:100.0};s
    }).collect()
}
#[derive(Debug)]
struct Reference {balance:f64,gains:f64,losses:f64,trades:usize,first_entry:Option<usize>}
impl Reference {fn pf(&self)->f64{self.gains/self.losses}}
fn oracle(prices:&[i64],start:usize,end:usize,initial:f64,long:bool)->Reference {
    let mut r=Reference{balance:initial,gains:0.0,losses:0.0,trades:0,first_entry:None};
    let mut entry=start.max(3); // SMA3 needs three closed candles before the fill.
    while entry<end {
        let signal=entry-1;
        let sum:i64=prices[signal-2..=signal].iter().sum();
        let comparison=prices[signal]*3-sum;
        // Integer-pip prices make this a direct arithmetic SMA check, without
        // floating indicator state or access to any future candle.
        assert_ne!(comparison,0,"fixture avoids threshold-rounding ambiguity");
        if (long && comparison<0)||(!long && comparison>0){entry+=1;continue;}
        let exit=(entry+1).min(end-1); // one occupied candle; final boundary liquidates
        // EURUSD/USD, 100000-unit lot, $10/pip, 100-pip sizing distance,
        // 2% current cash, 0.01-lot downward quantization, $3.50/lot round trip.
        let lots=(r.balance/500.0).floor()/100.0;
        let delta=(prices[exit]-prices[entry]) as f64 * if long{1.0}else{-1.0};
        let net=(delta*10.0-3.5)*lots;
        r.balance+=net;r.trades+=1;r.first_entry.get_or_insert(entry);
        if net>0.0{r.gains+=net;}else{r.losses-=net;}
        entry=exit+1; // next close decision can enter only at the following open
    }
    r
}
fn near(actual:f64,expected:f64){assert!((actual-expected).abs()<1e-7,"{actual} != {expected}");}
#[test]
fn walk_forward_matches_integer_price_reference_and_future_mutation(){
    let cycle=[10000,10020,10040,10010,9990,10030,9980,10060,10000,10010,10050];
    let prices:Vec<_>=(0..180).map(|i|cycle[i%cycle.len()]).collect();
    let bars:Vec<_>=prices.iter().enumerate().map(|(i,p)|{let p=*p as f64/10000.0;Bar::new(i as i64*60000,p,p,p,p,1.0)}).collect();
    let mut config=BacktestConfig::default();config.commission_per_lot=3.5;
    let candidates=family();let criteria=WalkForwardCriteria{min_is_trades:1,min_oos_trades_per_window:1};
    let run=|bars:&[Bar]|walk_forward_candidates(bars,&Instrument::fx("EURUSD"),&config,525600.0,&candidates,2,0.3,42,&criteria);
    let report=run(&bars);assert!(report.errors.is_empty(),"{:?}",report.errors);assert_eq!(report.windows,2);
    let mut balance=10000.0;let mut trades=0;let mut gains=0.0;let mut losses=0.0;
    for (w,window) in report.window_results.iter().enumerate(){
        let start=w*90;let split=start+63;let end=start+90;
        assert_eq!((window.train_start,window.test_start,window.test_end),(bars[start].time,bars[split].time,bars[end-1].time));
        near(window.starting_equity,balance);
        let expected:Vec<_>=[true,false].into_iter().map(|long|oracle(&prices,start,split,balance,long)).collect();
        for (candidate,r) in window.candidate_evaluations.iter().zip(&expected){
            assert!(r.gains>0.0 && r.losses>0.0,"nonvacuous win/loss fixture: {r:?}");
            let m=candidate.metrics.unwrap();near(m.net_profit,r.balance-balance);near(m.gross_profit,r.gains);near(m.gross_loss,-r.losses);near(m.profit_factor,r.pf());assert_eq!(m.number_of_trades,r.trades as f64);
        }
        let selected=usize::from(expected[1].pf()>expected[0].pf());assert_eq!(window.selected_candidate_index,Some(selected));
        let oos=oracle(&prices,split,end,balance,selected==0);
        near(window.final_equity,oos.balance);assert_eq!(window.oos_trades,oos.trades);
        balance=oos.balance;trades+=oos.trades;gains+=oos.gains;losses+=oos.losses;
        // At least one direction can fill on the first OOS bar using IS warmup.
        assert!([true,false].into_iter().any(|long|oracle(&prices,split,end,10000.0,long).first_entry==Some(split)));
    }
    assert_ne!(report.window_results[1].starting_equity,10000.0);
    assert_eq!(report.oos_trades,trades);near(report.oos_net_profit,balance-10000.0);near(report.oos_profit_factor,gains/losses);
    let mut future=bars.clone();for b in &mut future[63..90]{b.open+=0.02;b.high=b.open;b.low=b.open;b.close=b.open;}
    let changed=run(&future);
    assert_eq!(serde_json::to_value(&report.window_results[0].candidate_evaluations).unwrap(),serde_json::to_value(&changed.window_results[0].candidate_evaluations).unwrap());
    assert_eq!(report.window_results[0].selected_candidate_index,changed.window_results[0].selected_candidate_index);
    assert_ne!(report.window_results[0].final_equity,changed.window_results[0].final_equity,"future mutation must affect OOS to make causality test nonvacuous");
}

#[test]
fn impossible_window_count_is_rejected_before_generation_or_iteration(){
    let bars:Vec<_>=(0..180).map(|i|Bar::new(i*60000,1.0,1.0,1.0,1.0,1.0)).collect();
    let report=walk_forward_candidates(&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default(),525600.0,&family(),usize::MAX,0.3,42,&WalkForwardCriteria::default());
    assert_eq!(report.errors.len(),1);assert_eq!(report.candidates_attempted,0);
    assert!(report.window_results.is_empty());assert!(!report.evidence_sufficient);
}
