//! Hand-specified EURUSD/EUR signal/fill ledger, independent of engine helpers.
//! Synthetic OHLC scenarios only: no broker/platform execution certification.
use sqx_backtest::{backtest, AccountConfig, BacktestConfig};
use sqx_core::{Bar, Instrument};
use sqx_strategy::{Comparator, Condition, Join, MoneyManagement, Operand, PriceField, Signal, SignalKind, Strategy};

#[test]
fn eur_crossing_signals_fill_next_open_and_reconcile_signed_cash() {
    // 0.1 lot = 10,000 EUR, spread 0.0002, commission EUR 6/lot round trip.
    // Expected gross USD amounts are hand-calculated from the stated fills.
    for (long, exit_bid, expected_open, expected_close, gross_usd) in [
        (true, 1.0980, 1.1022, 1.0980, -42.0),
        (true, 1.1050, 1.1022, 1.1050, 28.0),
        (false, 1.1020, 1.0980, 1.1022, -42.0),
        (false, 1.0950, 1.0980, 1.0952, 28.0),
    ] {
        let mut strategy = Strategy::new("EUR next-open reference");
        strategy.long = long;
        strategy.short = !long;
        strategy.money_management = MoneyManagement::FixedSize { size: 0.1 };
        for (kind, comparator) in if long {
            [(SignalKind::LongEntry, Comparator::CrossAbove), (SignalKind::LongExit, Comparator::CrossBelow)]
        } else {
            [(SignalKind::ShortEntry, Comparator::CrossBelow), (SignalKind::ShortExit, Comparator::CrossAbove)]
        } {
            strategy.signals.push(Signal::new(kind, vec![Condition::new(Operand::Price(PriceField::Close, 0), comparator, Operand::Const(1.1))], Join::And));
        }
        let prices = if long { [(1.099, 1.099), (1.1, 1.101), (1.102, 1.099)] }
                     else { [(1.101, 1.101), (1.1, 1.099), (1.098, 1.101)] };
        let mut bars: Vec<_> = prices.into_iter().enumerate().map(|(i, (o, c)): (usize, (f64, f64))| Bar::new(i as i64 * 60000, o, o.max(c), o.min(c), c, 1.0)).collect();
        bars.push(Bar::new(180000, exit_bid, exit_bid, exit_bid, exit_bid, 1.0));
        let config = BacktestConfig { initial_deposit: 10000.0, spread_points: 2.0, commission_per_lot: 6.0,
            account: AccountConfig { currency: "EUR".into(), max_leverage: 30.0, ..Default::default() }, ..Default::default() };
        let result = backtest(&strategy, &bars, &Instrument::fx("EURUSD"), &config);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.trades.len(), 1);
        let trade = &result.trades[0];
        assert_eq!((trade.open_time, trade.close_time), (120000, 180000));
        assert!((trade.open_price - expected_open).abs() < 1e-12);
        assert!((trade.close_price - expected_close).abs() < 1e-12);
        let entry = result.execution.entry_attempts.iter().find(|attempt| attempt.trade_id == Some(0)).unwrap();
        assert_eq!(entry.signal.as_ref().unwrap().bar_index, 1);
        let exit = &result.execution.scheduled_exit_decisions[0];
        assert_eq!(exit.signal.as_ref().unwrap().bar_index, 2);
        assert_eq!(exit.executed_time, Some(180000));
        let settlement = &result.execution.cash_settlements[0];
        assert!((settlement.gross_profit_quote - gross_usd).abs() < 1e-8);
        assert!((settlement.commission - 0.6).abs() < 1e-12);
        let expected_net = gross_usd / expected_close - 0.6;
        assert!((trade.pnl - expected_net).abs() < 1e-8);
        assert!((result.final_equity - (10000.0 + expected_net)).abs() < 1e-8);
        assert_eq!(settlement.balance_after, result.final_equity);
        // Later prices cannot rewrite a completed trade or its signal evidence.
        bars.push(Bar::new(240000, 2.0, 2.0, 2.0, 2.0, 1.0));
        let later = backtest(&strategy, &bars, &Instrument::fx("EURUSD"), &config);
        assert!(later.errors.is_empty());
        assert_eq!(later.trades[0], *trade);
        assert_eq!(later.execution.scheduled_exit_decisions[0], *exit);
    }
}
