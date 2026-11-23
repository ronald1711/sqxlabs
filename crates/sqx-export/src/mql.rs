//! MQL4 / MQL5 exporters. Both dialects share the same i* indicator
//! functions; they differ only in the symbol/period/point built-ins and the
//! order-management skeleton.

use sqx_strategy::{
    Comparator, Condition, Join, MoneyManagement, Operand, Param, PriceField, Signal, SignalKind,
    StopLoss, ProfitTarget, Strategy,
};

#[derive(Clone, Copy)]
pub struct Dialect {
    pub symbol: &'static str,
    pub period: &'static str,
    pub point: &'static str,
    pub name: &'static str,
}

pub const MQL4: Dialect = Dialect { symbol: "Symbol()", period: "Period()", point: "Point", name: "MQL4" };
pub const MQL5: Dialect = Dialect { symbol: "_Symbol", period: "_Period", point: "_Point", name: "MQL5" };

fn num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.5}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn prd(params: &[Param], name: &str, def: i64) -> i64 {
    params.iter().find(|p| p.name == name).map(|p| p.value as i64).filter(|&v| v > 0).unwrap_or(def)
}

fn indicator(name: &str, params: &[Param], shift: i32, d: Dialect) -> Option<String> {
    let s = d.symbol;
    let per = d.period;
    let res = match name {
        "SMA" => format!("iMA({s},{per},{},0,MODE_SMA,PRICE_CLOSE,{shift})", prd(params, "period", 20)),
        "EMA" => format!("iMA({s},{per},{},0,MODE_EMA,PRICE_CLOSE,{shift})", prd(params, "period", 20)),
        "SMMA" => format!("iMA({s},{per},{},0,MODE_SMMA,PRICE_CLOSE,{shift})", prd(params, "period", 20)),
        "LWMA" => format!("iMA({s},{per},{},0,MODE_LWMA,PRICE_CLOSE,{shift})", prd(params, "period", 20)),
        "RSI" => format!("iRSI({s},{per},{},PRICE_CLOSE,{shift})", prd(params, "period", 14)),
        "CCI" => format!("iCCI({s},{per},{},PRICE_TYPICAL,{shift})", prd(params, "period", 14)),
        "ATR" => format!("iATR({s},{per},{},{shift})", prd(params, "period", 14)),
        "ADX" => format!("iADX({s},{per},{},PRICE_CLOSE,MODE_MAIN,{shift})", prd(params, "period", 14)),
        "DIPlus" => format!("iADX({s},{per},{},PRICE_CLOSE,MODE_PLUSDI,{shift})", prd(params, "period", 14)),
        "DIMinus" => format!("iADX({s},{per},{},PRICE_CLOSE,MODE_MINUSDI,{shift})", prd(params, "period", 14)),
        "Momentum" => format!("iMomentum({s},{per},{},PRICE_CLOSE,{shift})", prd(params, "period", 10)),
        "WilliamsR" => format!("iWPR({s},{per},{},{shift})", prd(params, "period", 14)),
        "StochK" => format!("iStochastic({s},{per},{},3,3,MODE_SMA,STO_LOWHIGH,MODE_MAIN,{shift})", prd(params, "period", 14)),
        "StochD" => format!("iStochastic({s},{per},{},3,3,MODE_SMA,STO_LOWHIGH,MODE_SIGNAL,{shift})", prd(params, "period", 14)),
        "MACD" => {
            let fast = prd(params, "fast", 12);
            let slow = prd(params, "slow", 26);
            let sig = prd(params, "signal", 9);
            format!("iMACD({s},{per},{fast},{slow},{sig},PRICE_CLOSE,MODE_MAIN,{shift})")
        }
        "MACDSignal" => {
            let fast = prd(params, "fast", 12);
            let slow = prd(params, "slow", 26);
            let sig = prd(params, "signal", 9);
            format!("iMACD({s},{per},{fast},{slow},{sig},PRICE_CLOSE,MODE_SIGNAL,{shift})")
        }
        "BBUpper" => format!("iBands({s},{per},{},2,0,PRICE_CLOSE,MODE_UPPER,{shift})", prd(params, "period", 20)),
        "BBMiddle" => format!("iBands({s},{per},{},2,0,PRICE_CLOSE,MODE_MAIN,{shift})", prd(params, "period", 20)),
        "BBLower" => format!("iBands({s},{per},{},2,0,PRICE_CLOSE,MODE_LOWER,{shift})", prd(params, "period", 20)),
        "AwesomeOsc" => format!("iAO({s},{per},{shift})"),
        "BullsPower" => format!("iBullsPower({s},{per},{},{shift})", prd(params, "period", 13)),
        "BearsPower" => format!("iBearsPower({s},{per},{},{shift})", prd(params, "period", 13)),
        "DeMarker" => format!("iDeMarker({s},{per},{},{shift})", prd(params, "period", 14)),
        "ParabolicSAR" => format!("iSAR({s},{per},0.02,0.2,{shift})"),
        _ => return None,
    };
    Some(res)
}

fn render_operand(op: &Operand, shift: i32, d: Dialect) -> String {
    match op {
        Operand::Const(v) => num(*v),
        Operand::Price(field, own) => {
            let s = own + shift;
            match field {
                PriceField::Typical => format!(
                    "(iHigh({0},{1},{2})+iLow({0},{1},{2})+iClose({0},{1},{2}))/3",
                    d.symbol, d.period, s
                ),
                _ => {
                    let f = match field {
                        PriceField::Open => "iOpen",
                        PriceField::High => "iHigh",
                        PriceField::Low => "iLow",
                        PriceField::Close => "iClose",
                        PriceField::Volume => "iVolume",
                        PriceField::Typical => unreachable!(),
                    };
                    format!("{f}({},{},{s})", d.symbol, d.period)
                }
            }
        }
        Operand::Indicator(name, params) => match indicator(name, params, shift, d) {
            Some(e) => e,
            None => format!("/*{name}:unmapped*/0.0"),
        },
    }
}

fn render_condition(c: &Condition, shift: i32, d: Dialect) -> String {
    let l = |sh: i32| render_operand(&c.left, sh, d);
    let r = |sh: i32| render_operand(&c.right, sh, d);
    match c.cmp {
        Comparator::Gt => format!("({} > {})", l(shift), r(shift)),
        Comparator::Gte => format!("({} >= {})", l(shift), r(shift)),
        Comparator::Lt => format!("({} < {})", l(shift), r(shift)),
        Comparator::Lte => format!("({} <= {})", l(shift), r(shift)),
        Comparator::Eq => format!("({} == {})", l(shift), r(shift)),
        Comparator::Neq => format!("({} != {})", l(shift), r(shift)),
        Comparator::CrossAbove => {
            format!("({} > {} && {} <= {})", l(shift), r(shift), l(shift + 1), r(shift + 1))
        }
        Comparator::CrossBelow => {
            format!("({} < {} && {} >= {})", l(shift), r(shift), l(shift + 1), r(shift + 1))
        }
        Comparator::Rising => format!("({} > {})", l(shift), l(shift + 1)),
        Comparator::Falling => format!("({} < {})", l(shift), l(shift + 1)),
    }
}

fn render_signal(sig: &Signal, shift: i32, d: Dialect) -> String {
    let joiner = match sig.join {
        Join::And => " && ",
        Join::Or => " || ",
    };
    let conds: Vec<String> = sig.conditions.iter().map(|c| render_condition(c, shift, d)).collect();
    if conds.is_empty() {
        "false".to_string()
    } else {
        conds.join(joiner)
    }
}

fn signal_fn(kind: SignalKind, sig: Option<&Signal>, d: Dialect) -> String {
    let fname = match kind {
        SignalKind::LongEntry => "LongEntry",
        SignalKind::LongExit => "LongExit",
        SignalKind::ShortEntry => "ShortEntry",
        SignalKind::ShortExit => "ShortExit",
    };
    match sig {
        Some(s) => format!("bool {fname}(int shift) {{ return ({}); }}", render_signal(s, 0, d)),
        None => format!("bool {fname}(int shift) {{ return false; }}"),
    }
}

fn find(s: &Strategy, k: SignalKind) -> Option<&Signal> {
    s.signals.iter().find(|sig| sig.kind == k)
}

fn lots(s: &Strategy) -> String {
    match &s.money_management {
        MoneyManagement::FixedSize { size } => num(*size),
        _ => "0.1".to_string(),
    }
}

fn sl_points(s: &Strategy) -> f64 {
    match &s.stop_loss {
        Some(StopLoss::Points(p)) => *p,
        _ => 0.0,
    }
}

fn tp_points(s: &Strategy) -> f64 {
    match &s.profit_target {
        Some(ProfitTarget::Points(p)) => *p,
        _ => 0.0,
    }
}

pub fn render_mql4(strategy: &Strategy) -> String {
    render(strategy, MQL4)
}

pub fn render_mql5(strategy: &Strategy) -> String {
    render(strategy, MQL5)
}

fn render(s: &Strategy, d: Dialect) -> String {
    let mut l: Vec<String> = Vec::new();
    l.push("//+------------------------------------------------------------------+".to_string());
    l.push(format!("//|  Expert Advisor generated by SqxLabs (clean-room) - {}", s.name));
    l.push("//|  Signals and order routing require platform parity validation.    |".to_string());
    l.push("//+------------------------------------------------------------------+".to_string());
    l.push("#property strict".to_string());
    l.push("".to_string());
    l.push(format!("input double Lots = {};", lots(s)));
    l.push(format!("input double StopLossPoints = {};", num(sl_points(s))));
    l.push(format!("input double TakeProfitPoints = {};", num(tp_points(s))));
    l.push("input int    MagicNumber = 20260101;".to_string());
    l.push("".to_string());
    l.push(signal_fn(SignalKind::LongEntry, find(s, SignalKind::LongEntry), d));
    l.push(signal_fn(SignalKind::LongExit, find(s, SignalKind::LongExit), d));
    l.push(signal_fn(SignalKind::ShortEntry, find(s, SignalKind::ShortEntry), d));
    l.push(signal_fn(SignalKind::ShortExit, find(s, SignalKind::ShortExit), d));
    l.push("".to_string());
    l.push(format!("double SL() {{ return StopLossPoints > 0 ? StopLossPoints * {} : 0; }}", d.point));
    l.push(format!("double TP() {{ return TakeProfitPoints > 0 ? TakeProfitPoints * {} : 0; }}", d.point));
    l.push("".to_string());

    if d.name == "MQL4" {
        l.push("int OnInit() { return(INIT_SUCCEEDED); }".to_string());
        l.push("void OnDeinit(const int reason) {}".to_string());
        l.push("void OnTick() {".to_string());
        l.push("   int shift = 1; // last completed bar".to_string());
        l.push("   bool flat = true;".to_string());
        l.push("   for (int i = OrdersTotal()-1; i >= 0; i--) {".to_string());
        l.push("      if (OrderSelect(i, SELECT_BY_POS) && OrderMagicNumber() == MagicNumber && OrderSymbol() == Symbol()) flat = false;".to_string());
        l.push("   }".to_string());
        l.push("   if (flat) {".to_string());
        l.push("      if (LongEntry(shift)) {".to_string());
        l.push("         int t = OrderSend(Symbol(), OP_BUY, Lots, Ask, 3, 0, 0, NULL, MagicNumber);".to_string());
        l.push("      } else if (ShortEntry(shift)) {".to_string());
        l.push("         int t = OrderSend(Symbol(), OP_SELL, Lots, Bid, 3, 0, 0, NULL, MagicNumber);".to_string());
        l.push("      }".to_string());
        l.push("   }".to_string());
        l.push("}".to_string());
    } else {
        l.push("int OnInit() { return(INIT_SUCCEEDED); }".to_string());
        l.push("void OnDeinit(const int reason) {}".to_string());
        l.push("void OnTick() {".to_string());
        l.push("   int shift = 1; // last completed bar".to_string());
        l.push("   if (PositionSelect(_Symbol)) {".to_string());
        l.push("      // exit on opposite signal (close the single netted position)".to_string());
        l.push("      if (LongExit(shift) || ShortExit(shift)) {".to_string());
        l.push("         // close position via your broker's order API".to_string());
        l.push("      }".to_string());
        l.push("   } else {".to_string());
        l.push("      if (LongEntry(shift)) {".to_string());
        l.push("         // open long at Ask with SL()/TP()".to_string());
        l.push("      } else if (ShortEntry(shift)) {".to_string());
        l.push("         // open short at Bid with SL()/TP()".to_string());
        l.push("      }".to_string());
        l.push("   }".to_string());
        l.push("}".to_string());
    }
    l.join("\n")
}
