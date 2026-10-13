//! Source-linked sequential par-quote bootstrapping.
//!
//! The legacy market helper owns a fixed annual schedule for built-in pricing.
//! This analytical contract keeps the quote source and every payment time
//! explicit, bootstraps one new discount factor per pillar, and reports quote
//! residuals. It does not silently infer a calendar, day count, collateral
//! agreement, forward measure, or production pricing policy.

use portfolio_compute_control::checkpoint;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCHEMA: &str = "market-quote-bootstrap-1";
const MAX_QUOTES: usize = 256;
const MAX_PAYMENTS: usize = 4096;
const MAX_WORK: usize = 1_000_000;
const RESIDUAL_TOLERANCE: f64 = 2e-12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payment {
    pub time_years: f64,
    pub accrual_fraction: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParQuote {
    pub quote_id: String,
    /// Decimal fixed coupon rate, not a percentage string or basis points.
    pub par_rate: f64,
    /// Complete supplied fixed-leg schedule ending at the quote maturity.
    pub payments: Vec<Payment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub model_version: String,
    pub source_id: String,
    pub source_revision: String,
    pub curve_id: String,
    /// Quotes used to create one new discount-factor pillar each.
    pub bootstrap_quotes: Vec<ParQuote>,
    /// Optional held-out or cross-check quotes whose maturities already exist.
    pub validation_quotes: Vec<ParQuote>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteDiagnostic {
    pub quote_id: String,
    pub kind: String,
    pub observed_par_rate: f64,
    pub model_par_rate: f64,
    pub residual: f64,
    pub unit_par_pv: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapDiagnostics {
    pub method: String,
    pub bootstrap_quote_count: usize,
    pub max_bootstrap_abs_residual: f64,
    pub residual_tolerance: f64,
    pub converged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Result {
    pub schema: String,
    pub model_version: String,
    pub source_id: String,
    pub source_revision: String,
    pub curve_id: String,
    pub source_status: String,
    pub adoption_eligible: bool,
    pub pillar_times_years: Vec<f64>,
    pub discount_factors: Vec<f64>,
    pub quote_diagnostics: Vec<QuoteDiagnostic>,
    pub diagnostics: BootstrapDiagnostics,
    pub work_units: usize,
}

fn label(value: &str, name: &str) -> std::result::Result<(), String> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(format!("{name} must be nonempty bounded text"));
    }
    Ok(())
}

fn finite(value: f64, low: f64, high: f64, name: &str) -> std::result::Result<(), String> {
    if !value.is_finite() || value < low || value > high {
        return Err(format!("{name} must be finite in [{low}, {high}]"));
    }
    Ok(())
}

fn quote_maturity(quote: &ParQuote) -> std::result::Result<f64, String> {
    if quote.payments.is_empty() || quote.payments.len() > MAX_PAYMENTS {
        return Err("quote requires 1..4096 explicit payments".into());
    }
    label(&quote.quote_id, "quote_id")?;
    finite(quote.par_rate, -2.0, 2.0, "par_rate")?;
    let mut prior = 0.0;
    for payment in &quote.payments {
        finite(payment.time_years, 1e-12, 100.0, "payment time")?;
        finite(payment.accrual_fraction, 1e-12, 10.0, "accrual fraction")?;
        if payment.time_years <= prior {
            return Err("quote payment times must be strictly increasing".into());
        }
        prior = payment.time_years;
    }
    Ok(prior)
}

fn lookup(time: f64, times: &[f64], discounts: &[f64]) -> Option<f64> {
    times
        .iter()
        .zip(discounts)
        .find(|(pillar, _)| {
            (**pillar - time).abs() <= 1e-12 * pillar.abs().max(time.abs()).max(1.0)
        })
        .map(|(_, discount)| *discount)
}

fn model_quote(
    quote: &ParQuote,
    times: &[f64],
    discounts: &[f64],
) -> std::result::Result<(f64, f64), String> {
    let mut annuity = 0.0;
    let mut final_discount = None;
    for payment in &quote.payments {
        let discount = lookup(payment.time_years, times, discounts)
            .ok_or_else(|| "quote payment time is not a bootstrapped pillar".to_owned())?;
        annuity += payment.accrual_fraction * discount;
        final_discount = Some(discount);
    }
    let final_discount = final_discount.ok_or("quote has no final payment")?;
    if !annuity.is_finite() || annuity <= 0.0 {
        return Err("quote annuity is outside the admitted domain".into());
    }
    let rate = (1.0 - final_discount) / annuity;
    let unit_pv = rate * annuity + final_discount;
    finite(rate, -2.0, 2.0, "model par rate")?;
    finite(unit_pv, 0.0, 1.0e12, "unit par PV")?;
    Ok((rate, unit_pv))
}

fn validate_request(request: &Request) -> std::result::Result<(), String> {
    if request.schema != SCHEMA || request.model_version != SCHEMA {
        return Err("market quote bootstrap schema/version mismatch".into());
    }
    for (value, name) in [
        (&request.source_id, "source_id"),
        (&request.source_revision, "source_revision"),
        (&request.curve_id, "curve_id"),
    ] {
        label(value, name)?;
    }
    if request.bootstrap_quotes.is_empty()
        || request.bootstrap_quotes.len() > MAX_QUOTES
        || request.validation_quotes.len() > MAX_QUOTES
    {
        return Err("quote bootstrap admission exceeded".into());
    }
    let work = request
        .bootstrap_quotes
        .iter()
        .chain(&request.validation_quotes)
        .map(|quote| quote.payments.len())
        .sum::<usize>();
    if work == 0 || work > MAX_WORK {
        return Err("quote bootstrap work admission exceeded".into());
    }
    let mut ids = HashSet::new();
    for quote in request
        .bootstrap_quotes
        .iter()
        .chain(&request.validation_quotes)
    {
        quote_maturity(quote)?;
        if !ids.insert(&quote.quote_id) {
            return Err("quote identities must be unique".into());
        }
    }
    Ok(())
}

pub fn evaluate(request: &Request) -> std::result::Result<Result, String> {
    evaluate_with_check(request, checkpoint)
}

pub fn evaluate_with_check(
    request: &Request,
    mut check: impl FnMut() -> std::result::Result<(), String>,
) -> std::result::Result<Result, String> {
    check()?;
    validate_request(request)?;
    let mut ordered = request.bootstrap_quotes.clone();
    ordered.sort_by(|left, right| {
        quote_maturity(left)
            .expect("validated quote maturity")
            .total_cmp(&quote_maturity(right).expect("validated quote maturity"))
            .then(left.quote_id.cmp(&right.quote_id))
    });
    let mut times = vec![0.0];
    let mut discounts = vec![1.0];
    let mut diagnostics =
        Vec::with_capacity(request.bootstrap_quotes.len() + request.validation_quotes.len());
    let mut max_bootstrap_residual: f64 = 0.0;
    for quote in &ordered {
        check()?;
        let maturity = quote_maturity(quote)?;
        if times.last().copied().unwrap_or(0.0) >= maturity {
            return Err("bootstrap quote maturities must be strictly increasing".into());
        }
        let final_payment = quote.payments.last().expect("validated payment");
        let mut known_coupon_pv = 0.0;
        for payment in &quote.payments[..quote.payments.len() - 1] {
            let discount = lookup(payment.time_years, &times, &discounts)
                .ok_or("non-final bootstrap payment must match an earlier pillar")?;
            known_coupon_pv += payment.accrual_fraction * discount;
        }
        let denominator = 1.0 + quote.par_rate * final_payment.accrual_fraction;
        finite(denominator, 1e-12, 1.0e12, "bootstrap denominator")?;
        let discount = (1.0 - quote.par_rate * known_coupon_pv) / denominator;
        finite(discount, 1e-12, 1.0e12, "bootstrapped discount factor")?;
        times.push(maturity);
        discounts.push(discount);
        let (model_rate, unit_pv) = model_quote(quote, &times, &discounts)?;
        let residual = model_rate - quote.par_rate;
        max_bootstrap_residual = max_bootstrap_residual.max(residual.abs());
        diagnostics.push(QuoteDiagnostic {
            quote_id: quote.quote_id.clone(),
            kind: "bootstrap".into(),
            observed_par_rate: quote.par_rate,
            model_par_rate: model_rate,
            residual,
            unit_par_pv: unit_pv,
        });
    }
    for quote in &request.validation_quotes {
        check()?;
        let _ = quote_maturity(quote)?;
        let (model_rate, unit_pv) = model_quote(quote, &times, &discounts)?;
        diagnostics.push(QuoteDiagnostic {
            quote_id: quote.quote_id.clone(),
            kind: "validation".into(),
            observed_par_rate: quote.par_rate,
            model_par_rate: model_rate,
            residual: model_rate - quote.par_rate,
            unit_par_pv: unit_pv,
        });
    }
    if max_bootstrap_residual > RESIDUAL_TOLERANCE {
        return Err("closed-form bootstrap residual exceeds tolerance".into());
    }
    Ok(Result {
        schema: SCHEMA.into(),
        model_version: SCHEMA.into(),
        source_id: request.source_id.clone(),
        source_revision: request.source_revision.clone(),
        curve_id: request.curve_id.clone(),
        source_status: "source_linked_explicit_schedule_par_bootstrap".into(),
        adoption_eligible: false,
        pillar_times_years: times,
        discount_factors: discounts,
        quote_diagnostics: diagnostics,
        diagnostics: BootstrapDiagnostics {
            method: "sequential_closed_form_par_bootstrap".into(),
            bootstrap_quote_count: ordered.len(),
            max_bootstrap_abs_residual: max_bootstrap_residual,
            residual_tolerance: RESIDUAL_TOLERANCE,
            converged: true,
        },
        work_units: request
            .bootstrap_quotes
            .iter()
            .chain(&request.validation_quotes)
            .map(|quote| quote.payments.len())
            .sum(),
    })
}
