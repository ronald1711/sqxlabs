use serde::{Deserialize, Serialize};

/// A named numeric parameter of a building block (e.g. period = 14).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub value: f64,
}

impl Param {
    pub fn new(name: &str, value: f64) -> Self {
        Self { name: name.to_string(), value }
    }
}

/// A market/price field usable as a condition operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PriceField {
    Open, High, Low, Close, Volume, Typical,
}

impl PriceField {
    pub fn label(&self) -> &'static str {
        match self {
            PriceField::Open => "Open",
            PriceField::High => "High",
            PriceField::Low => "Low",
            PriceField::Close => "Close",
            PriceField::Volume => "Volume",
            PriceField::Typical => "Typical",
        }
    }
}

/// Left or right side of a condition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Operand {
    /// A literal constant.
    Const(f64),
    /// A price/bar field, optionally shifted back `n` bars.
    Price(PriceField, i32),
    /// A named indicator value with parameters.
    Indicator(String, Vec<Param>),
}

impl Operand {
    /// Shift applied to this operand (0 for the current bar).
    pub fn shift(&self) -> i32 {
        match self {
            Operand::Price(_, s) => *s,
            Operand::Indicator(_, _) => 0,
            Operand::Const(_) => 0,
        }
    }
}

/// Comparison operator between two operands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Comparator {
    Gt, Gte, Lt, Lte, Eq, Neq,
    CrossAbove, CrossBelow,
    Rising, Falling,
}

impl Comparator {
    pub fn label(&self) -> &'static str {
        match self {
            Comparator::Gt => "is greater than",
            Comparator::Gte => "is greater than or equal to",
            Comparator::Lt => "is lower than",
            Comparator::Lte => "is lower than or equal to",
            Comparator::Eq => "equals",
            Comparator::Neq => "does not equal",
            Comparator::CrossAbove => "crosses above",
            Comparator::CrossBelow => "crosses below",
            Comparator::Rising => "is rising",
            Comparator::Falling => "is falling",
        }
    }
    /// Unary comparisons (Rising/Falling) do not use the right operand.
    pub fn is_unary(&self) -> bool {
        matches!(self, Comparator::Rising | Comparator::Falling)
    }
}

/// A single IF condition: left CMP right.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub left: Operand,
    pub cmp: Comparator,
    pub right: Operand,
}

impl Condition {
    pub fn new(left: Operand, cmp: Comparator, right: Operand) -> Self {
        Self { left, cmp, right }
    }
}

/// How the conditions of a signal combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Join {
    And, Or,
}

/// The four signal kinds a strategy can reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalKind {
    LongEntry, LongExit, ShortEntry, ShortExit,
}

impl SignalKind {
    pub fn label(&self) -> &'static str {
        match self {
            SignalKind::LongEntry => "LongEntrySignal",
            SignalKind::LongExit => "LongExitSignal",
            SignalKind::ShortEntry => "ShortEntrySignal",
            SignalKind::ShortExit => "ShortExitSignal",
        }
    }
}

/// A named true/false signal computed from conditions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    pub kind: SignalKind,
    pub conditions: Vec<Condition>,
    pub join: Join,
    /// Fuzzy threshold: fraction of conditions that must be true (0..1].
    /// `None` = strict AND/OR logic.
    pub fuzzy: Option<f64>,
}

impl Signal {
    pub fn new(kind: SignalKind, conditions: Vec<Condition>, join: Join) -> Self {
        Self { kind, conditions, join, fuzzy: None }
    }
}

/// The four entry order types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    EnterAtMarket, EnterAtLimit, EnterAtStop, EnterReverseAtMarket,
}

impl OrderType {
    pub fn label(&self) -> &'static str {
        match self {
            OrderType::EnterAtMarket => "Enter at market",
            OrderType::EnterAtLimit => "Enter at limit",
            OrderType::EnterAtStop => "Enter at stop",
            OrderType::EnterReverseAtMarket => "Enter reverse at market",
        }
    }
}

/// Trade-management exit types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExitType {
    TrailingStop,
    MoveToBreakEven,
    ExitAfterBars(usize),
}

/// Money-management (position sizing) method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MoneyManagement {
    /// Fixed contract size.
    FixedSize { size: f64 },
    /// Fixed monetary amount per trade.
    FixedAmount { amount: f64 },
    /// Risk a fixed % of the current balance; size derived from the stop-loss.
    RiskFixedBalancePct { risk: f64 },
    /// Risk a fixed % of the account, with a fixed SL distance in points.
    RiskFixedPctOfAccount { risk: f64, stop_loss_points: f64 },
    /// ATR-volatility-scaled risk sizing.
    AtrRiskBased { risk: f64, atr_period: usize, atr_mult: f64 },
}

impl Default for MoneyManagement {
    fn default() -> Self {
        MoneyManagement::FixedSize { size: 0.1 }
    }
}

impl MoneyManagement {
    pub fn label(&self) -> &'static str {
        match self {
            MoneyManagement::FixedSize { .. } => "FixedSize",
            MoneyManagement::FixedAmount { .. } => "FixedAmount",
            MoneyManagement::RiskFixedBalancePct { .. } => "RiskFixedBalancePct",
            MoneyManagement::RiskFixedPctOfAccount { .. } => "RiskFixedPctOfAccount",
            MoneyManagement::AtrRiskBased { .. } => "ATRRiskBasedSizing",
        }
    }
}

/// A complete trading strategy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Strategy {
    pub name: String,
    /// Trade long side.
    pub long: bool,
    /// Trade short side.
    pub short: bool,
    /// The signals (SQ X style: one signal rule computes all signals).
    pub signals: Vec<Signal>,
    /// Exit/trade-management blocks that are active.
    pub exits: Vec<ExitType>,
    /// Stop-loss definition (points, absolute, or ATR multiple).
    pub stop_loss: Option<StopLoss>,
    /// Profit-target definition.
    pub profit_target: Option<ProfitTarget>,
    /// Position sizing.
    pub money_management: MoneyManagement,
}

/// Stop-loss specification.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StopLoss {
    /// Fixed distance in points.
    Points(f64),
    /// ATR multiple distance.
    Atr { period: usize, mult: f64 },
}

/// Profit-target specification.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ProfitTarget {
    Points(f64),
    Atr { period: usize, mult: f64 },
}

impl Strategy {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            long: true,
            short: true,
            signals: Vec::new(),
            exits: Vec::new(),
            stop_loss: None,
            profit_target: None,
            money_management: MoneyManagement::default(),
        }
    }
}
