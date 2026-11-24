//! PineScript v5 exporter (TradingView).

use sqx_strategy::{
    Comparator, Condition, Join, MoneyManagement, Operand, Param, PriceField, Signal, SignalKind,
    StopLoss, ProfitTarget, Strategy,
};

const Q: char = '"';

fn num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v:.5}").trim_end_matches('0').trim_end_matches('.').to_string() }
}

fn prd(params: &[Param], name: &str, def: i64) -> i64 {
    params.iter().find(|p| p.name == name).map(|p| p.value as i64).filter(|&v| v > 0).unwrap_or(def)
}

fn indicator(name: &str, params: &[Param]) -> Option<String> {
    let res = match name {
        "SMA" => format!("ta.sma(close, {})", prd(params, "period", 20)),
        "EMA" => format!("ta.ema(close, {})", prd(params, "period", 20)),
        "SMMA" => format!("ta.rma(close, {})", prd(params, "period", 20)),
        "RSI" => format!("ta.rsi(close, {})", prd(params, "period", 14)),
        "CCI" => format!("ta.cci(high, low, close, {})", prd(params, "period", 14)),
        "ATR" => format!("ta.atr({})", prd(params, "period", 14)),
        "ADX" => format!("ta.dmi({}, {})[0]", prd(params, "period", 14), prd(params, "period", 14)),
        "DIPlus" => format!("ta.dmi({}, {})[1]", prd(params, "period", 14), prd(params, "period", 14)),
        "DIMinus" => format!("ta.dmi({}, {})[2]", prd(params, "period", 14), prd(params, "period", 14)),
        "Momentum" => format!("ta.mom(close, {})", prd(params, "period", 10)),
        "ROC" => format!("ta.roc(close, {})", prd(params, "period", 10)),
        "WilliamsR" => format!("ta.wr({})", prd(params, "period", 14)),
        "StochK" => format!("ta.stoch(close, high, low, {})", crate::stochastic_period(params, 14)),
        "StochD" => {
            // Registry contract: K always uses 14; period controls D smoothing.
            // Match registry rounding/defaults rather than truncating periods.
            let smoothing = crate::stochastic_period(params, 3);
            format!("ta.sma(ta.stoch(close, high, low, 14), {smoothing})")
        },
        "MACD" => format!("ta.macd(close, {}, {}, {})[0]", prd(params, "fast", 12), prd(params, "slow", 26), prd(params, "signal", 9)),
        "MACDSignal" => format!("ta.macd(close, {}, {}, {})[1]", prd(params, "fast", 12), prd(params, "slow", 26), prd(params, "signal", 9)),
        "BBUpper" => format!("ta.bb(close, {}, 2).upper", prd(params, "period", 20)),
        "BBMiddle" => format!("ta.bb(close, {}, 2).basis", prd(params, "period", 20)),
        "BBLower" => format!("ta.bb(close, {}, 2).lower", prd(params, "period", 20)),
        "KCUpper" => format!("ta.kc(close, high, low, {}, 2).upper", prd(params, "period", 20)),
        "KCMiddle" => format!("ta.kc(close, high, low, {}, 2).basis", prd(params, "period", 20)),
        "KCLower" => format!("ta.kc(close, high, low, {}, 2).lower", prd(params, "period", 20)),
        "Highest" => format!("ta.highest(high, {})", prd(params, "period", 20)),
        "Lowest" => format!("ta.lowest(low, {})", prd(params, "period", 20)),
        "SuperTrend" => format!("ta.supertrend({}, {}).direction", prd(params, "period", 10), prd(params, "mult", 3)),
        "VWAP" => "ta.vwap".to_string(),
        "TrueRange" => "ta.tr".to_string(),
        _ => return None,
    };
    Some(res)
}

fn operand(op: &Operand) -> String {
    match op {
        Operand::Const(v) => num(*v),
        Operand::Price(field, shift) => {
            let base = match field {
                PriceField::Open => "open",
                PriceField::High => "high",
                PriceField::Low => "low",
                PriceField::Close => "close",
                PriceField::Volume => "volume",
                PriceField::Typical => "hlc3",
            };
            if *shift == 0 { base.to_string() } else { format!("{base}[{shift}]") }
        }
        Operand::Indicator(name, params) => match indicator(name, params) {
            Some(e) => e,
            None => format!("/*{name}:unmapped*/ 0.0"),
        },
    }
}

fn condition(c: &Condition) -> String {
    match c.cmp {
        Comparator::Rising => format!("ta.rising({}, 1)", operand(&c.left)),
        Comparator::Falling => format!("ta.falling({}, 1)", operand(&c.left)),
        Comparator::CrossAbove => format!("ta.crossover({}, {})", operand(&c.left), operand(&c.right)),
        Comparator::CrossBelow => format!("ta.crossunder({}, {})", operand(&c.left), operand(&c.right)),
        Comparator::Gt => format!("({} > {})", operand(&c.left), operand(&c.right)),
        Comparator::Gte => format!("({} >= {})", operand(&c.left), operand(&c.right)),
        Comparator::Lt => format!("({} < {})", operand(&c.left), operand(&c.right)),
        Comparator::Lte => format!("({} <= {})", operand(&c.left), operand(&c.right)),
        Comparator::Eq => format!("({} == {})", operand(&c.left), operand(&c.right)),
        Comparator::Neq => format!("({} != {})", operand(&c.left), operand(&c.right)),
    }
}

fn signal(sig: &Signal) -> String {
    let joiner = match sig.join { Join::And => " and ", Join::Or => " or " };
    let conds: Vec<String> = sig.conditions.iter().map(condition).collect();
    if conds.is_empty() { "false".to_string() } else { conds.join(joiner) }
}

fn find(s: &Strategy, k: SignalKind) -> Option<&Signal> {
    s.signals.iter().find(|sig| sig.kind == k)
}

fn sl_points(s: &Strategy) -> f64 {
    match &s.stop_loss { Some(StopLoss::Points(p)) => *p, _ => 0.0 }
}

fn tp_points(s: &Strategy) -> f64 {
    match &s.profit_target { Some(ProfitTarget::Points(p)) => *p, _ => 0.0 }
}

fn lots_frac(s: &Strategy) -> f64 {
    match &s.money_management { MoneyManagement::FixedSize { size } => (*size).min(1.0), _ => 0.1 }
}

pub fn render_pine(s: &Strategy) -> String {
    let mut l: Vec<String> = Vec::new();
    l.push("//@version=5".to_string());
    l.push(format!("strategy({Q}{}{Q}, overlay=true, margin_long=100, margin_short=100)", s.name));
    l.push("".to_string());
    l.push("// ---- signals ----".to_string());
    for (kind, name) in [
        (SignalKind::LongEntry, "longEntry"),
        (SignalKind::LongExit, "longExit"),
        (SignalKind::ShortEntry, "shortEntry"),
        (SignalKind::ShortExit, "shortExit"),
    ] {
        match find(s, kind) {
            Some(sig) => l.push(format!("{name} = {}", signal(sig))),
            None => l.push(format!("{name} = false")),
        }
    }
    l.push("".to_string());
    l.push("// ---- entries and exits ----".to_string());
    if s.long {
        l.push("if longEntry".to_string());
        l.push(format!("    strategy.entry({Q}Long{Q}, strategy.long, qty=strategy.equity * {:.3} / close)", lots_frac(s)));
        l.push("if longExit".to_string());
        l.push(format!("    strategy.close({Q}Long{Q})"));
    }
    if s.short {
        l.push("if shortEntry".to_string());
        l.push(format!("    strategy.entry({Q}Short{Q}, strategy.short, qty=strategy.equity * {:.3} / close)", lots_frac(s)));
        l.push("if shortExit".to_string());
        l.push(format!("    strategy.close({Q}Short{Q})"));
    }
    l.push("".to_string());
    let sl = sl_points(s);
    let pt = tp_points(s);
    if sl > 0.0 {
        l.push(format!("strategy.exit({Q}SL{Q}, {Q}Long{Q}, loss = close - {})", num(sl)));
        l.push(format!("strategy.exit({Q}SL{Q}, {Q}Short{Q}, loss = close + {})", num(sl)));
    }
    if pt > 0.0 {
        l.push(format!("strategy.exit({Q}TP{Q}, {Q}Long{Q}, profit = close + {})", num(pt)));
        l.push(format!("strategy.exit({Q}TP{Q}, {Q}Short{Q}, profit = close - {})", num(pt)));
    }
    l.join("\n")
}
