//! Raw corporate/CD contract normalization and exact-time CSR schedules.
use crate::conventions::{year_fraction, Bdc, Calendar, CalendarSpec, Date, DayCount};
use portfolio_model_core::amortization::level_payment_principal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Product {
    Corporate,
    Cd,
}
#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Amortization {
    #[default]
    Bullet,
    /// Historical compatibility: equal principal, not equal total payments.
    Annuity,
    /// Fixed-coupon, equal total payment over the actual accrual schedule.
    LevelPayment,
    Sink,
}

fn cap_default() -> f64 {
    10.
}
fn floor_default() -> f64 {
    -10.
}
fn threshold_default() -> f64 {
    0.005
}
fn one() -> f64 {
    1.
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub maturity: i32,
    pub freq_months: Option<i32>,
    pub daycount: DayCount,
    pub coupon: f64,
    pub notional: f64,
    pub price: f64,
    #[serde(default)]
    pub is_float: bool,
    #[serde(default = "cap_default")]
    pub cap: f64,
    #[serde(default = "floor_default")]
    pub floor: f64,
    #[serde(default = "threshold_default")]
    pub call_threshold: f64,
    #[serde(default)]
    pub amort_type: Amortization,
    #[serde(default)]
    pub sink_schedule: Option<Vec<(i32, f64)>>,
    #[serde(default)]
    pub call_schedule: Option<Vec<(i32, f64)>>,
    #[serde(default)]
    pub put_schedule: Option<Vec<(i32, f64)>>,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub penalty_months: f64,
    #[serde(default = "one")]
    pub ew_mult: f64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeckRequest {
    pub product: Product,
    pub asof: i32,
    pub months: usize,
    pub calendar: CalendarSpec,
    #[serde(default)]
    pub bdc: Bdc,
    pub contracts: Vec<Contract>,
}
#[derive(Default, Serialize)]
pub struct Deck {
    pub per_off: Vec<f64>,
    pub pay_m: Vec<f64>,
    pub pay_frac: Vec<f64>,
    pub acc_m: Vec<f64>,
    pub t_pay: Vec<f64>,
    pub tau: Vec<f64>,
    pub fix_m: Vec<f64>,
    pub fix_w: Vec<f64>,
    pub prin: Vec<f64>,
    pub call_px: Vec<f64>,
    pub put_px: Vec<f64>,
    pub rem_y: Vec<f64>,
    pub is_float: Vec<f64>,
    pub cpn: Vec<f64>,
    pub cap: Vec<f64>,
    pub floor: Vec<f64>,
    pub call_thr: Vec<f64>,
    pub pen_m: Vec<f64>,
    pub ew_mult: Vec<f64>,
    pub notional: Vec<f64>,
    pub tgt: Vec<f64>,
    pub n: usize,
}
impl DeckRequest {
    pub fn build(&self) -> Result<Deck, String> {
        self.build_rows(&self.contracts)
    }
    pub(crate) fn build_rows(&self, contracts: &[Contract]) -> Result<Deck, String> {
        if !(2..=4096).contains(&self.months) {
            return Err("invalid term book or monthly grid".into());
        }
        let asof = Date::ordinal(self.asof)?;
        let mut calendar = Calendar::new(&self.calendar)?;
        let mut deck = Deck {
            per_off: vec![0.],
            n: contracts.len(),
            ..Default::default()
        };
        let terminal = self.months as f64 / 12. - 1e-9;
        let time = |d: Date| ((d.number() - asof.number()) as f64 / 365.).min(terminal);
        let month = |d: Date| ((time(d) * 12.) as i32).min(self.months as i32 - 1);
        for r in contracts {
            let maturity = Date::ordinal(r.maturity)?;
            if maturity <= asof
                || [
                    r.coupon,
                    r.notional,
                    r.price,
                    r.cap,
                    r.floor,
                    r.call_threshold,
                    r.penalty_months,
                    r.ew_mult,
                ]
                .iter()
                .any(|v| !v.is_finite())
                || r.notional < 0.
                || r.price <= 0.
                || r.floor > r.cap
                || r.penalty_months < 0.
                || r.ew_mult < 0.
            {
                return Err("invalid term contract".into());
            }
            let freq = r.freq_months.unwrap_or(0);
            if r.amort_type == Amortization::LevelPayment
                && (self.product != Product::Corporate || r.is_float || r.sink_schedule.is_some())
            {
                return Err(
                    "level_payment requires fixed-rate corporate terms without a sinking schedule"
                        .into(),
                );
            }
            let schedule = if self.product == Product::Cd && freq <= 0 {
                let pay = calendar.adjust(maturity, self.bdc)?;
                vec![(asof, pay, year_fraction(asof, pay, r.daycount))]
            } else {
                calendar.schedule(asof, maturity, freq, r.daycount, self.bdc)?
            };
            if deck.pay_m.len() + schedule.len() > 1024 * 1024 {
                return Err("term deck exceeds one million periods".into());
            }
            let t_mat =
                (calendar.adjust(maturity, self.bdc)?.number() - asof.number()) as f64 / 365.;
            if r.amort_type == Amortization::LevelPayment
                && schedule
                    .iter()
                    .any(|(_, pay, _)| (pay.number() - asof.number()) as f64 / 365. > terminal)
            {
                return Err(
                    "level_payment schedule exceeds the terminal market grid; extend months".into(),
                );
            }
            let mut principal = vec![0.; schedule.len()];
            if self.product == Product::Corporate {
                match r.amort_type {
                    Amortization::Bullet => *principal.last_mut().unwrap() = 1.,
                    Amortization::Annuity => principal.fill(1. / schedule.len() as f64),
                    Amortization::LevelPayment => {
                        principal = level_payment_principal(
                            r.coupon,
                            &schedule.iter().map(|(_, _, tau)| *tau).collect::<Vec<_>>(),
                        )?;
                    }
                    Amortization::Sink => {
                        let sink = r
                            .sink_schedule
                            .as_ref()
                            .ok_or("sink amortization requires a schedule")?;
                        // Python dict semantics: preserve first insertion order, last duplicate value.
                        let mut entries: Vec<(i32, f64, bool)> = Vec::new();
                        for &(day, amount) in sink {
                            Date::ordinal(day)?;
                            if !amount.is_finite() || amount < 0. {
                                return Err("invalid sinking amount".into());
                            }
                            if let Some(e) = entries.iter_mut().find(|e| e.0 == day) {
                                e.1 = amount;
                            } else {
                                entries.push((day, amount, false));
                            }
                        }
                        for (j, (_, pay, _)) in schedule.iter().enumerate() {
                            for e in &mut entries {
                                if !e.2 && (pay.number() - e.0).abs() <= 20 {
                                    principal[j] += e.1;
                                    e.2 = true;
                                }
                            }
                        }
                        let rest = (1. - principal.iter().sum::<f64>()).max(0.);
                        *principal.last_mut().unwrap() += rest;
                    }
                }
            }
            let map_options =
                |schedule: &Option<Vec<(i32, f64)>>| -> Result<HashMap<i32, f64>, String> {
                    let mut map = HashMap::new();
                    for &(day, price) in schedule.as_deref().unwrap_or(&[]) {
                        if !price.is_finite() || price < 0. {
                            return Err("invalid option exercise price".into());
                        }
                        map.insert(month(Date::ordinal(day)?), price);
                    }
                    Ok(map)
                };
            let calls = map_options(&r.call_schedule)?;
            let puts = map_options(&r.put_schedule)?;
            for (j, (start, pay, tau)) in schedule.iter().enumerate() {
                let tp = time(*pay);
                if tp < 0. || *tau < 0. || !tau.is_finite() {
                    return Err("adjusted cashflow precedes valuation/accrual start".into());
                }
                let m = month(*pay);
                let tf = (((start.number() - asof.number()).max(0)) as f64 / 365.).min(terminal);
                let fix = ((tf * 12.) as usize).min(self.months - 2);
                deck.pay_m.push(m as f64);
                deck.pay_frac
                    .push((tp - m as f64 / 12.).clamp(0., 1. / 12.));
                deck.acc_m.push(if self.product == Product::Cd {
                    ((((start.number() - asof.number()).max(0)) as f64 / 365. * 12.) as usize)
                        .min(self.months - 1) as f64
                } else {
                    fix as f64
                });
                deck.t_pay.push(tp);
                deck.tau.push(*tau);
                deck.fix_m.push(fix as f64);
                deck.fix_w.push((tf * 12. - fix as f64).clamp(0., 1.));
                deck.prin.push(principal[j]);
                deck.call_px.push(*calls.get(&m).unwrap_or(&-1.));
                deck.put_px.push(*puts.get(&m).unwrap_or(&-1.));
                deck.rem_y.push((t_mat - tp + tau).max(*tau));
            }
            deck.per_off.push(deck.pay_m.len() as f64);
            deck.is_float.push(f64::from(r.is_float));
            deck.cpn.push(r.coupon);
            deck.cap.push(r.cap);
            deck.floor.push(r.floor);
            deck.call_thr.push(r.call_threshold);
            deck.pen_m.push(r.penalty_months);
            deck.ew_mult.push(if r.channel == "brokered" {
                0.
            } else {
                r.ew_mult
            });
            deck.notional.push(r.notional);
            deck.tgt.push(r.price / 100.);
        }
        Ok(deck)
    }
}
