//! NinjaTrader 8 (NinjaScript / C#) exporter.

use sqx_strategy::{Comparator, Condition, Join, MoneyManagement, Operand, Param, PriceField, Signal, SignalKind, Strategy};

fn num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

fn prd(params: &[Param], name: &str, def: i64) -> i64 {
    params.iter().find(|p| p.name == name).map(|p| p.value as i64).filter(|&v| v > 0).unwrap_or(def)
}

fn ind_expr(name: &str, params: &[Param]) -> String {
    match name {
        "SMA" => format!("SMA({})", prd(params, "period", 20)),
        "EMA" => format!("EMA({})", prd(params, "period", 20)),
        "SMMA" => format!("SMA({})", prd(params, "period", 20)),
        "LWMA" => format!("WMA({})", prd(params, "period", 20)),
        "HMA" => format!("HMA({})", prd(params, "period", 20)),
        "RSI" => format!("RSI({})", prd(params, "period", 14)),
        "CCI" => format!("CCI({})", prd(params, "period", 14)),
        "StochK" => format!("Stochastics({}, 3, 3).K", prd(params, "period", 14)),
        "StochD" => format!("Stochastics({}, 3, 3).D", prd(params, "period", 14)),
        "MACD" => format!("MACD({}, {}, {}).Default", prd(params, "fast", 12), prd(params, "slow", 26), prd(params, "signal", 9)),
        "MACDSignal" => format!("MACD({}, {}, {}).Avg", prd(params, "fast", 12), prd(params, "slow", 26), prd(params, "signal", 9)),
        "ATR" => format!("ATR({})", prd(params, "period", 14)),
        "ADX" => format!("ADX({})", prd(params, "period", 14)),
        "DIPlus" => format!("DM({}).DiPlus", prd(params, "period", 14)),
        "DIMinus" => format!("DM({}).DiMinus", prd(params, "period", 14)),
        "Momentum" => format!("Momentum({})", prd(params, "period", 10)),
        "WilliamsR" => format!("WilliamsR({})", prd(params, "period", 14)),
        "BBUpper" => format!("Bollinger({}, 2).Upper", prd(params, "period", 20)),
        "BBMiddle" => format!("Bollinger({}, 2).Middle", prd(params, "period", 20)),
        "BBLower" => format!("Bollinger({}, 2).Lower", prd(params, "period", 20)),
        "Highest" => format!("MAX(High, {})", prd(params, "period", 20)),
        "Lowest" => format!("MIN(Low, {})", prd(params, "period", 20)),
        _ => format!("SMA({})", prd(params, "period", 20)),
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
                PriceField::Typical => "HLC3",
            };
            format!("{f}[{}]", own + shift)
        }
        Operand::Indicator(name, params) => format!("{}[{shift}]", ind_expr(name, params)),
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
        Comparator::Eq => format!("({} == {})", l(shift), r(shift)),
        Comparator::Neq => format!("({} != {})", l(shift), r(shift)),
        Comparator::CrossAbove => format!("({} > {} && {} <= {})", l(shift), r(shift), l(shift + 1), r(shift + 1)),
        Comparator::CrossBelow => format!("({} < {} && {} >= {})", l(shift), r(shift), l(shift + 1), r(shift + 1)),
        Comparator::Rising => format!("({} > {})", l(shift), l(shift + 1)),
        Comparator::Falling => format!("({} < {})", l(shift), l(shift + 1)),
    }
}

fn signal_expr(sig: Option<&Signal>) -> String {
    match sig {
        Some(s) if !s.conditions.is_empty() => {
            let joiner = match s.join { Join::And => " && ", Join::Or => " || " };
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

pub fn render_ninjascript(s: &Strategy) -> String {
    let mut l: Vec<String> = Vec::new();
    l.push("// SqxLabs NinjaScript (NinjaTrader 8) strategy - reference skeleton".to_string());
    l.push(format!("// Strategy: {}", s.name));
    l.push("#region Using declarations".to_string());
    l.push("using System;".to_string());
    l.push("using NinjaTrader.Cbi;".to_string());
    l.push("using NinjaTrader.NinjaScript;".to_string());
    l.push("using NinjaTrader.NinjaScript.Strategies;".to_string());
    l.push("#endregion".to_string());
    l.push("".to_string());
    l.push("public class SqxLabsStrategy : Strategy".to_string());
    l.push("{".to_string());
    l.push("    protected override void OnStateChange()".to_string());
    l.push("    {".to_string());
    l.push("        if (State == State.SetDefaults)".to_string());
    l.push("        {".to_string());
    l.push(format!("            Name = \"{}\";", s.name.replace('"', "'")));
    l.push("            Calculate = Calculate.OnBarClose;".to_string());
    l.push("            EntriesPerDirection = 1;".to_string());
    l.push("            IsExitOnSessionCloseStrategy = true;".to_string());
    l.push(format!("            Quantity = {};", lots(s)));
    l.push("        }".to_string());
    l.push("    }".to_string());
    l.push("".to_string());
    l.push("    protected override void OnBarUpdate()".to_string());
    l.push("    {".to_string());
    l.push("        if (CurrentBar < 50) return;".to_string());
    l.push("        if (Position.MarketPosition == MarketPosition.Flat)".to_string());
    l.push("        {".to_string());
    l.push("            if (LongEntry()) EnterLong(Quantity, \"Long\");".to_string());
    l.push("            else if (ShortEntry()) EnterShort(Quantity, \"Short\");".to_string());
    l.push("        }".to_string());
    l.push("        else if (Position.MarketPosition == MarketPosition.Long && LongExit()) ExitLong(\"LX\", \"Long\");".to_string());
    l.push("        else if (Position.MarketPosition == MarketPosition.Short && ShortExit()) ExitShort(\"SX\", \"Short\");".to_string());
    l.push("    }".to_string());
    l.push("".to_string());
    l.push(format!("    private bool LongEntry() {{ return {}; }}", signal_expr(find(s, SignalKind::LongEntry))));
    l.push(format!("    private bool ShortEntry() {{ return {}; }}", signal_expr(find(s, SignalKind::ShortEntry))));
    l.push(format!("    private bool LongExit() {{ return {}; }}", signal_expr(find(s, SignalKind::LongExit))));
    l.push(format!("    private bool ShortExit() {{ return {}; }}", signal_expr(find(s, SignalKind::ShortExit))));
    l.push("}".to_string());
    l.join("\n")
}
