use sqx_backtest::{backtest, AccountConfig, BacktestConfig, ConversionRate, ExitReason};
use sqx_core::{Bar, Instrument};
use sqx_strategy::{Comparator, Condition, Join, MoneyManagement, Operand, PriceField, Signal, SignalKind, StopLoss, Strategy};

fn strategy() -> Strategy {
    let mut s = Strategy::new("account-fixture");
    s.long = true; s.short = false;
    s.money_management = MoneyManagement::FixedSize { size: 1.0 };
    s.signals.push(Signal::new(SignalKind::LongEntry,
        vec![Condition::new(Operand::Price(PriceField::Close, 0), Comparator::Gt, Operand::Const(0.0))], Join::And));
    s
}
fn reconcile(result:&sqx_backtest::BacktestResult){
    assert!(result.errors.is_empty(),"{:?}",result.errors);
    assert_eq!(result.execution.cash_settlements.len(),result.trades.len());
    let mut balance=result.initial_deposit;
    for (entry,trade) in result.execution.cash_settlements.iter().zip(&result.trades) {
        assert_eq!(entry.trade_id,trade.id);assert_eq!(entry.time,trade.close_time);assert_eq!(entry.exit_reason,trade.exit_reason);
        assert_eq!(entry.balance_before.to_bits(),balance.to_bits());
        assert_eq!(entry.gross_profit_account.to_bits(),(entry.gross_profit_quote*entry.quote_to_account_rate).to_bits());
        assert_eq!(entry.net_change.to_bits(),(entry.gross_profit_account-entry.commission-entry.financing).to_bits());
        assert_eq!(entry.net_change.to_bits(),trade.pnl.to_bits());
        balance+=entry.net_change;assert_eq!(entry.balance_after.to_bits(),balance.to_bits());
    }
    assert_eq!(balance.to_bits(),result.final_equity.to_bits());
}
fn flat(time: i64, price: f64) -> Bar { Bar::new(time, price, price, price, price, 1.0) }

#[test]
fn usdjpy_profit_is_converted_at_liquidation_price() {
    let bars = [flat(0,150.0), flat(60_000,150.0), flat(120_000,151.0)];
    let r = backtest(&strategy(), &bars, &Instrument::fx("USDJPY"), &BacktestConfig::default());
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(r.trades.len(),1);reconcile(&r);
    let settlement=&r.execution.cash_settlements[0];
    assert_eq!(settlement.quote_currency,"JPY");assert_eq!(settlement.account_currency,"USD");
    assert!((settlement.gross_profit_quote-100_000.0).abs()<1e-8);
    assert_eq!(settlement.quote_to_account_rate,1.0/151.0);
    let open=r.execution.account_snapshots.iter().find(|row|row.bar_open_time==120000&&row.trade_id==Some(0)).unwrap();
    assert_eq!(open.quote_to_account_rate,Some(1.0/151.0));
    assert!((open.required_margin-100000.0/30.0).abs()<1e-8);
    assert!((open.equity-(10000.0+100000.0/151.0)).abs()<1e-8);
    assert!((r.trades[0].pnl - 100_000.0 / 151.0).abs() < 1e-8);
    assert!((r.final_equity - (10_000.0 + 100_000.0 / 151.0)).abs() < 1e-8);
}

#[test]
fn oversized_risk_order_is_rejected_not_silently_scaled() {
    let mut s = strategy();
    s.money_management = MoneyManagement::RiskFixedBalancePct { risk: 2.0 };
    s.stop_loss = Some(StopLoss::Points(1.0));
    let r = backtest(&s, &[flat(0,1.0),flat(60_000,1.0)], &Instrument::fx("EURUSD"), &BacktestConfig::default());
    assert!(r.trades.is_empty());reconcile(&r);
    assert!(r.execution.cash_settlements.is_empty());
    assert_eq!(r.rejected_orders.len(),1);
    assert_eq!(r.rejected_orders[0].reason,"insufficient_margin");
    assert!((r.rejected_orders[0].requested_lots-20.0).abs()<1e-8);
    assert_eq!(r.final_equity,10_000.0);
}

#[test]
fn margin_stopout_interpolates_continuous_bar_and_respects_gap() {
    let bars = [flat(0,1.0), flat(60_000,1.0), Bar::new(120_000,1.0,1.0,0.8,0.8,1.0)];
    let cfg=BacktestConfig::default();
    let r=backtest(&strategy(),&bars,&Instrument::fx("EURUSD"),&cfg);
    // 10000 + 100000*(p-1) = 0.5 * 100000*p/30.
    let expected=90_000.0/(100_000.0*(1.0-0.5/30.0));
    assert_eq!(r.trades[0].exit_reason,ExitReason::StopOut);
    assert!((r.trades[0].close_price-expected).abs()<1e-9);
    let gap=[flat(0,1.0),flat(60_000,1.0),flat(120_000,0.8),flat(180_000,0.8)];
    let r=backtest(&strategy(),&gap,&Instrument::fx("EURUSD"),&cfg);
    assert_eq!(r.trades[0].close_price,0.8);reconcile(&r);
    assert!(r.final_equity<0.0,"gaps can exceed margin; do not fabricate a zero floor");
    assert_eq!(r.trades.len(),1,"bankrupt account must not open another position");
}

#[test]
fn conversion_cannot_use_future_or_stale_rates() {
    let bars=[flat(0,150.0),flat(60_000,150.0),flat(120_000,151.0)];
    let mut cfg=BacktestConfig::default();
    let inst=Instrument::fx("EURJPY");
    let r=backtest(&strategy(),&bars,&inst,&cfg);
    assert!(!r.errors.is_empty()); assert!(r.final_equity.is_nan());
    cfg.account.conversion_rates.push(ConversionRate{time:120_000,from:"JPY".into(),to:"USD".into(),rate:0.006});
    assert!(!backtest(&strategy(),&bars,&inst,&cfg).errors.is_empty());
    cfg.account.conversion_rates.push(ConversionRate{time:0,from:"JPY".into(),to:"USD".into(),rate:0.007});
    let r=backtest(&strategy(),&bars,&inst,&cfg);
    assert!(r.errors.is_empty());
    assert!((r.trades[0].pnl-600.0).abs()<1e-8);
    cfg.account.max_conversion_age_ms=30_000;
    assert!(!backtest(&strategy(),&bars,&inst,&cfg).errors.is_empty());
}

#[test]
fn financing_and_commission_reconcile_trade_and_account() {
    let mut cfg=BacktestConfig::default();
    cfg.commission_per_lot=5.0;
    cfg.account.long_financing_per_lot=7.0;
    let bars=[flat(0,1.0),flat(60_000,1.0),flat(86_400_000,1.0),flat(172_800_000,1.0)];
    let r=backtest(&strategy(),&bars,&Instrument::fx("EURUSD"),&cfg);
    reconcile(&r);
    assert_eq!(r.execution.cash_settlements[0].commission,5.0);
    assert_eq!(r.execution.cash_settlements[0].financing,14.0);
    assert_eq!(r.trades[0].financing,14.0);
    assert_eq!(r.trades[0].pnl,-19.0);
    assert_eq!(r.final_equity,9981.0);
    assert_eq!(r.equity.last().unwrap().1,9981.0);
}

#[test]
fn minimum_lot_cannot_increase_requested_risk() {
    let account=AccountConfig::default();
    assert_eq!(account.lots(0.009),None);
    assert_eq!(account.lots(0.019),Some(0.01));
    assert_eq!(account.lots(f64::NAN),None);
}

#[test]
fn short_stopout_and_base_currency_margin_use_correct_side() {
    let mut s=strategy();s.long=false;s.short=true;s.signals[0].kind=SignalKind::ShortEntry;
    let bars=[flat(0,150.0),flat(60_000,150.0),Bar::new(120_000,150.0,180.0,150.0,180.0,1.0)];
    let r=backtest(&s,&bars,&Instrument::fx("USDJPY"),&BacktestConfig::default());
    // USD notional margin is 100000/30. Equity = 10000 + 100000*(150-p)/p.
    let price=15_000_000.0/(90_000.0+100_000.0/60.0);
    assert_eq!(r.trades[0].exit_reason,ExitReason::StopOut);
    assert!((r.trades[0].close_price-price).abs()<1e-8);
    assert!((r.final_equity-100_000.0/60.0).abs()<1e-7);
    let event=&r.execution.stop_out_decisions[0];
    assert!((event.quote_to_account_rate-1.0/price).abs()<1e-12);
    assert!((event.required_margin-100000.0/30.0).abs()<1e-8);
    assert!((event.unrealized_profit_account-100000.0*(150.0-price)/price).abs()<1e-8);
    assert!((event.equity_before_slippage-event.minimum_equity).abs()<1e-7);
}

#[test]
fn malformed_prices_and_impossible_slippage_fail_explicitly() {
    let mut cfg=BacktestConfig::default();cfg.slippage_points=20000.0;
    let bars=[flat(0,1.0),flat(60000,1.0)];
    assert!(!backtest(&strategy(),&bars,&Instrument::fx("EURUSD"),&cfg).errors.is_empty());
    assert!(!backtest(&strategy(),&[flat(0,1.0),flat(0,1.0)],&Instrument::fx("EURUSD"),&BacktestConfig::default()).errors.is_empty());
}

#[test]
fn price_based_allocation_converts_units_to_lots_and_account_currency(){
    for sizing in [MoneyManagement::StocksSizeByPrice{risk:2.0},MoneyManagement::CryptoSizeByPrice{risk:2.0}] {
        for (contract,rate,expected_lots) in [(1.0,1.0,2.0),(10.0,0.01,20.0)] {
            let mut s=strategy();s.money_management=sizing.clone();
            let instrument=Instrument{name:"allocation-fixture".into(),base_currency:"EUR".into(),quote_currency:if rate==1.0{"USD".into()}else{"JPY".into()},point:0.01,tick_size:0.01,tick_value:0.01,commission:0.0,contract_size:contract};
            let mut config=BacktestConfig::default();
            config.account.conversion_rates=vec![ConversionRate{from:"JPY".into(),to:"USD".into(),time:0,rate}];
            let run=backtest(&s,&[flat(0,100.0),flat(60000,100.0),flat(120000,110.0)],&instrument,&config);
            assert!(run.errors.is_empty(),"{:?}",run.errors);assert_eq!(run.trades.len(),1);
            assert!((run.trades[0].size-expected_lots).abs()<1e-10);
            assert!((run.trades[0].size*contract*100.0*rate-200.0).abs()<1e-10,"2% allocation of $10000");
            assert!((run.trades[0].pnl-20.0).abs()<1e-10,"10% price move on $200 allocation");
        }
    }
}

#[test]
fn settlement_chain_covers_repeated_long_short_exits_and_financing_credits(){
    for short in [false,true] {
        let mut s=strategy();s.long=!short;s.short=short;
        s.signals[0].kind=if short {SignalKind::ShortEntry}else{SignalKind::LongEntry};
        s.exits.push(sqx_strategy::ExitType::ExitAfterBars(1));
        let cfg=BacktestConfig{commission_per_lot:5.0,..Default::default()};
        let data:Vec<_>=(0..8).map(|i|flat(i*60_000,1.0)).collect();
        let result=backtest(&s,&data,&Instrument::fx("EURUSD"),&cfg);
        reconcile(&result);assert!(result.trades.len()>1);
        for event in &result.execution.cash_settlements {assert_eq!(event.gross_profit_quote,0.0);assert_eq!(event.net_change,-5.0);}
    }
    let mut cfg=BacktestConfig::default();cfg.account.long_financing_per_lot=-7.0;
    let result=backtest(&strategy(),&[flat(0,1.0),flat(60_000,1.0),flat(86_400_000,1.0)],&Instrument::fx("EURUSD"),&cfg);
    reconcile(&result);assert_eq!(result.execution.cash_settlements[0].financing,-7.0);assert_eq!(result.final_equity,10007.0);
}

#[test]
fn financing_events_identify_gap_rollovers_triple_charges_and_credits(){
    for cost in [7.0,-7.0] {
        let mut cfg=BacktestConfig::default();
        cfg.account.long_financing_per_lot=cost;
        cfg.account.triple_rollover_weekday=Some(4); // Friday: UTC day 1.
        let applied=3*86_400_000+1234;
        let r=backtest(&strategy(),&[flat(0,1.0),flat(60_000,1.0),flat(applied,1.0),flat(applied+60_000,1.0)],&Instrument::fx("EURUSD"),&cfg);
        reconcile(&r);let events=&r.execution.financing_accruals;
        assert_eq!(events.len(),3);let mut accrued=0.0;
        for (index,event) in events.iter().enumerate(){
            assert_eq!(event.trade_id,0);assert_eq!(event.rollover_utc_day,index as i64+1);
            assert_eq!(event.applied_at,applied);assert_eq!(event.daily_charge_per_lot,cost);
            assert_eq!(event.multiplier,if index==0{3}else{1});assert_eq!(event.lots,1.0);
            assert_eq!(event.accrued_before,accrued);
            assert_eq!(event.charge,cost*event.multiplier as f64);
            accrued+=event.charge;assert_eq!(event.accrued_after,accrued);
        }
        assert_eq!(accrued,5.0*cost);assert_eq!(r.trades[0].financing,accrued);
        assert_eq!(r.execution.cash_settlements[0].financing,accrued);
    }
}

#[test]
fn admission_evidence_reconciles_costs_margin_and_rejection(){
    for size in [1.0,20.0] {
        let mut s=strategy();s.money_management=MoneyManagement::FixedSize{size};
        let cfg=BacktestConfig{spread_points:2.0,slippage_points:1.0,commission_per_lot:5.0,..Default::default()};
        let result=backtest(&s,&[flat(0,1.0),flat(60_000,1.0)],&Instrument::fx("EURUSD"),&cfg);
        assert!(result.errors.is_empty());
        let attempt=&result.execution.entry_attempts[0];let check=attempt.margin_check.as_ref().unwrap();
        assert_eq!(check.balance,10000.0);assert_eq!(check.normalized_lots,size);
        assert!((check.proposed_fill_price-1.0003).abs()<1e-12);
        assert_eq!(check.quote_to_account_rate,1.0);assert_eq!(check.max_leverage,30.0);
        assert!((check.required_margin-size*100000.0*1.0003/30.0).abs()<1e-8);
        assert!((check.spread_and_slippage_cost-30.0*size).abs()<1e-8);
        assert_eq!(check.commission,5.0*size);
        assert!((check.equity_after_entry_costs-(10000.0-35.0*size)).abs()<1e-8);
        assert_eq!(check.free_margin_after_entry,check.equity_after_entry_costs-check.required_margin);
        assert_eq!(check.accepted,size==1.0);assert_eq!(attempt.trade_id.is_some(),check.accepted);
        if !check.accepted {assert!(attempt.blocked_by.iter().any(|r|r=="insufficient_margin"));assert!(attempt.fill_price.is_none());}
    }
    let mut s=strategy();s.money_management=MoneyManagement::FixedSize{size:0.001};
    let result=backtest(&s,&[flat(0,1.0),flat(60_000,1.0)],&Instrument::fx("EURUSD"),&BacktestConfig::default());
    let attempt=&result.execution.entry_attempts[0];assert!(attempt.margin_check.is_none());
    assert!(attempt.blocked_by.iter().any(|r|r=="invalid_volume"));
}

#[test]
fn stop_out_evidence_distinguishes_modelled_threshold_gap_and_slippage(){
    let cfg=BacktestConfig{slippage_points:2.0,commission_per_lot:5.0,..Default::default()};
    for gap in [false,true] {
        let last=if gap {flat(120_000,0.8)}else{Bar::new(120_000,1.0,1.0,0.8,0.8,1.0)};
        let r=backtest(&strategy(),&[flat(0,1.0),flat(60_000,1.0),last],&Instrument::fx("EURUSD"),&cfg);
        reconcile(&r);assert_eq!(r.execution.stop_out_decisions.len(),1);
        let event=&r.execution.stop_out_decisions[0];let trade=&r.trades[0];
        assert_eq!(event.trade_id,trade.id);assert_eq!(event.time,trade.close_time);assert_eq!(event.fill_price,trade.close_price);
        assert_eq!(event.balance,10000.0);assert_eq!(event.commission_reserve,5.0);
        assert_eq!(event.quote_to_account_rate,1.0);assert_eq!(event.stop_out_ratio,0.5);
        assert!((event.required_margin-100000.0*event.liquidation_price_before_slippage/30.0).abs()<1e-8);
        assert_eq!(event.minimum_equity,event.required_margin*0.5);
        assert!((event.fill_price-(event.liquidation_price_before_slippage-0.0002)).abs()<1e-12);
        assert!((event.equity_before_slippage-(10000.0+(event.liquidation_price_before_slippage-1.0002)*100000.0-5.0)).abs()<1e-8);
        if gap {assert_eq!(event.liquidation_price_before_slippage,0.8);assert!(event.equity_before_slippage<event.minimum_equity);assert!(event.threshold_price>0.8);}
        else {assert_eq!(event.liquidation_price_before_slippage,event.threshold_price);assert!((event.equity_before_slippage-event.minimum_equity).abs()<1e-8);}
    }
}

#[test]
fn protection_is_attached_only_to_filled_positions_with_correct_side_levels(){
    for short in [false,true] {
        let mut s=strategy();s.long=!short;s.short=short;s.signals[0].kind=if short{SignalKind::ShortEntry}else{SignalKind::LongEntry};
        s.stop_loss=Some(StopLoss::Points(100.0));s.profit_target=Some(sqx_strategy::ProfitTarget::Points(200.0));
        s.exits=vec![sqx_strategy::ExitType::TrailingStop,sqx_strategy::ExitType::MoveToBreakEven];
        let r=backtest(&s,&[flat(0,1.0),flat(60000,1.0)],&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(r.errors.is_empty());let p=r.execution.entry_attempts[0].initial_protection.as_ref().unwrap();
        assert!((p.stop_loss.unwrap()-if short{1.01}else{0.99}).abs()<1e-12);
        assert!((p.profit_target.unwrap()-if short{0.98}else{1.02}).abs()<1e-12);
        assert!(p.trailing_enabled&&p.break_even_enabled);assert!((p.dynamic_distance.unwrap()-0.01).abs()<1e-12);
        s.money_management=MoneyManagement::FixedSize{size:20.0};
        let rejected=backtest(&s,&[flat(0,1.0),flat(60000,1.0)],&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(rejected.execution.entry_attempts[0].initial_protection.is_none());
    }
    let r=backtest(&strategy(),&[flat(0,1.0),flat(60000,1.0)],&Instrument::fx("EURUSD"),&BacktestConfig::default());
    let p=r.execution.entry_attempts[0].initial_protection.as_ref().unwrap();
    assert!(p.stop_loss.is_none()&&p.profit_target.is_none()&&p.dynamic_distance.is_none());
}

#[test]
fn account_snapshots_explain_open_equity_and_final_liquidation_without_double_costs(){
    let cfg=BacktestConfig{commission_per_lot:5.0,slippage_points:2.0,..Default::default()};
    let r=backtest(&strategy(),&[flat(0,1.0),flat(60000,1.0),flat(120000,1.01)],&Instrument::fx("EURUSD"),&cfg);
    reconcile(&r);let rows=&r.execution.account_snapshots;
    assert_eq!(rows.len(),4);assert_eq!(rows[0].trade_id,None);assert_eq!(rows[0].equity,10000.0);
    let open=&rows[2];assert_eq!(open.phase,"bar_close");assert_eq!(open.trade_id,Some(0));
    assert_eq!(open.bar_open_time,120000);assert_eq!(open.quote_time,None);assert_eq!(open.liquidation_price,Some(1.01));
    assert_eq!(open.balance,10000.0);assert!((open.gross_unrealized-980.0).abs()<1e-8);
    assert_eq!(open.commission_reserve,5.0);assert!((open.equity-10975.0).abs()<1e-8);
    assert!((open.required_margin-101000.0/30.0).abs()<1e-8);assert_eq!(open.free_margin,open.equity-open.required_margin);
    let final_row=rows.last().unwrap();assert_eq!(final_row.phase,"terminal_liquidation");
    assert_eq!(final_row.trade_id,None);assert_eq!(final_row.required_margin,0.0);assert_eq!(final_row.commission_reserve,0.0);
    assert_eq!(final_row.equity,r.final_equity);assert_eq!(final_row.balance,r.final_equity);assert_eq!(final_row.free_margin,r.final_equity);
    assert!((final_row.balance-10955.0).abs()<1e-8); // final exit slippage, commission charged once
}

#[test]
fn short_snapshot_marks_at_ask_and_reserves_financing_once(){
    let mut s=strategy();s.long=false;s.short=true;s.signals[0].kind=SignalKind::ShortEntry;
    let mut cfg=BacktestConfig{spread_points:2.0,commission_per_lot:5.0,..Default::default()};
    cfg.account.short_financing_per_lot=7.0;
    let r=backtest(&s,&[flat(0,1.0),flat(60000,1.0),flat(86400000,1.0)],&Instrument::fx("EURUSD"),&cfg);
    reconcile(&r);
    let row=r.execution.account_snapshots.iter().find(|row|row.bar_open_time==86400000&&row.trade_id==Some(0)).unwrap();
    assert_eq!(row.liquidation_price,Some(1.0002));assert!((row.gross_unrealized+20.0).abs()<1e-8);
    assert_eq!(row.commission_reserve,5.0);assert_eq!(row.financing_accrued,7.0);
    assert!((row.equity-9968.0).abs()<1e-8);
    assert!((row.required_margin-100020.0/30.0).abs()<1e-8);
    assert_eq!(r.execution.account_snapshots.last().unwrap().balance,r.final_equity);
}

#[test]
fn account_evidence_rejects_missing_corrupted_and_nonfinite_records(){
    let mut cfg=BacktestConfig::default();cfg.account.long_financing_per_lot=7.0;
    let r=backtest(&strategy(),&[flat(0,1.0),flat(60000,1.0),flat(86400000,1.01)],&Instrument::fx("EURUSD"),&cfg);
    assert!(sqx_backtest::validate_account_evidence(&r).is_ok());
    let mutations:Vec<Box<dyn Fn(&mut sqx_backtest::BacktestResult)>>=vec![
        Box::new(|r|r.execution.cash_settlements.clear()),
        Box::new(|r|r.execution.cash_settlements[0].balance_before+=1.0),
        Box::new(|r|r.execution.cash_settlements[0].quote_to_account_rate=2.0),
        Box::new(|r|r.execution.cash_settlements[0].net_change+=1.0),
        Box::new(|r|r.execution.cash_settlements[0].account_currency="JPY".into()),
        Box::new(|r|r.execution.financing_accruals.clear()),
        Box::new(|r|r.execution.financing_accruals.push(r.execution.financing_accruals[0].clone())),
        Box::new(|r|r.execution.financing_accruals[0].charge+=1.0),
        Box::new(|r|r.execution.financing_accruals[0].applied_at=0),
        Box::new(|r|r.execution.account_snapshots.clear()),
        Box::new(|r|r.execution.account_snapshots[1].gross_unrealized=f64::NAN),
        Box::new(|r|r.execution.account_snapshots[1].free_margin+=1.0),
        Box::new(|r|r.execution.account_snapshots.last_mut().unwrap().balance+=1.0),
        Box::new(|r|r.equity[0].1+=1.0),
        Box::new(|r|r.final_equity+=1.0),
        Box::new(|r|r.execution.account_evidence_schema=0),
    ];
    for (index,mutate) in mutations.into_iter().enumerate(){let mut altered=r.clone();mutate(&mut altered);assert!(sqx_backtest::validate_account_evidence(&altered).is_err(),"mutation {index} was accepted");}
}

#[test]
fn account_evidence_rejects_impossible_sample_sequences(){
    let r=backtest(&strategy(),&[flat(0,1.0),flat(60000,1.0),flat(120000,1.01)],&Instrument::fx("EURUSD"),&BacktestConfig::default());
    assert!(sqx_backtest::validate_account_evidence(&r).is_ok());
    let mut duplicate=r.clone();
    duplicate.execution.account_snapshots.push(duplicate.execution.account_snapshots.last().unwrap().clone());
    assert!(sqx_backtest::validate_account_evidence(&duplicate).is_err());
    let mut premature=r.clone();
    premature.execution.account_snapshots.swap(0,3);
    assert!(sqx_backtest::validate_account_evidence(&premature).is_err());
    let mut wrong_time=r.clone();
    wrong_time.execution.account_snapshots.last_mut().unwrap().bar_open_time+=60000;
    assert!(sqx_backtest::validate_account_evidence(&wrong_time).is_err());
    let mut repeated_candle=r.clone();
    repeated_candle.execution.account_snapshots[1].bar_open_time=0;
    repeated_candle.equity[1].0=0;
    assert!(sqx_backtest::validate_account_evidence(&repeated_candle).is_err());
}

#[test]
fn account_evidence_links_each_trade_to_one_accepted_entry(){
    let r=backtest(&strategy(),&[flat(0,1.0),flat(60000,1.0)],&Instrument::fx("EURUSD"),&BacktestConfig::default());
    assert!(sqx_backtest::validate_account_evidence(&r).is_ok());
    let mutations:Vec<Box<dyn Fn(&mut sqx_backtest::BacktestResult)>>=vec![
        Box::new(|r|r.execution.entry_attempts.clear()),
        Box::new(|r|r.execution.entry_attempts.push(r.execution.entry_attempts[0].clone())),
        Box::new(|r|r.execution.entry_attempts[0].blocked_by.push("insufficient_margin".into())),
        Box::new(|r|r.execution.entry_attempts[0].fill_price=Some(2.0)),
        Box::new(|r|r.execution.entry_attempts[0].filled_lots=Some(2.0)),
        Box::new(|r|r.execution.entry_attempts[0].attempt_time+=1),
        Box::new(|r|r.execution.entry_attempts[0].signal_bar_open_time=60000),
        Box::new(|r|r.execution.entry_attempts[0].margin_check.as_mut().unwrap().accepted=false),
        Box::new(|r|r.execution.entry_attempts[0].initial_protection=None),
        Box::new(|r|r.execution.entry_attempts[0].trade_id=None),
    ];
    for (index,mutate) in mutations.into_iter().enumerate(){let mut altered=r.clone();mutate(&mut altered);assert!(sqx_backtest::validate_account_evidence(&altered).is_err(),"entry mutation {index} was accepted");}
}

#[test]
fn account_evidence_checks_accepted_and_rejected_margin_arithmetic(){
    for lots in [1.0,20.0] {
        let mut s=strategy();s.money_management=MoneyManagement::FixedSize{size:lots};
        let r=backtest(&s,&[flat(0,1.0),flat(60000,1.0)],&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(sqx_backtest::validate_account_evidence(&r).is_ok());
        let mutations:Vec<Box<dyn Fn(&mut sqx_backtest::MarginCheck)>>=vec![
            Box::new(|m|m.balance+=1.0),
            Box::new(|m|m.equity_after_entry_costs+=1.0),
            Box::new(|m|m.free_margin_after_entry+=1.0),
            Box::new(|m|m.commission=-1.0),
            Box::new(|m|m.quote_to_account_rate=f64::NAN),
            Box::new(|m|m.max_leverage=0.0),
            Box::new(|m|m.required_margin=if m.accepted{m.equity_after_entry_costs+1.0}else{0.0}),
        ];
        for (index,mutate) in mutations.into_iter().enumerate(){
            let mut altered=r.clone();let m=altered.execution.entry_attempts[0].margin_check.as_mut().unwrap();
            mutate(m);
            // Keep the derived free-margin number consistent in the last case:
            // the admission decision itself must still be rejected.
            if index==6 {m.free_margin_after_entry=m.equity_after_entry_costs-m.required_margin;}
            assert!(sqx_backtest::validate_account_evidence(&altered).is_err(),"lots {lots}, mutation {index}");
        }
    }
}

#[test]
fn account_evidence_reconciles_scheduled_exits_and_terminal_requests(){
    let mut s=strategy();s.exits=vec![sqx_strategy::ExitType::ExitAfterBars(1)];
    let bars=[flat(0,1.0),flat(60000,1.0),flat(120000,1.01)];
    for count in [2,3] {
        let r=backtest(&s,&bars[..count],&Instrument::fx("EURUSD"),&BacktestConfig::default());
        assert!(sqx_backtest::validate_account_evidence(&r).is_ok());
        let mut duplicate=r.clone();duplicate.execution.scheduled_exit_decisions.push(duplicate.execution.scheduled_exit_decisions[0].clone());
        assert!(sqx_backtest::validate_account_evidence(&duplicate).is_err());
        let mut timestamp=r.clone();timestamp.execution.scheduled_exit_decisions[0].decision_bar_open_time+=1;
        assert!(sqx_backtest::validate_account_evidence(&timestamp).is_err());
        let mut reason=r.clone();reason.execution.scheduled_exit_decisions[0].chosen_reason=sqx_backtest::ExitReason::ProfitTarget;
        assert!(sqx_backtest::validate_account_evidence(&reason).is_err());
        let mut execution=r.clone();execution.execution.scheduled_exit_decisions[0].executed_time=Some(60001);
        assert!(sqx_backtest::validate_account_evidence(&execution).is_err());
        if count==3 {
            let mut missing=r.clone();missing.execution.scheduled_exit_decisions.clear();
            assert!(sqx_backtest::validate_account_evidence(&missing).is_err());
        }
    }
}

#[test]
fn account_evidence_reconciles_stopout_decisions(){
    let r=backtest(&strategy(),&[flat(0,1.0),flat(60000,1.0),flat(120000,0.8)],&Instrument::fx("EURUSD"),&BacktestConfig::default());
    assert!(sqx_backtest::validate_account_evidence(&r).is_ok());
    assert_eq!(r.execution.stop_out_decisions.len(),1);
    let mutations:Vec<Box<dyn Fn(&mut sqx_backtest::BacktestResult)>>=vec![
        Box::new(|r|r.execution.stop_out_decisions.clear()),
        Box::new(|r|r.execution.stop_out_decisions.push(r.execution.stop_out_decisions[0].clone())),
        Box::new(|r|r.execution.stop_out_decisions[0].time+=1),
        Box::new(|r|r.execution.stop_out_decisions[0].fill_price+=0.01),
        Box::new(|r|r.execution.stop_out_decisions[0].balance+=1.0),
        Box::new(|r|r.execution.stop_out_decisions[0].minimum_equity+=1.0),
        Box::new(|r|r.execution.stop_out_decisions[0].quote_to_account_rate=f64::NAN),
        Box::new(|r|{let d=&mut r.execution.stop_out_decisions[0];d.equity_before_slippage=d.minimum_equity+100.0;d.unrealized_profit_account=d.equity_before_slippage-d.balance+d.commission_reserve+d.financing_accrued;}),
    ];
    for (i,mutate) in mutations.into_iter().enumerate(){let mut altered=r.clone();mutate(&mut altered);assert!(sqx_backtest::validate_account_evidence(&altered).is_err(),"stop-out mutation {i}");}
}

#[test]
fn account_evidence_rejects_broken_stop_amendment_history(){
    let mut s=strategy();s.stop_loss=Some(StopLoss::Points(100.0));s.exits=vec![sqx_strategy::ExitType::TrailingStop];
    let bars=[flat(0,1.0),Bar::new(60000,1.0,1.02,0.98,1.01,1.0)];
    let cfg=BacktestConfig{intrabar_policy:sqx_backtest::IntrabarPolicy::HighFirst,..Default::default()};
    let r=backtest(&s,&bars,&Instrument::fx("EURUSD"),&cfg);
    assert!(sqx_backtest::validate_account_evidence(&r).is_ok());assert_eq!(r.execution.stop_changes.len(),1);
    let mutations:Vec<Box<dyn Fn(&mut sqx_backtest::StopChange)>>=vec![
        Box::new(|e|e.previous_stop=Some(0.5)),
        Box::new(|e|e.new_stop=Some(0.5)),
        Box::new(|e|e.time=0),
        Box::new(|e|e.observed_price=f64::NAN),
        Box::new(|e|e.new_reason=sqx_backtest::ExitReason::MoveToBreakEven),
        Box::new(|e|e.source="unknown"),
        Box::new(|e|e.path_step=Some(4)),
    ];
    for (i,mutate) in mutations.into_iter().enumerate(){let mut altered=r.clone();mutate(&mut altered.execution.stop_changes[0]);assert!(sqx_backtest::validate_account_evidence(&altered).is_err(),"stop amendment {i}");}
}
