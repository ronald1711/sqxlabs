use crate::model::{Comparator, ExitType, OrderType, Param, PriceField};

/// Metadata for one indicator building block: how the generator may create it.
#[derive(Debug, Clone, PartialEq)]
pub struct IndicatorSpec {
    /// Registered name (used as the operand key).
    pub name: &'static str,
    /// Short human label.
    pub label: &'static str,
    /// Parameter names with their default values.
    pub params: &'static [(&'static str, f64)],
    /// Whether the value is bounded 0..100 (oscillator) — used to pick levels.
    pub is_oscillator: bool,
}

impl IndicatorSpec {
    pub fn default_params(&self) -> Vec<Param> {
        self.params.iter().map(|(n, v)| Param::new(n, *v)).collect()
    }
}

/// The catalog of indicator building blocks the generator may choose from.
/// This mirrors a representative subset of the shipped SQX building-block
/// catalog (see docs/overview/snippets/building-blocks).
pub fn indicator_catalog() -> &'static [IndicatorSpec] {
    &[
        IndicatorSpec { name: "SMA", label: "Simple MA", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "EMA", label: "Exponential MA", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "SMMA", label: "Smoothed MA", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "LWMA", label: "Linear-weighted MA", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "HMA", label: "Hull MA", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "RSI", label: "RSI", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "CCI", label: "CCI", params: &[("period", 14.0)], is_oscillator: false },
        IndicatorSpec { name: "StochK", label: "Stochastic %K", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "StochD", label: "Stochastic %D", params: &[("period", 3.0)], is_oscillator: true },
        IndicatorSpec { name: "MACD", label: "MACD line", params: &[("fast", 12.0), ("slow", 26.0)], is_oscillator: false },
        IndicatorSpec { name: "MACDSignal", label: "MACD signal", params: &[("fast", 12.0), ("slow", 26.0), ("signal", 9.0)], is_oscillator: false },
        IndicatorSpec { name: "ATR", label: "ATR", params: &[("period", 14.0)], is_oscillator: false },
        IndicatorSpec { name: "ADX", label: "ADX", params: &[("period", 14.0)], is_oscillator: false },
        IndicatorSpec { name: "DIPlus", label: "+DI", params: &[("period", 14.0)], is_oscillator: false },
        IndicatorSpec { name: "DIMinus", label: "-DI", params: &[("period", 14.0)], is_oscillator: false },
        IndicatorSpec { name: "Momentum", label: "Momentum", params: &[("period", 10.0)], is_oscillator: false },
        IndicatorSpec { name: "ROC", label: "Rate of change", params: &[("period", 10.0)], is_oscillator: false },
        IndicatorSpec { name: "WilliamsR", label: "Williams %R", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "BBUpper", label: "Bollinger upper", params: &[("period", 20.0), ("mult", 2.0)], is_oscillator: false },
        IndicatorSpec { name: "BBMiddle", label: "Bollinger middle", params: &[("period", 20.0), ("mult", 2.0)], is_oscillator: false },
        IndicatorSpec { name: "BBLower", label: "Bollinger lower", params: &[("period", 20.0), ("mult", 2.0)], is_oscillator: false },
        IndicatorSpec { name: "KCUpper", label: "Keltner upper", params: &[("period", 20.0), ("mult", 2.0)], is_oscillator: false },
        IndicatorSpec { name: "KCMiddle", label: "Keltner middle", params: &[("period", 20.0), ("mult", 2.0)], is_oscillator: false },
        IndicatorSpec { name: "KCLower", label: "Keltner lower", params: &[("period", 20.0), ("mult", 2.0)], is_oscillator: false },
        IndicatorSpec { name: "StdDev", label: "StdDev", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "Highest", label: "Highest", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "Lowest", label: "Lowest", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "LinReg", label: "Linear regression", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "LinRegSlope", label: "LinReg slope", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "AroonUp", label: "Aroon up", params: &[("period", 25.0)], is_oscillator: true },
        IndicatorSpec { name: "AroonDown", label: "Aroon down", params: &[("period", 25.0)], is_oscillator: true },
        IndicatorSpec { name: "AwesomeOsc", label: "Awesome Oscillator", params: &[], is_oscillator: false },
        IndicatorSpec { name: "BullsPower", label: "Bulls Power", params: &[("period", 13.0)], is_oscillator: false },
        IndicatorSpec { name: "BearsPower", label: "Bears Power", params: &[("period", 13.0)], is_oscillator: false },
        IndicatorSpec { name: "DeMarker", label: "DeMarker", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "ParabolicSAR", label: "Parabolic SAR", params: &[("step", 0.02), ("max", 0.2)], is_oscillator: false },
        IndicatorSpec { name: "SuperTrend", label: "SuperTrend", params: &[("period", 10.0), ("mult", 3.0)], is_oscillator: false },
        IndicatorSpec { name: "VWAP", label: "VWAP", params: &[], is_oscillator: false },
        IndicatorSpec { name: "TrueRange", label: "True range", params: &[], is_oscillator: false },
        IndicatorSpec { name: "IchimokuTenkan", label: "Ichimoku Tenkan", params: &[], is_oscillator: false },
        IndicatorSpec { name: "IchimokuKijun", label: "Ichimoku Kijun", params: &[], is_oscillator: false },
        IndicatorSpec { name: "IchimokuSenkouA", label: "Ichimoku Senkou A", params: &[], is_oscillator: false },
        IndicatorSpec { name: "IchimokuSenkouB", label: "Ichimoku Senkou B", params: &[], is_oscillator: false },
        IndicatorSpec { name: "VortexPlus", label: "Vortex +VI", params: &[("period", 14.0)], is_oscillator: false },
        IndicatorSpec { name: "VortexMinus", label: "Vortex -VI", params: &[("period", 14.0)], is_oscillator: false },
        IndicatorSpec { name: "KaufmanER", label: "Kaufman efficiency", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "GannHiLo", label: "Gann HiLo", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "GannHiLoTrend", label: "Gann HiLo trend", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "AroonOsc", label: "Aroon oscillator", params: &[("period", 25.0)], is_oscillator: true },
        IndicatorSpec { name: "StochRSI", label: "Stochastic RSI", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "DonchianUpper", label: "Donchian upper", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "DonchianLower", label: "Donchian lower", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "DonchianMiddle", label: "Donchian middle", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "OBV", label: "On-balance volume", params: &[], is_oscillator: false },
        IndicatorSpec { name: "MFI", label: "Money flow index", params: &[("period", 14.0)], is_oscillator: true },
    ]
}

/// Binary comparators suitable for the generator.
pub const BINARY_COMPARATORS: &[Comparator] = &[
    Comparator::Gt, Comparator::Lt, Comparator::CrossAbove, Comparator::CrossBelow,
];

/// Unary comparators suitable for the generator.
pub const UNARY_COMPARATORS: &[Comparator] = &[Comparator::Rising, Comparator::Falling];

/// Price fields the generator may use.
pub const PRICE_FIELDS: &[PriceField] = &[
    PriceField::Close, PriceField::Open, PriceField::High, PriceField::Low, PriceField::Typical,
];

/// Entry order types.
pub const ORDER_TYPES: &[OrderType] = &[
    OrderType::EnterAtMarket, OrderType::EnterAtLimit, OrderType::EnterAtStop,
];

/// Exit / trade-management types.
pub const EXIT_TYPES: &[ExitType] = &[
    ExitType::TrailingStop,
    ExitType::MoveToBreakEven,
    ExitType::ExitAfterBars(5),
];
