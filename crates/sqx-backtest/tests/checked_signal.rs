use sqx_backtest::{compile_signal,compile_signal_checked};
use sqx_core::Bar;
use sqx_strategy::{Comparator,Condition,Join,Operand,Param,PriceField,Signal,SignalKind};
#[test]
fn checked_compilation_rejects_invalid_signal_definitions(){
    let bars=[Bar::new(0,1.0,1.1,0.9,1.0,1.0)];
    for operand in [Operand::Indicator("unknown".into(),vec![]),Operand::Price(PriceField::Close,-1),Operand::Const(f64::NAN),Operand::Indicator("SMA".into(),vec![Param{name:"period".into(),value:0.0}])] {
        let signal=Signal::new(SignalKind::LongEntry,vec![Condition{left:operand,cmp:Comparator::Gt,right:Operand::Const(0.0)}],Join::And);
        assert!(compile_signal_checked(&signal,&bars).is_err(),"accepted {:?}",signal.conditions[0].left);
        assert_eq!(compile_signal(&signal,&bars),vec![false]);
    }
    let warmup=Signal::new(SignalKind::LongEntry,vec![Condition{left:Operand::Indicator("SMA".into(),vec![Param{name:"period".into(),value:20.0}]),cmp:Comparator::Gt,right:Operand::Const(0.0)}],Join::And);
    assert_eq!(compile_signal_checked(&warmup,&bars).unwrap(),vec![false],"ordinary warmup is not invalid configuration");
}
