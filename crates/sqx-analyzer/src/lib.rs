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

fn monthly(equity: &[(i64, f64)]) -> Vec<MonthRow> {
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
    let mut prev = equity.first().map(|(_, e)| *e).unwrap_or(0.0);
    for (y, m, e) in ends {
        let ret = if prev != 0.0 && prev.is_finite() { (e - prev) / prev * 100.0 } else { 0.0 };
        out.push(MonthRow { year: y, month: m, end_equity: e, return_pct: ret });
        prev = e;
    }
    out
}

fn drawdowns(equity: &[(i64, f64)]) -> Vec<DrawdownPeriod> {
    let mut out = Vec::new();
    if equity.is_empty() {
        return out;
    }
    let mut peak = equity[0].1;
    let mut peak_i = 0usize;
    let mut in_dd = false;
    let mut trough = peak;
    let mut trough_i = 0usize;
    for i in 1..equity.len() {
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
    let losses: Vec<f64> = trades.iter().filter(|t| t.pnl <= 0.0).map(|t| t.pnl).collect();
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
        } else {
            cur_loss += 1;
            cur_win = 0;
            max_loss_streak = max_loss_streak.max(cur_loss);
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

fn rolling_sharpe(equity: &[(i64, f64)], periods_per_year: f64) -> Vec<f64> {
    if equity.len() < 3 {
        return Vec::new();
    }
    let vals: Vec<f64> = equity.iter().map(|(_, e)| *e).collect();
    let rets: Vec<f64> = vals.windows(2).map(|w| if w[0] != 0.0 { (w[1] - w[0]) / w[0] } else { 0.0 }).collect();
    let window = (periods_per_year.max(2.0) / 4.0).round() as usize;
    let window = window.max(2).min(rets.len());
    let mut out = Vec::new();
    for w in rets.windows(window) {
        let m = w.iter().sum::<f64>() / w.len() as f64;
        let var = w.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (w.len() as f64 - 1.0);
        let sd = var.sqrt();
        out.push(if sd > 0.0 { m / sd * periods_per_year.sqrt() } else { 0.0 });
    }
    out
}

pub fn analyze(result: &BacktestResult, bars: &[Bar], periods_per_year: f64) -> AnalysisReport {
    AnalysisReport {
        summary: compute(result, bars.len(), periods_per_year),
        monthly: monthly(&result.equity),
        drawdowns: drawdowns(&result.equity),
        trade_stats: trade_stats(&result.trades),
        rolling_sharpe: rolling_sharpe(&result.equity, periods_per_year),
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
        let min = r.rolling_sharpe.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = r.rolling_sharpe.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let avg = r.rolling_sharpe.iter().sum::<f64>() / r.rolling_sharpe.len() as f64;
        out.push_str("## Rolling Sharpe\n\n");
        out.push_str(&format!("- windows: {}\n- min: {:.2}\n- mean: {:.2}\n- max: {:.2}\n", r.rolling_sharpe.len(), min, avg, max));
    }
    out
}
