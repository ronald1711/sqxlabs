# Indicator reference evidence index

Generated with `python3 scripts/indicator_evidence_index.py`; verify freshness with `--check`.

All 179 registered indicator names are listed. 35 have a reviewed reference link; 144 remain unindexed. These are inventory counts, not validation percentages.

A link identifies an existing test, not a successful test run. Rust `ta` and `ta-lib` are the packages pinned in Cargo.lock; this index does not imply execution of TA-Lib C or MT5.

Unindexed means evidence has not been reviewed here, not that no tests exist. Every row remains incomplete: parameter extremes, missing/nonfinite input, seed and warmup conventions, causal prefixes, and strategy-dispatch/default-parameter wiring need their own evidence. A function tested with period 2 does not validate its catalog default of 14. Local algebraic consistency tests are not promoted to independent comparisons.

| Registered indicator | Evidence status | Exercised scope / source |
| --- | --- | --- |
| SMA | Partial reference indexed | Rust ta-lib; periods 3/10/20; SMA seeds and warmup; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L53) |
| EMA | Partial reference indexed | Rust ta-lib; periods 3/10/20; SMA seeds and warmup; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L53) |
| SMMA | Not indexed | Review formula, conventions and independent reference |
| LWMA | Not indexed | Review formula, conventions and independent reference |
| HMA | Not indexed | Review formula, conventions and independent reference |
| RSI | Partial reference indexed | Rust ta-lib; periods 2/14; non-flat Wilder case; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L68) |
| CCI | Partial reference indexed | Rust ta-lib; periods 14/20; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L131) |
| StochK | Partial reference indexed | Rust ta-lib; K14/D3; comparison aligns to later D warmup; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L153) |
| StochD | Partial reference indexed | Rust ta-lib; K14/D3; comparison aligns to later D warmup; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L153) |
| MACD | Partial reference indexed | Fixed rational values; 2/3/2 SMA seeds; first signal and future-prefix invariance; [test](crates/sqx-indicators/tests/reference.rs#L29) |
| MACDSignal | Partial reference indexed | Fixed rational values; 2/3/2 SMA seeds; first signal and future-prefix invariance; [test](crates/sqx-indicators/tests/reference.rs#L29) |
| ATR | Partial reference indexed | Rust ta-lib; periods 14/20; Wilder smoothing; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L79) |
| ADX | Partial reference indexed | Converged tail only, period 14; seed/warmup NOT established; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L98) |
| DIPlus | Partial reference indexed | Converged tail only, period 14; seed/warmup NOT established; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L98) |
| DIMinus | Partial reference indexed | Converged tail only, period 14; seed/warmup NOT established; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L98) |
| Momentum | Partial reference indexed | Fixed arithmetic; period 2; [test](crates/sqx-indicators/tests/reference.rs#L86) |
| ROC | Partial reference indexed | Rust ta-lib; selected periods; first-bar TR is a local convention; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L218) |
| WilliamsR | Partial reference indexed | Rust ta-lib; selected periods; first-bar TR is a local convention; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L218) |
| BBUpper | Partial reference indexed | Rust ta-lib; period 20, multiplier 2; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L201) |
| BBMiddle | Partial reference indexed | Rust ta-lib; period 20, multiplier 2; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L201) |
| BBLower | Partial reference indexed | Rust ta-lib; period 20, multiplier 2; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L201) |
| KCUpper | Not indexed | Review formula, conventions and independent reference |
| KCMiddle | Not indexed | Review formula, conventions and independent reference |
| KCLower | Not indexed | Review formula, conventions and independent reference |
| StdDev | Partial reference indexed | Rust ta-lib; selected periods; first-bar TR is a local convention; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L218) |
| Highest | Partial reference indexed | Rust ta; period 20, defined tail only; [test](crates/sqx-indicators/tests/ta_crosscheck.rs#L118) |
| Lowest | Partial reference indexed | Rust ta; period 20, defined tail only; [test](crates/sqx-indicators/tests/ta_crosscheck.rs#L118) |
| LinReg | Not indexed | Review formula, conventions and independent reference |
| LinRegSlope | Not indexed | Review formula, conventions and independent reference |
| AroonUp | Partial reference indexed | Rust ta-lib; periods 14/25; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L164) |
| AroonDown | Partial reference indexed | Rust ta-lib; periods 14/25; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L164) |
| AwesomeOsc | Not indexed | Review formula, conventions and independent reference |
| BullsPower | Not indexed | Review formula, conventions and independent reference |
| BearsPower | Not indexed | Review formula, conventions and independent reference |
| DeMarker | Partial reference indexed | Fixed arithmetic; periods 1/2; N+1-bar warmup; local flat neutral value; [test](crates/sqx-indicators/tests/reference.rs#L181) |
| ParabolicSAR | Not indexed | Review formula, conventions and independent reference |
| SuperTrend | Not indexed | Review formula, conventions and independent reference |
| VWAP | Not indexed | Review formula, conventions and independent reference |
| TrueRange | Partial reference indexed | Rust ta-lib; selected periods; first-bar TR is a local convention; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L218) |
| IchimokuTenkan | Not indexed | Review formula, conventions and independent reference |
| IchimokuKijun | Not indexed | Review formula, conventions and independent reference |
| IchimokuSenkouA | Not indexed | Review formula, conventions and independent reference |
| IchimokuSenkouB | Not indexed | Review formula, conventions and independent reference |
| VortexPlus | Partial reference indexed | Fixed arithmetic; periods 1/2/3; movement warmup; [test](crates/sqx-indicators/tests/reference.rs#L100) |
| VortexMinus | Partial reference indexed | Fixed arithmetic; periods 1/2/3; movement warmup; [test](crates/sqx-indicators/tests/reference.rs#L100) |
| KaufmanER | Partial reference indexed | Rust ta; period 14, defined tail only; [test](crates/sqx-indicators/tests/ta_crosscheck.rs#L131) |
| GannHiLo | Not indexed | Review formula, conventions and independent reference |
| GannHiLoTrend | Not indexed | Review formula, conventions and independent reference |
| AroonOsc | Not indexed | Review formula, conventions and independent reference |
| StochRSI | Not indexed | Review formula, conventions and independent reference |
| DonchianUpper | Not indexed | Review formula, conventions and independent reference |
| DonchianLower | Not indexed | Review formula, conventions and independent reference |
| DonchianMiddle | Not indexed | Review formula, conventions and independent reference |
| OBV | Partial reference indexed | Rust ta-lib; selected periods; first-bar TR is a local convention; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L218) |
| MFI | Partial reference indexed | Fixed arithmetic; periods 1/2; classified-flow warmup; [test](crates/sqx-indicators/tests/reference.rs#L123) |
| TRIX | Not indexed | Review formula, conventions and independent reference |
| DPO | Partial reference indexed | Rust ta-lib; periods 4/20; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L120) |
| AD | Not indexed | Review formula, conventions and independent reference |
| ChaikinOsc | Not indexed | Review formula, conventions and independent reference |
| CMF | Not indexed | Review formula, conventions and independent reference |
| ForceIndex | Not indexed | Review formula, conventions and independent reference |
| MassIndex | Not indexed | Review formula, conventions and independent reference |
| UltimateOsc | Not indexed | Review formula, conventions and independent reference |
| CMO | Partial reference indexed | Fixed arithmetic; rolling period 3; not Wilder variant; [test](crates/sqx-indicators/tests/reference.rs#L136) |
| TSI | Not indexed | Review formula, conventions and independent reference |
| RVI | Not indexed | Review formula, conventions and independent reference |
| Coppock | Not indexed | Review formula, conventions and independent reference |
| AccelOsc | Not indexed | Review formula, conventions and independent reference |
| HeikinAshiClose | Not indexed | Review formula, conventions and independent reference |
| PivotP | Not indexed | Review formula, conventions and independent reference |
| UlcerIndex | Not indexed | Review formula, conventions and independent reference |
| RSquared | Not indexed | Review formula, conventions and independent reference |
| Percentile | Not indexed | Review formula, conventions and independent reference |
| Median | Not indexed | Review formula, conventions and independent reference |
| Skewness | Not indexed | Review formula, conventions and independent reference |
| PVT | Not indexed | Review formula, conventions and independent reference |
| Fisher | Not indexed | Review formula, conventions and independent reference |
| SMI | Not indexed | Review formula, conventions and independent reference |
| AlligatorJaw | Not indexed | Review formula, conventions and independent reference |
| AlligatorTeeth | Not indexed | Review formula, conventions and independent reference |
| AlligatorLips | Not indexed | Review formula, conventions and independent reference |
| GatorUpper | Not indexed | Review formula, conventions and independent reference |
| GatorLower | Not indexed | Review formula, conventions and independent reference |
| NVI | Not indexed | Review formula, conventions and independent reference |
| PVI | Not indexed | Review formula, conventions and independent reference |
| EOM | Not indexed | Review formula, conventions and independent reference |
| VHF | Not indexed | Review formula, conventions and independent reference |
| ChandelierLong | Not indexed | Review formula, conventions and independent reference |
| ChandelierShort | Not indexed | Review formula, conventions and independent reference |
| TwiggsMF | Not indexed | Review formula, conventions and independent reference |
| VolumeOsc | Not indexed | Review formula, conventions and independent reference |
| BOP | Not indexed | Review formula, conventions and independent reference |
| Bias | Not indexed | Review formula, conventions and independent reference |
| Disparity | Not indexed | Review formula, conventions and independent reference |
| PPO | Partial reference indexed | Rust ta-lib; selected periods; first-bar TR is a local convention; [test](crates/sqx-indicators/tests/talib_crosscheck.rs#L218) |
| BBPercentB | Not indexed | Review formula, conventions and independent reference |
| BBBandwidth | Not indexed | Review formula, conventions and independent reference |
| ZScore | Not indexed | Review formula, conventions and independent reference |
| ChaikinVolatility | Not indexed | Review formula, conventions and independent reference |
| PsychologicalLine | Not indexed | Review formula, conventions and independent reference |
| CenterOfGravity | Not indexed | Review formula, conventions and independent reference |
| Klinger | Not indexed | Review formula, conventions and independent reference |
| KST | Not indexed | Review formula, conventions and independent reference |
| RMI | Not indexed | Review formula, conventions and independent reference |
| TII | Partial reference indexed | Fixed arithmetic; periods 2/30; current baseline and 2:1 mapping; [test](crates/sqx-indicators/tests/reference.rs#L164) |
| VWMA | Not indexed | Review formula, conventions and independent reference |
| PivotR1 | Not indexed | Review formula, conventions and independent reference |
| PivotS1 | Not indexed | Review formula, conventions and independent reference |
| PriceChannelUpper | Not indexed | Review formula, conventions and independent reference |
| PriceChannelLower | Not indexed | Review formula, conventions and independent reference |
| Slope | Not indexed | Review formula, conventions and independent reference |
| MACDHist | Partial reference indexed | Fixed rational values; 2/3/2 SMA seeds; first signal and future-prefix invariance; [test](crates/sqx-indicators/tests/reference.rs#L29) |
| SMAEnvelopeUpper | Not indexed | Review formula, conventions and independent reference |
| SMAEnvelopeLower | Not indexed | Review formula, conventions and independent reference |
| ATRBandsUpper | Not indexed | Review formula, conventions and independent reference |
| ATRBandsLower | Not indexed | Review formula, conventions and independent reference |
| PivotR2 | Not indexed | Review formula, conventions and independent reference |
| PivotS2 | Not indexed | Review formula, conventions and independent reference |
| ChandeForecast | Not indexed | Review formula, conventions and independent reference |
| LaguerreRSI | Not indexed | Review formula, conventions and independent reference |
| UpDownVolume | Not indexed | Review formula, conventions and independent reference |
| Choppiness | Not indexed | Review formula, conventions and independent reference |
| PivotR3 | Not indexed | Review formula, conventions and independent reference |
| PivotS3 | Not indexed | Review formula, conventions and independent reference |
| Fib382 | Not indexed | Review formula, conventions and independent reference |
| Fib500 | Not indexed | Review formula, conventions and independent reference |
| Fib618 | Not indexed | Review formula, conventions and independent reference |
| VolumeRatio | Not indexed | Review formula, conventions and independent reference |
| PriceVolume | Not indexed | Review formula, conventions and independent reference |
| ForceRaw | Not indexed | Review formula, conventions and independent reference |
| TypicalPrice | Not indexed | Review formula, conventions and independent reference |
| Kurtosis | Not indexed | Review formula, conventions and independent reference |
| ATRPercent | Not indexed | Review formula, conventions and independent reference |
| RangeExpansion | Not indexed | Review formula, conventions and independent reference |
| CloseLocation | Not indexed | Review formula, conventions and independent reference |
| HighLowRange | Not indexed | Review formula, conventions and independent reference |
| OpenCloseRange | Not indexed | Review formula, conventions and independent reference |
| PriceChannelMiddle | Not indexed | Review formula, conventions and independent reference |
| Autocorrelation | Not indexed | Review formula, conventions and independent reference |
| Variance | Not indexed | Review formula, conventions and independent reference |
| MAD | Not indexed | Review formula, conventions and independent reference |
| IQR | Not indexed | Review formula, conventions and independent reference |
| LinRegForecast | Not indexed | Review formula, conventions and independent reference |
| HeikinAshiOpen | Not indexed | Review formula, conventions and independent reference |
| HeikinAshiHigh | Not indexed | Review formula, conventions and independent reference |
| HeikinAshiLow | Not indexed | Review formula, conventions and independent reference |
| ADR | Not indexed | Review formula, conventions and independent reference |
| RangeRatio | Not indexed | Review formula, conventions and independent reference |
| TrueRangePct | Not indexed | Review formula, conventions and independent reference |
| ChandeKrollLong | Not indexed | Review formula, conventions and independent reference |
| ChandeKrollShort | Not indexed | Review formula, conventions and independent reference |
| Hurst | Not indexed | Review formula, conventions and independent reference |
| FractalDimension | Not indexed | Review formula, conventions and independent reference |
| ShannonEntropy | Not indexed | Review formula, conventions and independent reference |
| ElderImpulse | Not indexed | Review formula, conventions and independent reference |
| MeanDeviation | Not indexed | Review formula, conventions and independent reference |
| MomentumSmooth | Not indexed | Review formula, conventions and independent reference |
| ROCSmooth | Not indexed | Review formula, conventions and independent reference |
| MedianPrice | Not indexed | Review formula, conventions and independent reference |
| WeightedClose | Not indexed | Review formula, conventions and independent reference |
| SqueezeMomentum | Not indexed | Review formula, conventions and independent reference |
| WaveTrend | Not indexed | Review formula, conventions and independent reference |
| PercentRank | Not indexed | Review formula, conventions and independent reference |
| KeltnerPosition | Not indexed | Review formula, conventions and independent reference |
| DonchianPosition | Not indexed | Review formula, conventions and independent reference |
| TrendQuality | Not indexed | Review formula, conventions and independent reference |
| Acceleration | Not indexed | Review formula, conventions and independent reference |
| Jerk | Not indexed | Review formula, conventions and independent reference |
| PriceVelocity | Not indexed | Review formula, conventions and independent reference |
| QQE | Not indexed | Review formula, conventions and independent reference |
| QQEUpper | Not indexed | Review formula, conventions and independent reference |
| QQELower | Not indexed | Review formula, conventions and independent reference |
| ConnorsRSI | Not indexed | Review formula, conventions and independent reference |
| Fib1272 | Not indexed | Review formula, conventions and independent reference |
| Fib1618 | Not indexed | Review formula, conventions and independent reference |
| CamarillaH3 | Not indexed | Review formula, conventions and independent reference |
| CamarillaH4 | Not indexed | Review formula, conventions and independent reference |
| CamarillaL3 | Not indexed | Review formula, conventions and independent reference |
| CamarillaL4 | Not indexed | Review formula, conventions and independent reference |
