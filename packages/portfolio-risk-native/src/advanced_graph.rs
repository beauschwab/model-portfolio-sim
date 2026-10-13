//! Supplied consumer history graph. Explicit measures, native generation, frozen base OAS.
//! No stochastic-policy fitting, legacy model switch, or implicit ledger adoption.
use crate::model_market::curves::{CurveRole, SuppliedCurve};
use portfolio_model_core::cashflow::{
    validate_ordinal, AccrualPeriod, CashflowAdjustment, CashflowComponents, CashflowMeasure,
    DatedCashflowEvent, DatedSchedule, EventProvenance, ScenarioSelector,
};
use portfolio_model_core::consumer_contract::{ConsumerState, MAX_CONSUMER_EVENTS};
use portfolio_model_core::consumer_installment::{
    self, InstallmentAction, InstallmentRequest, InstallmentResult, InstallmentStep,
};
use portfolio_model_core::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;

pub const SCHEMA: &str = "advanced-product-graph-1";
pub const MAX_RUNS: usize = 4096;
pub const MAX_ROWS: usize = 250_000;
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_IDENTITY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphIdentity {
    pub model_revision: String,
    pub input_revision: String,
    pub paired_history_source: String,
    pub paired_history_revision: String,
    /// This model consumes supplied action histories, not random numbers.
    pub randomness_domain: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub scenario_id: String,
    pub run_id: String,
    pub severity: f64,
    pub collateral_curve: SuppliedCurve,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub path_id: String,
    pub weight: f64,
    pub assumption_id: String,
    pub steps: Vec<InstallmentStep>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioHistories {
    pub scenario_id: String,
    pub physical: Vec<History>,
    pub pricing: Vec<History>,
    pub pricing_down: Vec<History>,
    pub pricing_up: Vec<History>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OasTarget {
    CalibratePositiveCash {
        target_cash_price: f64,
        source_id: String,
        source_revision: String,
    },
    Locked {
        base_oas: f64,
        source_id: String,
        source_revision: String,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Product {
    ConsumerInstallment {
        /// Known contract/opening state. Contractual measure and empty steps are
        /// required here; measure-specific history requests are built explicitly.
        contract: InstallmentRequest,
        histories: Vec<ScenarioHistories>,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub position_id: String,
    pub oas_target: OasTarget,
    pub product: Product,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub identity: GraphIdentity,
    pub as_of_ordinal: i32,
    pub currency: String,
    pub reporting_ordinals: Vec<i32>,
    pub base_scenario_id: String,
    pub bump_bp: f64,
    pub scenarios: Vec<Scenario>,
    pub positions: Vec<Position>,
}
#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Physical,
    Pricing,
    PricingDown,
    PricingUp,
}
#[derive(Clone, Serialize)]
pub struct NativeSource {
    pub position_id: String,
    pub scenario_id: String,
    pub run_id: String,
    pub path_id: String,
    pub weight: f64,
    pub role: Role,
    pub projection: InstallmentResult,
}
#[derive(Clone, Serialize)]
pub struct PositionOas {
    pub position_id: String,
    pub base_oas: f64,
    pub calibration_kind: String,
}
#[derive(Clone, Serialize)]
pub struct BaseAnchor {
    pub schema: String,
    /// Complete raw dependency content is retained; not a label-only hash.
    pub calibration_input: serde_json::Value,
    pub oas: Vec<PositionOas>,
}
#[derive(Clone, Serialize)]
pub struct Metrics {
    pub position_id: String,
    pub scenario_id: String,
    pub pv: f64,
    /// Central finite difference with supplied paired Q histories plus native
    /// +/- curve bumps. Not a fitted behavioral/effective DV01 claim.
    pub supplied_history_dv01: f64,
    pub gross_physical_contract_interest: Vec<f64>,
    pub physical_principal_cash: Vec<f64>,
    pub physical_interest_cash: Vec<f64>,
    pub physical_fee_cash: Vec<f64>,
    pub physical_default_principal: Vec<f64>,
    pub physical_principal_loss: Vec<f64>,
}
#[derive(Clone, Serialize)]
pub struct ResultData {
    pub schema: String,
    pub identity: GraphIdentity,
    pub base_anchor: BaseAnchor,
    pub metrics: Vec<Metrics>,
    pub native_sources: Vec<NativeSource>,
    pub income_policy: String,
    pub risk_policy: String,
    pub source_status: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureOwner {
    pub position_id: String,
    pub account_id: String,
    pub source_id: String,
    pub source_revision: String,
    pub ownership: String,
    pub direction: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturePolicy {
    pub policy_id: String,
    pub policy_revision: String,
    pub recognition_model: String,
    pub book_basis: String,
    pub fees: String,
}
#[derive(Clone, Serialize)]
pub struct CaptureSegment {
    pub origin_id: String,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub accrued_interest: f64,
    pub book_amortization: f64,
}
#[derive(Clone, Serialize)]
pub struct CapturePlan {
    pub position_id: String,
    pub event_id: String,
    pub run: ScenarioSelector,
    pub segments: Vec<CaptureSegment>,
}
#[derive(Clone, Serialize)]
pub struct CaptureSource {
    pub source_id: String,
    pub source_revision: String,
    pub schedule: DatedSchedule,
    pub coverage: Vec<CaptureCoverage>,
}
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptureCoverageStatus {
    CompleteEventSet {},
    NoEvents { rationale: String },
}
#[derive(Clone, Serialize)]
pub struct CaptureCoverage {
    pub position_id: String,
    pub run: ScenarioSelector,
    pub status: CaptureCoverageStatus,
}
#[derive(Clone, Serialize)]
pub struct CaptureOpening {
    pub position_id: String,
    pub principal: f64,
    pub accrued_interest: f64,
    pub gross_carrying_value: f64,
}
#[derive(Clone, Serialize)]
pub struct ScheduledCapture {
    pub schema: String,
    pub identity: GraphIdentity,
    pub policy: CapturePolicy,
    pub scenarios: Vec<Scenario>,
    pub owners: Vec<CaptureOwner>,
    pub openings: Vec<CaptureOpening>,
    pub sources: Vec<CaptureSource>,
    pub accrual_plans: Vec<CapturePlan>,
    pub scope: String,
}
#[derive(Clone)]
pub struct AdvancedGraph {
    base_key: Arc<Vec<u8>>,
    anchor: Arc<BaseAnchor>,
    request: Arc<Request>,
    result: Arc<ResultData>,
}
fn label(v: &str) -> Result<(), String> {
    if v.trim().is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
        return Err("advanced graph identity must be bounded nonblank text".into());
    }
    Ok(())
}
fn number(v: f64, lo: f64, hi: f64) -> Result<(), String> {
    if !v.is_finite() || !(lo..=hi).contains(&v) {
        return Err("advanced graph numeric admission exceeded".into());
    }
    Ok(())
}
fn money(v: f64) -> Result<f64, String> {
    number(v, -MAX_MONETARY_AMOUNT, MAX_MONETARY_AMOUNT)?;
    Ok(v)
}
fn bytes<T: Serialize>(v: &T) -> Result<Vec<u8>, String> {
    count_bytes(v, MAX_IDENTITY_BYTES)?;
    let result = serde_json::to_vec(v).map_err(|e| e.to_string())?;
    if result.len() > MAX_IDENTITY_BYTES {
        return Err("advanced graph canonical input exceeds 32MiB".into());
    }
    Ok(result)
}
struct ByteCount {
    size: usize,
    limit: usize,
}
impl Write for ByteCount {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.size = self.size.saturating_add(data.len());
        if self.size > self.limit {
            return Err(std::io::Error::other("advanced byte admission exceeded"));
        }
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn count_bytes<T: Serialize>(v: &T, limit: usize) -> Result<usize, String> {
    let mut counter = ByteCount { size: 0, limit };
    serde_json::to_writer(&mut counter, v).map_err(|e| e.to_string())?;
    Ok(counter.size)
}
fn histories(position: &Position) -> (&InstallmentRequest, &[ScenarioHistories]) {
    match &position.product {
        Product::ConsumerInstallment {
            contract,
            histories,
        } => (contract, histories),
    }
}
fn check_paths(paths: &[History]) -> Result<(), String> {
    if paths.is_empty() || paths.len() > 128 {
        return Err("advanced graph history path admission exceeded".into());
    }
    let mut ids = HashSet::new();
    let mut sum = 0.;
    for path in paths {
        label(&path.path_id)?;
        label(&path.assumption_id)?;
        number(path.weight, 0., 1.)?;
        if !ids.insert(&path.path_id) || path.steps.len() > MAX_CONSUMER_EVENTS {
            return Err("duplicate path or step admission exceeded".into());
        }
        sum += path.weight;
    }
    if (sum - 1.).abs() > 1e-12 {
        return Err("history probabilities must sum to one; no normalization fallback".into());
    }
    Ok(())
}
fn validate(r: &Request) -> Result<usize, String> {
    validate_with_work(r).map(|(base, _)| base)
}
/// Full pre-projection admission cost for bounded aggregate source consumers.
pub fn audit_admitted_work(r: &Request) -> Result<usize, String> {
    validate_with_work(r).map(|(_, work)| work)
}
fn validate_with_work(r: &Request) -> Result<(usize, usize), String> {
    if r.schema != SCHEMA || r.identity.randomness_domain != "supplied_paired_histories-1" {
        return Err(
            "advanced graph requires supplied paired histories; no inferred stochastic policy"
                .into(),
        );
    }
    for v in [
        &r.identity.model_revision,
        &r.identity.input_revision,
        &r.identity.paired_history_source,
        &r.identity.paired_history_revision,
        &r.currency,
        &r.base_scenario_id,
    ] {
        label(v)?;
    }
    validate_ordinal(r.as_of_ordinal)?;
    if r.currency.len() != 3 || !r.currency.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err("advanced graph requires explicit uppercase three-letter currency".into());
    }
    number(r.bump_bp, 0.001, 1000.)?;
    let h = r
        .reporting_ordinals
        .len()
        .checked_sub(1)
        .ok_or("actual reporting boundaries required")?;
    if !(1..=120).contains(&h)
        || r.scenarios.is_empty()
        || r.scenarios.len() > 13
        || r.positions.is_empty()
        || r.positions.len() > 256
    {
        return Err("advanced graph dimensions outside admission".into());
    }
    for &date in &r.reporting_ordinals {
        validate_ordinal(date)?;
    }
    if r.reporting_ordinals[0] != r.as_of_ordinal
        || r.reporting_ordinals.windows(2).any(|v| v[0] >= v[1])
    {
        return Err("advanced graph reporting dates must start at asof and increase".into());
    }
    let mut ids = HashSet::new();
    let mut run_ids = HashSet::new();
    for scenario in &r.scenarios {
        label(&scenario.scenario_id)?;
        label(&scenario.run_id)?;
        number(scenario.severity, 0., 100.)?;
        if !ids.insert(&scenario.scenario_id)
            || !run_ids.insert(&scenario.run_id)
            || scenario.collateral_curve.currency != r.currency
            || !matches!(scenario.collateral_curve.role, CurveRole::Collateral { .. })
        {
            return Err("duplicate advanced scenario or collateral curve/currency mismatch".into());
        }
        scenario.collateral_curve.validate()?;
    }
    let base = r
        .scenarios
        .iter()
        .position(|s| s.scenario_id == r.base_scenario_id)
        .ok_or("missing explicit advanced base scenario")?;
    let mut positions = HashSet::new();
    let mut runs = 0usize;
    let mut rows = 0usize;
    let mut work = 0usize;
    for position in &r.positions {
        label(&position.position_id)?;
        if !positions.insert(&position.position_id) {
            return Err("duplicate advanced position".into());
        }
        let (contract, sets) = histories(position);
        if contract.horizon_days > portfolio_model_core::cashflow::MAX_HORIZON_DAYS {
            return Err("advanced known contract horizon exceeds full date admission".into());
        }
        let contract_end = r
            .as_of_ordinal
            .checked_add(contract.horizon_days as i32)
            .ok_or("advanced known contract horizon overflow")?;
        if contract.identity.position_id != position.position_id
            || contract.identity.currency != r.currency
            || contract.as_of_ordinal != r.as_of_ordinal
            || contract.identity.measure != CashflowMeasure::Contractual
            || !contract.steps.is_empty()
            || sets.len() != r.scenarios.len()
            || r.reporting_ordinals[h] > contract_end
        {
            return Err("known contract/measure/date/history coverage mismatch".into());
        }
        let mut reference = None;
        for (scenario, set) in r.scenarios.iter().zip(sets) {
            if set.scenario_id != scenario.scenario_id {
                return Err(
                    "supplied scenario histories must match explicit scenario order".into(),
                );
            }
            for (role, paths) in [
                (Role::Physical, &set.physical),
                (Role::Pricing, &set.pricing),
                (Role::PricingDown, &set.pricing_down),
                (Role::PricingUp, &set.pricing_up),
            ] {
                check_paths(paths)?;
                runs = runs
                    .checked_add(paths.len())
                    .ok_or("advanced run count overflow")?;
                for path in paths {
                    let count = contract
                        .payment_dates
                        .len()
                        .checked_add(path.steps.len())
                        .and_then(|n| n.checked_add(2))
                        .ok_or("advanced row overflow")?;
                    // Cash, state and recognition output worst-case admission before projection.
                    rows = rows
                        .checked_add(count * 4)
                        .ok_or("advanced rows overflow")?;
                    let passes = if role == Role::Physical {
                        2 * h + 8
                    } else if role == Role::Pricing {
                        104
                    } else {
                        4
                    };
                    work = work
                        .checked_add(count * passes)
                        .ok_or("advanced pricing work overflow")?;
                    if rows > MAX_ROWS || work > MAX_WORK {
                        return Err(
                            "advanced graph aggregate run/row/work admission exceeded".into()
                        );
                    }
                    let mut raw = contract.clone();
                    raw.identity.measure = if role == Role::Physical {
                        CashflowMeasure::Physical
                    } else {
                        CashflowMeasure::Pricing
                    };
                    raw.identity.assumption_id = path.assumption_id.clone();
                    raw.identity.run_id = scenario.run_id.clone();
                    raw.steps = path.steps.clone();
                    consumer_installment::validate_request(&raw)?;
                }
            }
            for paths in [&set.pricing, &set.pricing_down, &set.pricing_up] {
                let pairing: Vec<_> = paths.iter().map(|p| (&p.path_id, p.weight)).collect();
                if let Some(ref known) = reference {
                    if known != &pairing {
                        return Err("Q histories require identical ordered path identity/weights across scenarios/bumps".into());
                    }
                } else {
                    reference = Some(pairing);
                }
            }
        }
        match &position.oas_target {
            OasTarget::CalibratePositiveCash {
                target_cash_price,
                source_id,
                source_revision,
            } => {
                number(*target_cash_price, 1e-12, MAX_MONETARY_AMOUNT)?;
                label(source_id)?;
                label(source_revision)?;
            }
            OasTarget::Locked {
                base_oas,
                source_id,
                source_revision,
            } => {
                number(*base_oas, -5., 5.)?;
                label(source_id)?;
                label(source_revision)?;
            }
        }
    }
    if runs > MAX_RUNS || rows > MAX_ROWS || work > MAX_WORK {
        return Err("advanced graph aggregate run/row/work admission exceeded".into());
    }
    bytes(r)?;
    Ok((base, work))
}
fn base_key(r: &Request, base: usize) -> Result<Vec<u8>, String> {
    let raw_positions: Vec<_> = r
        .positions
        .iter()
        .map(|p| {
            let (contract, sets) = histories(p);
            (&p.position_id, &p.oas_target, contract, &sets[base].pricing)
        })
        .collect();
    bytes(&(
        &r.schema,
        &r.identity.model_revision,
        &r.identity.paired_history_source,
        &r.identity.paired_history_revision,
        &r.identity.randomness_domain,
        r.as_of_ordinal,
        &r.currency,
        &r.base_scenario_id,
        &r.scenarios[base],
        raw_positions,
    ))
}
fn project(
    contract: &InstallmentRequest,
    history: &History,
    scenario: &Scenario,
    role: Role,
) -> Result<InstallmentResult, String> {
    portfolio_compute_control::checkpoint()?;
    let mut raw = contract.clone();
    raw.identity.measure = if role == Role::Physical {
        CashflowMeasure::Physical
    } else {
        CashflowMeasure::Pricing
    };
    raw.identity.assumption_id = history.assumption_id.clone();
    raw.identity.run_id = scenario.run_id.clone();
    raw.steps = history.steps.clone();
    let output = consumer_installment::project(&raw)?;
    if output.closing_principal != 0.
        || output.closing_accrued_interest != 0.
        || output.closing_unrecognized_interest != 0.
        || output.closing_fees != 0.
    {
        return Err("advanced consumer projection must retain complete terminal obligations; no truncated/implicit balloon rights".into());
    }
    Ok(output)
}
fn price(
    output: &InstallmentResult,
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    let mut total = 0.;
    let mut correction = 0.;
    for event in output.cashflows.iter().filter(|e| e.ordinal > asof) {
        let years = (event.ordinal - asof) as f64 / 365.;
        let discounted = (event.principal + event.interest + event.fees)
            * curve.value_validated(years)?.discount_factor
            * (-(oas + bump) * years).exp();
        let adjusted = money(discounted)? - correction;
        let next = money(total + adjusted)?;
        correction = (next - total) - adjusted;
        total = next;
    }
    money(total)
}
fn weighted_price(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    let mut total = 0.;
    for path in paths {
        total = money(total + path.weight * price(&path.projection, curve, asof, oas, bump)?)?;
    }
    Ok(total)
}
fn positive_log_price(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
) -> Result<f64, String> {
    let mut total: Option<f64> = None;
    for path in paths.iter().filter(|p| p.weight > 0.) {
        for event in path
            .projection
            .cashflows
            .iter()
            .filter(|e| e.ordinal > asof)
        {
            let cash = event.principal + event.interest + event.fees;
            if cash <= 0. {
                continue;
            }
            let years = (event.ordinal - asof) as f64 / 365.;
            let term =
                path.weight.ln() + cash.ln() + curve.value_validated(years)?.discount_factor.ln()
                    - oas * years;
            total = Some(match total {
                None => term,
                Some(old) => {
                    let largest = old.max(term);
                    largest + ((old - largest).exp() + (term - largest).exp()).ln()
                }
            });
        }
    }
    total.ok_or("no positive Q calibration rights".into())
}
fn solve(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    target: &OasTarget,
) -> Result<f64, String> {
    if let OasTarget::Locked { base_oas, .. } = target {
        return Ok(*base_oas);
    }
    let OasTarget::CalibratePositiveCash {
        target_cash_price, ..
    } = target
    else {
        unreachable!()
    };
    let mut positive = false;
    for path in paths {
        for event in path
            .projection
            .cashflows
            .iter()
            .filter(|e| e.ordinal > asof)
        {
            let cash = event.principal + event.interest + event.fees;
            if cash < 0. {
                return Err("signed Q cash requires explicit locked OAS; positive target solver unsupported".into());
            }
            positive |= path.weight > 0. && cash > 0.;
        }
    }
    if !positive {
        return Err("positive target solver needs positive future Q rights".into());
    }
    let (mut lo, mut hi) = (-5., 5.);
    let target = target_cash_price.ln();
    if positive_log_price(paths, curve, asof, lo)? < target
        || positive_log_price(paths, curve, asof, hi)? > target
    {
        return Err("advanced positive-cash target outside admitted OAS bracket".into());
    }
    for _ in 0..100 {
        portfolio_compute_control::checkpoint()?;
        let mid = (lo + hi) / 2.;
        if positive_log_price(paths, curve, asof, mid)? > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok((lo + hi) / 2.)
}
fn sources(r: &Request) -> Result<Vec<NativeSource>, String> {
    let mut result = Vec::new();
    let mut count = 0usize;
    for position in &r.positions {
        let (contract, sets) = histories(position);
        for (scenario, set) in r.scenarios.iter().zip(sets) {
            for (role, paths) in [
                (Role::Physical, &set.physical),
                (Role::Pricing, &set.pricing),
                (Role::PricingDown, &set.pricing_down),
                (Role::PricingUp, &set.pricing_up),
            ] {
                for path in paths {
                    let projection = project(contract, path, scenario, role)?;
                    count += projection.cashflows.len()
                        + projection.points.len()
                        + projection.recognition.len()
                        + usize::from(projection.default.is_some());
                    if count > MAX_ROWS {
                        return Err("advanced native projected row admission exceeded".into());
                    }
                    result.push(NativeSource {
                        position_id: position.position_id.clone(),
                        scenario_id: scenario.scenario_id.clone(),
                        run_id: scenario.run_id.clone(),
                        path_id: path.path_id.clone(),
                        weight: path.weight,
                        role,
                        projection,
                    });
                }
            }
        }
    }
    Ok(result)
}
fn slice<'a>(
    paths: &'a [NativeSource],
    position: &str,
    scenario: &str,
    role: Role,
) -> Vec<&'a NativeSource> {
    paths
        .iter()
        .filter(|p| p.position_id == position && p.scenario_id == scenario && p.role == role)
        .collect()
}
fn evaluate(
    r: &Request,
    raw_sources: Vec<NativeSource>,
    anchor: BaseAnchor,
) -> Result<ResultData, String> {
    let h = r.reporting_ordinals.len() - 1;
    let mut metrics = Vec::new();
    for (position, oas) in r.positions.iter().zip(&anchor.oas) {
        for scenario in &r.scenarios {
            portfolio_compute_control::checkpoint()?;
            let q = slice(
                &raw_sources,
                &position.position_id,
                &scenario.scenario_id,
                Role::Pricing,
            );
            let down = slice(
                &raw_sources,
                &position.position_id,
                &scenario.scenario_id,
                Role::PricingDown,
            );
            let up = slice(
                &raw_sources,
                &position.position_id,
                &scenario.scenario_id,
                Role::PricingUp,
            );
            let p = slice(
                &raw_sources,
                &position.position_id,
                &scenario.scenario_id,
                Role::Physical,
            );
            let pv = weighted_price(
                &q,
                &scenario.collateral_curve,
                r.as_of_ordinal,
                oas.base_oas,
                0.,
            )?;
            let bump = r.bump_bp * 1e-4;
            let dv = (weighted_price(
                &down,
                &scenario.collateral_curve,
                r.as_of_ordinal,
                oas.base_oas,
                -bump,
            )? - weighted_price(
                &up,
                &scenario.collateral_curve,
                r.as_of_ordinal,
                oas.base_oas,
                bump,
            )?) / (2. * r.bump_bp);
            let mut row = Metrics {
                position_id: position.position_id.clone(),
                scenario_id: scenario.scenario_id.clone(),
                pv,
                supplied_history_dv01: money(dv)?,
                gross_physical_contract_interest: vec![0.; h],
                physical_principal_cash: vec![0.; h],
                physical_interest_cash: vec![0.; h],
                physical_fee_cash: vec![0.; h],
                physical_default_principal: vec![0.; h],
                physical_principal_loss: vec![0.; h],
            };
            for path in &p {
                for (m, dates) in r.reporting_ordinals.windows(2).enumerate() {
                    for earned in &path.projection.recognition {
                        let amount = if earned.start_ordinal == earned.end_ordinal {
                            if earned.end_ordinal > dates[0] && earned.end_ordinal <= dates[1] {
                                earned.interest
                            } else {
                                0.
                            }
                        } else {
                            let overlap = (earned.end_ordinal.min(dates[1])
                                - earned.start_ordinal.max(dates[0]))
                            .max(0);
                            earned.interest * overlap as f64
                                / (earned.end_ordinal - earned.start_ordinal) as f64
                        };
                        row.gross_physical_contract_interest[m] =
                            money(row.gross_physical_contract_interest[m] + path.weight * amount)?;
                    }
                    for event in path
                        .projection
                        .cashflows
                        .iter()
                        .filter(|e| e.ordinal > dates[0] && e.ordinal <= dates[1])
                    {
                        row.physical_principal_cash[m] =
                            money(row.physical_principal_cash[m] + path.weight * event.principal)?;
                        row.physical_interest_cash[m] =
                            money(row.physical_interest_cash[m] + path.weight * event.interest)?;
                        row.physical_fee_cash[m] =
                            money(row.physical_fee_cash[m] + path.weight * event.fees)?;
                    }
                    if let Some(default) = &path.projection.default {
                        if default.ordinal > dates[0] && default.ordinal <= dates[1] {
                            row.physical_default_principal[m] = money(
                                row.physical_default_principal[m] + path.weight * default.principal,
                            )?;
                            row.physical_principal_loss[m] = money(
                                row.physical_principal_loss[m]
                                    + path.weight * default.principal_loss,
                            )?;
                        }
                    }
                }
            }
            metrics.push(row);
        }
    }
    Ok(ResultData { schema:SCHEMA.into(), identity:r.identity.clone(), base_anchor:anchor, metrics, native_sources:raw_sources,
        income_policy:"gross physical contractual note-interest recognition on actual (left,right] intervals; not frozen effective-yield income, provision-adjusted NII or net income".into(),
        risk_policy:"pricing-measure supplied paired down/up histories plus native collateral zero-rate bumps at unchanged base OAS; no fitted stochastic behavioral policy".into(),
        source_status:"native generation from supplied original contracts, known opening state and explicit P/Q histories; no random draws consumed, automatic legacy switch or ledger publication".into() })
}
impl AdvancedGraph {
    pub fn new(r: &Request) -> Result<Self, String> {
        let base = validate(r)?;
        let key = base_key(r, base)?;
        let projections = sources(r)?;
        let mut oas = Vec::new();
        for position in &r.positions {
            let base_paths = slice(
                &projections,
                &position.position_id,
                &r.base_scenario_id,
                Role::Pricing,
            );
            oas.push(PositionOas {
                position_id: position.position_id.clone(),
                base_oas: solve(
                    &base_paths,
                    &r.scenarios[base].collateral_curve,
                    r.as_of_ordinal,
                    &position.oas_target,
                )?,
                calibration_kind: match position.oas_target {
                    OasTarget::Locked { .. } => "supplied_locked_oas",
                    _ => "positive_q_cash_target",
                }
                .into(),
            });
        }
        let anchor = BaseAnchor {
            schema: "advanced-base-anchor-1".into(),
            calibration_input: serde_json::from_slice(&key).map_err(|e| e.to_string())?,
            oas,
        };
        let result = evaluate(r, projections, anchor.clone())?;
        retained(r, &result, key.len())?;
        Ok(Self {
            base_key: Arc::new(key),
            anchor: Arc::new(anchor),
            request: Arc::new(r.clone()),
            result: Arc::new(result),
        })
    }
    pub fn request(&self) -> &Request {
        &self.request
    }
    pub fn snapshot(&self) -> &ResultData {
        &self.result
    }
    pub fn base_anchor(&self) -> &BaseAnchor {
        &self.anchor
    }
    pub fn reprice(&self, r: &Request) -> Result<ResultData, String> {
        let base = validate(r)?;
        if base_key(r, base)?.as_slice() != self.base_key.as_slice() {
            return Err("BASE_ANCHOR_CHANGED: explicit new base calibration required".into());
        }
        let result = evaluate(r, sources(r)?, (*self.anchor).clone())?;
        retained(r, &result, self.base_key.len())?;
        Ok(result)
    }
    /// New immutable graph snapshot with the original anchor. Enables capture
    /// of revised physical sources without recalibration or mutable old state.
    pub fn repriced(&self, r: &Request) -> Result<Self, String> {
        let result = self.reprice(r)?;
        Ok(Self {
            base_key: Arc::clone(&self.base_key),
            anchor: Arc::clone(&self.anchor),
            request: Arc::new(r.clone()),
            result: Arc::new(result),
        })
    }
    /// Complete strict scheduled capture. This is typed preparation, not GL
    /// execution/publication. Unsupported states reject the entire operation.
    pub fn capture_scheduled(
        &self,
        owners: &[CaptureOwner],
        policy: &CapturePolicy,
    ) -> Result<ScheduledCapture, String> {
        label(&policy.policy_id)?;
        label(&policy.policy_revision)?;
        if policy.recognition_model != "consumer_simple_interest_par-1"
            || policy.book_basis != "par_note_simple_interest"
            || policy.fees != "reject"
            || owners.len() != self.request.positions.len()
        {
            return Err("scheduled consumer capture requires complete ownership and explicit par-note/fee policy".into());
        }
        let mut owner_ids = HashSet::new();
        let mut source_ids = HashSet::new();
        for owner in owners {
            for id in [
                &owner.position_id,
                &owner.account_id,
                &owner.source_id,
                &owner.source_revision,
            ] {
                label(id)?;
            }
            if !owner_ids.insert(&owner.position_id)
                || !source_ids.insert(&owner.source_id)
                || owner.ownership != "fully_scenario_generated"
                || owner.direction != "receipt"
                || owner.source_revision != self.request.identity.input_revision
                || !self
                    .request
                    .positions
                    .iter()
                    .any(|p| p.position_id == owner.position_id)
            {
                return Err("ambiguous/incomplete consumer owner or source identity".into());
            }
        }
        let mut sources = Vec::new();
        let mut plans = Vec::new();
        let mut openings = Vec::new();
        for position in &self.request.positions {
            portfolio_compute_control::checkpoint()?;
            let (contract, _) = histories(position);
            if contract.initial_state != ConsumerState::Performing
                || contract.opening_accrued_interest != 0.
                || contract.opening_unrecognized_interest != 0.
                || contract.opening_fees != 0.
            {
                return Err(
                    "scheduled capture rejects unsupported opening state/rights/fees".into(),
                );
            }
            let owner = owners
                .iter()
                .find(|o| o.position_id == position.position_id)
                .ok_or("missing consumer owner")?;
            let mut events = Vec::new();
            let mut coverage = Vec::new();
            for scenario in &self.request.scenarios {
                let paths = slice(
                    &self.result.native_sources,
                    &position.position_id,
                    &scenario.scenario_id,
                    Role::Physical,
                );
                if paths.len() != 1 || paths[0].weight != 1. {
                    return Err(
                        "scheduled ledger capture requires one explicit physical path per run"
                            .into(),
                    );
                }
                let projection = &paths[0].projection;
                coverage.push(CaptureCoverage {
                    position_id: position.position_id.clone(),
                    run: ScenarioSelector::Named {
                        scenario_id: scenario.run_id.clone(),
                    },
                    status: if projection.cashflows.is_empty() {
                        CaptureCoverageStatus::NoEvents {
                            rationale: "complete native physical projection has no cash events"
                                .into(),
                        }
                    } else {
                        CaptureCoverageStatus::CompleteEventSet {}
                    },
                });
                let (_, sets) = histories(position);
                let history = sets
                    .iter()
                    .find(|s| s.scenario_id == scenario.scenario_id)
                    .ok_or("missing physical scenario history")?;
                if projection.default.is_some()
                    || projection.cashflows.iter().any(|e| {
                        e.fees != 0.
                            || e.ordinal <= self.request.as_of_ordinal
                            || e.kind == "recovery"
                    })
                    || projection.points.iter().any(|p| {
                        !matches!(p.state, ConsumerState::Performing | ConsumerState::PaidOff)
                            || p.fees != 0.
                            || p.unrecognized_interest != 0.
                    })
                    || history.physical[0].steps.iter().any(|s| {
                        matches!(
                            s.action,
                            InstallmentAction::Default {}
                                | InstallmentAction::SetState { .. }
                                | InstallmentAction::Fee { .. }
                        )
                    })
                {
                    return Err("consumer default/nonaccrual/delinquency/fee/day0 capture unsupported; no partial event pruning".into());
                }
                let mut remaining: Vec<_> =
                    projection.recognition.iter().map(|r| r.interest).collect();
                let mut cursor = 0usize;
                for cash in &projection.cashflows {
                    let event_id = format!("{}:{}", cash.kind, cash.ordinal);
                    let selector = ScenarioSelector::Named {
                        scenario_id: scenario.run_id.clone(),
                    };
                    let mut needed = cash.interest;
                    let mut segments = Vec::new();
                    while needed > 0. && cursor < remaining.len() {
                        let origin = &projection.recognition[cursor];
                        if origin.end_ordinal > cash.ordinal
                            || origin.end_ordinal == origin.start_ordinal
                        {
                            return Err("consumer capture cannot invent settlement/accrual rights or cash-basis history".into());
                        }
                        let allocated = needed.min(remaining[cursor]);
                        if allocated > 0. {
                            segments.push(CaptureSegment {
                                origin_id: origin.origin_id.clone(),
                                start_ordinal: origin.start_ordinal,
                                end_ordinal: origin.end_ordinal,
                                accrued_interest: allocated,
                                book_amortization: 0.,
                            });
                        }
                        remaining[cursor] -= allocated;
                        needed -= allocated;
                        if remaining[cursor] == 0. {
                            cursor += 1;
                        }
                    }
                    if needed > 1e-9_f64.max(cash.interest * 1e-12) {
                        return Err(
                            "consumer cash interest lacks complete native recognition origins"
                                .into(),
                        );
                    }
                    let allocated: f64 = segments.iter().map(|s| s.accrued_interest).sum();
                    if let Some(last) = segments.last_mut() {
                        last.accrued_interest += cash.interest - allocated;
                        number(last.accrued_interest, 0., MAX_MONETARY_AMOUNT)?;
                    }
                    let accrual_period = segments.first().map(|s| AccrualPeriod {
                        start_ordinal: s.start_ordinal,
                        end_ordinal: segments.last().unwrap().end_ordinal,
                    });
                    events.push(DatedCashflowEvent {
                        position_id: position.position_id.clone(),
                        event_id: event_id.clone(),
                        scenario: selector.clone(),
                        payment_ordinal: cash.ordinal,
                        day_offset: (cash.ordinal - self.request.as_of_ordinal) as u32,
                        accrual_period,
                        currency: self.request.currency.clone(),
                        components: CashflowComponents {
                            principal: cash.principal,
                            cash_interest: cash.interest,
                            accrued_interest: cash.interest,
                            book_amortization: 0.,
                            fees: 0.,
                        },
                        measure: CashflowMeasure::Physical,
                        provenance: EventProvenance::Modeled {
                            model_id: consumer_installment::SCHEMA.into(),
                            model_version: consumer_installment::SCHEMA.into(),
                            input_revision: self.request.identity.input_revision.clone(),
                        },
                        adjustment: CashflowAdjustment::None {},
                    });
                    if !segments.is_empty() {
                        plans.push(CapturePlan {
                            position_id: position.position_id.clone(),
                            event_id,
                            run: selector,
                            segments,
                        });
                    }
                }
                if remaining.iter().sum::<f64>() > 1e-9 {
                    return Err(
                        "consumer capture must retain all unsettled recognition rights".into(),
                    );
                }
            }
            events.sort_by_key(|e| e.payment_ordinal);
            let schedule = DatedSchedule {
                schema: "dated-cashflow-1".into(),
                as_of_ordinal: self.request.as_of_ordinal,
                horizon_days: contract.horizon_days,
                scenarios: self
                    .request
                    .scenarios
                    .iter()
                    .map(|s| s.run_id.clone())
                    .collect(),
                events,
            };
            schedule.validate()?;
            sources.push(CaptureSource {
                source_id: owner.source_id.clone(),
                source_revision: owner.source_revision.clone(),
                schedule,
                coverage,
            });
            openings.push(CaptureOpening {
                position_id: position.position_id.clone(),
                principal: contract.opening_principal,
                accrued_interest: 0.,
                gross_carrying_value: contract.opening_principal,
            });
        }
        let output = ScheduledCapture { schema:"advanced-scheduled-capture-1".into(), identity:self.request.identity.clone(),
            policy:policy.clone(), scenarios:self.request.scenarios.clone(), owners:owners.to_vec(), openings, sources,
            accrual_plans:plans, scope:"native scheduled physical source with complete ownership and par-note/simple-interest basis; requires native dated-ledger mapping, exact run coverage, independent journals and publication gate; no credit/default adoption".into() };
        count_bytes(&output, MAX_RETAINED_BYTES / 4)?;
        Ok(output)
    }
}
fn retained(r: &Request, result: &ResultData, key: usize) -> Result<(), String> {
    let input = bytes(r)?.len();
    let output = count_bytes(result, MAX_RETAINED_BYTES)?;
    if input
        .saturating_add(output)
        .saturating_mul(4)
        .saturating_add(key)
        > MAX_RETAINED_BYTES
    {
        return Err("advanced graph retained component byte admission exceeded".into());
    }
    Ok(())
}
pub fn analyze(r: &Request) -> Result<ResultData, String> {
    AdvancedGraph::new(r).map(|g| (*g.result).clone())
}
