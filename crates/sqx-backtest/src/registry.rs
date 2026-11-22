//! Maps an indicator name + params to a computed series.

use sqx_core::Bar;
use sqx_indicators as ind;

fn p(params: &[sqx_strategy::Param], name: &str) -> Option<f64> {
    params.iter().find(|p| p.name == name).map(|p| p.value)
}

fn period(params: &[sqx_strategy::Param], default: usize) -> usize {
    p(params, "period")
        .map(|v| v.round() as usize)
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

/// Compute a named indicator series, or None if the name is unknown.
pub fn compute(name: &str, params: &[sqx_strategy::Param], bars: &[Bar]) -> Option<ind::Series> {
    let c = ind::closes(bars);
    let h = ind::highs(bars);
    let l = ind::lows(bars);
    let o = ind::opens(bars);
    let res = match name {
        "SMA" => ind::sma(&c, period(params, 20)),
        "EMA" => ind::ema(&c, period(params, 20)),
        "SMMA" => ind::smma(&c, period(params, 20)),
        "LWMA" => ind::lwma(&c, period(params, 20)),
        "HMA" => ind::hma(&c, period(params, 20)),
        "KAMA" => ind::kama(&c, period(params, 10)),
        "TEMA" => ind::tema(&c, period(params, 20)),
        "RSI" => ind::rsi(&c, period(params, 14)),
        "CCI" => ind::cci(&h, &l, &c, period(params, 14)),
        "StochK" => ind::stoch_k(&h, &l, &c, period(params, 14)),
        "StochD" => ind::stoch_d(&ind::stoch_k(&h, &l, &c, 14), period(params, 3)),
        "MACD" => ind::macd(
            &c,
            p(params, "fast").unwrap_or(12.0).round() as usize,
            p(params, "slow").unwrap_or(26.0).round() as usize,
        ),
        "MACDSignal" => {
            let fast = p(params, "fast").unwrap_or(12.0).round() as usize;
            let slow = p(params, "slow").unwrap_or(26.0).round() as usize;
            let sig = p(params, "signal").unwrap_or(9.0).round() as usize;
            let line = ind::macd(&c, fast, slow);
            ind::macd_signal(&line, sig)
        }
        "ATR" => ind::atr(bars, period(params, 14)),
        "ADX" => {
            let (_, _, a) = ind::adx(bars, period(params, 14));
            a
        }
        "DIPlus" => {
            let (a, _, _) = ind::adx(bars, period(params, 14));
            a
        }
        "DIMinus" => {
            let (_, b, _) = ind::adx(bars, period(params, 14));
            b
        }
        "Momentum" => ind::momentum(&c, period(params, 10)),
        "ROC" => ind::roc(&c, period(params, 10)),
        "WilliamsR" => ind::williams_r(&h, &l, &c, period(params, 14)),
        "BBUpper" => {
            let (u, _, _) = ind::bollinger(&c, period(params, 20), p(params, "mult").unwrap_or(2.0));
            u
        }
        "BBMiddle" => {
            let (_, m, _) = ind::bollinger(&c, period(params, 20), p(params, "mult").unwrap_or(2.0));
            m
        }
        "BBLower" => {
            let (_, _, d) = ind::bollinger(&c, period(params, 20), p(params, "mult").unwrap_or(2.0));
            d
        }
        "KCUpper" => {
            let (u, _, _) = ind::keltner(bars, period(params, 20), p(params, "mult").unwrap_or(2.0));
            u
        }
        "KCMiddle" => {
            let (_, m, _) = ind::keltner(bars, period(params, 20), p(params, "mult").unwrap_or(2.0));
            m
        }
        "KCLower" => {
            let (_, _, d) = ind::keltner(bars, period(params, 20), p(params, "mult").unwrap_or(2.0));
            d
        }
        "StdDev" => ind::stddev(&c, period(params, 20)),
        "Highest" => ind::highest(&h, period(params, 20)),
        "Lowest" => ind::lowest(&l, period(params, 20)),
        "LinReg" => {
            let (_, i) = ind::linreg(&c, period(params, 20));
            i
        }
        "LinRegSlope" => {
            let (s, _) = ind::linreg(&c, period(params, 20));
            s
        }
        "AroonUp" => {
            let (u, _) = ind::aroon(&h, &l, period(params, 25));
            u
        }
        "AroonDown" => {
            let (_, d) = ind::aroon(&h, &l, period(params, 25));
            d
        }
        "AwesomeOsc" => ind::awesome_oscillator(&h, &l),
        "BullsPower" => ind::bulls_power(&h, &c),
        "BearsPower" => ind::bears_power(&l, &c),
        "DeMarker" => ind::demarker(&h, &l, period(params, 14)),
        "ParabolicSAR" => ind::parabolic_sar(
            bars,
            p(params, "step").unwrap_or(0.02),
            p(params, "max").unwrap_or(0.2),
        ),
        "SuperTrend" => {
            let (_, s) = ind::supertrend(bars, period(params, 10), p(params, "mult").unwrap_or(3.0));
            s
        }
        "VWAP" => ind::vwap(bars),
        "TrueRange" => ind::true_range(bars),
        "IchimokuTenkan" => ind::ichimoku_tenkan(&h, &l),
        "IchimokuKijun" => ind::ichimoku_kijun(&h, &l),
        "IchimokuSenkouA" => ind::ichimoku_senkou_a(&h, &l),
        "IchimokuSenkouB" => ind::ichimoku_senkou_b(&h, &l),
        "VortexPlus" => { let (p, _) = ind::vortex(&h, &l, &c, period(params, 14)); p },
        "VortexMinus" => { let (_, m) = ind::vortex(&h, &l, &c, period(params, 14)); m },
        "KaufmanER" => ind::kaufman_er(&c, period(params, 14)),
        "GannHiLo" => ind::gann_hilo(&h, &l, &c, period(params, 20)),
        "GannHiLoTrend" => ind::gann_hilo_trend(&c, period(params, 20)),
        "AroonOsc" => ind::aroon_osc(&h, &l, period(params, 25)),
        "StochRSI" => ind::stoch_rsi(&c, 14, period(params, 14)),
        "DonchianUpper" => ind::donchian_upper(&h, period(params, 20)),
        "DonchianLower" => ind::donchian_lower(&l, period(params, 20)),
        "DonchianMiddle" => ind::donchian_middle(&h, &l, period(params, 20)),
        "OBV" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::obv(&c, &vol) },
        "MFI" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::mfi(&h, &l, &c, &vol, period(params, 14)) },
        "TRIX" => ind::trix(&c, period(params, 15)),
        "DPO" => ind::dpo(&c, period(params, 20)),
        "AD" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::ad_line(&h, &l, &c, &vol) },
        "ChaikinOsc" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::chaikin_osc(&h, &l, &c, &vol) },
        "CMF" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::cmf(&h, &l, &c, &vol, period(params, 20)) },
        "ForceIndex" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::force_index(&c, &vol, period(params, 13)) },
        "MassIndex" => ind::mass_index(&h, &l),
        "UltimateOsc" => ind::ultimate_osc(&h, &l, &c, 7, 14, 28),
        "CMO" => ind::cmo(&c, period(params, 14)),
        "TSI" => ind::tsi(&c, 25, 13),
        "RVI" => ind::rvi(&h, &l, &c, &o, period(params, 10)),
        "Coppock" => ind::coppock(&c),
        "AccelOsc" => ind::accel_osc(&h, &l),
        "HeikinAshiClose" => ind::heikin_ashi_close(&h, &l, &c, &o),
        "PivotP" => ind::pivot_p(&h, &l, &c),
        "UlcerIndex" => ind::ulcer_index(&c, period(params, 14)),
        "RSquared" => ind::r_squared(&c, period(params, 20)),
        "Percentile" => ind::percentile(&c, period(params, 20), 0.5),
        "Median" => ind::median(&c, period(params, 20)),
        "Skewness" => ind::skewness(&c, period(params, 20)),
        "PVT" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::pvt(&c, &vol) },
        "Fisher" => ind::fisher(&h, &l, period(params, 9)),
        "SMI" => ind::smi(&h, &l, &c, period(params, 14), 3),
        "AlligatorJaw" => ind::alligator_jaw(&c),
        "AlligatorTeeth" => ind::alligator_teeth(&c),
        "AlligatorLips" => ind::alligator_lips(&c),
        "GatorUpper" => ind::gator_upper(&c),
        "GatorLower" => ind::gator_lower(&c),
        "NVI" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::nvi(&c, &vol) },
        "PVI" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::pvi(&c, &vol) },
        "EOM" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::eom(&h, &l, &vol, period(params, 14)) },
        "VHF" => ind::vhf(&c, period(params, 28)),
        "ChandelierLong" => ind::chandelier_long(&h, &l, &c, period(params, 22), 3.0),
        "ChandelierShort" => ind::chandelier_short(&h, &l, &c, period(params, 22), 3.0),
        "TwiggsMF" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::twiggs_money_flow(&h, &l, &c, &vol, period(params, 20)) },
        "VolumeOsc" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::volume_osc(&vol, 5, 20) },
        "BOP" => ind::bop(&o, &h, &l, &c),
        "Bias" => ind::bias(&c, period(params, 20)),
        "Disparity" => ind::disparity(&c, period(params, 14)),
        "PPO" => ind::ppo(&c, 12, 26),
        "BBPercentB" => ind::bb_percent_b(&c, period(params, 20), 2.0),
        "BBBandwidth" => ind::bb_bandwidth(&c, period(params, 20), 2.0),
        "ZScore" => ind::zscore(&c, period(params, 20)),
        "ChaikinVolatility" => ind::chaikin_volatility(&h, &l, period(params, 10)),
        "PsychologicalLine" => ind::psychological_line(&c, period(params, 12)),
        "CenterOfGravity" => ind::center_of_gravity(&c, period(params, 10)),
        "Klinger" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::klinger(&h, &l, &c, &vol, 34, 55) },
        "KST" => ind::kst(&c),
        "RMI" => ind::rmi(&c, period(params, 14)),
        "TII" => ind::tii(&c, period(params, 14)),
        "VWMA" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::vwma(&c, &vol, period(params, 20)) },
        "PivotR1" => ind::pivot_r1(&h, &l, &c),
        "PivotS1" => ind::pivot_s1(&h, &l, &c),
        "PriceChannelUpper" => ind::price_channel_upper(&h, period(params, 20)),
        "PriceChannelLower" => ind::price_channel_lower(&l, period(params, 20)),
        "Slope" => ind::slope(&c, period(params, 14)),
        "MACDHist" => {
            let f = ind::ema(&c, 12);
            let s = ind::ema(&c, 26);
            let line: Vec<f64> = (0..c.len()).map(|i| f[i] - s[i]).collect();
            let sig = ind::ema(&line, 9);
            ind::macd_hist(&line, &sig)
        },
        "SMAEnvelopeUpper" => ind::sma_envelope_upper(&c, period(params, 20), 1.0),
        "SMAEnvelopeLower" => ind::sma_envelope_lower(&c, period(params, 20), 1.0),
        "ATRBandsUpper" => ind::atr_bands_upper(&h, &l, &c, period(params, 14), 2.0),
        "ATRBandsLower" => ind::atr_bands_lower(&h, &l, &c, period(params, 14), 2.0),
        "PivotR2" => ind::pivot_r2(&h, &l, &c),
        "PivotS2" => ind::pivot_s2(&h, &l, &c),
        "ChandeForecast" => ind::chande_forecast(&c, period(params, 14)),
        "LaguerreRSI" => ind::laguerre_rsi(&c, 0.7),
        "UpDownVolume" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::up_down_volume(&c, &vol, period(params, 14)) },
        "Choppiness" => ind::choppiness(&h, &l, &c, period(params, 14)),
        "PivotR3" => ind::pivot_r3(&h, &l, &c),
        "PivotS3" => ind::pivot_s3(&h, &l, &c),
        "Fib382" => ind::fib_382(&h, &l, period(params, 20)),
        "Fib500" => ind::fib_500(&h, &l, period(params, 20)),
        "Fib618" => ind::fib_618(&h, &l, period(params, 20)),
        "VolumeRatio" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::volume_ratio(&vol, period(params, 20)) },
        "PriceVolume" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::price_volume(&o, &c, &vol) },
        "ForceRaw" => { let vol: Vec<f64> = bars.iter().map(|b| b.volume).collect(); ind::force_raw(&c, &vol) },
        "TypicalPrice" => ind::typical_price(&h, &l, &c),
        _ => return None,
    };
    Some(res)
}
