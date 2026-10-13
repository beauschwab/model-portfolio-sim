//! Explicit commercial/issuer-credit source graph. Economic Pricing cash and
//! reviewed Physical accounting have separate owners; original OAS never moves.
use crate::{
    advanced_graph::{GraphIdentity, OasTarget, Role, Scenario},
    commercial_credit_compiler as owner,
};
use portfolio_model_core::{cashflow::validate_ordinal, credit::CreditMeasure, strict_json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "commercial-product-graph-1";
pub const MAX_WORK: usize = 32_000_000;
pub const MAX_BYTES: usize = 128 << 20;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreditIdentity {
    CommercialLending,
    IssuerCreditDebt,
    CreditFreeReference,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub path_id: String,
    pub weight: f64,
    pub source: owner::Request,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Histories {
    pub scenario_id: String,
    pub physical: Vec<History>,
    pub pricing: Vec<History>,
    pub pricing_down: Vec<History>,
    pub pricing_up: Vec<History>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub position_id: String,
    pub credit_identity: CreditIdentity,
    pub oas_target: OasTarget,
    pub original_signed_cash_quote: f64,
    pub original_pricing: Vec<History>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub original_physical: Vec<History>,
    pub histories: Vec<Histories>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exercise: Option<crate::products::commercial::exercise::Policy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_policy: Option<Box<crate::products::commercial::generated_policy::Request>>,
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
    pub original_scenario: Scenario,
    pub scenarios: Vec<Scenario>,
    pub positions: Vec<Position>,
}
pub use crate::products::commercial::cash::{economic_cash, EconomicCash};
pub struct NativeSource {
    pub position_id: String,
    pub scenario_id: String,
    pub run_id: String,
    pub path_id: String,
    pub path_key: u64,
    pub weight: f64,
    pub role: Role,
    pub source_request: owner::Request,
    pub cash: Vec<EconomicCash>,
    pub projection: Value,
    /// Native-only financial owner retained from the very same graph pass.
    /// JSON receipts cannot be deserialized into a journal-eligible owner.
    pub(crate) physical_projection: Option<Box<owner::ResultData>>,
    pub(crate) generated_projection: Option<(
        Box<crate::products::commercial::generated_policy::PhysicalSource>,
        std::sync::Arc<std::sync::Mutex<owner::NativeOriginalAcquisitionProof>>,
    )>,
}
impl Serialize for NativeSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        #[derive(Serialize)]
        struct GeneratedProjection<'a> {
            #[serde(flatten)]
            receipt: &'a Value,
            book_points: &'a [crate::products::commercial::generated_policy::BookPoint],
        }
        let mut out = serializer.serialize_struct("NativeSource", 10)?;
        out.serialize_field("position_id", &self.position_id)?;
        out.serialize_field("scenario_id", &self.scenario_id)?;
        out.serialize_field("run_id", &self.run_id)?;
        out.serialize_field("path_id", &self.path_id)?;
        out.serialize_field("path_key", &self.path_key)?;
        out.serialize_field("weight", &self.weight)?;
        out.serialize_field("role", &self.role)?;
        out.serialize_field("source_request", &self.source_request)?;
        out.serialize_field("cash", &self.cash)?;
        if let Some((recipe, _)) = &self.generated_projection {
            out.serialize_field(
                "projection",
                &GeneratedProjection {
                    receipt: &self.projection,
                    book_points: recipe.book_points(),
                },
            )?;
        } else {
            out.serialize_field("projection", &self.projection)?;
        }
        out.end()
    }
}
impl NativeSource {
    pub fn physical_accounting_source(&self) -> Option<&owner::ResultData> {
        self.physical_projection.as_deref()
    }
    pub fn physical_period(&self, left: f64, right: f64) -> Result<(f64, f64), String> {
        if let Some((recipe, _)) = &self.generated_projection {
            return recipe.metric_period(left, right);
        }
        let groups = self
            .physical_accounting_source()
            .ok_or("commercial physical sealed owner/recipe absent")?
            .accounting()
            .groups();
        let principal = groups
            .iter()
            .rfind(|g| g.year <= left)
            .ok_or("commercial opening state absent")?
            .next
            .principal;
        let mut nii = 0.;
        for group in groups.iter().filter(|g| g.year > left && g.year <= right) {
            for line in &group.postings {
                if line.posting.account
                    == portfolio_model_core::credit_accounting::AccountCode::InterestIncome
                {
                    nii = money(nii - line.posting.amount)?;
                }
            }
        }
        Ok((principal, nii))
    }
    pub fn materialize_generated_physical(&self) -> Result<Option<owner::ResultData>, String> {
        self.generated_projection
            .as_ref()
            .map(|(recipe, proof)| {
                let proof = proof
                    .lock()
                    .map_err(|_| "generated original proof owner poisoned")?;
                recipe.materialize(&proof)
            })
            .transpose()
    }
}
#[derive(Clone, Serialize)]
pub struct Oas {
    pub position_id: String,
    pub base_oas: f64,
    pub original_signed_cash_quote: f64,
}
#[derive(Clone, Serialize)]
pub struct Metric {
    pub position_id: String,
    pub scenario_id: String,
    pub ordinal: i32,
    pub pv: f64,
    pub dv01: f64,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_source: Request,
    pub original_oas: Vec<Oas>,
    pub native_sources: Vec<NativeSource>,
    pub metrics: Vec<Metric>,
    pub admitted_work_upper: usize,
    pub shared_draws_retained: bool,
    pub physical_accounting_source_retained: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_physical_recipes_retained: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_oas_calibration:
        Option<crate::products::commercial::stochastic_exercise::calibration::Receipt>,
    pub empirical_validation: bool,
    pub scope: &'static str,
    pub exercise_policies: Vec<ExerciseReceipt>,
    pub original_physical_sources: Vec<NativeSource>,
    pub original_acquisition_proofs: Vec<Value>,
}
#[derive(Serialize)]
pub struct ExerciseReceipt {
    pub position_id: String,
    pub scenario_id: String,
    pub role: Role,
    pub policy: Value,
}
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
pub(crate) fn money(x: f64) -> Result<f64, String> {
    crate::products::term::contracts::amount(x)
}
fn label(x: &str) -> Result<(), String> {
    crate::products::term::projection::label(x)
}
fn exact<T: Serialize>(v: &T) -> Result<Vec<u8>, String> {
    strict_json::encode(v, 32 << 20)
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-12 * b.abs()
}
fn roles(h: &Histories) -> [(Role, &[History]); 4] {
    [
        (Role::Physical, &h.physical),
        (Role::Pricing, &h.pricing),
        (Role::PricingDown, &h.pricing_down),
        (Role::PricingUp, &h.pricing_up),
    ]
}
fn measure(r: Role) -> portfolio_model_core::cashflow::CashflowMeasure {
    if r == Role::Physical {
        portfolio_model_core::cashflow::CashflowMeasure::Physical
    } else {
        portfolio_model_core::cashflow::CashflowMeasure::Pricing
    }
}
pub(crate) fn validate_history(
    r: &Request,
    p: &Position,
    h: &History,
    role: Role,
) -> Result<(), String> {
    check()?;
    label(&h.path_id)?;
    if !h.weight.is_finite()
        || !(0. ..=1.).contains(&h.weight)
        || h.source.credit.paths.len() != 1
        || h.source.credit.paths[0].path_id != h.path_id
    {
        return Err(
            "one explicit unique native credit path and bounded weight required per graph history"
                .into(),
        );
    }
    let s = &h.source.source;
    let required = match p.credit_identity {
        CreditIdentity::CommercialLending => owner::InstrumentKind::CommercialLoan,
        CreditIdentity::IssuerCreditDebt => owner::InstrumentKind::IssuerCreditDebt,
        CreditIdentity::CreditFreeReference => owner::InstrumentKind::CreditFreeReference,
    };
    if h.source.credit.instrument_kind != required {
        return Err(
            "commercial graph and native source credit instrument identities differ".into(),
        );
    }
    for c in [&s.original, &s.current] {
        if c.position_id != p.position_id
            || c.currency != r.currency
            || c.asof_ordinal != r.as_of_ordinal
            || c.measure != measure(role)
            || c.direction != portfolio_model_core::floating_rate::Direction::Receipt
        {
            return Err(
                "commercial source identity/date/currency/measure/direction mismatch".into(),
            );
        }
    }
    if s.current.opening_principal <= 0.
        || s.current.tail_end_ordinal < *r.reporting_ordinals.last().unwrap()
    {
        return Err("positive complete commercial source and report tail required".into());
    }
    if p.credit_identity == CreditIdentity::CreditFreeReference
        && h.source.credit.intervals.iter().any(|i| {
            i.hazards_per_year.iter().flatten().any(|x| *x != 0.)
                || i.credit_conversion_factor.iter().any(|x| *x != 0.)
        })
    {
        return Err(
            "credit-free reference must explicitly disable every transition/default/draw hazard"
                .into(),
        );
    }
    Ok(())
}
/// Charges original calibration, all roles, report marks and retained receipts
/// before any financial generator executes.
pub fn audit_admitted_work(r: &Request) -> Result<usize, String> {
    check()?;
    if r.schema != SCHEMA
        || r.positions.is_empty()
        || r.positions.len() > 32
        || r.scenarios.is_empty()
        || r.scenarios.len() > 13
        || !(2..=121).contains(&r.reporting_ordinals.len())
        || !r.bump_bp.is_finite()
        || !(0.001..=1000.).contains(&r.bump_bp)
    {
        return Err("explicit commercial graph shape/model/bump admission required".into());
    }
    for s in [
        &r.identity.model_revision,
        &r.identity.input_revision,
        &r.identity.paired_history_source,
        &r.identity.paired_history_revision,
        &r.identity.randomness_domain,
        &r.currency,
        &r.base_scenario_id,
    ] {
        label(s)?;
    }
    validate_ordinal(r.as_of_ordinal)?;
    for d in &r.reporting_ordinals {
        validate_ordinal(*d)?;
    }
    if r.reporting_ordinals[0] != r.as_of_ordinal
        || r.reporting_ordinals.windows(2).any(|w| w[0] >= w[1])
    {
        return Err(
            "commercial graph requires exact increasing actual reporting dates from source opening"
                .into(),
        );
    }
    let mut scene_names = BTreeSet::new();
    for s in std::iter::once(&r.original_scenario).chain(r.scenarios.iter()) {
        label(&s.scenario_id)?;
        label(&s.run_id)?;
        s.collateral_curve.validate()?;
        if !s.severity.is_finite()
            || s.collateral_curve.currency != r.currency
            || !matches!(
                s.collateral_curve.role,
                crate::market::analysis::curves::CurveRole::Collateral { .. }
            )
        {
            return Err(
                "commercial graph requires explicit collateral curves and finite scenarios".into(),
            );
        }
    }
    for s in &r.scenarios {
        if !scene_names.insert(s.scenario_id.as_str()) {
            return Err("duplicate commercial scenario".into());
        }
    }
    if !scene_names.contains(r.base_scenario_id.as_str()) {
        return Err("commercial base scenario absent".into());
    }
    let raw = strict_json::serialized_size(r, 32 << 20)?;
    let mut work = raw.checked_mul(8).ok_or("commercial graph work overflow")?;
    let mut bytes = raw
        .checked_mul(16)
        .ok_or("commercial graph bytes overflow")?;
    let mut names = BTreeSet::new();
    let mut sources = 0usize;
    for p in &r.positions {
        label(&p.position_id)?;
        money(p.original_signed_cash_quote)?;
        if !names.insert(&p.position_id)
            || p.original_signed_cash_quote <= 0.
            || (p.generated_policy.is_none() && p.histories.len() != r.scenarios.len())
        {
            return Err("unique commercial position, positive original quote and complete scenarios required".into());
        }
        match &p.oas_target {
            OasTarget::Locked {
                base_oas,
                source_id,
                source_revision,
            } => {
                money(*base_oas)?;
                label(source_id)?;
                label(source_revision)?;
                if !(-5. ..=5.).contains(base_oas) {
                    return Err("commercial locked OAS outside existing bracket".into());
                }
            }
            OasTarget::CalibratePositiveCash {
                target_cash_price,
                source_id,
                source_revision,
            } => {
                label(source_id)?;
                label(source_revision)?;
                if target_cash_price.to_bits() != p.original_signed_cash_quote.to_bits() {
                    return Err("original quote and calibration target differ".into());
                }
            }
        }
        if let Some(generated) = &p.generated_policy {
            let upper = crate::products::commercial::generated_graph::admit(r, p, generated)?;
            work = work
                .checked_add(upper.work_upper)
                .ok_or("generated graph work overflow")?;
            bytes = bytes
                .checked_add(upper.retained_bytes_upper)
                .ok_or("generated graph retention overflow")?;
            sources = sources
                .checked_add(upper.physical_paths)
                .and_then(|v| v.checked_add(upper.pricing_paths))
                .ok_or("generated graph source overflow")?;
            continue;
        }
        let mut origins = BTreeMap::new();
        let mut physical_origins = BTreeMap::new();
        let mut stream = None;
        let mut sets = BTreeSet::new();
        for (scene, role, hs) in std::iter::once((
            r.original_scenario.scenario_id.as_str(),
            Role::Pricing,
            p.original_pricing.as_slice(),
        ))
        .chain(p.histories.iter().flat_map(|h| {
            roles(h)
                .into_iter()
                .map(move |(role, hs)| (h.scenario_id.as_str(), role, hs))
        }))
        .chain((!p.original_physical.is_empty()).then_some((
            r.original_scenario.scenario_id.as_str(),
            Role::Physical,
            p.original_physical.as_slice(),
        ))) {
            if !scene_names.contains(scene) && scene != r.original_scenario.scenario_id {
                return Err("unknown commercial scenario history".into());
            }
            if hs.is_empty() || hs.len() > 64 {
                return Err("commercial graph needs 1..64 paths per measure/role".into());
            }
            let mut path_names = BTreeSet::new();
            let mut total = 0.;
            for h in hs {
                validate_history(r, p, h, role)?;
                sources = sources
                    .checked_add(1)
                    .ok_or("commercial graph source overflow")?;
                if sources > 256 {
                    return Err("commercial graph joined source count exceeds 256".into());
                }
                total += h.weight;
                if !path_names.insert(&h.path_id) {
                    return Err("duplicate commercial path".into());
                }
                let binding = exact(
                    &serde_json::json!({"draw_stream":h.source.credit.draw_stream,"paths":h.source.credit.paths}),
                )?;
                let by_path = stream.get_or_insert_with(BTreeMap::<String, Vec<u8>>::new);
                if by_path.get(&h.path_id).is_some_and(|old| *old != binding) {
                    return Err("commercial paired scenarios and bumps require the exact shared draw stream/path key".into());
                }
                by_path.entry(h.path_id.clone()).or_insert(binding);
                let anchor = exact(
                    &serde_json::json!({"source":h.source.source.original,"acquisition":h.source.source.acquisition}),
                )?;
                let original = if role == Role::Physical {
                    &mut physical_origins
                } else {
                    &mut origins
                };
                if original.get(&h.path_id).is_some_and(|old| *old != anchor) {
                    return Err("original commercial source/cost/yield must stay bitwise identical across scenarios and bump sides".into());
                }
                original.entry(h.path_id.clone()).or_insert(anchor);
                let admitted = if role == Role::Physical {
                    owner::admit(&h.source)?
                } else {
                    owner::admit_economic_with_measure(&h.source, CreditMeasure::Pricing)?
                };
                work = work
                    .checked_add(admitted.joined_work_upper)
                    .ok_or("commercial graph work overflow")?;
                bytes = bytes
                    .checked_add(admitted.joined_bytes_upper)
                    .ok_or("commercial graph bytes overflow")?;
                if role == Role::Physical {
                    // Keep the sealed financial owner alongside its JSON
                    // receipt for native candidate journal reuse.
                    bytes = bytes
                        .checked_add(admitted.joined_bytes_upper)
                        .ok_or("commercial retained journal owner bytes overflow")?;
                }
                if let Some(policy) = &p.exercise {
                    let possible = if let Some(date) = policy.exercise_dates.first() {
                        let (action, factor, fee) = if let Some(f) = date.call_clean_price_factor {
                            (owner::NativeStopAction::Call, f, date.call_fee)
                        } else {
                            (
                                owner::NativeStopAction::Put,
                                date.put_clean_price_factor
                                    .ok_or("native exercise right absent")?,
                                date.put_fee,
                            )
                        };
                        vec![owner::NativeStop::from_compiled_policy(
                            h.source.credit.paths[0].path_key,
                            format!("native-exercise:{}:{}", policy.identity.id, date.ordinal),
                            date.ordinal,
                            action,
                            factor,
                            fee,
                            policy.identity.clone(),
                        )?]
                    } else {
                        Vec::new()
                    };
                    let reserved = if role == Role::Physical {
                        owner::admit_with_stops(&h.source, &possible)?
                    } else {
                        owner::admit_economic_with_stops(
                            &h.source,
                            CreditMeasure::Pricing,
                            &possible,
                        )?
                    };
                    work = work
                        .checked_add(reserved.joined_work_upper)
                        .ok_or("commercial exercise joined work overflow")?;
                    bytes = bytes
                        .checked_add(reserved.joined_bytes_upper)
                        .ok_or("commercial exercise joined bytes overflow")?;
                }
            }
            if (total - 1.).abs() > 1e-12 {
                return Err(
                    "complete commercial pricing/physical path probability required".into(),
                );
            }
        }
        for hs in &p.histories {
            if !sets.insert(&hs.scenario_id) {
                return Err("duplicate commercial history scenario".into());
            }
            let keys = |h: &[History]| {
                h.iter()
                    .map(|x| (x.path_id.clone(), x.weight.to_bits()))
                    .collect::<BTreeSet<_>>()
            };
            if keys(&hs.pricing) != keys(&hs.pricing_down)
                || keys(&hs.pricing) != keys(&hs.pricing_up)
                || keys(&hs.pricing) != keys(&p.original_pricing)
            {
                return Err(
                    "pricing sides/scenarios must retain original path IDs and exact weights"
                        .into(),
                );
            }
            if keys(&hs.physical) != keys(&p.histories[0].physical) {
                return Err("physical scenarios must retain path IDs and exact weights".into());
            }
            if !p.original_physical.is_empty() && keys(&hs.physical) != keys(&p.original_physical) {
                return Err(
                    "current Physical paths must retain original acquisition path IDs and weights"
                        .into(),
                );
            }
        }
        if !p.original_physical.is_empty() && p.exercise.is_none() {
            return Err("original Physical exercise acquisition requires an explicit native exercise policy".into());
        }
        // Original prefix conditioning, independent proof replay and acquired
        // journal construction remain separate native owners. Reserve all four
        // full passes even when the original policy elects to continue.
        for h in &p.original_physical {
            let a = owner::admit(&h.source)?;
            work = work
                .checked_add(
                    a.joined_work_upper
                        .checked_mul(2)
                        .ok_or("original Physical proof work overflow")?,
                )
                .ok_or("original Physical proof work overflow")?;
            bytes = bytes
                .checked_add(
                    a.joined_bytes_upper
                        .checked_mul(2)
                        .ok_or("original Physical proof bytes overflow")?,
                )
                .ok_or("original Physical proof bytes overflow")?;
        }
        if let Some(policy) = &p.exercise {
            let originals = p
                .original_pricing
                .iter()
                .map(|h| (&h.source, h.weight))
                .collect::<Vec<_>>();
            work = work
                .checked_add(
                    crate::products::commercial::exercise::admitted_work(policy, &originals)?
                        .checked_mul(102)
                        .ok_or("original continuation solve work overflow")?,
                )
                .ok_or("commercial continuation aggregate overflow")?;
            for set in &p.histories {
                for hs in [&set.pricing, &set.pricing_down, &set.pricing_up] {
                    let raws = hs.iter().map(|h| (&h.source, h.weight)).collect::<Vec<_>>();
                    work = work
                        .checked_add(crate::products::commercial::exercise::admitted_work(
                            policy, &raws,
                        )?)
                        .ok_or("commercial current continuation work overflow")?;
                }
            }
        }
    }
    work = work
        .checked_add(sources * r.reporting_ordinals.len() * 4096)
        .ok_or("commercial mark work overflow")?;
    let max_work = if r.positions.iter().any(|p| p.generated_policy.is_some()) {
        4_000_000_000
    } else {
        MAX_WORK
    };
    if work > max_work || bytes > MAX_BYTES {
        return Err(
            "joined commercial graph work/retention exceeds 32M/128MiB before generation".into(),
        );
    }
    Ok(work)
}
fn generate(
    p: &Position,
    h: &History,
    role: Role,
    scene: &Scenario,
) -> Result<NativeSource, String> {
    check()?;
    let (cash, projection, physical_projection) = if role == Role::Physical {
        let v = owner::evaluate(&h.source)?;
        (
            economic_cash(v.carry())?,
            serde_json::to_value(&v).map_err(|e| e.to_string())?,
            Some(Box::new(v)),
        )
    } else {
        let v = owner::evaluate_economic_with_measure(&h.source, CreditMeasure::Pricing)?;
        (
            economic_cash(v.carry())?,
            serde_json::to_value(v).map_err(|e| e.to_string())?,
            None,
        )
    };
    Ok(NativeSource {
        position_id: p.position_id.clone(),
        scenario_id: scene.scenario_id.clone(),
        run_id: scene.run_id.clone(),
        path_id: h.path_id.clone(),
        path_key: h.source.credit.paths[0].path_key,
        weight: h.weight,
        role,
        source_request: h.source.clone(),
        cash,
        projection,
        physical_projection,
        generated_projection: None,
    })
}
fn pricing_bundle(hs: &[History]) -> Result<Vec<owner::EconomicResult>, String> {
    hs.iter()
        .map(|h| owner::evaluate_economic_with_measure(&h.source, CreditMeasure::Pricing))
        .collect()
}
fn compile_policy(
    policy: &crate::products::commercial::exercise::Policy,
    hs: &[History],
    bundle: &[owner::EconomicResult],
    scene: &Scenario,
    oas: f64,
    bump: f64,
) -> Result<crate::products::commercial::exercise::NativeExercisePolicy, String> {
    let views = hs
        .iter()
        .zip(bundle)
        .map(
            |(h, v)| crate::products::commercial::exercise::PricingView {
                raw: &h.source,
                carry: v.carry(),
                weight: h.weight,
            },
        )
        .collect::<Vec<_>>();
    crate::products::commercial::exercise::compile(
        policy,
        &views,
        &scene.collateral_curve,
        oas,
        bump,
    )
}
fn source_from_economic(
    p: &Position,
    h: &History,
    role: Role,
    scene: &Scenario,
    v: &owner::EconomicResult,
) -> Result<NativeSource, String> {
    Ok(NativeSource {
        position_id: p.position_id.clone(),
        scenario_id: scene.scenario_id.clone(),
        run_id: scene.run_id.clone(),
        path_id: h.path_id.clone(),
        path_key: h.source.credit.paths[0].path_key,
        weight: h.weight,
        role,
        source_request: h.source.clone(),
        cash: economic_cash(v.carry())?,
        projection: serde_json::to_value(v).map_err(|e| e.to_string())?,
        physical_projection: None,
        generated_projection: None,
    })
}
fn stopped_pricing(
    p: &Position,
    hs: &[History],
    bundle: &[owner::EconomicResult],
    role: Role,
    scene: &Scenario,
    policy: &crate::products::commercial::exercise::NativeExercisePolicy,
) -> Result<Vec<NativeSource>, String> {
    let mut sources = Vec::new();
    for (h, v) in hs.iter().zip(bundle) {
        check()?;
        let stops = policy.stops_for(&h.source, v.carry())?;
        if stops.is_empty() {
            sources.push(source_from_economic(p, h, role, scene, v)?);
        } else {
            let revised =
                owner::evaluate_economic_with_stops(&h.source, CreditMeasure::Pricing, &stops)?;
            sources.push(source_from_economic(p, h, role, scene, &revised)?);
        }
    }
    Ok(sources)
}
fn physical_with_policy(
    p: &Position,
    h: &History,
    scene: &Scenario,
    policy: &crate::products::commercial::exercise::NativeExercisePolicy,
    acquisition: Option<&owner::NativeOriginalAcquisitionProof>,
) -> Result<NativeSource, String> {
    let revised = if let Some(proof) = acquisition {
        let prefix = owner::evaluate_physical_prefix_for_acquisition(&h.source)?;
        let stops = policy.stops_for(&h.source, prefix.carry())?;
        owner::evaluate_with_native_original_acquisition(&h.source, &stops, proof)?
    } else {
        let raw = owner::evaluate(&h.source)?;
        let stops = policy.stops_for(&h.source, raw.carry())?;
        if stops.is_empty() {
            raw
        } else {
            owner::evaluate_with_stops(&h.source, &stops)?
        }
    };
    Ok(NativeSource {
        position_id: p.position_id.clone(),
        scenario_id: scene.scenario_id.clone(),
        run_id: scene.run_id.clone(),
        path_id: h.path_id.clone(),
        path_key: h.source.credit.paths[0].path_key,
        weight: h.weight,
        role: Role::Physical,
        source_request: h.source.clone(),
        cash: economic_cash(revised.carry())?,
        projection: serde_json::to_value(&revised).map_err(|e| e.to_string())?,
        physical_projection: Some(Box::new(revised)),
        generated_projection: None,
    })
}
/// Native discounting excludes already settled cash and uses exact event time;
/// the immutable original OAS and parallel bump enter once each.
pub fn price(
    source: &NativeSource,
    curve: &crate::market::analysis::curves::SuppliedCurve,
    asof: i32,
    mark: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    let t0 = f64::from(mark - asof) / 365.;
    let df0 = curve.value_validated(t0)?.discount_factor;
    let mut sum = 0.;
    let mut correction = 0.;
    for c in source.cash.iter().filter(|c| c.year > t0) {
        check()?;
        let value = money(
            c.total()? * curve.value_validated(c.year)?.discount_factor / df0
                * (-(oas + bump) * (c.year - t0)).exp(),
        )?;
        let adjusted = value - correction;
        let next = money(sum + adjusted)?;
        correction = (next - sum) - adjusted;
        sum = next;
    }
    Ok(sum)
}
fn weighted_price(
    sources: &[NativeSource],
    curve: &crate::market::analysis::curves::SuppliedCurve,
    asof: i32,
    mark: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    sources.iter().try_fold(0., |sum, s| {
        money(sum + s.weight * price(s, curve, asof, mark, oas, bump)?)
    })
}
fn solve(
    p: &Position,
    sources: &[NativeSource],
    r: &Request,
    bundle: &[owner::EconomicResult],
) -> Result<f64, String> {
    if let OasTarget::Locked { base_oas, .. } = p.oas_target {
        return Ok(base_oas);
    }
    if sources
        .iter()
        .any(|s| s.cash.iter().any(|c| c.total().is_ok_and(|x| x < 0.)))
    {
        return Err("signed draw/coupon Pricing cash requires explicit locked original OAS".into());
    }
    let curve = &r.original_scenario.collateral_curve;
    let original_price = |oas: f64| -> Result<f64, String> {
        if let Some(policy) = &p.exercise {
            Ok(compile_policy(
                policy,
                &p.original_pricing,
                bundle,
                &r.original_scenario,
                oas,
                0.,
            )?
            .value)
        } else {
            weighted_price(sources, curve, r.as_of_ordinal, r.as_of_ordinal, oas, 0.)
        }
    };
    let mut lo = -5.;
    let mut hi = 5.;
    if original_price(lo)? < p.original_signed_cash_quote
        || original_price(hi)? > p.original_signed_cash_quote
    {
        return Err("commercial original quote outside existing OAS bracket".into());
    }
    for _ in 0..100 {
        check()?;
        let mid = (lo + hi) / 2.;
        if original_price(mid)? > p.original_signed_cash_quote {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok((lo + hi) / 2.)
}
pub fn analyze(r: &Request) -> Result<ResultData, String> {
    let work = audit_admitted_work(r)?;
    if let Some(generated) = r.positions[0].generated_policy.as_ref() {
        return crate::products::commercial::generated_graph::analyze(
            r,
            &r.positions[0],
            generated,
            work,
        );
    }
    let mut anchors = Vec::new();
    let mut native_sources = Vec::new();
    let mut metrics = Vec::new();
    let mut exercise_policies = Vec::new();
    let mut original_physical_sources = Vec::new();
    let mut original_acquisition_proofs = Vec::new();
    for p in &r.positions {
        let original_bundle = pricing_bundle(&p.original_pricing)?;
        let mut original = p
            .original_pricing
            .iter()
            .zip(&original_bundle)
            .map(|(h, v)| source_from_economic(p, h, Role::Pricing, &r.original_scenario, v))
            .collect::<Result<Vec<_>, _>>()?;
        let oas = solve(p, &original, r, &original_bundle)?;
        let mut acquisition = None;
        if let Some(exercise) = &p.exercise {
            let policy = compile_policy(
                exercise,
                &p.original_pricing,
                &original_bundle,
                &r.original_scenario,
                oas,
                0.,
            )?;
            original = stopped_pricing(
                p,
                &p.original_pricing,
                &original_bundle,
                Role::Pricing,
                &r.original_scenario,
                &policy,
            )?;
            let cash_value = weighted_price(
                &original,
                &r.original_scenario.collateral_curve,
                r.as_of_ordinal,
                r.as_of_ordinal,
                oas,
                0.,
            )?;
            if !close(cash_value, policy.value) {
                return Err(
                    "native original continuation and actual stopped source cash differ".into(),
                );
            }
            if !p.original_physical.is_empty() {
                let prefixes = p
                    .original_physical
                    .iter()
                    .map(|h| owner::evaluate_physical_prefix_for_acquisition(&h.source))
                    .collect::<Result<Vec<_>, _>>()?;
                let stops = p
                    .original_physical
                    .iter()
                    .zip(&prefixes)
                    .map(|(h, v)| policy.stops_for(&h.source, v.carry()))
                    .collect::<Result<Vec<_>, _>>()?;
                let inputs = p
                    .original_physical
                    .iter()
                    .zip(&stops)
                    .map(|(h, s)| (&h.source, h.weight, s.as_slice()))
                    .collect::<Vec<_>>();
                let proof = owner::NativeOriginalAcquisitionProof::compile(&inputs)?;
                for h in &p.original_physical {
                    original_physical_sources.push(physical_with_policy(
                        p,
                        h,
                        &r.original_scenario,
                        &policy,
                        Some(&proof),
                    )?);
                }
                original_acquisition_proofs
                    .push(serde_json::to_value(&proof).map_err(|e| e.to_string())?);
                acquisition = Some(proof);
            }
            exercise_policies.push(ExerciseReceipt {
                position_id: p.position_id.clone(),
                scenario_id: r.original_scenario.scenario_id.clone(),
                role: Role::Pricing,
                policy: serde_json::to_value(policy).map_err(|e| e.to_string())?,
            });
        }
        if !close(
            weighted_price(
                &original,
                &r.original_scenario.collateral_curve,
                r.as_of_ordinal,
                r.as_of_ordinal,
                oas,
                0.,
            )?,
            p.original_signed_cash_quote,
        ) {
            return Err(
                "locked original OAS does not reconcile complete original Pricing cash quote"
                    .into(),
            );
        }
        anchors.push(Oas {
            position_id: p.position_id.clone(),
            base_oas: oas,
            original_signed_cash_quote: p.original_signed_cash_quote,
        });
        for scene in &r.scenarios {
            let h = p
                .histories
                .iter()
                .find(|h| h.scenario_id == scene.scenario_id)
                .ok_or("complete commercial scenario absent")?;
            let begin = native_sources.len();
            if let Some(exercise) = &p.exercise {
                let bundle = pricing_bundle(&h.pricing)?;
                let policy = compile_policy(exercise, &h.pricing, &bundle, scene, oas, 0.)?;
                for source in &h.physical {
                    native_sources.push(physical_with_policy(
                        p,
                        source,
                        scene,
                        &policy,
                        acquisition.as_ref(),
                    )?);
                }
                let priced =
                    stopped_pricing(p, &h.pricing, &bundle, Role::Pricing, scene, &policy)?;
                if !close(
                    weighted_price(
                        &priced,
                        &scene.collateral_curve,
                        r.as_of_ordinal,
                        r.as_of_ordinal,
                        oas,
                        0.,
                    )?,
                    policy.value,
                ) {
                    return Err(
                        "native current continuation and stopped Pricing source cash differ".into(),
                    );
                }
                native_sources.extend(priced);
                exercise_policies.push(ExerciseReceipt {
                    position_id: p.position_id.clone(),
                    scenario_id: scene.scenario_id.clone(),
                    role: Role::Pricing,
                    policy: serde_json::to_value(policy).map_err(|e| e.to_string())?,
                });
                for (role, hs, bump) in [
                    (
                        Role::PricingDown,
                        h.pricing_down.as_slice(),
                        -r.bump_bp * 1e-4,
                    ),
                    (Role::PricingUp, h.pricing_up.as_slice(), r.bump_bp * 1e-4),
                ] {
                    let bundle = pricing_bundle(hs)?;
                    let policy = compile_policy(exercise, hs, &bundle, scene, oas, bump)?;
                    let priced = stopped_pricing(p, hs, &bundle, role, scene, &policy)?;
                    if !close(
                        weighted_price(
                            &priced,
                            &scene.collateral_curve,
                            r.as_of_ordinal,
                            r.as_of_ordinal,
                            oas,
                            bump,
                        )?,
                        policy.value,
                    ) {
                        return Err(
                            "native bumped continuation and stopped Pricing source cash differ"
                                .into(),
                        );
                    }
                    native_sources.extend(priced);
                    exercise_policies.push(ExerciseReceipt {
                        position_id: p.position_id.clone(),
                        scenario_id: scene.scenario_id.clone(),
                        role,
                        policy: serde_json::to_value(policy).map_err(|e| e.to_string())?,
                    });
                }
            } else {
                for (role, hs) in roles(h) {
                    for source in hs {
                        native_sources.push(generate(p, source, role, scene)?);
                    }
                }
            }
            for &day in &r.reporting_ordinals {
                let mut pv = 0.;
                let mut down = 0.;
                let mut up = 0.;
                for source in &native_sources[begin..] {
                    let bump = if source.role == Role::PricingDown {
                        -r.bump_bp * 1e-4
                    } else if source.role == Role::PricingUp {
                        r.bump_bp * 1e-4
                    } else {
                        0.
                    };
                    if source.role == Role::Physical {
                        continue;
                    }
                    let x = source.weight
                        * price(
                            source,
                            &scene.collateral_curve,
                            r.as_of_ordinal,
                            day,
                            oas,
                            bump,
                        )?;
                    match source.role {
                        Role::Pricing => pv = money(pv + x)?,
                        Role::PricingDown => down = money(down + x)?,
                        Role::PricingUp => up = money(up + x)?,
                        Role::Physical => unreachable!(),
                    }
                }
                metrics.push(Metric {
                    position_id: p.position_id.clone(),
                    scenario_id: scene.scenario_id.clone(),
                    ordinal: day,
                    pv,
                    dv01: money((down - up) / (2. * r.bump_bp))?,
                });
            }
        }
    }
    let out=ResultData{schema:SCHEMA,raw_source:r.clone(),original_oas:anchors,native_sources,metrics,admitted_work_upper:work,
        shared_draws_retained:true,physical_accounting_source_retained:true,generated_physical_recipes_retained:None,generated_oas_calibration:None,empirical_validation:false,
        scope:"native complete supplied commercial/issuer-credit economics and reviewed Physical accounting; exact source/default/recovery time; optional native observed-prefix finite-distribution continuation/actual stopping and verified original stopped Physical acquisition; paired Pricing risk at immutable original OAS; supplied hazards/index paths/recognition are not empirical fits or automatic saved-book adoption",exercise_policies,original_physical_sources,original_acquisition_proofs};
    strict_json::serialized_size(&out, MAX_BYTES / 4)?;
    check()?;
    Ok(out)
}
