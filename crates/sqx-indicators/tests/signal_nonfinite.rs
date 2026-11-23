use sqx_indicators::*;
#[test]
fn undefined_and_infinite_operands_never_trigger_signal_helpers(){
 for invalid in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY]{
  for a in [[0.0,invalid],[invalid,2.0]]{
   assert!(!crosses_above(&a,&[1.0,1.0])[1]);assert!(!crosses_below(&a,&[1.0,1.0])[1]);
   assert!(!crosses_above(&[0.0,2.0],&a)[1]);assert!(!crosses_below(&[2.0,0.0],&a)[1]);
   assert!(!crosses_above_level(&a,1.0)[1]);assert!(!crosses_below_level(&a,1.0)[1]);
   assert!(!changes_up(&a)[1]);assert!(!changes_down(&a)[1]);assert!(!is_rising(&a,1)[1]);assert!(!is_falling(&a,1)[1]);
  }
  assert_eq!(above(&[invalid],0.0),vec![false]);assert_eq!(below(&[invalid],0.0),vec![false]);
  assert_eq!(above(&[1.0],invalid),vec![false]);assert_eq!(below(&[1.0],invalid),vec![false]);
  assert!(!crosses_above_level(&[0.0,2.0],invalid)[1]);assert!(!crosses_below_level(&[2.0,0.0],invalid)[1]);
 }
 assert!(crosses_above(&[0.0,2.0],&[1.0,1.0])[1]);assert!(crosses_below(&[2.0,0.0],&[1.0,1.0])[1]);assert!(changes_up(&[0.0,2.0])[1]);assert!(changes_down(&[2.0,0.0])[1]);
}
