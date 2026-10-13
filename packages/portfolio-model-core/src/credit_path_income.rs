//! Copied-only reviewed carry primitive, inside the existing exact credit-path loop.
//! This is not a corporate raw-source compiler, ledger owner, or accounting approval.
use crate::credit_path::{self, CreditPathRequest, CreditPathResult, PathDefault, PathOpening};
use crate::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

pub const SCHEMA: &str = "credit-path-income-proof-1";
pub const MAX_RAW_BYTES: usize = 1 << 20;
pub const MAX_RETAINED_BYTES: usize = 32 << 20;

/// Borrowed compiler-owned source authority, not a deserializable caller override.
/// The source compiler must reserve generation before constructing this view.
pub struct NativeFactorBound {
    pub maximum_atoms_per_query: usize,
    pub retained_source_bytes: usize,
    pub source_work: usize,
}
pub struct NativeFactorQuery {
    pub path_key: u64,
    pub interval_index: usize,
    pub origin_index: usize,
    pub start_year: f64,
    pub end_year: f64,
}
pub struct NativeFactorAtom<'a> {
    pub coupon_id: &'a str,
    pub source_event_id: &'a str,
    pub source_id: &'a str,
    pub source_revision: &'a str,
    pub captured_segment_index: usize,
    pub delta_factor: f64,
}
/// Native-only immutable source law; no JSON monetary override is accepted.
#[derive(Debug, Clone, Serialize)]
pub struct NativeCouponEntitlement {
    pub native_audit_index: usize,
    pub native_cash_index: usize,
    pub opening_entitlement_principal: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct NativeContractualCash {
    pub coupon_id: String,
    pub source_event_id: String,
    pub native_cash_index: usize,
    pub principal: f64,
    pub interest: f64,
    /// Integral contractual fee cash. It does not change nominal principal or
    /// legal coupon accrual; retained original EIR owns its basis recognition.
    #[serde(skip_serializing_if = "zero_fee")]
    pub fee: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tranche_id: Option<String>,
}
fn zero_fee(value: &f64) -> bool {
    *value == 0.
}
#[derive(Debug, Clone, Serialize)]
pub struct SourceCashReceipt {
    pub path_key: u64,
    pub year: f64,
    pub cash: NativeContractualCash,
}
#[derive(Debug, Clone, Serialize)]
pub struct NativeCouponRecovery {
    pub path_key: u64,
    pub default_year: f64,
    pub recovery_index: usize,
    pub year: f64,
    pub coupon_id: String,
    pub source_event_id: String,
    pub native_audit_index: usize,
    pub earning_origin_index: usize,
    pub interest: f64,
    pub applied_in_horizon: bool,
}
/// Native factor authority; money is always derived by this owner.
pub struct NativeCumulativeCouponFactor {
    pub coupon_id: String,
    pub native_audit_index: usize,
    pub native_cash_index: usize,
    pub opening_entitlement_principal: f64,
    pub cumulative_factor: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct CanonicalCouponLegalBalance {
    pub coupon_id: String,
    pub native_audit_index: usize,
    pub native_cash_index: usize,
    pub opening_entitlement_principal: f64,
    pub cumulative_factor: f64,
    pub contractual_earned: f64,
    pub actual_interest_collected: f64,
    pub legal_remaining: f64,
}
pub struct NativeExitQuery<'a> {
    pub path_key: u64,
    pub event_id: &'a str,
    pub year: f64,
    pub principal: f64,
    pub legal_interest: f64,
    pub is_prepayment: bool,
}
/// Borrowed native contractual terms, never caller-prepared cash.
pub struct NativeExitCash {
    pub principal_cash: f64,
    pub fee: f64,
}
pub struct NativeDrawQuery<'a> {
    pub path_key: u64,
    pub event_id: &'a str,
    pub year: f64,
    pub origin_index: usize,
    pub undrawn: f64,
}
pub enum NativeDrawResolution {
    Skip,
    Fund {
        tranche_id: String,
        principal: f64,
        cost: f64,
        original_annual_effective_rate: f64,
    },
}
#[derive(Debug, Clone, Serialize)]
pub struct DrawTrancheCarry {
    pub tranche_id: String,
    pub original_annual_effective_rate: f64,
    pub nominal_draw: f64,
    pub acquisition_cost: f64,
    pub gross_carry: f64,
}
pub(crate) struct NativeRecoveryQuery {
    pub path_key: u64,
    pub default_year: f64,
    pub recovery_index: usize,
    pub year: f64,
    pub fraction: f64,
    pub weight: f64,
    pub fallback: f64,
    pub horizon: f64,
}
pub trait NativeEarnedFactors {
    fn begin_path(&mut self, _path_key: u64) -> Result<(), String> {
        Ok(())
    }
    fn draw(
        &mut self,
        _query: &NativeDrawQuery<'_>,
    ) -> Result<Option<NativeDrawResolution>, String> {
        Ok(None)
    }
    fn tranche_accounting_rate(
        &self,
        _tranche_id: &str,
        _interval: usize,
        _origin: usize,
    ) -> Result<Option<f64>, String> {
        Ok(None)
    }
    fn cumulative_coupon_factors(
        &self,
        _year: f64,
    ) -> Result<Option<Vec<NativeCumulativeCouponFactor>>, String> {
        Ok(None)
    }
    fn boundary_applies(&self, _path_key: u64, _event_id: &str) -> Result<bool, String> {
        Ok(true)
    }
    fn exit_cash(&self, _query: &NativeExitQuery<'_>) -> Result<Option<NativeExitCash>, String> {
        Ok(None)
    }
    fn bound(&self) -> NativeFactorBound;
    fn maximum_source_cash_per_path(&self) -> usize {
        0
    }
    fn maximum_gross_coupon_claims_per_path(&self) -> usize {
        0
    }
    /// Legacy providers return None and retain actual live-P multiplication.
    /// New source compilers resolve this from the exact native period audit.
    fn entitlement(
        &self,
        _atom: &NativeFactorAtom<'_>,
    ) -> Result<Option<NativeCouponEntitlement>, String> {
        Ok(None)
    }
    /// Only exact native cash event IDs may opt into source-owned settlement.
    fn contractual_cash(
        &self,
        _event: &str,
        _year: f64,
    ) -> Result<Option<NativeContractualCash>, String> {
        Ok(None)
    }
    fn earned(
        &mut self,
        query: &NativeFactorQuery,
        sink: &mut dyn FnMut(NativeFactorAtom<'_>) -> Result<(), String>,
    ) -> Result<(), String>;
}

#[derive(Debug, Serialize)]
pub struct EarnedOriginContribution {
    pub path_key: u64,
    pub interval_index: usize,
    pub origin_index: usize,
    pub start_year: f64,
    pub end_year: f64,
    pub coupon_id: String,
    pub source_event_id: String,
    pub source_id: String,
    pub source_revision: String,
    pub captured_segment_index: usize,
    pub delta_factor: f64,
    pub actual_principal: f64,
    pub contractual_earned: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_entitlement: Option<NativeCouponEntitlement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalAnchor {
    pub path_key: u64,
    pub original_yield_anchor: String,
    pub acquisition_cost: f64,
    pub original_annual_effective_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionRate {
    pub interval_index: usize,
    pub review: String,
    pub annual_effective_rates: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub raw_credit_path: CreditPathRequest,
    pub original_anchors: Vec<OriginalAnchor>,
    pub recognition_rates: Vec<RecognitionRate>,
    pub policy_review: String,
    pub partial_payment_policy: String,
    pub default_policy: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub raw_bytes: usize,
    pub row_upper: usize,
    pub work_upper: usize,
    pub retained_bytes_upper: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CarryPoint {
    pub path_key: u64,
    pub year: f64,
    pub kind: &'static str,
    pub origin_index: Option<usize>,
    pub principal: f64,
    pub contractual_accrued_interest: f64,
    pub book_adjustment: f64,
    pub gross_carry: f64,
    pub accounting_rate: Option<f64>,
    pub effective_interest: f64,
    pub cash: f64,
    pub basis_realization: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub canonical_legal_coupons: Vec<CanonicalCouponLegalBalance>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub draw_tranches: Vec<DrawTrancheCarry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undrawn_commitment: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecoveryReceipt {
    pub path_key: u64,
    pub default_year: f64,
    pub recovery_index: usize,
    pub year: f64,
    pub principal: f64,
    pub interest: f64,
    pub applied_in_horizon: bool,
}

#[derive(Debug, Serialize)]
pub struct ResultData {
    schema: &'static str,
    raw_request: Request,
    admission: Admission,
    credit: crate::credit_path::CreditPathTraceResult,
    carry_points: Vec<CarryPoint>,
    complete_recovery_receipts: Vec<RecoveryReceipt>,
    native_factor_provider_selected: bool,
    earned_origin_contributions: Vec<EarnedOriginContribution>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    native_source_cash: Vec<SourceCashReceipt>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    native_coupon_recoveries: Vec<NativeCouponRecovery>,
    complete_coupon_settlement_ownership: bool,
    original_anchor_refitted: bool,
    default_interest_stopped: bool,
    corporate_source_binding: bool,
    mixed_ledger_adoption: bool,
    accounting_approval: bool,
}

impl ResultData {
    pub fn native_coupon_recoveries(&self) -> &[NativeCouponRecovery] {
        &self.native_coupon_recoveries
    }
    pub fn native_source_cash(&self) -> &[SourceCashReceipt] {
        &self.native_source_cash
    }
    pub fn earned_origin_contributions(&self) -> &[EarnedOriginContribution] {
        &self.earned_origin_contributions
    }
    pub fn raw_request(&self) -> &Request {
        &self.raw_request
    }
    pub fn credit(&self) -> &crate::credit_path::CreditPathTraceResult {
        &self.credit
    }
    pub fn carry_points(&self) -> &[CarryPoint] {
        &self.carry_points
    }
    pub fn admission(&self) -> &Admission {
        &self.admission
    }
}

fn label(v: &str) -> Result<(), String> {
    if v.is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
        Err("review identities require 1..256 control-free bytes".into())
    } else {
        Ok(())
    }
}
fn rate(v: f64) -> Result<(), String> {
    if !v.is_finite() || !(-0.99..=10.0).contains(&v) {
        Err("accounting rates require finite [-.99,10]".into())
    } else {
        Ok(())
    }
}
fn bounded(v: f64) -> Result<(), String> {
    if !v.is_finite() || v.abs() > MAX_MONETARY_AMOUNT {
        Err("carry monetary domain exceeded".into())
    } else {
        Ok(())
    }
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "carry admission overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "carry admission overflow".into())
}

struct Counter<'a> {
    bytes: usize,
    check: &'a mut dyn FnMut() -> Result<(), String>,
}
impl Write for Counter<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        (self.check)().map_err(io::Error::other)?;
        self.bytes = self
            .bytes
            .checked_add(data.len())
            .ok_or_else(|| io::Error::other("raw overflow"))?;
        if self.bytes > MAX_RAW_BYTES {
            return Err(io::Error::other("carry raw input exceeds 1 MiB"));
        }
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        (self.check)().map_err(io::Error::other)
    }
}

fn admit(
    r: &Request,
    source: Option<&NativeFactorBound>,
    source_cash_per_path: usize,
    coupon_claims_per_path: usize,
    pricing_economics: bool,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<Admission, String> {
    check()?;
    if r.schema != SCHEMA
        || r.partial_payment_policy != "cash_reduces_gross_no_proportional_basis_disposal-1"
        || r.default_policy != "stop_income_transfer_actual_gross_retain_nominal_claims-1"
    {
        return Err("unsupported reviewed carry policy".into());
    }
    label(&r.policy_review)?;
    let p = &r.raw_credit_path;
    if p.measure != crate::credit::CreditMeasure::Physical
        && !(pricing_economics && source.is_some())
    {
        return Err("reviewed carry accounting requires an explicit Physical credit path".into());
    }
    if p.paths.is_empty()
        || p.paths.len() > credit_path::MAX_PATHS
        || p.intervals.is_empty()
        || p.intervals.len() > 4096
        || p.boundaries.len() > 4096
        || p.max_jumps_per_path == 0
        || p.max_jumps_per_path > credit_path::MAX_EVENTS
        || r.original_anchors.len() != p.paths.len()
        || r.recognition_rates.len() != p.intervals.len()
    {
        return Err("carry shape/anchor/rate coverage exceeded".into());
    }
    // At most I+B+J+1 sojourns. There is at most one default per path,
    // with <=32 recovery records and <=32 applied-cash checkpoints.
    // Events, transfers, funded default draw and closing need at most B+J+68.
    let sojourns = add(
        add(p.intervals.len(), p.boundaries.len())?,
        add(p.max_jumps_per_path, 1)?,
    )?;
    let per_path = add(
        sojourns,
        add(add(p.boundaries.len(), p.max_jumps_per_path)?, 68)?,
    )?;
    let mut rows = mul(p.paths.len(), per_path)?;
    if let Some(source) = source {
        if source_cash_per_path > p.boundaries.len() {
            return Err("native source cash bound exceeds complete boundary count".into());
        }
        rows = add(rows, mul(p.paths.len(), source_cash_per_path)?)?;
        if coupon_claims_per_path > 4096 {
            return Err("native gross coupon claims require a bounded 0..4096 source upper".into());
        }
        rows = add(rows, mul(mul(p.paths.len(), coupon_claims_per_path)?, 33)?)?;
        if source.maximum_atoms_per_query == 0 || source.maximum_atoms_per_query > 4096 {
            return Err("native factor atom bound must be 1..4096 per query".into());
        }
        rows = add(
            rows,
            mul(
                mul(p.paths.len(), sojourns)?,
                source.maximum_atoms_per_query,
            )?,
        )?;
    }
    let mut counter = Counter { bytes: 0, check };
    serde_json::to_writer(&mut counter, r).map_err(|e| e.to_string())?;
    counter.flush().map_err(|e| e.to_string())?;
    let raw_bytes = counter.bytes;
    let work = add(
        add(mul(raw_bytes, 4)?, mul(rows, 128)?)?,
        source.map_or(0, |s| s.source_work),
    )?;
    let bytes = add(
        add(mul(raw_bytes, 20)?, mul(rows, 8192)?)?,
        source.map_or(0, |s| s.retained_source_bytes),
    )?;
    if rows > credit_path::MAX_EVENTS || work > credit_path::MAX_WORK || bytes > MAX_RETAINED_BYTES
    {
        return Err("carry aggregate preflight exceeded before native path generation".into());
    }
    for (anchor, path) in r.original_anchors.iter().zip(&p.paths) {
        check()?;
        label(&anchor.original_yield_anchor)?;
        rate(anchor.original_annual_effective_rate)?;
        bounded(anchor.acquisition_cost)?;
        if anchor.path_key != path.path_key
            || anchor.acquisition_cost < 0.0
            || anchor.acquisition_cost.to_bits()
                != (path.principal + path.accrued_interest + path.book_adjustment).to_bits()
        {
            return Err(
                "original acquisition must exactly match ordered opening P+A+B and path key".into(),
            );
        }
    }
    for (i, review) in r.recognition_rates.iter().enumerate() {
        check()?;
        if review.interval_index != i {
            return Err("recognition rate reviews require exact ordered interval coverage".into());
        }
        label(&review.review)?;
        for value in review.annual_effective_rates {
            rate(value)?;
        }
    }
    Ok(Admission {
        raw_bytes,
        row_upper: rows,
        work_upper: work,
        retained_bytes_upper: bytes,
    })
}

pub(crate) struct Owner<'a> {
    request: &'a Request,
    check: &'a mut dyn FnMut() -> Result<(), String>,
    gross: f64,
    path_index: usize,
    points: Vec<CarryPoint>,
    recoveries: Vec<RecoveryReceipt>,
    row_upper: usize,
    factor_source: Option<&'a mut dyn NativeEarnedFactors>,
    maximum_atoms_per_query: usize,
    contributions: Vec<EarnedOriginContribution>,
    source_cash: Vec<SourceCashReceipt>,
    source_cash_per_path: usize,
    path_cash_count: usize,
    coupon_claims_per_path: usize,
    default_lots: Vec<crate::commercial_coupon_accounting::CouponLot>,
    coupon_recoveries: Vec<NativeCouponRecovery>,
    canonical_legal_coupons: Vec<CanonicalCouponLegalBalance>,
    draw_tranches: Vec<DrawTrancheCarry>,
}

impl Owner<'_> {
    pub(crate) fn native_draw(
        &mut self,
        p: &CreditPathResult,
        event_id: &str,
        year: f64,
        origin_index: usize,
    ) -> Result<Option<NativeDrawResolution>, String> {
        self.check()?;
        let Some(source) = self.factor_source.as_deref_mut() else {
            return Ok(None);
        };
        let result = source.draw(&NativeDrawQuery {
            path_key: p.path_key,
            event_id,
            year,
            origin_index,
            undrawn: p.closing_undrawn_commitment,
        })?;
        if let Some(NativeDrawResolution::Fund {
            tranche_id,
            principal,
            cost,
            original_annual_effective_rate,
        }) = &result
        {
            label(tranche_id)?;
            bounded(*principal)?;
            bounded(*cost)?;
            rate(*original_annual_effective_rate)?;
            if *principal < 0.
                || *principal > p.closing_undrawn_commitment
                || *cost < 0.
                || self
                    .draw_tranches
                    .iter()
                    .any(|t| t.tranche_id == *tranche_id)
            {
                return Err("native performing draw exceeds undrawn commitment or repeats tranche ownership".into());
            }
            self.draw_tranches.push(DrawTrancheCarry {
                tranche_id: tranche_id.clone(),
                original_annual_effective_rate: *original_annual_effective_rate,
                nominal_draw: *principal,
                acquisition_cost: *cost,
                gross_carry: *cost,
            });
        }
        Ok(result)
    }
    fn canonical_legal(&mut self, p: &mut CreditPathResult, year: f64) -> Result<(), String> {
        self.check()?;
        let Some(source) = self.factor_source.as_deref() else {
            return Ok(());
        };
        let Some(factors) = source.cumulative_coupon_factors(year)? else {
            return Ok(());
        };
        if factors.len() > self.coupon_claims_per_path {
            return Err("canonical coupon factors exceed admitted gross claims".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut balances = Vec::new();
        let mut legal = 0.;
        for factor in factors {
            self.check()?;
            label(&factor.coupon_id)?;
            if !ids.insert(factor.coupon_id.clone()) || factor.opening_entitlement_principal < 0. {
                return Err("canonical coupon entitlement must be unique and nonnegative".into());
            }
            bounded(factor.opening_entitlement_principal)?;
            if !factor.cumulative_factor.is_finite() {
                return Err("nonfinite canonical coupon factor".into());
            }
            let earned = factor.opening_entitlement_principal * factor.cumulative_factor;
            let collected: f64 = self
                .source_cash
                .iter()
                .filter(|r| r.path_key == p.path_key && r.cash.coupon_id == factor.coupon_id)
                .map(|r| r.cash.interest)
                .sum();
            let remaining = earned - collected;
            bounded(earned)?;
            bounded(collected)?;
            bounded(remaining)?;
            legal += remaining;
            balances.push(CanonicalCouponLegalBalance {
                coupon_id: factor.coupon_id,
                native_audit_index: factor.native_audit_index,
                native_cash_index: factor.native_cash_index,
                opening_entitlement_principal: factor.opening_entitlement_principal,
                cumulative_factor: factor.cumulative_factor,
                contractual_earned: earned,
                actual_interest_collected: collected,
                legal_remaining: remaining,
            });
        }
        bounded(legal)?;
        p.closing_accrued_interest = legal;
        self.canonical_legal_coupons = balances;
        Ok(())
    }
    pub(crate) fn check(&mut self) -> Result<(), String> {
        (self.check)()
    }
    pub(crate) fn boundary_applies(
        &mut self,
        path_key: u64,
        event_id: &str,
    ) -> Result<bool, String> {
        self.check()?;
        self.factor_source
            .as_deref()
            .map_or(Ok(true), |s| s.boundary_applies(path_key, event_id))
    }
    pub(crate) fn native_exit(
        &mut self,
        p: &CreditPathResult,
        event_id: &str,
        year: f64,
        is_prepayment: bool,
    ) -> Result<Option<(f64, f64)>, String> {
        self.check()?;
        let Some(source) = self.factor_source.as_deref() else {
            return Ok(None);
        };
        let query = NativeExitQuery {
            path_key: p.path_key,
            event_id,
            year,
            principal: p.closing_principal,
            legal_interest: p.closing_accrued_interest,
            is_prepayment,
        };
        let Some(terms) = source.exit_cash(&query)? else {
            return Ok(None);
        };
        if event_id.starts_with("native-maturity-basis:")
            && self
                .canonical_legal_coupons
                .iter()
                .any(|c| c.legal_remaining != 0.)
        {
            return Err(
                "reviewed maturity cannot net opposite gross canonical coupon legal claims".into(),
            );
        }
        bounded(terms.principal_cash)?;
        bounded(terms.fee)?;
        if terms.principal_cash < 0. || terms.fee < 0. {
            return Err("native exit terms require nonnegative principal cash and fee".into());
        }
        let cash = terms.principal_cash + p.closing_accrued_interest + terms.fee;
        let basis = self.gross - cash;
        bounded(cash)?;
        bounded(basis)?;
        Ok(Some((cash, basis)))
    }
    pub(crate) fn contractual_earned(
        &mut self,
        interval: usize,
        origin: usize,
        start: f64,
        end: f64,
        principal: f64,
        fallback_rate: f64,
    ) -> Result<f64, String> {
        self.check()?;
        let Some(source) = self.factor_source.as_deref_mut() else {
            return Ok(principal * fallback_rate * (end - start));
        };
        let query = NativeFactorQuery {
            path_key: self.request.original_anchors[self.path_index - 1].path_key,
            interval_index: interval,
            origin_index: origin,
            start_year: start,
            end_year: end,
        };
        // The source callback supplies factors only. Copy bounded atoms, then
        // resolve immutable entitlement after the mutable source query returns.
        // This avoids permitting a callback to supply monetary P/A/G overrides.
        let mut atoms = Vec::new();
        let maximum = self.maximum_atoms_per_query;
        source.earned(&query, &mut |atom| {
            (self.check)()?;
            if atoms.len() >= maximum {
                return Err("native factor atoms exceed reserved source bound".into());
            }
            for identity in [
                atom.coupon_id,
                atom.source_event_id,
                atom.source_id,
                atom.source_revision,
            ] {
                label(identity)?;
            }
            if !atom.delta_factor.is_finite() {
                return Err("native factor must be finite".into());
            }
            atoms.push((
                atom.coupon_id.to_owned(),
                atom.source_event_id.to_owned(),
                atom.source_id.to_owned(),
                atom.source_revision.to_owned(),
                atom.captured_segment_index,
                atom.delta_factor,
            ));
            Ok(())
        })?;
        let mut earned = 0.0;
        for (coupon, event, id, revision, segment, delta) in atoms {
            (self.check)()?;
            if self.contributions.len()
                + self.points.len()
                + self.recoveries.len()
                + self.source_cash.len()
                + self.coupon_recoveries.len()
                >= self.row_upper
            {
                return Err("native factor atoms exceed reserved source bound".into());
            }
            let atom = NativeFactorAtom {
                coupon_id: &coupon,
                source_event_id: &event,
                source_id: &id,
                source_revision: &revision,
                captured_segment_index: segment,
                delta_factor: delta,
            };
            let entitlement = source.entitlement(&atom)?;
            if self.coupon_claims_per_path > 0 && entitlement.is_none() {
                return Err(
                    "native gross default rights require source entitlement on every factor atom"
                        .into(),
                );
            }
            let actual_principal = entitlement
                .as_ref()
                .map_or(principal, |v| v.opening_entitlement_principal);
            bounded(actual_principal)?;
            if actual_principal < 0.0 {
                return Err("native coupon entitlement principal must be nonnegative".into());
            }
            let amount = actual_principal * delta;
            bounded(amount)?;
            earned += amount;
            bounded(earned)?;
            self.contributions.push(EarnedOriginContribution {
                path_key: query.path_key,
                interval_index: interval,
                origin_index: origin,
                start_year: start,
                end_year: end,
                coupon_id: coupon,
                source_event_id: event,
                source_id: id,
                source_revision: revision,
                captured_segment_index: segment,
                delta_factor: delta,
                actual_principal,
                contractual_earned: amount,
                native_entitlement: entitlement,
            });
        }
        self.check()?;
        Ok(earned)
    }
    pub(crate) fn source_repayment(
        &mut self,
        event: &str,
        year: f64,
        principal: f64,
    ) -> Result<Option<(f64, f64, f64)>, String> {
        self.check()?;
        let Some(source) = self.factor_source.as_deref() else {
            return Ok(None);
        };
        let Some(cash) = source.contractual_cash(event, year)? else {
            if self.coupon_claims_per_path > 0 {
                return Err("unmapped native contractual cash under selected gross owner".into());
            }
            return Ok(None);
        };
        if self.path_cash_count >= self.source_cash_per_path {
            return Err("native source cash exceeds declared per-path bound".into());
        }
        label(&cash.coupon_id)?;
        label(&cash.source_event_id)?;
        bounded(cash.principal)?;
        bounded(cash.interest)?;
        bounded(cash.fee)?;
        if cash.source_event_id != event || cash.principal < 0.0 || cash.principal > principal {
            return Err(
                "native contractual cash identity/principal does not match live legal owner".into(),
            );
        }
        if self.points.len()
            + self.recoveries.len()
            + self.contributions.len()
            + self.source_cash.len()
            + self.coupon_recoveries.len()
            >= self.row_upper
        {
            return Err("native source cash exceeds reserved source bound".into());
        }
        let amounts = (cash.principal, cash.interest, cash.fee);
        self.source_cash.push(SourceCashReceipt {
            path_key: self.request.original_anchors[self.path_index - 1].path_key,
            year,
            cash,
        });
        self.path_cash_count += 1;
        Ok(Some(amounts))
    }
    pub(crate) fn default_begin(&mut self, p: &CreditPathResult) -> Result<(), String> {
        self.check()?;
        if self.coupon_claims_per_path == 0 {
            return Ok(());
        }
        self.default_lots = crate::commercial_coupon_accounting::native_default_lots(
            p,
            &self.contributions,
            &self.source_cash,
            self.check,
        )?;
        self.canonical_legal_coupons.clear();
        self.draw_tranches.clear();
        if self.default_lots.len() > self.coupon_claims_per_path {
            return Err("native gross coupon claims exceed preflight source bound".into());
        }
        Ok(())
    }
    pub(crate) fn recovery_interest(&mut self, query: NativeRecoveryQuery) -> Result<f64, String> {
        let NativeRecoveryQuery {
            path_key,
            default_year,
            recovery_index,
            year,
            fraction,
            weight,
            fallback,
            horizon,
        } = query;
        self.check()?;
        if self.coupon_claims_per_path == 0 {
            return Ok(fallback);
        }
        let mut cash = 0.;
        for lot in &self.default_lots {
            (self.check)()?;
            if self.points.len()
                + self.recoveries.len()
                + self.contributions.len()
                + self.source_cash.len()
                + self.coupon_recoveries.len()
                >= self.row_upper
            {
                return Err("native gross recovery receipts exceed aggregate reservation".into());
            }
            let interest = lot.legal_remaining * fraction * weight;
            bounded(interest)?;
            cash += interest;
            self.coupon_recoveries.push(NativeCouponRecovery {
                path_key,
                default_year,
                recovery_index,
                year,
                coupon_id: lot.coupon_id.clone(),
                source_event_id: lot.source_event_id.clone(),
                native_audit_index: lot.native_audit_index,
                earning_origin_index: lot.earning_origin_index,
                interest,
                applied_in_horizon: year <= horizon,
            });
        }
        bounded(cash)?;
        Ok(cash)
    }
    fn point(
        &mut self,
        p: &CreditPathResult,
        year: f64,
        kind: &'static str,
        rate_origin: Option<(usize, f64)>,
        changes: [f64; 3],
    ) -> Result<(), String> {
        self.check()?;
        bounded(self.gross)?;
        if self.points.len()
            + self.recoveries.len()
            + self.contributions.len()
            + self.source_cash.len()
            + self.coupon_recoveries.len()
            >= self.row_upper
        {
            return Err("observed carry rows exceeded admitted upper".into());
        }
        self.points.push(CarryPoint {
            path_key: p.path_key,
            year,
            kind,
            origin_index: rate_origin.map(|x| x.0),
            principal: p.closing_principal,
            contractual_accrued_interest: p.closing_accrued_interest,
            book_adjustment: p.closing_book_adjustment,
            gross_carry: self.gross,
            accounting_rate: rate_origin.map(|x| x.1),
            effective_interest: changes[0],
            cash: changes[1],
            basis_realization: changes[2],
            canonical_legal_coupons: self.canonical_legal_coupons.clone(),
            draw_tranches: self.draw_tranches.clone(),
            undrawn_commitment: self
                .factor_source
                .as_ref()
                .map(|_| p.closing_undrawn_commitment),
        });
        Ok(())
    }
    pub(crate) fn begin(&mut self, opening: &PathOpening) -> Result<(), String> {
        self.check()?;
        self.path_cash_count = 0;
        self.default_lots.clear();
        self.canonical_legal_coupons.clear();
        self.draw_tranches.clear();
        if let Some(source) = self.factor_source.as_deref_mut() {
            source.begin_path(opening.path_key)?;
        }
        self.gross = self.request.original_anchors[self.path_index].acquisition_cost;
        self.path_index += 1;
        if opening.path_key != self.request.original_anchors[self.path_index - 1].path_key {
            return Err("native path order changed after admission".into());
        }
        if self.points.len()
            + self.recoveries.len()
            + self.contributions.len()
            + self.source_cash.len()
            + self.coupon_recoveries.len()
            >= self.row_upper
        {
            return Err("observed opening rows exceeded admitted upper".into());
        }
        self.points.push(CarryPoint {
            path_key: opening.path_key,
            year: 0.0,
            kind: "reviewed_opening_snapshot",
            origin_index: None,
            principal: opening.principal,
            contractual_accrued_interest: opening.accrued_interest,
            book_adjustment: opening.book_adjustment,
            gross_carry: self.gross,
            accounting_rate: None,
            effective_interest: 0.0,
            cash: 0.0,
            basis_realization: 0.0,
            canonical_legal_coupons: Vec::new(),
            draw_tranches: Vec::new(),
            undrawn_commitment: self
                .factor_source
                .as_ref()
                .map(|_| opening.undrawn_commitment),
        });
        Ok(())
    }
    fn reconcile(&self, p: &mut CreditPathResult) -> Result<(), String> {
        p.closing_book_adjustment = self.gross - p.closing_principal - p.closing_accrued_interest;
        bounded(p.closing_book_adjustment)
    }
    pub(crate) fn advance(
        &mut self,
        p: &mut CreditPathResult,
        interval: usize,
        origin: usize,
        start: f64,
        end: f64,
    ) -> Result<(), String> {
        self.check()?;
        let annual = self.request.recognition_rates[interval].annual_effective_rates[origin];
        let tranche_gross: f64 = self.draw_tranches.iter().map(|t| t.gross_carry).sum();
        let mut earned = if self.draw_tranches.is_empty() {
            self.gross * (annual.ln_1p() * (end - start)).exp_m1()
        } else {
            (self.gross - tranche_gross) * (annual.ln_1p() * (end - start)).exp_m1()
        };
        for tranche in &mut self.draw_tranches {
            let selected = self
                .factor_source
                .as_deref()
                .ok_or("missing native drawn tranche owner")?
                .tranche_accounting_rate(&tranche.tranche_id, interval, origin)?
                .ok_or("missing reviewed tranche accounting rate")?;
            rate(selected)?;
            let income = tranche.gross_carry * (selected.ln_1p() * (end - start)).exp_m1();
            bounded(income)?;
            tranche.gross_carry += income;
            bounded(tranche.gross_carry)?;
            earned += income;
        }
        bounded(earned)?;
        self.gross += earned;
        self.canonical_legal(p, end)?;
        self.reconcile(p)?;
        self.point(
            p,
            end,
            "live_sojourn",
            Some((origin, annual)),
            [earned, 0.0, 0.0],
        )
    }
    pub(crate) fn settle(
        &mut self,
        p: &mut CreditPathResult,
        year: f64,
        cash: f64,
    ) -> Result<(), String> {
        self.gross -= cash;
        if let Some(id) = self
            .source_cash
            .last()
            .filter(|r| r.path_key == p.path_key && r.year == year)
            .and_then(|r| r.cash.tranche_id.as_ref())
        {
            let tranche = self
                .draw_tranches
                .iter_mut()
                .find(|t| &t.tranche_id == id)
                .ok_or("source cash references unfunded tranche")?;
            tranche.gross_carry -= cash;
            bounded(tranche.gross_carry)?;
        }
        self.canonical_legal(p, year)?;
        self.reconcile(p)?;
        self.point(p, year, "contractual_cash", None, [0.0, cash, 0.0])
    }
    pub(crate) fn fund(
        &mut self,
        p: &mut CreditPathResult,
        year: f64,
        cash: f64,
    ) -> Result<(), String> {
        self.gross += cash;
        self.canonical_legal(p, year)?;
        self.reconcile(p)?;
        self.point(p, year, "funded_draw", None, [0.0, -cash, 0.0])
    }
    pub(crate) fn exit(
        &mut self,
        p: &CreditPathResult,
        year: f64,
        cash: f64,
        basis: f64,
    ) -> Result<(), String> {
        // Cash and basis realization are distinct. No invented cash pays B.
        self.gross = 0.0;
        self.canonical_legal_coupons.clear();
        self.draw_tranches.clear();
        self.point(p, year, "full_live_exit", None, [0.0, cash, basis])
    }
    pub(crate) fn default_tail(
        &mut self,
        p: &mut CreditPathResult,
        claim: &PathDefault,
        horizon: f64,
    ) -> Result<(), String> {
        self.check()?;
        let old_formula_p = p.closing_principal;
        let old_formula_a = p.closing_accrued_interest;
        p.closing_principal = claim.principal;
        p.closing_accrued_interest = claim.accrued_interest;
        p.closing_book_adjustment = claim.book_adjustment;
        self.point(
            p,
            claim.year,
            "default_gross_transfer",
            None,
            [0.0, 0.0, 0.0],
        )?;
        let mut paid_principal = 0.0;
        let mut paid_interest = 0.0;
        for (i, recovery) in claim.recoveries.iter().enumerate() {
            self.check()?;
            if self.points.len() + self.recoveries.len() + self.contributions.len()
                >= self.row_upper
            {
                return Err("observed recovery rows exceeded admitted upper".into());
            }
            let applied = recovery.year <= horizon;
            self.recoveries.push(RecoveryReceipt {
                path_key: p.path_key,
                default_year: claim.year,
                recovery_index: i,
                year: recovery.year,
                principal: recovery.principal,
                interest: recovery.interest,
                applied_in_horizon: applied,
            });
            if applied {
                paid_principal += recovery.principal;
                paid_interest += recovery.interest;
                p.closing_principal = claim.principal - paid_principal;
                p.closing_accrued_interest = claim.accrued_interest - paid_interest;
                if p.closing_principal < 0.0 {
                    return Err("actual principal recovery overcollection; no clamp".into());
                }
                self.gross -= recovery.principal + recovery.interest;
                self.reconcile(p)?;
                self.point(
                    p,
                    recovery.year,
                    "default_actual_cash",
                    None,
                    [0.0, recovery.principal + recovery.interest, 0.0],
                )?;
            }
        }
        // The historical formula is intentionally not substituted for actual summed cash.
        // None callers retain it byte-for-byte. Both remain numerical evidence, not disposal.
        bounded(old_formula_p)?;
        bounded(old_formula_a)?;
        Ok(())
    }
    pub(crate) fn finish(&mut self, p: &CreditPathResult, horizon: f64) -> Result<(), String> {
        self.point(p, horizon, "closing_claims_retained", None, [0.0, 0.0, 0.0])
    }
}

pub fn evaluate(request: &Request) -> Result<ResultData, String> {
    evaluate_with_check(request, &mut || Ok(()))
}

/// Structural joined admission for native source compilers, before either
/// original/current generator runs. The supplied bound is compiler authority,
/// not a deserializable request field.
pub fn admit_with_native_bound(
    request: &Request,
    bound: &NativeFactorBound,
    source_cash_per_path: usize,
    coupon_claims_per_path: usize,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<Admission, String> {
    admit(
        request,
        Some(bound),
        source_cash_per_path,
        coupon_claims_per_path,
        false,
        check,
    )
}

pub fn evaluate_with_check(
    request: &Request,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    evaluate_owned(request, None, false, check)
}

pub fn evaluate_with_native_factors<'a>(
    request: &'a Request,
    source: &'a mut dyn NativeEarnedFactors,
    check: &'a mut dyn FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    evaluate_owned(request, Some(source), false, check)
}

/// Explicit native economic use of the identical credit/source cash loop.
/// Pricing cash does not grant financial-accounting or physical-journal rights.
pub fn admit_economic_with_native_bound(
    request: &Request,
    bound: &NativeFactorBound,
    source_cash_per_path: usize,
    coupon_claims_per_path: usize,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<Admission, String> {
    admit(
        request,
        Some(bound),
        source_cash_per_path,
        coupon_claims_per_path,
        true,
        check,
    )
}
pub fn evaluate_economic_with_native_factors<'a>(
    request: &'a Request,
    source: &'a mut dyn NativeEarnedFactors,
    check: &'a mut dyn FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    evaluate_owned(request, Some(source), true, check)
}

fn evaluate_owned<'a>(
    request: &'a Request,
    mut source: Option<&'a mut dyn NativeEarnedFactors>,
    pricing_economics: bool,
    check: &'a mut dyn FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    check()?;
    let selected = source.is_some();
    let bound = source.as_deref_mut().map(|s| s.bound());
    let source_cash_per_path = source
        .as_deref()
        .map_or(0, |s| s.maximum_source_cash_per_path());
    let coupon_claims_per_path = source
        .as_deref()
        .map_or(0, |s| s.maximum_gross_coupon_claims_per_path());
    let admission = admit(
        request,
        bound.as_ref(),
        source_cash_per_path,
        coupon_claims_per_path,
        pricing_economics,
        check,
    )?;
    let mut owner = Owner {
        request,
        check,
        gross: 0.0,
        path_index: 0,
        points: Vec::new(),
        recoveries: Vec::new(),
        row_upper: admission.row_upper,
        factor_source: source,
        maximum_atoms_per_query: bound.as_ref().map_or(0, |b| b.maximum_atoms_per_query),
        contributions: Vec::new(),
        source_cash: Vec::new(),
        source_cash_per_path,
        path_cash_count: 0,
        coupon_claims_per_path,
        default_lots: Vec::new(),
        coupon_recoveries: Vec::new(),
        canonical_legal_coupons: Vec::new(),
        draw_tranches: Vec::new(),
    };
    let credit = credit_path::evaluate_income_internal(&request.raw_credit_path, &mut owner)?;
    owner.check()?;
    let raw_request = request.clone();
    owner.check()?;
    let points = owner.points;
    let recoveries = owner.recoveries;
    let contributions = owner.contributions;
    Ok(ResultData {
        schema: SCHEMA,
        raw_request,
        admission,
        credit,
        carry_points: points,
        complete_recovery_receipts: recoveries,
        native_factor_provider_selected: selected,
        earned_origin_contributions: contributions,
        native_source_cash: owner.source_cash,
        native_coupon_recoveries: owner.coupon_recoveries,
        complete_coupon_settlement_ownership: false,
        original_anchor_refitted: false,
        default_interest_stopped: true,
        corporate_source_binding: false,
        mixed_ledger_adoption: false,
        accounting_approval: false,
    })
}
