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
        IndicatorSpec { name: "TRIX", label: "TRIX", params: &[("period", 15.0)], is_oscillator: true },
        IndicatorSpec { name: "DPO", label: "Detrended price oscillator", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "AD", label: "Accumulation/Distribution", params: &[], is_oscillator: false },
        IndicatorSpec { name: "ChaikinOsc", label: "Chaikin oscillator", params: &[], is_oscillator: true },
        IndicatorSpec { name: "CMF", label: "Chaikin money flow", params: &[("period", 20.0)], is_oscillator: true },
        IndicatorSpec { name: "ForceIndex", label: "Force index", params: &[("period", 13.0)], is_oscillator: true },
        IndicatorSpec { name: "MassIndex", label: "Mass index", params: &[], is_oscillator: false },
        IndicatorSpec { name: "UltimateOsc", label: "Ultimate oscillator", params: &[], is_oscillator: true },
        IndicatorSpec { name: "CMO", label: "Chande momentum", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "TSI", label: "True strength index", params: &[], is_oscillator: true },
        IndicatorSpec { name: "RVI", label: "Relative vigor index", params: &[("period", 10.0)], is_oscillator: true },
        IndicatorSpec { name: "Coppock", label: "Coppock curve", params: &[], is_oscillator: true },
        IndicatorSpec { name: "AccelOsc", label: "Accelerator oscillator", params: &[], is_oscillator: true },
        IndicatorSpec { name: "HeikinAshiClose", label: "Heikin Ashi close", params: &[], is_oscillator: false },
        IndicatorSpec { name: "PivotP", label: "Pivot point", params: &[], is_oscillator: false },
        IndicatorSpec { name: "UlcerIndex", label: "Ulcer index", params: &[("period", 14.0)], is_oscillator: false },
        IndicatorSpec { name: "RSquared", label: "R-squared", params: &[("period", 20.0)], is_oscillator: true },
        IndicatorSpec { name: "Percentile", label: "Rolling percentile", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "Median", label: "Rolling median", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "Skewness", label: "Skewness", params: &[("period", 20.0)], is_oscillator: true },
        IndicatorSpec { name: "PVT", label: "Price volume trend", params: &[], is_oscillator: false },
        IndicatorSpec { name: "Fisher", label: "Fisher transform", params: &[("period", 9.0)], is_oscillator: true },
        IndicatorSpec { name: "SMI", label: "Stochastic momentum index", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "AlligatorJaw", label: "Alligator jaw", params: &[], is_oscillator: false },
        IndicatorSpec { name: "AlligatorTeeth", label: "Alligator teeth", params: &[], is_oscillator: false },
        IndicatorSpec { name: "AlligatorLips", label: "Alligator lips", params: &[], is_oscillator: false },
        IndicatorSpec { name: "GatorUpper", label: "Gator upper", params: &[], is_oscillator: true },
        IndicatorSpec { name: "GatorLower", label: "Gator lower", params: &[], is_oscillator: true },
        IndicatorSpec { name: "NVI", label: "Negative volume index", params: &[], is_oscillator: false },
        IndicatorSpec { name: "PVI", label: "Positive volume index", params: &[], is_oscillator: false },
        IndicatorSpec { name: "EOM", label: "Ease of movement", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "VHF", label: "Vertical horizontal filter", params: &[("period", 28.0)], is_oscillator: true },
        IndicatorSpec { name: "ChandelierLong", label: "Chandelier exit (long)", params: &[("period", 22.0)], is_oscillator: false },
        IndicatorSpec { name: "ChandelierShort", label: "Chandelier exit (short)", params: &[("period", 22.0)], is_oscillator: false },
        IndicatorSpec { name: "TwiggsMF", label: "Twiggs money flow", params: &[("period", 20.0)], is_oscillator: true },
        IndicatorSpec { name: "VolumeOsc", label: "Volume oscillator", params: &[], is_oscillator: true },
        IndicatorSpec { name: "BOP", label: "Balance of power", params: &[], is_oscillator: true },
        IndicatorSpec { name: "Bias", label: "Bias", params: &[("period", 20.0)], is_oscillator: true },
        IndicatorSpec { name: "Disparity", label: "Disparity index", params: &[("period", 14.0)], is_oscillator: false },
        IndicatorSpec { name: "PPO", label: "Percentage price oscillator", params: &[], is_oscillator: true },
        IndicatorSpec { name: "BBPercentB", label: "Bollinger %B", params: &[("period", 20.0)], is_oscillator: true },
        IndicatorSpec { name: "BBBandwidth", label: "Bollinger bandwidth", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "ZScore", label: "Z-score", params: &[("period", 20.0)], is_oscillator: true },
        IndicatorSpec { name: "ChaikinVolatility", label: "Chaikin volatility", params: &[("period", 10.0)], is_oscillator: true },
        IndicatorSpec { name: "PsychologicalLine", label: "Psychological line", params: &[("period", 12.0)], is_oscillator: true },
        IndicatorSpec { name: "CenterOfGravity", label: "Center of gravity", params: &[("period", 10.0)], is_oscillator: true },
        IndicatorSpec { name: "Klinger", label: "Klinger volume oscillator", params: &[], is_oscillator: true },
        IndicatorSpec { name: "KST", label: "Know sure thing", params: &[], is_oscillator: true },
        IndicatorSpec { name: "RMI", label: "Relative momentum index", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "TII", label: "Trend intensity index", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "VWMA", label: "Volume-weighted MA", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "PivotR1", label: "Pivot R1", params: &[], is_oscillator: false },
        IndicatorSpec { name: "PivotS1", label: "Pivot S1", params: &[], is_oscillator: false },
        IndicatorSpec { name: "LinRegSlope", label: "Linear regression slope", params: &[("period", 14.0)], is_oscillator: true },
        IndicatorSpec { name: "PriceChannelUpper", label: "Price channel upper", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "PriceChannelLower", label: "Price channel lower", params: &[("period", 20.0)], is_oscillator: false },
        IndicatorSpec { name: "Slope", label: "SMA slope", params: &[("period", 14.0)], is_oscillator: true },
    ]
}

/// A selectable building block: a base indicator, or a signal variant of it.
#[derive(Debug, Clone)]
pub struct BlockSpec {
    pub name: String,
    pub base: String,
    /// Some(comparator) for signal variants (e.g. RSI Rising); None for the base indicator.
    pub cmp: Option<Comparator>,
    pub params: Vec<(&'static str, f64)>,
    pub is_oscillator: bool,
}

/// The full block catalog used by the generator: every base indicator plus its
/// Rising/Falling signal variants.
pub fn block_catalog() -> Vec<BlockSpec> {
    let mut out = Vec::new();
    for spec in indicator_catalog() {
        out.push(BlockSpec {
            name: spec.name.to_string(),
            base: spec.name.to_string(),
            cmp: None,
            params: spec.params.to_vec(),
            is_oscillator: spec.is_oscillator,
        });
        for (suffix, cmp) in [("Rising", Comparator::Rising), ("Falling", Comparator::Falling)] {
            out.push(BlockSpec {
                name: format!("{}{}", spec.name, suffix),
                base: spec.name.to_string(),
                cmp: Some(cmp),
                params: spec.params.to_vec(),
                is_oscillator: spec.is_oscillator,
            });
        }
    }
    out
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
