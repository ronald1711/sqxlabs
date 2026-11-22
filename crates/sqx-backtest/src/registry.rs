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
        _ => return None,
    };
    Some(res)
}
