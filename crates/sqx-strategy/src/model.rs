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
    /// Validate the signal definition before compiling it against source data.
    pub fn validate_execution_settings(&self)->Result<(),String>{
        if self.fuzzy.is_some_and(|value|!value.is_finite() || value<=0.0 || value>1.0){return Err("Fuzzy threshold must be finite and in (0, 1]".into());}
        for condition in &self.conditions {
            for operand in [&condition.left,&condition.right] {
                match operand {
                    Operand::Const(value) if !value.is_finite()=>return Err("Signal constants must be finite".into()),
                    Operand::Price(_,shift) if *shift<0=>return Err("Price shifts must be nonnegative; future offsets are unsupported".into()),
                    Operand::Indicator(name,params)=>{
                        // KAMA/TEMA remain supported legacy registry names.
                        let declared=match crate::catalog::indicator_catalog().iter().find(|spec|spec.name==name){
                            Some(spec)=>spec.params,
                            None if name=="KAMA" || name=="TEMA"=>&[("period",20.0)][..],
                            None=>return Err(format!("Unknown indicator {name}")),
                        };
                        let mut names=std::collections::HashSet::new();
                        for param in params {
                            if !declared.iter().any(|(key,_)|*key==param.name){return Err(format!("Unknown parameter {} for {name}",param.name));}
                            if !param.value.is_finite(){return Err(format!("Indicator parameter {} must be finite",param.name));}
                            if matches!(param.name.as_str(),"period"|"fast"|"slow"|"signal") {
                                if param.value<1.0 || param.value.fract()!=0.0 || param.value>=usize::MAX as f64 {
                                    return Err(format!("{name}.{} must be a positive, representable whole number",param.name));
                                }
                            } else if param.value<=0.0 {return Err(format!("{name}.{} must be positive",param.name));}
                            if !names.insert(&param.name){return Err(format!("Duplicate indicator parameter {}",param.name));}
                        }
                        let resolved=|key:&str|params.iter().find(|p|p.name==key).map(|p|p.value).or_else(||declared.iter().find(|(k,_)|*k==key).map(|(_,v)|*v));
                        if let (Some(step),Some(max))=(resolved("step"),resolved("max")) {
                            if step>max{return Err(format!("{name}.step cannot exceed max"));}
                        }
                    }
                    _=>{}
                }
            }
        }
        Ok(())
    }

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
    /// Trail by the initial stop-loss distance; requires an explicit stop-loss.
    TrailingStop,
    /// Activate after one initial stop-loss distance of favorable movement.
    /// Requires an explicit stop-loss; break-even is entry price before costs.
    MoveToBreakEven,
    /// Count the entry candle as one; after N occupied candle closes, exit at
    /// the next executable bar open or tick. N must be positive.
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
    /// Allocate `risk` percent of balance to stock notional, converted to lots
    /// using contract size and account currency. This is not stop-loss risk.
    StocksSizeByPrice { risk: f64 },
    /// Allocate `risk` percent of balance to crypto notional, converted to lots
    /// using contract size and account currency. This is not stop-loss risk.
    CryptoSizeByPrice { risk: f64 },
    /// Fixed amount (crypto variant).
    CryptoFixedAmount { amount: f64 },
    /// Risk a fixed % of balance (crypto variant).
    CryptoFixedBalancePct { risk: f64 },
    /// Simple Martingale: after a loss multiply the size, reset after a win.
    SimpleMartingale { lots_start: f64, lots_multiplier: f64, lots_reset: f64 },
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
            MoneyManagement::StocksSizeByPrice { .. } => "StocksSizeByPrice",
            MoneyManagement::CryptoSizeByPrice { .. } => "CryptoSizeByPrice",
            MoneyManagement::CryptoFixedAmount { .. } => "CryptoFixedAmount",
            MoneyManagement::CryptoFixedBalancePct { .. } => "CryptoFixedBalancePct",
            MoneyManagement::SimpleMartingale { .. } => "SimpleMartingaleMM",
        }
    }
}

/// A higher-timeframe confirmation signal (multi-timeframe strategy).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Confirmation {
    pub timeframe: sqx_core::Timeframe,
    pub signal: Signal,
}

/// Trade-management options applied during the backtest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradingOptions {
    /// Close any open position at the end of the trading day.
    pub exit_at_end_of_day: bool,
    /// Close any open position at the end of Friday.
    pub exit_on_friday: bool,
    /// Explicit UTC cutoff (0..1440 minutes). Required for scheduled daily/Friday exits.
    /// Bars execute at the first open at/after the cutoff; ticks at the first quote.
    #[serde(default)]
    pub session_close_utc_minute: Option<u16>,
    /// Maximum number of new trades opened per day.
    pub max_trades_per_day: Option<usize>,
    /// UTC entry hours [start,end); start>end spans midnight. Start is 0..23,
    /// end is 0..24; equal endpoints are invalid. (0,24) permits the full day.
    pub limit_hours: Option<(u8, u8)>,
}

impl Default for TradingOptions {
    fn default() -> Self {
        Self {
            exit_at_end_of_day: false,
            exit_on_friday: false,
            session_close_utc_minute: None,
            max_trades_per_day: None,
            limit_hours: None,
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
    /// Trade-management options.
    #[serde(default)]
    pub trading_options: TradingOptions,
    /// Optional higher-timeframe confirmation gate.
    #[serde(default)]
    pub confirmation: Option<Confirmation>,
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
    /// Validate numeric sizing and protective-exit settings before execution.
    pub fn validate_execution_settings(&self)->Result<(),String>{
        if self.trading_options.limit_hours.is_some_and(|(start,end)|start>23 || end>24 || start==end){
            return Err("Entry hours require start 0..23 and end 0..24 with distinct endpoints".into());
        }
        let mut kinds=std::collections::HashSet::new();
        for signal in &self.signals {
            if !kinds.insert(signal.kind){return Err(format!("Duplicate signal kind {:?}; combine conditions explicitly",signal.kind));}
            signal.validate_execution_settings()?;
        }
        if let Some(confirmation)=&self.confirmation{
            if confirmation.timeframe==sqx_core::Timeframe::Tick {
                return Err("Tick confirmation cannot be reconstructed by bar resampling".into());
            }
            confirmation.signal.validate_execution_settings()?;
        }
        let positive=|name:&str,value:f64|if value.is_finite() && value>0.0 {Ok(())}else{Err(format!("{name} must be finite and positive"))};
        let atr=|period:usize,mult:f64|{if period==0{return Err("ATR period must be positive".into());}positive("ATR multiplier",mult)};
        match self.stop_loss {Some(StopLoss::Points(value))=>positive("Stop-loss distance",value)?,Some(StopLoss::Atr{period,mult})=>atr(period,mult)?,None=>{}}
        match self.profit_target {Some(ProfitTarget::Points(value))=>positive("Profit-target distance",value)?,Some(ProfitTarget::Atr{period,mult})=>atr(period,mult)?,None=>{}}
        if self.stop_loss.is_none() && matches!(self.money_management,
            MoneyManagement::FixedAmount{..}|MoneyManagement::CryptoFixedAmount{..}|
            MoneyManagement::RiskFixedBalancePct{..}|MoneyManagement::CryptoFixedBalancePct{..}) {
            return Err("This sizing method requires an explicit stop-loss; no default stop distance is assumed".into());
        }
        match self.money_management {
            MoneyManagement::FixedSize{size}=>positive("Position size",size)?,
            MoneyManagement::FixedAmount{amount}|MoneyManagement::CryptoFixedAmount{amount}=>positive("Position amount",amount)?,
            MoneyManagement::RiskFixedBalancePct{risk}|MoneyManagement::CryptoFixedBalancePct{risk}|MoneyManagement::StocksSizeByPrice{risk}|MoneyManagement::CryptoSizeByPrice{risk}=>positive("Risk",risk)?,
            MoneyManagement::RiskFixedPctOfAccount{risk,stop_loss_points}=>{positive("Risk",risk)?;positive("Sizing stop distance",stop_loss_points)?;}
            MoneyManagement::AtrRiskBased{risk,atr_period,atr_mult}=>{positive("Risk",risk)?;atr(atr_period,atr_mult)?;}
            MoneyManagement::SimpleMartingale{lots_start,lots_multiplier,lots_reset}=>{positive("Initial lots",lots_start)?;positive("Lot multiplier",lots_multiplier)?;positive("Reset lots",lots_reset)?;}
        }
        if self.exits.iter().any(|exit|matches!(exit,ExitType::ExitAfterBars(0))){return Err("Exit-after-bars must be positive".into());}
        let mut exit_kinds=std::collections::HashSet::new();
        for exit in &self.exits {
            if !exit_kinds.insert(std::mem::discriminant(exit)){return Err("Duplicate exit type; each exit must have one explicit configuration".into());}
            if self.stop_loss.is_none() && matches!(exit,ExitType::TrailingStop|ExitType::MoveToBreakEven) {
                return Err("Trailing and break-even exits require an initial stop-loss distance".into());
            }
        }
        Ok(())
    }

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
            trading_options: TradingOptions::default(),
            confirmation: None,
        }
    }
}
