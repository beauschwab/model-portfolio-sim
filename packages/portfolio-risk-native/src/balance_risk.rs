//! Mixed-book parallel revaluation: common calibrated paths and fixed base OAS.
use crate::accounting_lifecycle::{cd_flows, AccountingRequest};
use crate::deposit_lifecycle::{cashflows, deposit_paths, equilibrium, DepositDeck};
use crate::hedge_lifecycle::{
    option_values, swap_values, term_flows, term_price, HedgeDeck, Swaption,
};
use crate::lifecycle_market::{finite, monthly_price, RateMarket};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BalanceRiskRequest {
    pub books: AccountingRequest,
    pub oas: BTreeMap<String, Vec<f64>>,
    pub bump_bp: f64,
    pub swaptions: Option<Vec<Swaption>>,
}
#[derive(Serialize)]
pub struct BalanceRiskResult {
    pub dv01s: BTreeMap<String, f64>,
    pub valued_prices: BTreeMap<String, Vec<f64>>,
}
fn weighted(values: &[f64], weights: &[f64]) -> f64 {
    values.iter().zip(weights).map(|(a, b)| a * b).sum()
}
impl BalanceRiskRequest {
    pub fn run(&self) -> Result<BalanceRiskResult, String> {
        crate::compute_span!("parallel_risk");
        if !self.bump_bp.is_finite() || self.bump_bp <= 0. {
            return Err("parallel bump must be positive and finite".into());
        }
        let a = &self.books;
        a.validate()?;
        let c = &a.market.config;
        let (p, t) = (c.paths, c.months);
        let market = RateMarket::new(&a.market)?;
        if a.forecast.is_some() || a.draws.is_some() {
            return Err("risk requires seeded risk-neutral market inputs".into());
        }
        let base = market.base()?;
        let down: Vec<_> = a
            .market
            .swap_rates
            .iter()
            .map(|x| x - self.bump_bp * 1e-4)
            .collect();
        let up: Vec<_> = a
            .market
            .swap_rates
            .iter()
            .map(|x| x + self.bump_bp * 1e-4)
            .collect();
        let down = market.paths(&down, &market.abcd)?;
        let up = market.paths(&up, &market.abcd)?;
        let mut out = BalanceRiskResult {
            dv01s: BTreeMap::new(),
            valued_prices: BTreeMap::new(),
        };
        for (key, values) in &self.oas {
            let n = match key.as_str() {
                "mbs" => a.mbs.as_ref().map(|b| b.ids.len()),
                "deposits" => a.deposits.as_ref().map(|b| b.ids.len()),
                _ => a.terms.iter().find(|b| &b.key == key).map(|b| b.ids.len()),
            }
            .ok_or("fixed OAS names missing book")?;
            if n != values.len() {
                return Err("fixed OAS shape mismatch".into());
            }
            finite(values)?;
        }
        if let Some(book) = &a.mbs {
            let mut request = book.request.borrow();
            request.config.base_paths = p;
            request.config.sensitivity_paths = p;
            request.fixed_oas = self.oas.get("mbs").map_or(&[], Vec::as_slice);
            let crate::mortgage_risk::ParallelValues {
                base: prices,
                down: d,
                up: u,
            } = request.parallel_values(self.bump_bp)?;
            let weights: Vec<_> = request.book.chunks_exact(13).map(|r| r[11]).collect();
            out.dv01s.insert(
                "mbs".into(),
                (weighted(&d, &weights) - weighted(&u, &weights)) / (2. * self.bump_bp),
            );
            if self.oas.contains_key("mbs") {
                out.valued_prices.insert("mbs".into(), prices);
            }
        }
        for book in &a.terms {
            let mut down_total = 0.;
            let mut up_total = 0.;
            let mut prices = vec![];
            for first in (0..book.deck.contracts.len()).step_by(256) {
                let end = (first + 256).min(book.deck.contracts.len());
                let deck = book.deck.build_rows(&book.deck.contracts[first..end])?;
                let flows = |paths| {
                    if book.key == "cds" {
                        cd_flows(&deck, paths, p, t, &a.withdrawal_parameters)
                    } else {
                        term_flows(&deck, paths, p, t)
                    }
                };
                let base_flows = flows(&base);
                let spreads = if let Some(x) = self.oas.get(&book.key) {
                    x[first..end].to_vec()
                } else {
                    term_price(&deck, &base_flows[0], &deck.tgt, p, true).remove(0)
                };
                if self.oas.contains_key(&book.key) {
                    prices.extend(term_price(&deck, &base_flows[0], &spreads, p, false).remove(0));
                }
                let d = term_price(&deck, &flows(&down)[0], &spreads, p, false).remove(0);
                let u = term_price(&deck, &flows(&up)[0], &spreads, p, false).remove(0);
                down_total += weighted(&d, &deck.notional);
                up_total += weighted(&u, &deck.notional);
            }
            out.dv01s.insert(
                book.key.clone(),
                (down_total - up_total) / (2. * self.bump_bp),
            );
            if self.oas.contains_key(&book.key) {
                out.valued_prices.insert(book.key.clone(), prices);
            }
        }
        if let Some(book) = &a.deposits {
            let history: Vec<_> = book.history.iter().flatten().copied().collect();
            let params = crate::calibration::fit_deposits(&history)?.x;
            let r0 = equilibrium(
                &params,
                (0..p).map(|i| base.short[i * t]).sum::<f64>() / p as f64,
            );
            let dep_base = deposit_paths(&base.short, &base.df, &params, r0, p, t);
            let dep_down = deposit_paths(&down.short, &down.df, &params, r0, p, t);
            let dep_up = deposit_paths(&up.short, &up.df, &params, r0, p, t);
            let mut down_total = 0.;
            let mut up_total = 0.;
            let mut prices = vec![];
            for first in (0..book.book.len()).step_by(256) {
                let end = (first + 256).min(book.book.len());
                let deck = DepositDeck::new(&book.book[first..end], &book.assumptions)?;
                let flows = |paths| {
                    cashflows(
                        &deck,
                        &book.assumptions,
                        paths,
                        r0,
                        p,
                        t,
                        &vec![0.; deck.n],
                        &[0.],
                        false,
                        None,
                    )
                    .remove(0)
                };
                let base_flows = flows(&dep_base);
                let spreads = if let Some(x) = self.oas.get("deposits") {
                    x[first..end].to_vec()
                } else {
                    monthly_price(&base_flows, deck.n, t, p, &deck.tgt, true, -0.15).remove(0)
                };
                if self.oas.contains_key("deposits") {
                    prices.extend(
                        monthly_price(&base_flows, deck.n, t, p, &spreads, false, -0.15).remove(0),
                    );
                }
                let d = monthly_price(&flows(&dep_down), deck.n, t, p, &spreads, false, -0.15)
                    .remove(0);
                let u =
                    monthly_price(&flows(&dep_up), deck.n, t, p, &spreads, false, -0.15).remove(0);
                down_total += weighted(&d, &deck.bal);
                up_total += weighted(&u, &deck.bal);
            }
            out.dv01s.insert(
                "deposits".into(),
                (down_total - up_total) / (2. * self.bump_bp),
            );
            if self.oas.contains_key("deposits") {
                out.valued_prices.insert("deposits".into(), prices);
            }
        }
        out.dv01s.insert("mm".into(), 0.);
        if let Some(book) = &a.hedges {
            let mut values = [0.; 2];
            for first in (0..book.len()).step_by(256) {
                let end = (first + 256).min(book.len());
                let deck = HedgeDeck::new(&book[first..end], a.asof, t)?;
                for (j, paths) in [&down, &up].iter().enumerate() {
                    values[j] += weighted(&swap_values(&deck, paths, p, t, 1).0, &deck.notional);
                }
            }
            if let Some(options) = &self.swaptions {
                for (j, paths) in [&down, &up].iter().enumerate() {
                    values[j] += option_values(options, paths, p, t)?
                        .iter()
                        .zip(options)
                        .map(|(v, r)| v * r.notional)
                        .sum::<f64>();
                }
            }
            out.dv01s.insert(
                "hedges".into(),
                (values[0] - values[1]) / (2. * self.bump_bp),
            );
        }
        for v in out.dv01s.values() {
            finite(&[*v])?;
        }
        for v in out.valued_prices.values() {
            finite(v)?;
        }
        Ok(out)
    }
}
