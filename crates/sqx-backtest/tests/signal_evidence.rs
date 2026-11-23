use sqx_backtest::{backtest,BacktestConfig};
use sqx_core::{Bar,Instrument};
use sqx_strategy::{Strategy,Signal,SignalKind,Condition,Operand,PriceField,Comparator,Join,MoneyManagement};
fn fixture()->(Strategy,Vec<Bar>){
 let mut s=Strategy::new("crossing evidence");s.long=true;s.short=false;s.money_management=MoneyManagement::FixedSize{size:0.1};
 s.signals.push(Signal::new(SignalKind::LongEntry,vec![Condition::new(Operand::Price(PriceField::Close,0),Comparator::CrossAbove,Operand::Const(2.0))],Join::And));
 s.signals.push(Signal::new(SignalKind::LongExit,vec![Condition::new(Operand::Price(PriceField::Close,0),Comparator::CrossBelow,Operand::Const(2.0))],Join::And));
 let bars=[1.0,3.0,1.0,1.0,1.0].into_iter().enumerate().map(|(i,p)|Bar::new(i as i64*60000,p,p+0.1,p-0.1,p,10.0)).collect();(s,bars)
}
#[test]
fn crossing_evidence_uses_signal_candle_not_fill_candle_and_survives_future_changes(){
 let (s,mut bars)=fixture();let instrument=Instrument::fx("EURUSD");let config=BacktestConfig::default();
 let result=backtest(&s,&bars,&instrument,&config);assert!(result.errors.is_empty());
 let entry=result.execution.entry_attempts[0].signal.as_ref().unwrap();assert_eq!(entry.bar_index,1);assert!(entry.matched);let c=&entry.conditions[0];assert_eq!((c.left,c.previous_left,c.right,c.previous_right),(Some(3.0),Some(1.0),Some(2.0),Some(2.0)));assert!(c.matched);
 let exit=result.execution.scheduled_exit_decisions[0].signal.as_ref().unwrap();assert_eq!(exit.bar_index,2);let c=&exit.conditions[0];assert_eq!((c.left,c.previous_left),(Some(1.0),Some(3.0)));assert_eq!(result.execution.scheduled_exit_decisions[0].executed_time,Some(180000));
 bars[4]=Bar::new(240000,9.0,10.0,8.0,9.5,100.0);
 let changed=backtest(&s,&bars,&instrument,&config);assert!(changed.errors.is_empty());assert_eq!(&changed.execution.entry_attempts[0].signal.as_ref().unwrap(),&entry);assert_eq!(changed.execution.scheduled_exit_decisions[0].signal.as_ref().unwrap(),exit);
}
#[test]
fn or_signal_keeps_undefined_and_false_conditions_visible(){
 let (mut s,bars)=fixture();s.signals[0].join=Join::Or;
 s.signals[0].conditions.push(Condition::new(Operand::Price(PriceField::Close,100),Comparator::Gt,Operand::Const(2.0)));
 let result=backtest(&s,&bars,&Instrument::fx("EURUSD"),&BacktestConfig::default());assert!(result.errors.is_empty());
 let signal=result.execution.entry_attempts[0].signal.as_ref().unwrap();assert_eq!(signal.join,Join::Or);assert_eq!(signal.conditions.len(),2);assert!(signal.matched);assert_eq!(signal.conditions[1].left,None);assert_eq!(signal.conditions[1].right,Some(2.0));assert!(!signal.conditions[1].matched);
}
