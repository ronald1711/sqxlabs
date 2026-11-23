//! QuantAnalyzer-style post-analysis report: monthly returns, drawdown periods,
//! trade statistics and rolling Sharpe.

use sqx_backtest::{BacktestResult, Trade};
use sqx_core::Bar;
use sqx_metrics::{compute, Metrics};

#[derive(Debug, Clone)]
pub struct MonthRow {
    pub year: i32,
    pub month: u32,
    pub end_equity: f64,
    pub return_pct: f64,
}

#[derive(Debug, Clone)]
pub struct DrawdownPeriod {
    pub start: i64,
    pub trough: i64,
    pub end: Option<i64>,
    pub depth_pct: f64,
    pub recovery_bars: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct TradeStats {
    pub trades: usize,
    pub wins: usize,
    pub losses: usize,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub payoff: f64,
    pub max_win_streak: usize,
    pub max_loss_streak: usize,
    pub avg_mae: f64,
    pub avg_mfe: f64,
}

#[derive(Debug, Clone)]
pub struct AnalysisReport {
    pub errors: Vec<String>,
    pub summary: Metrics,
    pub monthly: Vec<MonthRow>,
    pub drawdowns: Vec<DrawdownPeriod>,
    pub trade_stats: TradeStats,
    pub rolling_sharpe: Vec<f64>,
    pub bars: usize,
    pub periods_per_year: f64,
}

fn ymd(ms: i64) -> (i32, u32, u32) {
    let days = ms.div_euclid(86_400_000) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

fn monthly(equity: &[(i64, f64)], initial:f64) -> Vec<MonthRow> {
    let mut ends: Vec<(i32, u32, f64)> = Vec::new();
    let mut cur: Option<(i32, u32, f64)> = None;
    for (t, e) in equity {
        let (y, m, _) = ymd(*t);
        let eq = *e;
        match cur {
            None => cur = Some((y, m, eq)),
            Some((cy, cm, ce)) => {
                if (y, m) != (cy, cm) {
                    ends.push((cy, cm, ce));
                    cur = Some((y, m, eq));
                } else {
                    cur = Some((cy, cm, eq));
                }
            }
        }
    }
    if let Some((y, m, e)) = cur {
        ends.push((y, m, e));
    }
    let mut out = Vec::new();
    let mut prev = initial;
    for (y, m, e) in ends {
        let ret = if prev > 0.0 && prev.is_finite() && e.is_finite() { (e - prev) / prev * 100.0 } else { f64::NAN };
        out.push(MonthRow { year: y, month: m, end_equity: e, return_pct: ret });
        prev = e;
    }
    out
}

fn drawdowns(equity: &[(i64, f64)], initial:f64) -> Vec<DrawdownPeriod> {
    let mut out = Vec::new();
    if equity.is_empty() {
        return out;
    }
    let mut peak = initial;
    let mut peak_i = 0usize;
    let mut in_dd = false;
    let mut trough = peak;
    let mut trough_i = 0usize;
    for i in 0..equity.len() {
        let e = equity[i].1;
        if e >= peak {
            if in_dd {
                out.push(DrawdownPeriod {
                    start: equity[peak_i].0,
                    trough: equity[trough_i].0,
                    end: Some(equity[i].0),
                    depth_pct: if peak != 0.0 { (peak - trough) / peak * 100.0 } else { 0.0 },
                    recovery_bars: Some(i - trough_i),
                });
                in_dd = false;
            }
            peak = e;
            peak_i = i;
            trough = e;
            trough_i = i;
        } else {
            in_dd = true;
            if e < trough {
                trough = e;
                trough_i = i;
            }
        }
    }
    if in_dd {
        out.push(DrawdownPeriod {
            start: equity[peak_i].0,
            trough: equity[trough_i].0,
            end: None,
            depth_pct: if peak != 0.0 { (peak - trough) / peak * 100.0 } else { 0.0 },
            recovery_bars: None,
        });
    }
    out
}

fn mean(xs: impl Iterator<Item = f64>) -> f64 {
    let v: Vec<f64> = xs.filter(|x| x.is_finite()).collect();
    if v.is_empty() { f64::NAN } else { v.iter().sum::<f64>() / v.len() as f64 }
}

fn trade_stats(trades: &[Trade]) -> TradeStats {
    let wins: Vec<f64> = trades.iter().filter(|t| t.pnl > 0.0).map(|t| t.pnl).collect();
    let losses: Vec<f64> = trades.iter().filter(|t| t.pnl < 0.0).map(|t| t.pnl).collect();
    let avg_win = mean(wins.iter().copied());
    let avg_loss = mean(losses.iter().copied());
    let payoff = if avg_loss.is_finite() && avg_loss != 0.0 { avg_win / -avg_loss } else { f64::NAN };
    let mut max_win_streak = 0;
    let mut max_loss_streak = 0;
    let mut cur_win = 0;
    let mut cur_loss = 0;
    for t in trades {
        if t.pnl > 0.0 {
            cur_win += 1;
            cur_loss = 0;
            max_win_streak = max_win_streak.max(cur_win);
        } else if t.pnl < 0.0 {
            cur_loss += 1;
            cur_win = 0;
            max_loss_streak = max_loss_streak.max(cur_loss);
        } else {
            cur_win=0;cur_loss=0;
        }
    }
    TradeStats {
        trades: trades.len(),
        wins: wins.len(),
        losses: losses.len(),
        avg_win,
        avg_loss,
        payoff,
        max_win_streak,
        max_loss_streak,
        avg_mae: mean(trades.iter().map(|t| t.mae_points)),
        avg_mfe: mean(trades.iter().map(|t| t.mfe_points)),
    }
}

fn rolling_sharpe(equity: &[(i64, f64)], initial:f64, periods_per_year: f64) -> Vec<f64> {
    if !periods_per_year.is_finite() || periods_per_year<=0.0 {return Vec::new();}
    let window=((periods_per_year/4.0).round() as usize).max(2);
    // Never label a shortened history as a complete quarterly window.
    if equity.len()<window {return Vec::new();}
    equity.windows(window).enumerate().map(|(start,observations)| {
        let preceding=if start==0 {initial}else{equity[start-1].1};
        sqx_metrics::sharpe_from_equity(observations,preceding,periods_per_year)
    }).collect()
}

pub fn analyze(result: &BacktestResult, bars: &[Bar], periods_per_year: f64) -> AnalysisReport {
    let mut errors=result.errors.clone();
    if let Err(error)=sqx_metrics::validate_observations(result){errors.push(error);}
    if !periods_per_year.is_finite() || periods_per_year<=0.0 {errors.push("Annualization must be finite and positive".into());}
    if !result.initial_deposit.is_finite() || result.initial_deposit<=0.0 || !result.final_equity.is_finite() {
        errors.push("Invalid initial or final equity".into());
    }
    if result.equity.iter().any(|(_,e)|!e.is_finite()) || result.equity.windows(2).any(|w|w[1].0<=w[0].0) {
        errors.push("Equity observations must be finite and strictly chronological".into());
    }
    if !errors.is_empty(){
        let mut invalid=result.clone();invalid.errors=errors.clone();
        return AnalysisReport{errors,summary:compute(&invalid,bars.len(),periods_per_year),monthly:Vec::new(),drawdowns:Vec::new(),trade_stats:trade_stats(&[]),rolling_sharpe:Vec::new(),bars:bars.len(),periods_per_year};
    }
    AnalysisReport {
        errors,
        summary: compute(result, bars.len(), periods_per_year),
        monthly: monthly(&result.equity,result.initial_deposit),
        drawdowns: drawdowns(&result.equity,result.initial_deposit),
        trade_stats: trade_stats(&result.trades),
        rolling_sharpe: rolling_sharpe(&result.equity, result.initial_deposit, periods_per_year),
        bars: bars.len(),
        periods_per_year,
    }
}

fn date(ms: i64) -> String {
    let (y, m, d) = ymd(ms);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ymd_converts_epoch() {
        assert_eq!(ymd(0), (1970, 1, 1));
        assert_eq!(ymd(1_700_000_000_000), (2023, 11, 14));
    }
}

pub fn render_markdown(r: &AnalysisReport) -> String {
    if !r.errors.is_empty(){return format!("# Analysis unavailable\n\n{}\n",r.errors.iter().map(|e|format!("- {e}")).collect::<Vec<_>>().join("\n"));}
    let mut out = String::new();
    out.push_str("# Strategy analysis report\n\n");
    out.push_str(&format!("Bars: {} | Periods/year: {:.0}\n\n", r.bars, r.periods_per_year));
    out.push_str("## Summary\n\n");
    out.push_str(&sqx_metrics::render(&r.summary));
    out.push('\n');
    out.push_str("## Monthly returns\n\n| Month | Equity | Return % |\n|---|---:|---:|\n");
    for m in &r.monthly {
        out.push_str(&format!("| {:04}-{:02} | {:.2} | {:+.2} |\n", m.year, m.month, m.end_equity, m.return_pct));
    }
    out.push('\n');
    out.push_str("## Drawdown periods\n\n| Start | Trough | Recovery | Depth % | Bars |\n|---|---|---|---:|---:|\n");
    for d in &r.drawdowns {
        out.push_str(&format!("| {} | {} | {} | {:.2} | {} |\n", date(d.start), date(d.trough), d.end.map(date).unwrap_or_else(|| "open".to_string()), d.depth_pct, d.recovery_bars.map(|x| x.to_string()).unwrap_or_else(|| "-".to_string())));
    }
    out.push('\n');
    let t = &r.trade_stats;
    out.push_str("## Trade statistics\n\n");
    out.push_str(&format!("- Trades: {} ({} wins / {} losses)\n", t.trades, t.wins, t.losses));
    out.push_str(&format!("- Average win: {:.2}\n- Average loss: {:.2}\n", t.avg_win, t.avg_loss));
    out.push_str(&format!("- Payoff ratio: {:.2}\n", t.payoff));
    out.push_str(&format!("- Max win streak: {}\n- Max loss streak: {}\n", t.max_win_streak, t.max_loss_streak));
    out.push_str(&format!("- Average MAE: {:.1} pts\n- Average MFE: {:.1} pts\n", t.avg_mae, t.avg_mfe));
    out.push('\n');
    if !r.rolling_sharpe.is_empty() {
        let valid:Vec<f64>=r.rolling_sharpe.iter().copied().filter(|v|v.is_finite()).collect();
        out.push_str("## Rolling Sharpe\n\n");
        out.push_str(&format!("- Complete quarterly windows: {}\n- Undefined windows: {}\n",r.rolling_sharpe.len(),r.rolling_sharpe.len()-valid.len()));
        if valid.is_empty(){out.push_str("- No defined Sharpe values\n");}
        else {
            let min=valid.iter().copied().fold(f64::INFINITY,f64::min);
            let max=valid.iter().copied().fold(f64::NEG_INFINITY,f64::max);
            let avg=valid.iter().sum::<f64>()/valid.len() as f64;
            out.push_str(&format!("- Defined windows only — min: {min:.2}; mean: {avg:.2}; max: {max:.2}\n"));
        }
    }
    out
}

#[cfg(test)]
mod flat_trade_tests {
    use super::*;
    #[test]
    fn break_even_trades_match_core_statistics() {
        let trades:Vec<Trade>=[10.0,0.0,20.0,-5.0,-0.0,-15.0].into_iter().enumerate().map(|(id,pnl)|Trade{
            id,direction:sqx_core::Direction::Long,open_time:id as i64,close_time:id as i64+1,
            open_price:1.0,close_price:1.0,size:1.0,pnl,pnl_points:pnl,commission:0.0,financing:0.0,
            bars:1,mae_points:0.0,mfe_points:0.0,exit_reason:sqx_backtest::ExitReason::ExitSignal,
        }).collect();
        let stats=trade_stats(&trades);
        assert_eq!((stats.trades,stats.wins,stats.losses),(6,2,2));
        assert_eq!(stats.avg_win,15.0);assert_eq!(stats.avg_loss,-10.0);
        assert_eq!(stats.max_win_streak,1);assert_eq!(stats.max_loss_streak,1);
    }
}

#[cfg(test)]
mod rolling_sharpe_tests {
    use super::*;
    #[test]
    fn full_windows_share_core_definition_without_fabricated_zeros(){
        let equity=[(0,110.0),(1,99.0),(2,118.8),(3,118.8)];
        let values=rolling_sharpe(&equity,100.0,12.0);
        assert_eq!(values.len(),2);
        let expected=(1.0/15.0)/(7.0_f64/300.0).sqrt()*12.0_f64.sqrt();
        assert!((values[0]-expected).abs()<1e-12);
        assert_eq!(values[1],sqx_metrics::sharpe_from_equity(&equity[1..],110.0,12.0));
        assert!(rolling_sharpe(&equity[..2],100.0,12.0).is_empty());
        let flat=rolling_sharpe(&[(0,100.0),(1,100.0),(2,100.0)],100.0,12.0);
        assert_eq!(flat.len(),1);assert!(flat[0].is_nan());
        let insolvent=rolling_sharpe(&[(0,110.0),(1,0.0),(2,10.0)],100.0,12.0);
        assert!(insolvent[0].is_nan());
    }
}

#[cfg(test)]
mod initial_equity_tests {
    use super::*;
    #[test]
    fn monthly_returns_include_first_bar_and_compound_from_prior_month_end(){
        let jan=0;let feb=31*86_400_000;
        let rows=monthly(&[(jan,80.0),(feb,88.0)],100.0);
        assert_eq!(rows.len(),2);assert_eq!(rows[0].return_pct,-20.0);
        assert!((rows[1].return_pct-10.0).abs()<1e-12);
        assert!(((1.0+rows[0].return_pct/100.0)*(1.0+rows[1].return_pct/100.0)-0.88).abs()<1e-12);
        let exhausted=monthly(&[(jan,0.0),(feb,10.0)],100.0);
        assert_eq!(exhausted[0].return_pct,-100.0);assert!(exhausted[1].return_pct.is_nan());
    }
    #[test]
    fn first_bar_loss_is_a_drawdown_even_with_only_one_observation(){
        let periods=drawdowns(&[(0,80.0)],100.0);
        assert_eq!(periods.len(),1);assert_eq!(periods[0].depth_pct,20.0);
        assert_eq!(periods[0].end,None);
        let recovered=drawdowns(&[(0,80.0),(1,70.0),(2,100.0)],100.0);
        assert_eq!(recovered.len(),1);assert_eq!(recovered[0].depth_pct,30.0);
        assert_eq!(recovered[0].trough,1);assert_eq!(recovered[0].end,Some(2));
        assert_eq!(recovered[0].recovery_bars,Some(1));
        assert!(drawdowns(&[(0,110.0)],100.0).is_empty());
    }
}

#[cfg(test)]
mod invalid_analysis_tests {
    use super::*;
    fn result()->BacktestResult {BacktestResult{errors:vec![],rejected_orders:vec![],ambiguous_bars:vec![],execution:Default::default(),trades:vec![],equity:vec![(0,100.0),(1,90.0)],initial_deposit:100.0,final_equity:90.0}}
    #[test]
    fn invalid_runs_cannot_render_successful_analysis(){
        for case in 0..4 {
            let mut run=result();let mut annualization=252.0;
            match case {0=>run.errors.push("execution failed".into()),1=>annualization=0.0,2=>run.equity[1].1=f64::NAN,_=>run.equity[1].0=0}
            let report=analyze(&run,&[],annualization);
            assert!(!report.errors.is_empty());assert!(report.summary.net_profit.is_nan());
            assert!(report.monthly.is_empty());assert!(report.drawdowns.is_empty());assert!(report.rolling_sharpe.is_empty());
            let text=render_markdown(&report);assert!(text.contains("Analysis unavailable"));assert!(!text.contains("## Summary"));
        }
        assert!(analyze(&result(),&[],252.0).errors.is_empty());
    }
}
