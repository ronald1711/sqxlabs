use sqx_backtest::BacktestResult;
use sqx_metrics::compute;

#[test]
fn zero_trades_metrics_are_safe() {
    let result = BacktestResult {
        errors: Vec::new(), rejected_orders: Vec::new(), ambiguous_bars: Vec::new(), execution: Default::default(), trades: vec![],
        equity: vec![(0, 10_000.0)],
        initial_deposit: 10_000.0,
        final_equity: 10_000.0,
    };
    let m = compute(&result, 100, 252.0);
    assert_eq!(m.number_of_trades, 0.0);
    assert!(m.profit_factor.is_nan() || m.profit_factor.is_infinite());
    assert!(m.net_profit.abs() < 1e-9);
}

#[test]
fn sharpe_includes_initial_to_first_bar_and_does_not_skip_insolvency() {
    let mut result=BacktestResult {
        errors:vec![],rejected_orders:vec![],ambiguous_bars:vec![],execution:Default::default(),trades:vec![],
        equity:vec![(0,110.0),(1,99.0),(2,118.8)],initial_deposit:100.0,final_equity:118.8,
    };
    // Returns are +10%, -10%, +20%; sample variance = 7/300.
    let expected=(1.0/15.0)/(7.0_f64/300.0).sqrt()*12.0_f64.sqrt();
    assert!((compute(&result,3,12.0).sharpe_ratio-expected).abs()<1e-12);
    for invalid in [0.0,-1.0,f64::NAN,f64::INFINITY] {
        assert!(compute(&result,3,invalid).sharpe_ratio.is_nan());
    }
    result.equity=vec![(0,110.0),(1,0.0),(2,10.0)];
    assert!(compute(&result,3,12.0).sharpe_ratio.is_nan());
    result.equity=vec![(0,110.0),(1,f64::NAN),(2,118.8)];
    assert!(compute(&result,3,12.0).sharpe_ratio.is_nan());
}

#[test]
fn schema_one_account_evidence_must_reconcile_before_metrics_are_computed(){
    let mut result=BacktestResult{errors:vec![],rejected_orders:vec![],ambiguous_bars:vec![],execution:Default::default(),trades:vec![],equity:vec![(0,10000.0)],initial_deposit:10000.0,final_equity:10000.0};
    result.execution.account_evidence_schema=1;result.execution.account_currency=Some("USD".into());
    assert!(sqx_metrics::validate_observations(&result).is_err());
    assert!(compute(&result,1,252.0).net_profit.is_nan());
    result.execution.account_snapshots.push(sqx_backtest::AccountSnapshot{phase:"bar_close",bar_open_time:0,quote_time:None,trade_id:None,liquidation_price:None,quote_to_account_rate:None,margin_base_to_account_rate:None,balance:10000.0,gross_unrealized:0.0,commission_reserve:0.0,financing_accrued:0.0,equity:10000.0,required_margin:0.0,free_margin:10000.0});
    assert!(sqx_metrics::validate_observations(&result).is_ok());assert_eq!(compute(&result,1,252.0).net_profit,0.0);
    result.execution.account_snapshots[0].free_margin=9000.0;
    assert!(sqx_metrics::validate_observations(&result).is_err());assert!(compute(&result,1,252.0).net_profit.is_nan());
}
