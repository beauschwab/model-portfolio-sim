//! UNAPPLIED deterministic supplied-curve analytical pilot, never a fixing source.
mod strict_curves;
mod types;
mod validation;
use crate::model_market::curves::SuppliedCurve;
pub use types::*;
use validation::{admit, checkpoint, fraction};

pub const SCHEMA: &str = "market-product-source-1";
pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_WORK: usize = 2_000_000;

#[derive(Clone, Copy)]
struct Shift {
    collateral: f64,
    index: f64,
}

fn value(curve: &SuppliedCurve, t: f64, shift: f64) -> Result<(f64, bool), String> {
    // One-time immutable validation is completed by admit(). Same native curve lookup.
    let receipt = curve.value_validated(t)?;
    let df = receipt.discount_factor * (-shift * t).exp();
    if !df.is_finite() || df <= 0. {
        return Err("shifted discount factor domain".into());
    }
    Ok((df, receipt.extrapolated))
}

fn project(
    r: &Request,
    market: &Market,
    shift: Shift,
    scenario: &str,
    check: &mut impl FnMut() -> bool,
) -> Result<Valuation, String> {
    let collateral = market
        .curves
        .iter()
        .find(|c| c.curve_id == r.binding.collateral_curve_id)
        .ok_or("missing collateral curve")?;
    let index = market
        .curves
        .iter()
        .find(|c| c.curve_id == r.binding.index_curve_id)
        .ok_or("missing index curve")?;
    let mut rows = Vec::with_capacity(r.contract.periods.len());
    let mut pv = 0.;
    let mut annuity = 0.;
    let mut floating_pv = 0.;
    for p in &r.contract.periods {
        checkpoint(check)?;
        let start = (p.start_ordinal - r.identity.as_of_ordinal) as f64 / 365.;
        let end = (p.end_ordinal - r.identity.as_of_ordinal) as f64 / 365.;
        let pay = (p.payment_ordinal - r.identity.as_of_ordinal) as f64 / 365.;
        let index_tau = fraction(p.start_ordinal, p.end_ordinal, &r.contract.index_day_count);
        let fixed_tau = fraction(p.start_ordinal, p.end_ordinal, &r.contract.fixed_day_count);
        let a = value(index, start, shift.index)?;
        let b = value(index, end, shift.index)?;
        let rate = (a.0.ln() - b.0.ln()).exp_m1() / index_tau;
        if !rate.is_finite() || !(-100. ..=100.).contains(&rate) {
            return Err("conditional index forward domain".into());
        }
        let c = value(collateral, pay, shift.collateral)?;
        let discount = c.0 * (-r.original.locked_oas * pay).exp();
        let float = r.contract.notional * (rate + r.contract.margin) * index_tau;
        let fixed = r.contract.notional * r.contract.fixed_rate * fixed_tau;
        let cash = r.contract.sign() * (float - fixed);
        let discounted = cash * discount;
        if [discount, float, fixed, cash, discounted]
            .iter()
            .any(|x| !x.is_finite())
        {
            return Err("cash/PV domain".into());
        }
        // Stable contractual order; no change to any existing production reduction.
        pv += discounted;
        annuity += fixed_tau * discount;
        floating_pv += (rate + r.contract.margin) * index_tau * discount;
        rows.push(Projection {
            period_id: p.period_id.clone(),
            scenario_id: scenario.into(),
            reset_ordinal: p.reset_ordinal,
            reset_cutoff_unix_seconds: p.reset_cutoff_unix_seconds,
            start_ordinal: p.start_ordinal,
            end_ordinal: p.end_ordinal,
            payment_ordinal: p.payment_ordinal,
            index_accrual_fraction: index_tau,
            fixed_accrual_fraction: fixed_tau,
            index_forward: rate,
            fixed_cash: -r.contract.sign() * fixed,
            floating_cash: r.contract.sign() * float,
            net_cash: cash,
            collateral_discount_factor: c.0,
            oas_discount_factor: discount,
            discounted_cash: discounted,
            index_tail_used: a.1 || b.1,
            collateral_tail_used: c.1,
            source_kind: "conditional_projection".into(),
            information_unix_seconds: market.information_unix_seconds,
            source: market.identity.clone(),
            index_id: r.binding.index_id.clone(),
            calendar_mapping_verified: false,
        });
    }
    if !pv.is_finite() || !annuity.is_finite() || annuity <= 0. || !floating_pv.is_finite() {
        return Err("aggregate PV/annuity domain".into());
    }
    Ok(Valuation {
        scenario_id: scenario.into(),
        pv,
        par_fixed_rate: floating_pv / annuity,
        fixed_annuity: annuity,
        projections: rows,
    })
}

/// All input/work/escaped output admission precedes allocation of projection rows.
pub fn evaluate_with_check(
    r: &Request,
    mut check: impl FnMut() -> bool,
) -> Result<ResultData, String> {
    let admission = admit(r, &mut check)?;
    let zero = Shift {
        collateral: 0.,
        index: 0.,
    };
    let original = project(r, &r.original.market, zero, "original", &mut check)?;
    let current = project(r, &r.current_market, zero, "current", &mut check)?;
    let b = r.risk_bump;
    let mut risks = Vec::with_capacity(3);
    for (id, c, i) in [("collateral", b, 0.), ("index", 0., b), ("parallel", b, b)] {
        let down = project(
            r,
            &r.current_market,
            Shift {
                collateral: -c,
                index: -i,
            },
            &format!("{id}_down"),
            &mut check,
        )?;
        let up = project(
            r,
            &r.current_market,
            Shift {
                collateral: c,
                index: i,
            },
            &format!("{id}_up"),
            &mut check,
        )?;
        let dollar_pv01 = (down.pv - up.pv) * 0.0001 / (2. * b);
        risks.push(RiskComparison {
            domain: id.into(),
            down,
            up,
            dollar_pv01,
        });
    }
    checkpoint(&mut check)?;
    let result = ResultData {
        schema: SCHEMA.into(),
        model_version: SCHEMA.into(),
        measure: "pricing".into(),
        source_status: "supplied_curves_deterministic_asof_projection".into(),
        cache_parent: r.clone(),
        admission,
        original,
        current,
        risks,
        observed_fixings: false,
        stochastic_dynamics: false,
        smile_adoption: false,
        physical_cash: false,
        production_adoption: false,
        calendar_mapping_verified: false,
        paired_history_policy: "same_contract_dates_deterministic_bumps_no_random_draws-1".into(),
    };
    // Conservative preflight is authoritative; this is an assertion of its bound, not late admission.
    let actual_bytes = serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len();
    if actual_bytes > result.admission.retained_bytes {
        return Err("internal retained-byte bound violated".into());
    }
    checkpoint(&mut check)?;
    Ok(result)
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_with_check(r, || false)
}

pub(crate) fn validate(r: &Request) -> Result<Admission, String> {
    let result = validation::admit(r, &mut || portfolio_compute_control::checkpoint().is_err());
    portfolio_compute_control::checkpoint()?;
    result
}
