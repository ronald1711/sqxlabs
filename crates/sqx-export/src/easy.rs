//! EasyLanguage (TradeStation / PowerLanguage) exporter.

use sqx_strategy::{Comparator, Condition, Join, MoneyManagement, Operand, Param, PriceField, Signal, SignalKind, Strategy};

fn num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

fn prd(params: &[Param], name: &str, def: i64) -> i64 {
    params.iter().find(|p| p.name == name).map(|p| p.value as i64).filter(|&v| v > 0).unwrap_or(def)
}

fn ind_expr(name: &str, params: &[Param]) -> String {
    match name {
        "SMA" => format!("Average(Close, {})", prd(params, "period", 20)),
        "EMA" => format!("XAverage(Close, {})", prd(params, "period", 20)),
        "SMMA" => format!("Average(Close, {})", prd(params, "period", 20)),
        "LWMA" => format!("WAverage(Close, {})", prd(params, "period", 20)),
        "RSI" => format!("RSI(Close, {})", prd(params, "period", 14)),
        "CCI" => format!("CCI({})", prd(params, "period", 14)),
        "StochK" => format!("SlowK({})", prd(params, "period", 14)),
        "StochD" => format!("SlowD({})", prd(params, "period", 14)),
        "MACD" => format!("MACD(Close, {}, {})", prd(params, "fast", 12), prd(params, "slow", 26)),
        "ATR" => format!("AvgTrueRange({})", prd(params, "period", 14)),
        "ADX" => format!("ADX({})", prd(params, "period", 14)),
        "Momentum" => format!("Momentum(Close, {})", prd(params, "period", 10)),
        "WilliamsR" => format!("PercentR({})", prd(params, "period", 14)),
        "BBUpper" => format!("BollingerBand(Close, {}, 2)", prd(params, "period", 20)),
        "BBLower" => format!("BollingerBand(Close, {}, -2)", prd(params, "period", 20)),
        "Highest" => format!("Highest(High, {})", prd(params, "period", 20)),
        "Lowest" => format!("Lowest(Low, {})", prd(params, "period", 20)),
        _ => format!("/*{name}:unmapped*/0.0"),
    }
}

fn render_at(op: &Operand, shift: i32) -> String {
    match op {
        Operand::Const(v) => num(*v),
        Operand::Price(field, own) => {
            let f = match field {
                PriceField::Open => "Open",
                PriceField::High => "High",
                PriceField::Low => "Low",
                PriceField::Close => "Close",
                PriceField::Volume => "Volume",
                PriceField::Typical => "(High+Low+Close)/3",
            };
            if own + shift == 0 { f.to_string() } else { format!("{f}[{}]", own + shift) }
        }
        Operand::Indicator(name, params) => {
            let e = ind_expr(name, params);
            if shift == 0 { e } else { format!("{e}[{shift}]") }
        }
    }
}

fn condition(c: &Condition, shift: i32) -> String {
    let l = |s: i32| render_at(&c.left, s);
    let r = |s: i32| render_at(&c.right, s);
    match c.cmp {
        Comparator::Gt => format!("({} > {})", l(shift), r(shift)),
        Comparator::Gte => format!("({} >= {})", l(shift), r(shift)),
        Comparator::Lt => format!("({} < {})", l(shift), r(shift)),
        Comparator::Lte => format!("({} <= {})", l(shift), r(shift)),
        Comparator::Eq => format!("({} = {})", l(shift), r(shift)),
        Comparator::Neq => format!("({} <> {})", l(shift), r(shift)),
        Comparator::CrossAbove => format!("({} crosses over {})", l(shift), r(shift)),
        Comparator::CrossBelow => format!("({} crosses under {})", l(shift), r(shift)),
        Comparator::Rising => format!("({} > {})", l(shift), l(shift + 1)),
        Comparator::Falling => format!("({} < {})", l(shift), l(shift + 1)),
    }
}

fn signal_expr(sig: Option<&Signal>) -> String {
    match sig {
        Some(s) if !s.conditions.is_empty() => {
            let joiner = match s.join { Join::And => " and ", Join::Or => " or " };
            let parts: Vec<String> = s.conditions.iter().map(|c| condition(c, 0)).collect();
            parts.join(joiner)
        }
        _ => "false".to_string(),
    }
}

fn find<'a>(s: &'a Strategy, k: SignalKind) -> Option<&'a Signal> {
    s.signals.iter().find(|sig| sig.kind == k)
}

fn lots(s: &Strategy) -> String {
    match &s.money_management {
        MoneyManagement::FixedSize { size } => num(*size),
        _ => "1".to_string(),
    }
}

pub fn render_easylanguage(s: &Strategy) -> String {
    let mut l: Vec<String> = Vec::new();
    l.push("// SqxLabs EasyLanguage (TradeStation) strategy - reference skeleton".to_string());
    l.push(format!("// Strategy: {}", s.name));
    l.push("inputs:".to_string());
    l.push(format!("    Lots({});", lots(s)));
    l.push("".to_string());
    l.push(format!("if (Position = 0) and ({}) then Buy Lots shares next bar at market;", signal_expr(find(s, SignalKind::LongEntry))));
    l.push(format!("if (Position = 0) and ({}) then SellShort Lots shares next bar at market;", signal_expr(find(s, SignalKind::ShortEntry))));
    l.push(format!("if (Position > 0) and ({}) then Sell next bar at market;", signal_expr(find(s, SignalKind::LongExit))));
    l.push(format!("if (Position < 0) and ({}) then BuyToCover next bar at market;", signal_expr(find(s, SignalKind::ShortExit))));
    l.join("\n")
}
