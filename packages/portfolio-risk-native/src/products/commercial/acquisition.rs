//! Complete-origin contractual transfer -> buyer-owned opening. No seller book.
//! Monetary inventories are constructed by native source replay, never decoded.
use super::{
    factor_source::{owner::NativeFactors, segments::FactorPolicy},
    source_income,
};
use crate::products::term::contracts::{self as term, indexed};
use portfolio_model_core::{credit_path_income as carry, strict_json};
use serde::{Deserialize, Serialize};

pub(crate) mod counted;
pub use counted::SourceWorkPolicy;

pub const SCHEMA: &str = "commercial-performing-secondary-acquisition-1";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CouponAllocation {
    pub period_id: String,
    pub purchased_accrued_cost: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regime {
    pub review: term::Identity,
    pub framework: String,
    pub adoption_version: term::Identity,
    pub effective_from_ordinal: i32,
    pub initial_credit_policy: String,
    pub income_policy: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transfer {
    pub schema: String,
    pub review: term::Identity,
    /// Full contractual source from its true origin; contains no seller EIR/cost.
    pub contract_origin: indexed::ContractPathRequest,
    pub economic_ordinal: i32,
    pub recognition_ordinal: i32,
    pub settlement_ordinal: i32,
    pub coincident_order: String,
    pub conveyance: String,
    pub loan_clean_cost: f64,
    pub transaction_cost: f64,
    pub transaction_cost_policy: String,
    pub coupon_allocations: Vec<CouponAllocation>,
    pub regime: Regime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_work_policy: Option<SourceWorkPolicy>,
}
#[derive(Debug, Clone, Serialize)]
pub struct TransferredCoupon {
    pub period_id: String,
    pub native_audit_index: usize,
    pub native_cash_index: usize,
    pub opening_entitlement_principal: f64,
    pub purchased_factor: f64,
    pub purchased_legal_interest: f64,
    pub allocated_cost: f64,
}
/// Only `compile` constructs this authority; no caller monetary checkpoint.
#[derive(Debug, Serialize)]
pub struct NativeTransfer {
    contract_origin_ordinal: i32,
    acquisition_ordinal: i32,
    principal: f64,
    purchased_legal_interest: f64,
    buyer_cost: f64,
    buyer_book_adjustment: f64,
    coupons: Vec<TransferredCoupon>,
    full_contractual_source: indexed::ContractPathResult,
    seller_income_inherited: bool,
    accounting_approval: bool,
}
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn same<T: Serialize>(a: &T, b: &T) -> Result<(), String> {
    check()?;
    if strict_json::encode(a, 1 << 20)? != strict_json::encode(b, 1 << 20)? {
        return Err("secondary transfer contractual origin/conveyance identity differs".into());
    }
    Ok(())
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-12 * b.abs()
}
/// Structural reservations before either current/original factor generation.
pub(crate) fn admit(r: &source_income::Request) -> Result<(usize, usize), String> {
    check()?;
    let t = r
        .transfer
        .as_ref()
        .ok_or("secondary transfer policy absent")?;
    let o = &t.contract_origin;
    let at = r.original.asof_ordinal;
    let mut metadata = 0;
    for id in [&t.review, &t.regime.review, &t.regime.adoption_version] {
        term::identity(id, &mut metadata)?;
    }
    if t.schema != SCHEMA
        || t.economic_ordinal != at
        || t.recognition_ordinal != at
        || t.settlement_ordinal != at
        || o.asof_ordinal >= at
        || t.regime.effective_from_ordinal > at
        || t.coincident_order != "transfer_before_acquisition_day_service_and_cash-1"
        || t.conveyance != "all_remaining_principal_and_unsettled_coupon_rights-1"
        || t.regime.framework != "reviewed_model_policy-1"
        || t.regime.initial_credit_policy != "performing_no_initial_credit_grossup-1"
        || t.regime.income_policy != "dirty_buyer_loan_carry_original_effective_yield-1"
        || t.transaction_cost_policy != "explicit_zero_transaction_cost-1"
        || t.transaction_cost != 0.
        || t.coupon_allocations.len() != r.original.periods.len()
        || t.coupon_allocations.len() > 4096
        || !matches!(
            o.principal_plan,
            indexed::PrincipalPlan::Supplied { .. }
                | indexed::PrincipalPlan::DatedSuppliedPeriodOpening { .. }
        )
        || o.periods.is_empty()
        || o.periods[0].start_ordinal != o.asof_ordinal
        || o.periods
            .windows(2)
            .any(|w| w[0].payment_ordinal >= w[1].payment_ordinal)
        || o.periods.iter().any(|p| p.payment_ordinal < p.end_ordinal)
    {
        return Err("explicit bounded performing complete-origin transfer, reviewed regime, coincident dates and whole conveyed rights required".into());
    }
    portfolio_model_core::cashflow::validate_ordinal(t.regime.effective_from_ordinal)?;
    if o.path != r.original.path
        || o.assumption != r.original.assumption
        || o.scenario_id != r.original.scenario_id
        || o.schema != r.original.schema
        || o.model_version != r.original.model_version
    {
        return Err("secondary original source path/model provenance differs".into());
    }
    term::nonnegative(t.loan_clean_cost)?;
    let mut cost = t.loan_clean_cost;
    for (a, p) in t.coupon_allocations.iter().zip(&r.original.periods) {
        if a.period_id != p.period_id {
            return Err("secondary transfer exact ordered coupon allocation required".into());
        }
        cost = term::amount(cost + term::amount(a.purchased_accrued_cost)?)?;
    }
    if !near(cost, r.acquisition.cost) {
        return Err(
            "secondary clean/accrued/transaction cost does not reconcile dirty buyer cost".into(),
        );
    }
    for current in [&r.original, &r.current] {
        if current.measure != o.measure
            || current.contract != o.contract
            || current.currency != o.currency
            || current.monetary_unit != o.monetary_unit
            || current.position_id != o.position_id
            || current.direction != o.direction
            || current.tail_end_ordinal != o.tail_end_ordinal
        {
            return Err(
                "secondary transfer original contract, units or complete tail changed".into(),
            );
        }
    }
    if indexed::principal::selected(&o.principal_plan) {
        indexed::principal::validate(o)?;
        let wanted: Vec<_> = indexed::principal::events(&o.principal_plan)
            .iter()
            .filter(|e| e.payment_ordinal >= at)
            .cloned()
            .collect();
        for buyer in [&r.original, &r.current] {
            if !indexed::principal::selected(&buyer.principal_plan) {
                return Err(
                    "dated principal transfer requires exact independently dated buyer plan".into(),
                );
            }
            same(
                &wanted,
                &indexed::principal::events(&buyer.principal_plan).to_vec(),
            )?;
            indexed::principal::validate(buyer)?;
        }
    } else if indexed::principal::selected(&r.original.principal_plan)
        || indexed::principal::selected(&r.current.principal_plan)
    {
        return Err("dated buyer principal requires the same full-origin principal policy".into());
    }
    let suffix: Vec<_> = o
        .periods
        .iter()
        .filter(|p| p.payment_ordinal >= at)
        .collect();
    if suffix.len() != r.original.periods.len() {
        return Err("secondary transfer must convey every unsettled original coupon".into());
    }
    for ((p, q), c) in suffix
        .iter()
        .zip(&r.original.periods)
        .zip(&r.current.periods)
    {
        same(*p, q)?;
        // The already served part cannot be re-earned from revised past rates.
        if p.start_ordinal < at {
            match (&p.coupon, &c.coupon) {
                (indexed::Coupon::Fixed { .. }, _) => same(&p.coupon, &c.coupon)?,
                (
                    indexed::Coupon::Indexed { observations, .. },
                    indexed::Coupon::Indexed {
                        observations: new, ..
                    },
                ) => {
                    if observations.len() != new.len() {
                        return Err("secondary acquired observation inventory changed".into());
                    }
                    let mut masked = c.coupon.clone();
                    if let indexed::Coupon::Indexed {
                        observations: future,
                        ..
                    } = &mut masked
                    {
                        for ((a, b), m) in observations.iter().zip(new).zip(future) {
                            if a.interest_start_ordinal < at {
                                same(a, b)?;
                            } else {
                                *m = a.clone();
                            }
                        }
                    }
                    // Margin, day-count mappings and compounding conventions
                    // that determined the acquired stock are immutable too.
                    same(&p.coupon, &masked)?;
                }
                _ => return Err("secondary acquired coupon type changed".into()),
            }
        }
        if let indexed::Coupon::Indexed {
            observations,
            reset_cutoff_unix_seconds,
            ..
        } = &p.coupon
        {
            let cut = (i64::from(at) - 719163) * 86400;
            for obs in observations
                .iter()
                .filter(|x| x.interest_start_ordinal < at)
            {
                let available = match &obs.source {
                    indexed::RateSource::Observed {
                        publication_unix_seconds,
                        ..
                    } => *publication_unix_seconds,
                    indexed::RateSource::ConditionalProjection {
                        information_unix_seconds,
                        ..
                    } => *information_unix_seconds,
                    indexed::RateSource::ConditionalPublishedProjection {
                        publication_unix_seconds,
                        ..
                    } => *publication_unix_seconds,
                };
                if available
                    .max(obs.selection_cutoff_unix_seconds)
                    .max(reset_cutoff_unix_seconds.unwrap_or(0))
                    > cut
                {
                    return Err("unquantified acquired coupon prefixes require separate contingent-transfer ownership".into());
                }
            }
        }
    }
    let bytes = strict_json::serialized_size(t, 1 << 20)?;
    let history_days =
        usize::try_from(at - o.asof_ordinal).map_err(|_| "secondary history span")?;
    if history_days > 1080 {
        return Err("secondary full-origin history exceeds1080-day transfer tier".into());
    }
    // Reserve full origin arithmetic, retained metadata/audit and transfer lots.
    let finite = counted::Authority::selected(r, counted::Role::Origin);
    let a = if let Some(finite) = finite.as_ref() {
        NativeFactors::admit_finite_transfer(o, &default_policy(), 0, o.measure, finite, false)?
    } else {
        NativeFactors::admit_for_measure(o, &default_policy(), 0, o.measure)?
    };
    // prepare_origin() invokes build_transfer_path once per evaluated owner.
    // Original/current buyer factor compiles remain separately reserved by the
    // credit compiler; both retained origin/raw bodies are still charged below.
    let work = a
        .work_upper
        .checked_add(
            bytes * 8
                + r.original.periods.len() * 512
                + if indexed::principal::selected(&o.principal_plan) {
                    r.original
                        .periods
                        .len()
                        .checked_mul(o.periods.len())
                        .and_then(|n| n.checked_mul(512))
                        .ok_or("secondary principal basis work overflow")?
                } else {
                    0
                },
        )
        .ok_or("secondary work overflow")?;
    if indexed::principal::selected(&o.principal_plan) {
        let count = r.original.periods.len();
        let live = indexed::principal::AcquiredBasis::retained_upper(count)?
            .checked_add(
                count
                    .checked_mul(2 * std::mem::size_of::<TransferredCoupon>())
                    .ok_or("secondary coupon inventory bytes overflow")?
                    + 64,
            )
            .ok_or("secondary sealed basis bytes overflow")?;
        if live > count * 4096 {
            return Err("secondary sealed basis exceeds reserved coupon inventory".into());
        }
    }
    let retained = a
        .retained_bytes_upper
        .checked_mul(2)
        .and_then(|b| b.checked_add(bytes * 20 + r.original.periods.len() * 4096))
        .ok_or("secondary bytes overflow")?;
    Ok((work, retained))
}
// Origin admission has no monetary factor queries. Actual factor policy is
// independently admitted by the normal compiler before both sources run.
fn default_policy() -> FactorPolicy {
    FactorPolicy {
        review: term::Identity {
            id: "secondary-origin-admission".into(),
            revision: "1".into(),
        },
        clock: "credit_act365_elapsed_from_source_asof-1".into(),
        partial_calendar_day: "simple_partial_native_daycount-1".into(),
        overnight: "observable_index_prefix_simple_margin-1".into(),
        observation_mapping: "selected_observation_weight_over_interest_daycount-1".into(),
        full_checkpoints: "retain_actual_native_endpoints-1".into(),
        terminal_obligations: None,
    }
}
pub(crate) struct OriginOwner {
    full: indexed::ContractPathResult,
    basis: Option<indexed::principal::AcquiredBasis>,
    principal: f64,
}
pub(crate) fn prepare_origin(r: &source_income::Request) -> Result<OriginOwner, String> {
    admit(r)?;
    let t = r.transfer.as_ref().ok_or("secondary policy absent")?;
    let full = if t.source_work_policy.is_some() {
        counted::build_origin(r)?
    } else {
        indexed::build_transfer_path(&t.contract_origin, t.economic_ordinal)?
    };
    let principal = full
        .schedule
        .events
        .iter()
        .filter(|e| e.payment_ordinal < t.economic_ordinal)
        .try_fold(t.contract_origin.opening_principal, |p, e| {
            term::amount(p - e.components.principal)
        })?;
    if !near(principal, r.current.opening_principal) {
        return Err(
            "secondary remaining principal differs from complete original cash replay".into(),
        );
    }
    let basis = if indexed::principal::selected(&t.contract_origin.principal_plan) {
        Some(indexed::principal::AcquiredBasis::from_origin(
            &r.original,
            &full,
        )?)
    } else {
        None
    };
    Ok(OriginOwner {
        full,
        basis,
        principal,
    })
}
impl OriginOwner {
    pub(crate) fn basis(&self) -> Option<&indexed::principal::AcquiredBasis> {
        self.basis.as_ref()
    }
}
pub(crate) fn compile(
    r: &source_income::Request,
    current: &NativeFactors,
    origin: OriginOwner,
) -> Result<NativeTransfer, String> {
    let t = r.transfer.as_ref().ok_or("secondary policy absent")?;
    let OriginOwner {
        full, principal, ..
    } = origin;
    let factors = current.cumulative_coupon_factors(0.)?;
    let mut coupons = Vec::with_capacity(factors.len());
    let mut accrued = 0.;
    for ((audit, cash, factor), allocation) in factors.into_iter().zip(&t.coupon_allocations) {
        let a = &current.source().audit[audit];
        let original = full
            .audit
            .iter()
            .find(|p| p.period_id == a.period_id)
            .ok_or("secondary original coupon audit absent")?;
        if !near(a.opening_principal, original.opening_principal)
            || !near(a.principal_paid, original.principal_paid)
        {
            return Err(
                "secondary coupon entitlement principal differs from full-origin replay".into(),
            );
        }
        let legal = term::amount(a.opening_principal * factor)?;
        if !near(legal, allocation.purchased_accrued_cost) {
            return Err(
                "first secondary tier requires exact par allocated accrued-right cost".into(),
            );
        }
        accrued = term::amount(accrued + legal)?;
        coupons.push(TransferredCoupon {
            period_id: a.period_id.clone(),
            native_audit_index: audit,
            native_cash_index: cash,
            opening_entitlement_principal: a.opening_principal,
            purchased_factor: factor,
            purchased_legal_interest: legal,
            allocated_cost: allocation.purchased_accrued_cost,
        });
    }
    Ok(NativeTransfer {
        contract_origin_ordinal: t.contract_origin.asof_ordinal,
        acquisition_ordinal: t.economic_ordinal,
        principal,
        purchased_legal_interest: accrued,
        buyer_cost: r.acquisition.cost,
        buyer_book_adjustment: term::amount(r.acquisition.cost - principal - accrued)?,
        coupons,
        full_contractual_source: full,
        seller_income_inherited: false,
        accounting_approval: false,
    })
}
impl NativeTransfer {
    pub(crate) fn apply(&self, r: &mut carry::Request) -> Result<(), String> {
        for p in &mut r.raw_credit_path.paths {
            if p.initial_live_state != portfolio_model_core::credit::CreditState::Performing {
                return Err("secondary acquisition of existing default/watch/delinquent state is unsupported".into());
            }
            p.accrued_interest = self.purchased_legal_interest;
            p.book_adjustment = self.buyer_book_adjustment;
        }
        Ok(())
    }
    pub(crate) fn coupons(&self) -> &[TransferredCoupon] {
        &self.coupons
    }
}
