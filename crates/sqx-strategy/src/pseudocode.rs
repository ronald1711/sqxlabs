use crate::model::*;

/// Renders a strategy as human-readable pseudo-code.
pub fn render(strategy: &Strategy) -> String {
    let mut out = String::new();
    out.push_str(&format!("// ===== Strategy: {} =====\n", strategy.name));
    out.push_str(&format!("// Direction: {} / {}\n",
        if strategy.long { "Long" } else { "-" },
        if strategy.short { "Short" } else { "-" }));
    out.push_str("// Money management: ");
    out.push_str(strategy.money_management.label());
    out.push_str("\n\n");

    for sig in &strategy.signals {
        out.push_str(&format!("{} =\n", sig.kind.label()));
        out.push_str(&render_join(sig));
        out.push_str("\n");
    }

    if let Some(sl) = &strategy.stop_loss {
        out.push_str("\nStopLoss = ");
        out.push_str(&render_sl(*sl));
        out.push_str("\n");
    }
    if let Some(pt) = &strategy.profit_target {
        out.push_str("ProfitTarget = ");
        out.push_str(&render_pt(*pt));
        out.push_str("\n");
    }
    out
}

fn render_sl(sl: StopLoss) -> String {
    match sl {
        StopLoss::Points(p) => format!("{p} points"),
        StopLoss::Atr { period, mult } => format!("{mult} x ATR({period})"),
    }
}

fn render_pt(pt: ProfitTarget) -> String {
    match pt {
        ProfitTarget::Points(p) => format!("{p} points"),
        ProfitTarget::Atr { period, mult } => format!("{mult} x ATR({period})"),
    }
}

fn render_join(sig: &Signal) -> String {
    let joiner = match sig.join { Join::And => " AND\n", Join::Or => " OR\n" };
    let mut parts: Vec<String> = sig.conditions.iter().map(render_condition).collect();
    if let Some(f) = sig.fuzzy {
        parts.push(format!("   (fuzzy: {:.0}% of conditions must be true)", f * 100.0));
    }
    parts.join(joiner)
}

fn render_condition(c: &Condition) -> String {
    if c.cmp.is_unary() {
        format!("   {} {}", render_operand(&c.left), c.cmp.label())
    } else {
        format!("   {} {} {}", render_operand(&c.left), c.cmp.label(), render_operand(&c.right))
    }
}

fn render_operand(o: &Operand) -> String {
    match o {
        Operand::Const(v) => format!("{v}"),
        Operand::Price(p, s) => {
            if *s == 0 { p.label().to_string() } else { format!("{}({})", p.label(), s) }
        }
        Operand::Indicator(name, params) => {
            if params.is_empty() {
                name.clone()
            } else {
                let ps: Vec<String> = params.iter().map(|p| format!("{}={}", p.name, p.value)).collect();
                format!("{}({})", name, ps.join(", "))
            }
        }
    }
}
