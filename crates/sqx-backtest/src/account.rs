//! Explicit account-currency conversion and broker account constraints.
use serde::{Deserialize, Serialize};
use sqx_core::{Direction, Instrument, Timestamp};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionRate {
    pub time: Timestamp,
    pub from: String,
    pub to: String,
    /// Units of `to` per unit of `from`, available at `time`.
    pub rate: f64,
}

/// Explicit margin convention. ForexBase is for declared forex base-unit margin,
/// not CFDs, futures or exchange margin schedules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum MarginModel {
    #[default]
    QuoteNotional,
    ForexBase,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum ProfitConversionModel {
    #[default]
    SingleRate,
    Directional,
}

/// Historical units of `to` per unit of `from`, by position direction.
/// Rates are supplied explicitly; no inferred live FX or future observation is used.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectionalConversionRate {
    pub time: Timestamp,
    pub from: String,
    pub to: String,
    pub long_rate: f64,
    pub short_rate: f64,
}

pub type MarginConversionRate = DirectionalConversionRate;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccountConfig {
    pub currency: String,
    pub max_leverage: f64,
    pub margin_model: MarginModel,
    pub margin_conversion_rates: Vec<MarginConversionRate>,
    /// Liquidate when equity / required margin reaches this fraction.
    pub stop_out_ratio: f64,
    pub min_lots: f64,
    pub lot_step: f64,
    pub max_lots: f64,
    pub conversion_rates: Vec<ConversionRate>,
    pub profit_conversion_model: ProfitConversionModel,
    pub profit_conversion_rates: Vec<DirectionalConversionRate>,
    pub max_conversion_age_ms: i64,
    /// Signed account-currency cost per lot per UTC rollover (negative = credit).
    pub long_financing_per_lot: f64,
    pub short_financing_per_lot: f64,
    /// Monday=0, Wednesday=2. None means no triple rollover.
    pub triple_rollover_weekday: Option<u8>,
}

impl Default for AccountConfig {
    fn default() -> Self {
        Self {
            currency: "USD".into(), max_leverage: 30.0,
            margin_model: MarginModel::QuoteNotional, margin_conversion_rates: Vec::new(), stop_out_ratio: 0.5,
            min_lots: 0.01, lot_step: 0.01, max_lots: 100.0,
            conversion_rates: Vec::new(), profit_conversion_model: ProfitConversionModel::SingleRate, profit_conversion_rates: Vec::new(), max_conversion_age_ms: 86_400_000,
            long_financing_per_lot: 0.0, short_financing_per_lot: 0.0,
            triple_rollover_weekday: None,
        }
    }
}

impl AccountConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.currency.len() != 3 || !self.currency.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err("account currency must be a three-letter uppercase code".into());
        }
        if !self.max_leverage.is_finite() || self.max_leverage < 1.0
            || !self.stop_out_ratio.is_finite() || !(0.0..=1.0).contains(&self.stop_out_ratio)
            || !self.min_lots.is_finite() || self.min_lots <= 0.0
            || !self.lot_step.is_finite() || self.lot_step <= 0.0
            || !self.max_lots.is_finite() || self.max_lots < self.min_lots
            || self.max_conversion_age_ms < 0
            || !self.long_financing_per_lot.is_finite() || !self.short_financing_per_lot.is_finite()
            || self.triple_rollover_weekday.is_some_and(|d| d > 6) {
            return Err("invalid account, margin, volume or financing configuration".into());
        }
        if self.conversion_rates.iter().any(|r| !r.rate.is_finite() || r.rate <= 0.0) {
            return Err("conversion rates must be finite and positive".into());
        }
        for (label,rates) in [("margin",&self.margin_conversion_rates),("profit",&self.profit_conversion_rates)] {
            if rates.iter().any(|r| !r.long_rate.is_finite() || r.long_rate<=0.0 || !r.short_rate.is_finite() || r.short_rate<=0.0
                || r.from.len()!=3 || r.to.len()!=3 || !r.from.bytes().chain(r.to.bytes()).all(|b|b.is_ascii_uppercase())) {
                return Err(format!("invalid {label} conversion observation"));
            }
            let mut keys=std::collections::HashSet::new();
            if rates.iter().any(|r|!keys.insert((&r.from,&r.to,r.time))) {
                return Err(format!("duplicate {label} conversion observation"));
            }
        }
        Ok(())
    }

    /// Direct-pair conversion uses the contemporaneous execution/mark price.
    /// Crosses require explicit historical conversion observations; never future rates.
    pub fn quote_rate(&self, instrument: &Instrument, time: Timestamp, price: f64) -> Result<f64, String> {
        if !price.is_finite() || price <= 0.0 { return Err("invalid conversion price".into()); }
        if instrument.quote_currency == self.currency { return Ok(1.0); }
        if instrument.base_currency == self.currency { return Ok(1.0 / price); }
        if self.profit_conversion_model==ProfitConversionModel::Directional {return Err("direction required for directional profit conversion".into());}
        self.conversion_rates.iter()
            .filter(|r| r.from == instrument.quote_currency && r.to == self.currency && r.time <= time
                && time.saturating_sub(r.time) <= self.max_conversion_age_ms)
            .max_by_key(|r| r.time).map(|r| r.rate)
            .ok_or_else(|| format!("missing or stale {}->{} conversion at {}", instrument.quote_currency, self.currency, time))
    }

    /// Direction selects an explicit historical conversion observation; never an
    /// inferred bid/ask convention or the sign of unrealized/realized profit.
    pub fn profit_rate(&self, instrument:&Instrument, time:Timestamp, price:f64, direction:Direction)->Result<f64,String> {
        if !price.is_finite() || price<=0.0 {return Err("invalid conversion price".into());}
        if instrument.quote_currency==self.currency {return Ok(1.0);}
        if instrument.base_currency==self.currency {return Ok(1.0/price);}
        if self.profit_conversion_model==ProfitConversionModel::SingleRate {return self.quote_rate(instrument,time,price);}
        self.profit_conversion_rates.iter()
            .filter(|r|r.from==instrument.quote_currency && r.to==self.currency && r.time<=time && time.saturating_sub(r.time)<=self.max_conversion_age_ms)
            .max_by_key(|r|r.time)
            .map(|r|if direction==Direction::Long {r.long_rate}else{r.short_rate})
            .ok_or_else(||format!("missing or stale {}->{} directional profit conversion at {}",instrument.quote_currency,self.currency,time))
    }

    /// Account-currency notional per base unit used for margin, separately from P&L.
    pub fn margin_base_rate(&self, instrument:&Instrument, time:Timestamp, price:f64, direction:Direction)->Result<f64,String> {
        if !price.is_finite() || price<=0.0 {return Err("invalid margin valuation price".into());}
        if self.margin_model==MarginModel::QuoteNotional {return Ok(price*self.profit_rate(instrument,time,price,direction)?);}
        if instrument.base_currency==self.currency {return Ok(1.0);}
        if instrument.quote_currency==self.currency {return Ok(price);}
        self.margin_conversion_rates.iter()
            .filter(|r|r.from==instrument.base_currency && r.to==self.currency && r.time<=time && time.saturating_sub(r.time)<=self.max_conversion_age_ms)
            .max_by_key(|r|r.time)
            .map(|r|if direction==Direction::Long {r.long_rate}else{r.short_rate})
            .ok_or_else(||format!("missing or stale {}->{} margin conversion at {}",instrument.base_currency,self.currency,time))
    }

    pub fn lots(&self, requested: f64) -> Option<f64> {
        if !requested.is_finite() || requested < self.min_lots || requested > self.max_lots { return None; }
        let rounded = (requested / self.lot_step + 1e-10).floor() * self.lot_step;
        (rounded >= self.min_lots).then_some(rounded)
    }
}
