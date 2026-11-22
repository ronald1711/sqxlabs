//! # sqx-metrics
//!
//! Performance statistics computed from a backtest result. Values are f64
//! with NAN for "undefined" (e.g. profit factor when there is no loss).
//! The field set mirrors the StrategyQuant databank column catalog.

use sqx_backtest::BacktestResult;

/// The computed performance metrics of a strategy.
#[derive(Debug, Clone, Copy, Default)]
pub struct Metrics {
    pub net_profit: f64,
    pub net_profit_pct: f64,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub profit_factor: f64,
    pub max_drawdown: f64,
    pub max_drawdown_pct: f64,
    pub return_dd_ratio: f64,
    pub number_of_trades: f64,
    pub number_of_profits: f64,
    pub number_of_losses: f64,
    pub winning_pct: f64,
    pub avg_trade: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub max_profit: f64,
    pub max_loss: f64,
    pub payout_ratio: f64,
    pub win_loss_ratio: f64,
    pub max_consec_wins: f64,
    pub max_consec_losses: f64,
    pub sharpe_ratio: f64,
    pub sqn: f64,
    pub cagr: f64,
    pub stability: f64,
    pub rsquared: f64,
    pub expectancy: f64,
    pub r_expectancy: f64,
    pub stddev: f64,
    pub exposure_pct: f64,
    pub stagnation_bars: f64,
    pub recovery_factor: f64,
    pub calmar_ratio: f64,
}

/// Compute metrics over a backtest result.
///
/// bar_count is the total number of bars in the dataset and periods_per_year
/// the number of bars per calendar year (used to annualize Sharpe and CAGR,
/// and to measure stagnation in bars).
pub fn compute(result: &BacktestResult, bar_count: usize, periods_per_year: f64) -> Metrics {
    let mut m = Metrics::default();
    let trades = &result.trades;
    let n = trades.len();

    let mut gross_profit = 0.0;
    let mut gross_loss = 0.0;
    let mut wins = 0.0;
    let mut losses = 0.0;
    let mut sum_win = 0.0;
    let mut sum_loss = 0.0;
    let mut max_profit: f64 = f64::NEG_INFINITY;
    let mut max_loss: f64 = f64::INFINITY;
    let mut consec_wins: f64 = 0.0;
    let mut consec_losses: f64 = 0.0;
    let mut max_consec_wins: f64 = 0.0;
    let mut max_consec_losses: f64 = 0.0;
    let mut total_bars_held = 0usize;
    let mut pnls: Vec<f64> = Vec::with_capacity(n);

    for t in trades {
        pnls.push(t.pnl);
        total_bars_held += t.bars;
        if t.pnl >= 0.0 {
            gross_profit += t.pnl;
            wins += 1.0;
            sum_win += t.pnl;
            max_profit = max_profit.max(t.pnl);
            consec_wins += 1.0;
            consec_losses = 0.0;
            max_consec_wins = max_consec_wins.max(consec_wins);
        } else {
            gross_loss += t.pnl;
            losses += 1.0;
            sum_loss += t.pnl;
            max_loss = max_loss.min(t.pnl);
            consec_losses += 1.0;
            consec_wins = 0.0;
            max_consec_losses = max_consec_losses.max(consec_losses);
        }
    }

    let net = result.net_profit();
    m.net_profit = net;
    m.net_profit_pct = if result.initial_deposit != 0.0 {
        net / result.initial_deposit * 100.0
    } else {
        f64::NAN
    };
    m.gross_profit = gross_profit;
    m.gross_loss = gross_loss;
    m.profit_factor = if gross_loss != 0.0 {
        gross_profit / -gross_loss
    } else if gross_profit > 0.0 {
        f64::INFINITY
    } else {
        f64::NAN
    };
    m.number_of_trades = n as f64;
    m.number_of_profits = wins;
    m.number_of_losses = losses;
    m.winning_pct = if n > 0 { wins / n as f64 * 100.0 } else { f64::NAN };
    m.avg_trade = if n > 0 { net / n as f64 } else { f64::NAN };
    m.avg_win = if wins > 0.0 { sum_win / wins } else { f64::NAN };
    m.avg_loss = if losses > 0.0 { sum_loss / losses } else { f64::NAN };
    m.max_profit = if wins > 0.0 { max_profit } else { f64::NAN };
    m.max_loss = if losses > 0.0 { max_loss } else { f64::NAN };
    m.payout_ratio = if losses > 0.0 && wins > 0.0 {
        m.avg_win / -m.avg_loss
    } else {
        f64::NAN
    };
    m.win_loss_ratio = if losses > 0.0 { wins / losses } else { f64::NAN };
    m.max_consec_wins = max_consec_wins;
    m.max_consec_losses = max_consec_losses;
    m.stddev = stddev(&pnls);
    m.expectancy = m.avg_trade;

    // R-expectancy: pnl expressed in units of the average losing trade.
    let risk_unit = if losses > 0.0 { -m.avg_loss } else { f64::NAN };
    m.r_expectancy = if risk_unit.is_finite() && risk_unit > 0.0 && n > 0 {
        net / n as f64 / risk_unit
    } else {
        f64::NAN
    };

    // SQN: sqrt(N) * mean(R) / stddev(R), R = pnl / |avg loss|.
    m.sqn = if risk_unit.is_finite() && risk_unit > 0.0 && n > 1 {
        let rs: Vec<f64> = pnls.iter().map(|p| p / risk_unit).collect();
        let sd = stddev(&rs);
        if sd > 0.0 {
            (n as f64).sqrt() * mean(&rs) / sd
        } else {
            f64::NAN
        }
    } else {
        f64::NAN
    };

    let (max_dd, max_dd_pct) = max_drawdown(&result.equity, result.initial_deposit);
    m.max_drawdown = max_dd;
    m.max_drawdown_pct = max_dd_pct;
    m.return_dd_ratio = if max_dd > 0.0 {
        net / max_dd
    } else if net > 0.0 {
        f64::INFINITY
    } else {
        f64::NAN
    };
    m.recovery_factor = if max_dd > 0.0 { net / max_dd } else { f64::NAN };

    let (r2, _slope) = linear_fit_r2(&result.equity);
    m.rsquared = r2;
    m.stability = r2;

    m.sharpe_ratio = sharpe(&result.equity, periods_per_year);
    m.cagr = cagr(result.initial_deposit, result.final_equity, bar_count as f64, periods_per_year);
    m.calmar_ratio = if m.max_drawdown_pct.is_finite() && m.max_drawdown_pct > 0.0 && m.cagr.is_finite() {
        m.cagr / (m.max_drawdown_pct / 100.0)
    } else {
        f64::NAN
    };

    m.exposure_pct = if bar_count > 0 {
        total_bars_held as f64 / bar_count as f64 * 100.0
    } else {
        f64::NAN
    };

    m.stagnation_bars = stagnation(&result.equity);

    m
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn stddev(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return f64::NAN;
    }
    let m = mean(xs);
    (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / xs.len() as f64).sqrt()
}

fn max_drawdown(equity: &[(i64, f64)], initial: f64) -> (f64, f64) {
    let mut peak = initial;
    let mut max_dd = 0.0;
    let mut max_dd_pct = 0.0;
    for (_, eq) in equity {
        if *eq > peak {
            peak = *eq;
        }
        let dd = peak - *eq;
        if dd > max_dd {
            max_dd = dd;
        }
        if peak > 0.0 {
            let dd_pct = dd / peak * 100.0;
            if dd_pct > max_dd_pct {
                max_dd_pct = dd_pct;
            }
        }
    }
    (max_dd, max_dd_pct)
}

fn linear_fit_r2(equity: &[(i64, f64)]) -> (f64, f64) {
    let n = equity.len();
    if n < 2 {
        return (f64::NAN, f64::NAN);
    }
    let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let ys: Vec<f64> = equity.iter().map(|(_, e)| *e).collect();
    let xm = mean(&xs);
    let ym = mean(&ys);
    let mut sxy = 0.0;
    let mut sxx = 0.0;
    for i in 0..n {
        sxy += (xs[i] - xm) * (ys[i] - ym);
        sxx += (xs[i] - xm).powi(2);
    }
    let slope = if sxx == 0.0 { 0.0 } else { sxy / sxx };
    let intercept = ym - slope * xm;
    let mut ss_res = 0.0;
    let mut ss_tot = 0.0;
    for i in 0..n {
        let fit = slope * xs[i] + intercept;
        ss_res += (ys[i] - fit).powi(2);
        ss_tot += (ys[i] - ym).powi(2);
    }
    let r2 = if ss_tot == 0.0 { 1.0 } else { 1.0 - ss_res / ss_tot };
    (r2, slope)
}

fn sharpe(equity: &[(i64, f64)], periods_per_year: f64) -> f64 {
    if equity.len() < 2 {
        return f64::NAN;
    }
    let mut rets = Vec::with_capacity(equity.len() - 1);
    for wd in equity.windows(2) {
        let prev = wd[0].1;
        if prev > 0.0 {
            rets.push((wd[1].1 - prev) / prev);
        }
    }
    let sd = stddev(&rets);
    if sd == 0.0 || sd.is_nan() {
        return f64::NAN;
    }
    mean(&rets) / sd * periods_per_year.sqrt()
}

fn cagr(initial: f64, final_equity: f64, bar_count: f64, periods_per_year: f64) -> f64 {
    if initial <= 0.0 || final_equity <= 0.0 || bar_count <= 0.0 || periods_per_year <= 0.0 {
        return f64::NAN;
    }
    let years = bar_count / periods_per_year;
    if years <= 0.0 {
        return f64::NAN;
    }
    (final_equity / initial).powf(1.0 / years) - 1.0
}

fn stagnation(equity: &[(i64, f64)]) -> f64 {
    let mut peak = f64::NEG_INFINITY;
    let mut cur: f64 = 0.0;
    let mut longest: f64 = 0.0;
    for (_, eq) in equity {
        if *eq >= peak {
            peak = *eq;
            cur = 0.0;
        } else {
            cur += 1.0;
            longest = longest.max(cur);
        }
    }
    longest
}

/// Render metrics as an aligned "name = value" block, with NAN shown as "-".
pub fn render(m: &Metrics) -> String {
    let rows: Vec<(&str, f64, usize)> = vec![
        ("Net profit", m.net_profit, 2),
        ("Net profit %", m.net_profit_pct, 2),
        ("Gross profit", m.gross_profit, 2),
        ("Gross loss", m.gross_loss, 2),
        ("Profit factor", m.profit_factor, 2),
        ("Max drawdown", m.max_drawdown, 2),
        ("Max drawdown %", m.max_drawdown_pct, 2),
        ("Return/DD", m.return_dd_ratio, 2),
        ("Number of trades", m.number_of_trades, 0),
        ("Winning %", m.winning_pct, 2),
        ("Avg trade", m.avg_trade, 2),
        ("Avg win", m.avg_win, 2),
        ("Avg loss", m.avg_loss, 2),
        ("Max profit", m.max_profit, 2),
        ("Max loss", m.max_loss, 2),
        ("Payout ratio", m.payout_ratio, 2),
        ("Win/Loss ratio", m.win_loss_ratio, 2),
        ("Max consec. wins", m.max_consec_wins, 0),
        ("Max consec. losses", m.max_consec_losses, 0),
        ("Sharpe ratio", m.sharpe_ratio, 3),
        ("SQN", m.sqn, 2),
        ("CAGR", m.cagr, 4),
        ("Stability (R2)", m.stability, 4),
        ("Expectancy", m.expectancy, 2),
        ("R Expectancy", m.r_expectancy, 2),
        ("StdDev", m.stddev, 2),
        ("Exposure %", m.exposure_pct, 2),
        ("Stagnation (bars)", m.stagnation_bars, 0),
        ("Recovery factor", m.recovery_factor, 2),
        ("Calmar ratio", m.calmar_ratio, 2),
    ];
    let mut out = String::new();
    for (name, val, decimals) in rows {
        let v = if val.is_nan() {
            "-".to_string()
        } else if val == f64::INFINITY {
            "inf".to_string()
        } else {
            format!("{:.*}", decimals, val)
        };
        out.push_str(&format!("{:<24} {}", name, v));
        out.push('\n');
    }
    out
}
