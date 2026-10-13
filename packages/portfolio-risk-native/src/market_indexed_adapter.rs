//! UNAPPLIED explicit DTO adapter. Coupons and clean value belong to copied native products.
use crate::derivative_contract::swaps::*;
use crate::market_product_source::{self as bridge, Market, Valuation};
use crate::term_enhancements::{self, indexed::*, Identity};
use portfolio_model_core::cashflow::{ActualDayCount, CashflowMeasure};
use portfolio_model_core::floating_rate::{
    Calculation, Direction, IndexDescriptor, IndexKind, MarginTreatment, RateBounds,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA: &str = "market-indexed-adapter-1";
pub const MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub index_tenor_months: u32,
    pub calendar: Identity,
    pub projection_model: Identity,
    pub index_basis: Identity,
    pub collateral_policy: Identity,
    pub original_oas_review: Identity,
    pub bridge: bridge::Request,
}
#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub retained_bytes: usize,
    pub numerical_work: usize,
    pub case_count: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct ProductCase {
    pub scenario_id: String,
    pub rate_source: bridge::Identity,
    pub indexed_request: ContractPathRequest,
    pub indexed_result: ContractPathResult,
    pub swap_request: SwapRequest,
    pub swap_result: SwapResult,
    pub original_locked_oas: f64,
    pub original_oas_review: Identity,
    pub spread_adjusted_swap_pv: f64,
    pub original_parent: bridge::OriginalAnchor,
    pub native_contract_descriptor: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResultData {
    pub schema: String,
    pub cache_parent: Request,
    pub admission: Admission,
    pub bridge_receipt: bridge::ResultData,
    pub cases: Vec<ProductCase>,
    pub production_adoption: bool,
    pub observed_fixings: bool,
    pub physical_accounting: bool,
    pub joint_dynamics: bool,
    pub calendar_mapping_verified: bool,
    pub accounting_adoption: bool,
    pub capital_ftp_eligible: bool,
    pub empirical_validation: bool,
}
fn checkpoint(check: &mut impl FnMut() -> bool) -> Result<(), String> {
    if check() {
        Err("COMPUTE_CANCELLED".into())
    } else {
        Ok(())
    }
}
fn day_count(s: &str) -> ActualDayCount {
    if s == "actual360" {
        ActualDayCount::Act360
    } else {
        ActualDayCount::Act365Fixed
    }
}
fn id(s: &str, revision: &str) -> Identity {
    Identity {
        id: s.into(),
        revision: revision.into(),
    }
}
pub(crate) fn admission(r: &Request) -> Result<Admission, String> {
    if r.schema != SCHEMA || !(1..=1200).contains(&r.index_tenor_months) {
        return Err("adapter version/explicit index tenor mismatch".into());
    }
    let mut metadata = 0;
    for v in [
        &r.calendar,
        &r.projection_model,
        &r.index_basis,
        &r.collateral_policy,
        &r.original_oas_review,
    ] {
        term_enhancements::identity(v, &mut metadata)?;
    }
    // New adapter identities explicitly describe selected model/mapping, not observed index calibration.
    if r.calendar.id != r.bridge.binding.calendar_identity
        || r.projection_model.id != r.bridge.projection_policy
        || r.projection_model.revision != r.bridge.model_version
        || r.original_oas_review.id != r.bridge.original.review_identity
    {
        return Err("adapter source/calendar/original OAS review identity mismatch".into());
    }
    for s in [
        &r.bridge.contract.contract_id,
        &r.bridge.contract.contract_revision,
        &r.bridge.binding.index_id,
        &r.bridge.identity.source_id,
        &r.bridge.identity.source_revision,
        &r.bridge.current_market.identity.source_id,
        &r.bridge.current_market.identity.source_revision,
        &r.bridge.original.market.identity.source_id,
        &r.bridge.original.market.identity.source_revision,
    ] {
        term_enhancements::text(s, &mut metadata)?;
    }
    for p in &r.bridge.contract.periods {
        term_enhancements::text(&p.period_id, &mut metadata)?;
    }
    let input = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    if input > bridge::MAX_INPUT_BYTES
        || r.bridge.contract.periods.is_empty()
        || r.bridge.contract.periods.len() > 512
    {
        return Err("adapter raw input/period admission exceeded".into());
    }
    let nodes = r
        .bridge
        .original
        .market
        .curves
        .iter()
        .chain(&r.bridge.current_market.curves)
        .try_fold(0usize, |n, c| {
            n.checked_add(c.times_years.len())
                .ok_or("adapter node overflow")
        })?;
    let rows = r
        .bridge
        .contract
        .periods
        .len()
        .checked_mul(8)
        .ok_or("adapter row overflow")?;
    let numerical_work = nodes
        .checked_mul(4)
        .and_then(|n| n.checked_add(rows.checked_mul(320)?))
        .ok_or("adapter work overflow")?;
    let retained = rows
        .checked_mul(input.checked_add(16384).ok_or("adapter bytes overflow")?)
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_add(input.checked_mul(4)?))
        .and_then(|n| n.checked_add(65536))
        .ok_or("adapter retained bytes overflow")?;
    if retained > MAX_RETAINED_BYTES || numerical_work > 2_000_000 {
        return Err("adapter aggregate retained/work admission exceeded".into());
    }
    Ok(Admission {
        retained_bytes: retained,
        numerical_work,
        case_count: 8,
    })
}
pub(crate) fn requests(
    r: &Request,
    market: &Market,
    v: &Valuation,
    check: &mut impl FnMut() -> bool,
) -> Result<(ContractPathRequest, SwapRequest), String> {
    checkpoint(check)?;
    let known = &r.bridge.contract;
    let path = id(&v.scenario_id, &market.identity.source_revision);
    let mut periods = Vec::with_capacity(known.periods.len());
    let mut fixed = Vec::with_capacity(known.periods.len());
    let mut points = BTreeMap::from([(r.bridge.identity.as_of_ordinal, 1.)]);
    for (p, o) in known.periods.iter().zip(&v.projections) {
        checkpoint(check)?;
        let observations = vec![IndexObservation {
            interest_start_ordinal: p.start_ordinal,
            interest_end_ordinal: p.end_ordinal,
            observation_start_ordinal: p.start_ordinal,
            observation_end_ordinal: p.end_ordinal,
            fixing_ordinal: p.reset_ordinal,
            selection_cutoff_unix_seconds: p.reset_cutoff_unix_seconds,
            rate: o.index_forward,
            source: RateSource::ConditionalProjection {
                model: r.projection_model.clone(),
                path: path.clone(),
                information_unix_seconds: o.information_unix_seconds,
                measure: CashflowMeasure::Pricing,
            },
        }];
        periods.push(ContractPeriod {
            period_id: p.period_id.clone(),
            start_ordinal: p.start_ordinal,
            end_ordinal: p.end_ordinal,
            payment_ordinal: p.payment_ordinal,
            day_count: day_count(&known.index_day_count),
            fee: 0.,
            coupon: Coupon::Indexed {
                index: IndexDescriptor {
                    index_id: r.bridge.binding.index_id.clone(),
                    kind: IndexKind::Term,
                    tenor_months: Some(r.index_tenor_months),
                    average_calendar_days: None,
                },
                calendar: Box::new(r.calendar.clone()),
                complete_calendar_mapping: true,
                calculation: Calculation::TermSimple,
                margin_treatment: MarginTreatment::Simple,
                margin: known.margin,
                index_bounds: RateBounds {
                    floor: None,
                    cap: None,
                },
                all_in_bounds: RateBounds {
                    floor: None,
                    cap: None,
                },
                reset_cutoff_unix_seconds: Some(p.reset_cutoff_unix_seconds),
                lookback_business_days: 0,
                observation_shift: false,
                observations,
            },
        });
        fixed.push(FixedPeriod {
            period_id: p.period_id.clone(),
            start_ordinal: p.start_ordinal,
            end_ordinal: p.end_ordinal,
            payment_ordinal: p.payment_ordinal,
            day_count: day_count(&known.fixed_day_count),
        });
        if points
            .insert(p.payment_ordinal, o.collateral_discount_factor)
            .is_some_and(|df| df != o.collateral_discount_factor)
        {
            return Err("different discounts at identical payment date".into());
        }
    }
    let tail = known
        .periods
        .iter()
        .map(|p| p.payment_ordinal)
        .max()
        .ok_or("empty periods")?;
    let mut principal = vec![0.; periods.len()];
    *principal.last_mut().ok_or("empty principal")? = known.notional;
    let indexed_request = ContractPathRequest {
        schema: term_enhancements::SCHEMA.into(),
        model_version: term_enhancements::MODEL_VERSION.into(),
        contract: id(&known.contract_id, &known.contract_revision),
        assumption: r.index_basis.clone(),
        path: path.clone(),
        position_id: known.contract_id.clone(),
        scenario_id: v.scenario_id.clone(),
        currency: r.bridge.identity.currency.clone(),
        monetary_unit: "currency_units".into(),
        direction: Direction::Receipt,
        measure: CashflowMeasure::Pricing,
        asof_ordinal: r.bridge.identity.as_of_ordinal,
        reporting_end_ordinal: r.bridge.identity.as_of_ordinal,
        tail_end_ordinal: tail,
        observed_publication_cutoff_unix_seconds: market.information_unix_seconds,
        opening_principal: known.notional,
        principal_plan: PrincipalPlan::Supplied {
            principal_payments: principal,
        },
        periods: periods.clone(),
    };
    let swap_request = SwapRequest {
        contract: indexed_request.contract.clone(),
        index_basis: r.index_basis.clone(),
        rate_path: path,
        position_id: known.contract_id.clone(),
        scenario_id: v.scenario_id.clone(),
        currency: r.bridge.identity.currency.clone(),
        monetary_unit: "currency_units".into(),
        asof_ordinal: r.bridge.identity.as_of_ordinal,
        valuation_cutoff_unix_seconds: market.information_unix_seconds,
        tail_end_ordinal: tail,
        payment_timing: PaymentTiming::AfterScheduledPayments,
        side: if known.direction == "payer_fixed" {
            Side::Payer
        } else {
            Side::Receiver
        },
        notional: known.notional,
        fixed_rate: known.fixed_rate,
        fixed_periods: fixed,
        floating_periods: periods,
        discount_curve: DiscountCurve {
            source: id(&market.identity.source_id, &market.identity.source_revision),
            collateral_discount_policy: r.collateral_policy.clone(),
            currency: r.bridge.identity.currency.clone(),
            asof_ordinal: r.bridge.identity.as_of_ordinal,
            information_unix_seconds: market.information_unix_seconds,
            basis: "collateralized_clean".into(),
            points: points
                .into_iter()
                .map(|(ordinal, discount_factor)| DiscountPoint {
                    ordinal,
                    discount_factor,
                })
                .collect(),
        },
    };
    Ok((indexed_request, swap_request))
}

fn case(
    r: &Request,
    market: &Market,
    v: &Valuation,
    check: &mut impl FnMut() -> bool,
) -> Result<ProductCase, String> {
    let (indexed_request, swap_request) = requests(r, market, v, check)?;
    let indexed_result = build_path(&indexed_request)?;
    checkpoint(check)?;
    let swap_result = value_swap(&swap_request)?;
    let descriptor = contract_key(&swap_request)?;
    let mut spread_pv = 0.;
    for e in &swap_result.schedule.events {
        checkpoint(check)?;
        let t = (e.payment_ordinal - r.bridge.identity.as_of_ordinal) as f64 / 365.;
        spread_pv += e.components.cash_total()?
            * swap_request.discount_curve.at(e.payment_ordinal)?
            * (-r.bridge.original.locked_oas * t).exp();
    }
    if !spread_pv.is_finite() || (spread_pv - v.pv).abs() > 1e-9 + 1e-12 * v.pv.abs() {
        return Err("actual native swap cash/PV differs from curve bridge".into());
    }
    Ok(ProductCase {
        scenario_id: v.scenario_id.clone(),
        rate_source: market.identity.clone(),
        indexed_request,
        indexed_result,
        swap_request,
        swap_result,
        original_locked_oas: r.bridge.original.locked_oas,
        original_oas_review: r.original_oas_review.clone(),
        spread_adjusted_swap_pv: spread_pv,
        original_parent: r.bridge.original.clone(),
        native_contract_descriptor: descriptor,
    })
}
pub fn evaluate_with_check(
    r: &Request,
    mut check: impl FnMut() -> bool,
) -> Result<ResultData, String> {
    checkpoint(&mut check)?;
    let admission = admission(r)?;
    let projected = bridge::evaluate_with_check(&r.bridge, &mut check)?;
    let mut cases = Vec::with_capacity(8);
    cases.push(case(
        r,
        &r.bridge.original.market,
        &projected.original,
        &mut check,
    )?);
    cases.push(case(
        r,
        &r.bridge.current_market,
        &projected.current,
        &mut check,
    )?);
    for risk in &projected.risks {
        cases.push(case(r, &r.bridge.current_market, &risk.down, &mut check)?);
        cases.push(case(r, &r.bridge.current_market, &risk.up, &mut check)?);
    }
    checkpoint(&mut check)?;
    let result = ResultData {
        schema: SCHEMA.into(),
        cache_parent: r.clone(),
        admission,
        bridge_receipt: projected,
        cases,
        production_adoption: false,
        observed_fixings: false,
        physical_accounting: false,
        joint_dynamics: false,
        calendar_mapping_verified: false,
        accounting_adoption: false,
        capital_ftp_eligible: false,
        empirical_validation: false,
    };
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > result.admission.retained_bytes
    {
        return Err("internal adapter output bound violated".into());
    }
    checkpoint(&mut check)?;
    Ok(result)
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_with_check(r, || false)
}
