//! Complete-origin holding continuation; no monetary checkpoint or CTMC restart.
use crate::{commercial_credit_compiler as owner, term_enhancements::indexed};
use portfolio_compute_control::checkpoint;
use portfolio_model_core::{
    commercial_coupon_accounting as book, credit_path_income as carry, strict_json,
};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "commercial-holding-continuation-1";
const MAX_RAW: usize = 1 << 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub cut_ordinal: i32,
    pub completed_source: owner::Request,
    pub revised_source: owner::Request,
}

/// Constructed only by replaying complete raw owners. Never deserializable.
#[derive(Debug, Serialize)]
pub struct Opening {
    path_key: u64,
    through_native_group_index: usize,
    book: book::Snapshot,
    carry: carry::CarryPoint,
    subsequent_native_group_indices: Vec<usize>,
    subsequent_cash: f64,
    subsequent_effective_interest: f64,
    subsequent_basis_realization: f64,
}

#[derive(Debug, Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_source: Request,
    pub cut_year: f64,
    pub completed: owner::ResultData,
    pub revised: owner::ResultData,
    pub openings: Vec<Opening>,
    pub admitted_work_upper: usize,
    pub retained_bytes_upper: usize,
    pub complete_origin_replayed: bool,
    pub credit_clock_restarted: bool,
    pub original_anchor_refitted: bool,
    pub second_acquisition_posted: bool,
    pub mixed_ledger_adoption: bool,
    pub verified_prior_publication: bool,
    pub secondary_acquisition_adoption: bool,
    pub empirical_validation: bool,
}

fn same<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    checkpoint()?;
    Ok(strict_json::encode(a, 32 << 20)? == strict_json::encode(b, 32 << 20)?)
}
fn require_same<T: Serialize>(a: &T, b: &T) -> Result<(), String> {
    if same(a, b)? {
        Ok(())
    } else {
        Err("commercial completed prefix or original authority changed".into())
    }
}
fn sum(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("commercial continuation admission overflow".into())
}

/// Current contractual periods already begun belong to the completed prefix.
/// Future projections may change, but available observations never may.
fn source_prefix(
    a: &indexed::ContractPathRequest,
    b: &indexed::ContractPathRequest,
    cut: i32,
) -> Result<(), String> {
    let mut av = serde_json::to_value(a).map_err(|e| e.to_string())?;
    let mut bv = serde_json::to_value(b).map_err(|e| e.to_string())?;
    for v in [&mut av, &mut bv] {
        v.as_object_mut()
            .ok_or("commercial source object required")?
            .remove("periods");
    }
    require_same(&av, &bv)?;
    if a.periods.len() != b.periods.len() {
        return Err("commercial continuation contractual period coverage changed".into());
    }
    for (old, new) in a.periods.iter().zip(&b.periods) {
        checkpoint()?;
        // Dates, entitlement identities and principal plan are retained even for
        // future periods. Only future coupon/fee assumptions can be revised here.
        require_same(
            &(
                old.period_id.as_str(),
                old.start_ordinal,
                old.end_ordinal,
                old.payment_ordinal,
                old.day_count,
            ),
            &(
                new.period_id.as_str(),
                new.start_ordinal,
                new.end_ordinal,
                new.payment_ordinal,
                new.day_count,
            ),
        )?;
        if old.start_ordinal < cut || old.payment_ordinal <= cut {
            require_same(old, new)?;
        }
        let available = |p: &indexed::ContractPeriod| match &p.coupon {
            indexed::Coupon::Indexed { observations, .. } => observations
                .iter()
                .filter(|o| o.fixing_ordinal <= cut)
                .cloned()
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        require_same(&available(old), &available(new))?;
    }
    Ok(())
}

fn raw_prefix(r: &Request, cut: f64) -> Result<(), String> {
    let a = &r.completed_source;
    let b = &r.revised_source;
    require_same(&a.source.original, &b.source.original)?;
    require_same(&a.source.acquisition, &b.source.acquisition)?;
    require_same(&a.source.policy, &b.source.policy)?;
    require_same(&a.factor_policy, &b.factor_policy)?;
    require_same(&a.performing_draw_tranches, &b.performing_draw_tranches)?;
    require_same(&a.prepayment_terms, &b.prepayment_terms)?;
    require_same(&a.maturity_basis_policy, &b.maturity_basis_policy)?;
    require_same(&a.reviewed_credit_actions, &b.reviewed_credit_actions)?;
    source_prefix(&a.source.current, &b.source.current, r.cut_ordinal)?;
    let mut previous = 0.;
    if a.credit.intervals.len() != b.credit.intervals.len() {
        return Err("commercial continuation credit interval coverage changed".into());
    }
    for (i, (old, new)) in a
        .credit
        .intervals
        .iter()
        .zip(&b.credit.intervals)
        .enumerate()
    {
        require_same(&old.end_year, &new.end_year)?;
        if previous < cut {
            require_same(old, new)?;
            require_same(
                &a.accounting
                    .recognition
                    .iter()
                    .filter(|p| p.interval_index == i)
                    .collect::<Vec<_>>(),
                &b.accounting
                    .recognition
                    .iter()
                    .filter(|p| p.interval_index == i)
                    .collect::<Vec<_>>(),
            )?;
            require_same(
                &a.accounting_rates
                    .iter()
                    .filter(|p| p.interval_index == i)
                    .collect::<Vec<_>>(),
                &b.accounting_rates
                    .iter()
                    .filter(|p| p.interval_index == i)
                    .collect::<Vec<_>>(),
            )?;
        }
        previous = old.end_year;
    }
    let mut av = serde_json::to_value(&a.credit).map_err(|e| e.to_string())?;
    let mut bv = serde_json::to_value(&b.credit).map_err(|e| e.to_string())?;
    for v in [&mut av, &mut bv] {
        v.as_object_mut()
            .ok_or("credit object required")?
            .remove("intervals");
    }
    require_same(&av, &bv)?;
    // Accounting identity/application/disposition are retained. Prospective
    // recognition/rate changes are permitted only in wholly future intervals.
    let mut ap = a.accounting.clone();
    let mut bp = b.accounting.clone();
    ap.recognition.clear();
    bp.recognition.clear();
    require_same(&ap, &bp)?;
    let past: Vec<_> = a
        .source
        .current_accounting_rate_reviews
        .iter()
        .filter(|p| p.reset_ordinal < r.cut_ordinal)
        .collect();
    let now: Vec<_> = b
        .source
        .current_accounting_rate_reviews
        .iter()
        .filter(|p| p.reset_ordinal < r.cut_ordinal)
        .collect();
    require_same(&past, &now)
}

/// Joined coordinators reserve both complete owners before generating any path.
#[derive(Debug)]
pub struct Admission {
    pub cut_year: f64,
    pub work_upper: usize,
    pub retained_bytes_upper: usize,
}

pub fn admit(r: &Request) -> Result<Admission, String> {
    checkpoint()?;
    if portfolio_compute_control::active().is_none() {
        return Err("commercial continuation requires original execution control".into());
    }
    if r.schema != SCHEMA {
        return Err("explicit commercial holding continuation schema required".into());
    }
    let raw = strict_json::serialized_size(r, MAX_RAW)?;
    let origin = r.completed_source.source.current.asof_ordinal;
    let elapsed = r
        .cut_ordinal
        .checked_sub(origin)
        .ok_or("commercial cut calendar overflow")?;
    if elapsed <= 0 {
        return Err("commercial holding cut must follow original acquisition".into());
    }
    let cut = f64::from(elapsed) / 365.;
    raw_prefix(r, cut)?;
    // Both complete tails plus the prefix comparison, sealed opening copies and
    // retained suffix indices are charged before either owner is materialized.
    let a = owner::admit(&r.completed_source)?;
    let b = owner::admit(&r.revised_source)?;
    if cut > a.full_credit_end_year || cut > b.full_credit_end_year {
        return Err("commercial cut exceeds complete original tail".into());
    }
    let points = sum(a.carry_points_upper, b.carry_points_upper)?;
    let cells = points
        .checked_mul(sum(a.lots_per_path_upper.max(b.lots_per_path_upper), 1)?)
        .ok_or("commercial continuation upper overflow")?;
    let work = sum(
        sum(a.joined_work_upper, b.joined_work_upper)?,
        sum(
            cells
                .checked_mul(256)
                .ok_or("commercial continuation upper overflow")?,
            raw * 8,
        )?,
    )?;
    let bytes = sum(
        sum(a.joined_bytes_upper, b.joined_bytes_upper)?,
        sum(
            cells
                .checked_mul(8192)
                .ok_or("commercial continuation upper overflow")?,
            raw * 20,
        )?,
    )?;
    if work > 8_000_000 || bytes > 128 << 20 {
        return Err(
            "commercial complete-origin continuation exceeds8M/128MiB before generation".into(),
        );
    }
    Ok(Admission {
        cut_year: cut,
        work_upper: work,
        retained_bytes_upper: bytes,
    })
}

pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    let admission = admit(r)?;
    let cut = admission.cut_year;
    let work = admission.work_upper;
    let bytes = admission.retained_bytes_upper;
    let completed = owner::evaluate(&r.completed_source)?;
    let revised = owner::evaluate(&r.revised_source)?;
    let mut openings = Vec::new();
    for path in &r.completed_source.credit.paths {
        checkpoint()?;
        let key = path.path_key;
        let old: Vec<_> = completed
            .accounting()
            .groups()
            .iter()
            .enumerate()
            .filter(|(_, g)| g.path_key == key && g.year <= cut)
            .collect();
        let new: Vec<_> = revised
            .accounting()
            .groups()
            .iter()
            .enumerate()
            .filter(|(_, g)| g.path_key == key && g.year <= cut)
            .collect();
        require_same(
            &old.iter().map(|(_, g)| *g).collect::<Vec<_>>(),
            &new.iter().map(|(_, g)| *g).collect::<Vec<_>>(),
        )?;
        let &(index, group) = new
            .last()
            .ok_or("commercial complete opening prefix absent")?;
        let carry_prefix = |o: &owner::ResultData| {
            o.carry()
                .carry_points()
                .iter()
                .filter(|p| p.path_key == key && p.year <= cut)
                .cloned()
                .collect::<Vec<_>>()
        };
        let old_carry = carry_prefix(&completed);
        let new_carry = carry_prefix(&revised);
        require_same(&old_carry, &new_carry)?;
        let point = new_carry
            .last()
            .ok_or("commercial opening carry prefix absent")?;
        if group.year.to_bits() != cut.to_bits() && !group.next.in_default {
            return Err("commercial holding cut requires an exact native earning boundary or frozen default owner".into());
        }
        // Ordered future records are borrowed from this same complete-origin
        // replay. No opening snapshot is fed back into a second credit engine.
        let mut opening = Opening {
            path_key: key,
            through_native_group_index: index,
            book: group.next.clone(),
            carry: point.clone(),
            subsequent_native_group_indices: Vec::new(),
            subsequent_cash: 0.,
            subsequent_effective_interest: 0.,
            subsequent_basis_realization: 0.,
        };
        for (i, g) in revised
            .accounting()
            .groups()
            .iter()
            .enumerate()
            .filter(|(_, g)| g.path_key == key && g.year > cut)
        {
            checkpoint()?;
            if g.phase == "reviewed_opening_snapshot" {
                return Err("commercial continuation contains a second acquisition".into());
            }
            opening.subsequent_native_group_indices.push(i);
            opening.subsequent_cash += g.cash;
            opening.subsequent_effective_interest += g.effective_interest;
            opening.subsequent_basis_realization += g.basis_realization;
        }
        if [
            opening.subsequent_cash,
            opening.subsequent_effective_interest,
            opening.subsequent_basis_realization,
        ]
        .iter()
        .any(|v| !v.is_finite())
        {
            return Err("commercial continuation monetary overflow".into());
        }
        openings.push(opening);
    }
    checkpoint()?;
    Ok(ResultData {
        schema: SCHEMA,
        raw_source: r.clone(),
        cut_year: cut,
        completed,
        revised,
        openings,
        admitted_work_upper: work,
        retained_bytes_upper: bytes,
        complete_origin_replayed: true,
        credit_clock_restarted: false,
        original_anchor_refitted: false,
        second_acquisition_posted: false,
        mixed_ledger_adoption: false,
        verified_prior_publication: false,
        secondary_acquisition_adoption: false,
        empirical_validation: false,
    })
}
