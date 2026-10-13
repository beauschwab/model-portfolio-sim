//! Supplied performing commitments acquire distinct source entitlements and yields.
use super::owner::NativeFactors;
use crate::{
    commercial_credit_compiler::{serialize_factors, InstrumentKind, Request},
    commercial_source_income as source_income, term_enhancements as term,
};
use portfolio_model_core::{cashflow::CashflowMeasure, credit::CreditMeasure};
use serde::{Deserialize, Serialize};
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn source_measure(measure: CreditMeasure) -> CashflowMeasure {
    match measure {
        CreditMeasure::Physical => CashflowMeasure::Physical,
        CreditMeasure::Pricing => CashflowMeasure::Pricing,
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerformingDrawTranche {
    pub review: term::Identity,
    pub draw_ordinal: i32,
    pub eligible_origins: [bool; 3],
    pub source: source_income::Request,
}
#[derive(Debug, Serialize)]
pub struct NativeDrawSource {
    pub terms: PerformingDrawTranche,
    #[serde(serialize_with = "serialize_factors")]
    pub original: NativeFactors,
    #[serde(serialize_with = "serialize_factors")]
    pub current: NativeFactors,
}
pub(crate) fn draw_event_id(draw: &PerformingDrawTranche) -> String {
    format!("native-draw:{}", draw.review.id)
}
pub(crate) fn source_end_year(r: &Request) -> Result<f64, String> {
    let mut end = r.source.current.tail_end_ordinal;
    for d in &r.performing_draw_tranches {
        end = end.max(d.source.current.tail_end_ordinal);
    }
    let days = end
        .checked_sub(r.source.current.asof_ordinal)
        .ok_or("tranche complete tail overflow")?;
    if !(1..=36600).contains(&days) {
        return Err("tranche complete tail domain exceeded".into());
    }
    Ok(f64::from(days) / 365.)
}
pub(crate) fn validate_draws(r: &Request, measure: CreditMeasure) -> Result<(), String> {
    if r.performing_draw_tranches.len() > 32 {
        return Err("performing draw tranche bound32 exceeded".into());
    }
    let mut events = std::collections::BTreeSet::new();
    let mut contracts = std::collections::BTreeSet::new();
    contracts.insert(r.source.current.contract.id.clone());
    for p in &r.source.current.periods {
        events.insert(p.period_id.clone());
    }
    let mut nominal = 0.;
    for draw in &r.performing_draw_tranches {
        check()?;
        let mut meta = 0;
        term::identity(&draw.review, &mut meta)?;
        source_income::admit_for_measure(&draw.source, source_measure(measure))?;
        let s = &draw.source.current;
        if draw.draw_ordinal <= r.source.current.asof_ordinal
            || s.asof_ordinal != draw.draw_ordinal
            || draw.source.original.asof_ordinal != draw.draw_ordinal
            || !draw.eligible_origins.iter().any(|v| *v)
            || s.currency != r.source.current.currency
            || s.monetary_unit != r.source.current.monetary_unit
            || s.scenario_id != r.source.current.scenario_id
            || s.periods
                .iter()
                .any(|p| p.payment_ordinal <= p.start_ordinal)
            || !contracts.insert(s.contract.id.clone())
            || !events.insert(draw_event_id(draw))
            || s.periods
                .iter()
                .any(|p| !events.insert(p.period_id.clone()))
        {
            return Err("performing draw requires unique complete eligible native tranche sources, exact draw-date origin, currency/scenario and frozen original EIR".into());
        }
        if draw.source.policy.fees != "integral_cash-1" && s.periods.iter().any(|p| p.fee != 0.) {
            return Err("draw tranche nonintegral fee needs its recognition owner".into());
        }
        nominal += s.opening_principal;
        term::nonnegative(nominal)?;
    }
    if nominal > r.credit.opening_undrawn_commitment
        || (!r.performing_draw_tranches.is_empty()
            && r.credit.instrument_kind != InstrumentKind::CommercialLoan)
    {
        return Err(
            "performing drawn nominal exceeds explicit loan commitment/instrument eligibility"
                .into(),
        );
    }
    Ok(())
}
pub(crate) fn project_draws(
    r: &Request,
    measure: CreditMeasure,
    queries: usize,
) -> Result<Vec<NativeDrawSource>, String> {
    project_draws_internal(r, measure, queries, true)
}
pub(crate) fn project_draws_for_acquisition(
    r: &Request,
    measure: CreditMeasure,
    queries: usize,
) -> Result<Vec<NativeDrawSource>, String> {
    project_draws_internal(r, measure, queries, false)
}
fn project_draws_internal(
    r: &Request,
    measure: CreditMeasure,
    queries: usize,
    check_quote: bool,
) -> Result<Vec<NativeDrawSource>, String> {
    r.performing_draw_tranches
        .iter()
        .map(|terms| {
            check()?;
            let original = NativeFactors::compile_for_measure(
                terms.source.original.clone(),
                r.factor_policy.clone(),
                0,
                source_measure(measure),
            )?;
            let current = NativeFactors::compile_for_measure(
                terms.source.current.clone(),
                r.factor_policy.clone(),
                queries,
                source_measure(measure),
            )?;
            if measure == CreditMeasure::Physical && check_quote {
                source_income::original_cash_quote(&terms.source, original.source())?;
            }
            Ok(NativeDrawSource {
                terms: terms.clone(),
                original,
                current,
            })
        })
        .collect()
}
