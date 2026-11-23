//! Explicit account-currency conversion and broker account constraints.
use serde::{Deserialize, Serialize};
use sqx_core::{Instrument, Timestamp};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionRate {
    pub time: Timestamp,
    pub from: String,
    pub to: String,
    /// Units of `to` per unit of `from`, available at `time`.
    pub rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccountConfig {
    pub currency: String,
    pub max_leverage: f64,
    /// Liquidate when equity / required margin reaches this fraction.
    pub stop_out_ratio: f64,
    pub min_lots: f64,
    pub lot_step: f64,
    pub max_lots: f64,
    pub conversion_rates: Vec<ConversionRate>,
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
            currency: "USD".into(), max_leverage: 30.0, stop_out_ratio: 0.5,
            min_lots: 0.01, lot_step: 0.01, max_lots: 100.0,
            conversion_rates: Vec::new(), max_conversion_age_ms: 86_400_000,
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
        Ok(())
    }

    /// Direct-pair conversion uses the contemporaneous execution/mark price.
    /// Crosses require explicit historical conversion observations; never future rates.
    pub fn quote_rate(&self, instrument: &Instrument, time: Timestamp, price: f64) -> Result<f64, String> {
        if !price.is_finite() || price <= 0.0 { return Err("invalid conversion price".into()); }
        if instrument.quote_currency == self.currency { return Ok(1.0); }
        if instrument.base_currency == self.currency { return Ok(1.0 / price); }
        self.conversion_rates.iter()
            .filter(|r| r.from == instrument.quote_currency && r.to == self.currency && r.time <= time
                && time.saturating_sub(r.time) <= self.max_conversion_age_ms)
            .max_by_key(|r| r.time).map(|r| r.rate)
            .ok_or_else(|| format!("missing or stale {}->{} conversion at {}", instrument.quote_currency, self.currency, time))
    }

    pub fn lots(&self, requested: f64) -> Option<f64> {
        if !requested.is_finite() || requested < self.min_lots || requested > self.max_lots { return None; }
        let rounded = (requested / self.lot_step + 1e-10).floor() * self.lot_step;
        (rounded >= self.min_lots).then_some(rounded)
    }
}
