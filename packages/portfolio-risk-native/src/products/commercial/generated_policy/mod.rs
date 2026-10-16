//! Sealed generated Q-policy / separately supplied Physical bookkeeping roles.
//! Supplied P laws are never inferred from Pricing laws. Cash and native stops
//! originate in the shared source owner; Physical outcomes never fit continuation.
use crate::{
    commercial_cash::{economic_cash, EconomicCash},
    commercial_credit_compiler as owner, commercial_stochastic_exercise as stochastic,
    term_enhancements::exercise::{Decision, ExerciseTiming},
};
use portfolio_model_core::{credit::CreditMeasure, strict_json};
use serde::{Deserialize, Serialize};
mod capacity;
#[cfg(test)]
mod counted_work_tests;
mod physical;
#[cfg(test)]
mod physical_tests;
pub use physical::PhysicalPathInputs;
pub(crate) fn raw_recipe_upper(raw: &owner::Request) -> Result<usize, String> {
    capacity::raw_recipe_upper(raw)
}
const MAX_WORK: usize = 4_000_000_000;
const MAX_BYTES: usize = 128 << 20;
pub const SCHEMA: &str = "commercial-generated-policy-1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub admission_tier: String,
    pub pricing_training: stochastic::Request,
    pub original_physical: stochastic::Request,
    pub current_physical: Vec<stochastic::Request>,
    pub current_pricing: Vec<stochastic::Request>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_path_inputs: Option<PhysicalPathInputs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oas_calibration: Option<stochastic::calibration::Policy>,
}
#[derive(Debug, Serialize)]
pub struct Admission {
    pub work_upper: usize,
    pub retained_bytes_upper: usize,
    pub physical_paths: usize,
    pub pricing_paths: usize,
}
#[derive(Serialize)]
pub struct EconomicSource {
    raw_request: owner::Request,
    native_stops: Vec<owner::NativeStop>,
    cash: Vec<EconomicCash>,
    pub weight: f64,
}
impl EconomicSource {
    pub(crate) fn replay_pricing(
        raw_request: owner::Request,
        native_stops: Vec<owner::NativeStop>,
        weight: f64,
    ) -> Result<Self, String> {
        let result = owner::evaluate_economic_with_stops(
            &raw_request,
            CreditMeasure::Pricing,
            &native_stops,
        )?;
        Ok(Self {
            raw_request,
            native_stops,
            cash: economic_cash(result.carry())?,
            weight,
        })
    }
    pub fn raw(&self) -> &owner::Request {
        &self.raw_request
    }
    pub fn stops(&self) -> &[owner::NativeStop] {
        &self.native_stops
    }
    pub fn cash(&self) -> &[EconomicCash] {
        &self.cash
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        owner::Request,
        Vec<owner::NativeStop>,
        Vec<EconomicCash>,
        f64,
    ) {
        (self.raw_request, self.native_stops, self.cash, self.weight)
    }
}
#[derive(Serialize)]
pub struct PricingRole {
    pub raw_law: stochastic::Request,
    pub fitted_policy: stochastic::NativeTrainedPolicy,
    pub sources: Vec<EconomicSource>,
}
#[derive(Serialize)]
pub struct PhysicalSource {
    pub weight: f64,
    raw_request: owner::Request,
    native_stops: Vec<owner::NativeStop>,
    cash: Vec<EconomicCash>,
    book_points: Vec<BookPoint>,
}
#[derive(Serialize)]
pub struct BookPoint {
    pub year: f64,
    pub principal: f64,
    /// Preserve native posting order for exactly the existing income reduction.
    pub interest_income_lines: Vec<f64>,
}
impl PhysicalSource {
    pub fn raw(&self) -> &owner::Request {
        &self.raw_request
    }
    pub fn stops(&self) -> &[owner::NativeStop] {
        &self.native_stops
    }
    pub fn cash(&self) -> &[EconomicCash] {
        &self.cash
    }
    pub fn book_points(&self) -> &[BookPoint] {
        &self.book_points
    }
    pub fn metric_period(&self, left: f64, right: f64) -> Result<(f64, f64), String> {
        if !left.is_finite() || !right.is_finite() || left < 0. || right < left {
            return Err("generated Physical report period invalid".into());
        }
        let state = self
            .book_points
            .iter()
            .rfind(|g| g.year <= left)
            .ok_or("complete generated Physical opening report state absent")?;
        let mut nii = 0.;
        for group in self
            .book_points
            .iter()
            .filter(|g| g.year > left && g.year <= right)
        {
            for amount in &group.interest_income_lines {
                nii -= amount;
                if !nii.is_finite() || nii.abs() > portfolio_model_core::limits::MAX_MONETARY_AMOUNT
                {
                    return Err("generated Physical income amount invalid".into());
                }
            }
        }
        Ok((state.principal, nii))
    }
    pub(crate) fn materialize(
        &self,
        proof: &owner::NativeOriginalAcquisitionProof,
    ) -> Result<owner::ResultData, String> {
        owner::evaluate_with_native_original_acquisition(
            &self.raw_request,
            &self.native_stops,
            proof,
        )
    }
}
#[derive(Serialize)]
pub struct PhysicalRole {
    pub raw_law: stochastic::Request,
    pub sources: Vec<PhysicalSource>,
}
#[derive(Serialize)]
pub struct Prepared {
    pub schema: &'static str,
    pub raw_request: Request,
    pub admission: Admission,
    pub original_pricing: PricingRole,
    pub original_physical_sources: Vec<EconomicSource>,
    pub(crate) original_acquisition_proof: owner::NativeOriginalAcquisitionProof,
    pub current_pricing: Vec<PricingRole>,
    pub current_physical: Vec<PhysicalRole>,
    pub physical_outcomes_used_for_policy_fit: bool,
}
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("generated role bound overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("generated role bound overflow".into())
}
fn encoded_equal<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    Ok(strict_json::encode(a, 1 << 20)? == strict_json::encode(b, 1 << 20)?)
}
fn contractual_coupon_shape(
    coupon: &crate::products::term::contracts::indexed::Coupon,
    asof: i32,
) -> Result<Vec<u8>, String> {
    use crate::products::term::contracts::indexed::{Coupon, RateSource};
    let mut shape = coupon.clone();
    if let Coupon::Indexed { observations, .. } = &mut shape {
        for observation in observations {
            if observation.fixing_ordinal > asof
                && matches!(observation.source, RateSource::ConditionalProjection { .. })
            {
                observation.rate = 0.;
                observation.source = RateSource::ConditionalProjection {
                    model: crate::products::term::contracts::Identity {
                        id: "native-contractual-shape".into(),
                        revision: "1".into(),
                    },
                    path: crate::products::term::contracts::Identity {
                        id: "native-contractual-shape".into(),
                        revision: "1".into(),
                    },
                    information_unix_seconds: observation.selection_cutoff_unix_seconds,
                    measure: portfolio_model_core::cashflow::CashflowMeasure::Physical,
                };
            }
        }
    }
    strict_json::encode(&shape, 1 << 20)
}
fn same_contract(a: &owner::Request, b: &owner::Request) -> Result<(), String> {
    same_contract_source(
        &a.source.current,
        &b.source.current,
        a.source.current.asof_ordinal,
    )
}
fn same_contract_source(
    a: &crate::products::term::contracts::indexed::ContractPathRequest,
    b: &crate::products::term::contracts::indexed::ContractPathRequest,
    asof: i32,
) -> Result<(), String> {
    if a.contract != b.contract
        || a.position_id != b.position_id
        || a.currency != b.currency
        || a.monetary_unit != b.monetary_unit
        || a.asof_ordinal != b.asof_ordinal
        || a.opening_principal.to_bits() != b.opening_principal.to_bits()
        || a.periods.len() != b.periods.len()
        || !encoded_equal(&a.principal_plan, &b.principal_plan)?
    {
        return Err("generated roles require unchanged native acquired contract/face/principal/date identity".into());
    }
    for (a, b) in a.periods.iter().zip(&b.periods) {
        if a.period_id != b.period_id
            || a.start_ordinal != b.start_ordinal
            || a.end_ordinal != b.end_ordinal
            || a.payment_ordinal != b.payment_ordinal
            || a.day_count != b.day_count
            || a.fee.to_bits() != b.fee.to_bits()
            || contractual_coupon_shape(&a.coupon, asof)?
                != contractual_coupon_shape(&b.coupon, asof)?
        {
            return Err(
                "generated contractual coupon dates/fees changed without modification ownership"
                    .into(),
            );
        }
    }
    Ok(())
}
fn compatible(q: &stochastic::Request, p: &stochastic::Request) -> Result<(), String> {
    same_contract(&q.source, &p.source)?;
    physical::same_draw_catalog(&q.source, &p.source)?;
    if q.regression_policy != p.regression_policy
        || !encoded_equal(&q.policy, &p.policy)?
        || q.time_ordinals != p.time_ordinals
        || !encoded_equal(&q.coupon_loadings, &p.coupon_loadings)?
        || q.drivers.len() != p.drivers.len()
        || q.drivers.iter().zip(&p.drivers).any(|(a, b)| {
            a.driver_id != b.driver_id || a.observation_policy != b.observation_policy
        })
    {
        return Err("generated role observability/features and reviewed exercise dates must match Pricing training contract".into());
    }
    Ok(())
}
pub(crate) fn compact_upper(law: &stochastic::Request) -> Result<usize, String> {
    let raw = raw_recipe_upper(&law.source)?;
    let legacy_cash = law.source.source.current.periods.len()
        + 36
        + law
            .source
            .performing_draw_tranches
            .iter()
            .map(|d| d.source.current.periods.len() + 1)
            .sum::<usize>();
    let cash = if law.source.counted_work.is_some() {
        stochastic::cash_event_bound(&law.source)?
    } else {
        legacy_cash
    };
    // EconomicCash contains a native <=256-byte event id, a native kind, four
    // scalars and Vec/String capacity. The single stop has bounded identities.
    let samples = mul(
        law.evaluation.paths,
        add(add(raw, mul(cash, 1024)?)?, 4096)?,
    )?;
    // A policy retains a regression at every actual reviewed exercise date.
    // Account for all numeric/feature/state vectors and their Vec capacities.
    let features = stochastic::feature_bound(law)?;
    let columns = stochastic::regression_columns_upper(law)?;
    let vector_elements = add(mul(features, 3)?, mul(columns, 2)?)?;
    let regression = add(
        std::mem::size_of::<stochastic::Regression>(),
        add(
            mul(vector_elements, 16)?,
            6 * std::mem::size_of::<portfolio_model_core::credit::CreditState>(),
        )?,
    )?;
    add(
        samples,
        add(
            add(
                mul(strict_json::serialized_size(law, 1 << 20)?, 20)?,
                law.source
                    .counted_work
                    .as_ref()
                    .map_or(0, |a| a.retained_bytes()),
            )?,
            mul(mul(law.policy.exercise_dates.len(), regression)?, 2)?,
        )?,
    )
}
fn acquisition_law(r: &Request) -> Result<stochastic::Request, String> {
    let mut law = r.original_physical.clone();
    law.source.source.current = law.source.source.original.clone();
    for draw in &mut law.source.performing_draw_tranches {
        draw.source.current = draw.source.original.clone();
    }
    refresh_counted_source(&mut law.source)?;
    Ok(law)
}
fn pricing_admission(r: &Request, law: &stochastic::Request) -> Result<(usize, usize), String> {
    let (mut work, bytes) = stochastic::admitted_law(law, CreditMeasure::Pricing)?;
    if r.physical_path_inputs.is_some() {
        // This owner fits one Q training sample, then compiles each evaluation
        // prefix and stopped tail. Standalone exercise's extra hypothetical
        // training replay is not executed here. Preserve all regression work.
        let source = stochastic::admitted_source(law, CreditMeasure::Pricing)?;
        work = work
            .checked_sub(mul(law.training.paths, source.joined_work_upper)?)
            .ok_or("generated Pricing pass ownership differs")?;
    }
    Ok((work, bytes))
}
pub(crate) fn bind_counted_source(raw: &mut owner::Request) -> Result<(), String> {
    let authority =
        crate::products::term::contracts::indexed::work::GeneratedCatalogAuthority::bind(raw)?;
    raw.counted_work = Some(Box::new(authority));
    Ok(())
}
pub(crate) fn refresh_counted_source(raw: &mut owner::Request) -> Result<(), String> {
    if raw.counted_work.is_some() {
        bind_counted_source(raw)?;
    }
    Ok(())
}
pub(crate) fn selected_request(r: &Request) -> Result<std::borrow::Cow<'_, Request>, String> {
    if r.physical_path_inputs
        .as_ref()
        .is_none_or(|p| p.source_work_policy.is_none())
    {
        return Ok(std::borrow::Cow::Borrowed(r));
    }
    let mut selected = r.clone();
    for law in std::iter::once(&mut selected.pricing_training)
        .chain(std::iter::once(&mut selected.original_physical))
        .chain(selected.current_physical.iter_mut())
        .chain(selected.current_pricing.iter_mut())
    {
        if let Some(authority) = &law.source.counted_work {
            authority.verify(&law.source)?;
        } else {
            bind_counted_source(&mut law.source)?;
        }
    }
    Ok(std::borrow::Cow::Owned(selected))
}
/// Arc bodies are shared by the original proof and the cloned NativeSource.
/// Subtract only a guaranteed body minimum; every clone retains its own header.
pub(crate) fn shared_authority_lower(
    r: &Request,
    law: &stochastic::Request,
    original: bool,
) -> Result<usize, String> {
    let selected = selected_request(r)?;
    let r = selected.as_ref();
    if r.physical_path_inputs
        .as_ref()
        .is_none_or(|p| p.source_work_policy.is_none())
    {
        return Ok(0);
    }
    let mut law = law.clone();
    if law.source.counted_work.is_none() {
        bind_counted_source(&mut law.source)?;
    }
    let inputs = if original {
        physical::original(r)
    } else {
        physical::current(r, &law)?
    };
    let mut n = 0;
    for index in 0..law.evaluation.paths {
        let raw = physical::source(&law, inputs, index, original)?;
        let cloned = raw.clone();
        n = add(
            n,
            raw.counted_work
                .as_ref()
                .unwrap()
                .shared_body_lower_with(cloned.counted_work.as_ref().unwrap()),
        )?;
    }
    Ok(n)
}
pub fn admit(r: &Request) -> Result<Admission, String> {
    let selected = selected_request(r)?;
    let r = selected.as_ref();
    check()?;
    if r.schema != SCHEMA
        || r.admission_tier != "bounded_generated_policy_replay-1"
        || r.current_physical.is_empty()
        || r.current_physical.len() > 4
        || r.current_pricing.is_empty()
        || r.current_pricing.len() > 12
        || r.pricing_training.parallel_bump != 0.
    {
        return Err("explicit bounded generated Pricing/Physical role tier required".into());
    }
    let (mut work, trainbytes) = pricing_admission(r, &r.pricing_training)?;
    let mut peak_bytes = trainbytes;
    let mut bytes = compact_upper(&r.pricing_training)?;
    compatible(&r.pricing_training, &r.original_physical)?;
    let original_law = acquisition_law(r)?;
    physical::validate(r)?;
    same_contract(&r.original_physical.source, &original_law.source)?;
    physical::same_draw_catalog(&r.original_physical.source, &original_law.source)?;
    let original_upper = physical::upper(&original_law, physical::original(r), true)?;
    let (pwork, pbytes) = (original_upper.0, original_upper.1);
    work = add(work, mul(pwork, 2)?)?;
    peak_bytes = peak_bytes.max(pbytes);
    bytes = add(
        bytes,
        mul(
            physical::expanded_compact(&r.original_physical, &original_upper)?,
            2,
        )?
        .checked_sub(shared_authority_lower(r, &original_law, true)?)
        .ok_or("shared original authority reservation differs")?,
    )?;
    bytes = add(bytes, physical::retained_inputs(r)?)?;
    let mut pricing_paths = r.pricing_training.evaluation.paths;
    let mut physical_paths = r.original_physical.evaluation.paths;
    let mut roles = std::collections::BTreeSet::new();
    for law in &r.current_pricing {
        compatible(&r.pricing_training, law)?;
        if law.fixed_base_oas.to_bits() != r.pricing_training.fixed_base_oas.to_bits()
            || !roles.insert((
                law.source.source.current.scenario_id.as_str(),
                law.parallel_bump.to_bits(),
            ))
            || !encoded_equal(&law.evaluation, &r.pricing_training.evaluation)?
            || !encoded_equal(&law.training, &r.pricing_training.training)?
        {
            return Err("generated Pricing scenarios require immutable baseOAS, paired CRN and distinct bump roles".into());
        }
        let (w, b) = pricing_admission(r, law)?;
        work = add(work, w)?;
        peak_bytes = peak_bytes.max(b);
        bytes = add(bytes, compact_upper(law)?)?;
        pricing_paths = add(pricing_paths, law.evaluation.paths)?;
    }
    for law in &r.current_physical {
        compatible(&r.pricing_training, law)?;
        if !encoded_equal(&law.evaluation, &r.original_physical.evaluation)?
            || !encoded_equal(
                &law.source.source.acquisition,
                &r.original_physical.source.source.acquisition,
            )?
            || !encoded_equal(
                &law.source.source.original,
                &r.original_physical.source.source.original,
            )?
        {
            return Err("generated current Physical law requires retained original acquisition/source and shared Physical CRN".into());
        }
        if !r.current_pricing.iter().any(|q| {
            q.parallel_bump == 0.
                && q.source.source.current.scenario_id == law.source.source.current.scenario_id
        }) {
            return Err("generated Physical scenario lacks explicit base Pricing policy".into());
        }
        let upper = physical::upper(law, physical::current(r, law)?, false)?;
        let (w, b) = (upper.0, upper.1);
        work = add(work, w)?;
        peak_bytes = peak_bytes.max(b);
        bytes = add(
            bytes,
            // One genuine financial owner exists at a time. All retained raw
            // recipes, cash and ordered book coefficient points are admitted.
            add(
                physical::expanded_compact(law, &upper)?,
                mul(
                    mul(upper.2, add(512, mul(upper.3, 32)?)?)?,
                    law.evaluation.paths,
                )?,
            )?,
        )?;
        physical_paths = add(physical_paths, law.evaluation.paths)?;
    }
    // Single-source component bounds are distinct from the selected retained
    // sample tier. Every role and original proof is reserved before generation.
    bytes = add(bytes, peak_bytes)?;
    if work > MAX_WORK || bytes > MAX_BYTES || strict_json::serialized_size(r, 4 << 20)? > 4 << 20 {
        return Err(format!(
            "generated policy role admission exceeds4B/128MiB: work={work},bytes={bytes}"
        ));
    }
    Ok(Admission {
        work_upper: work,
        retained_bytes_upper: bytes,
        physical_paths,
        pricing_paths,
    })
}
fn first_stop(
    policy: &stochastic::NativeTrainedPolicy,
    law: &stochastic::Request,
    raw: &owner::Request,
    carry: &portfolio_model_core::credit_path_income::ResultData,
    tape: &stochastic::Tape,
    index: usize,
) -> Result<Vec<owner::NativeStop>, String> {
    let cash = economic_cash(carry)?;
    for (j, date) in policy.request().policy.exercise_dates.iter().enumerate() {
        let point = stochastic::compiled_point_features(law, raw, carry, tape, index, date)?;
        let (decision, _) = stochastic::fitted_decision(policy, j, &point, &cash)?;
        let (action, factor, fee) = match decision {
            Decision::Continue => continue,
            Decision::Call => (
                owner::NativeStopAction::Call,
                date.call_clean_price_factor
                    .ok_or("generated call absent")?,
                date.call_fee,
            ),
            Decision::Put => (
                owner::NativeStopAction::Put,
                date.put_clean_price_factor.ok_or("generated put absent")?,
                date.put_fee,
            ),
        };
        return Ok(vec![owner::NativeStop::from_compiled_policy_with_timing(
            raw.credit.paths[0].path_key,
            format!(
                "generated-policy:{}:{}",
                policy.request().identity.id,
                date.ordinal
            ),
            date.ordinal,
            action,
            factor,
            fee,
            policy.request().policy.identity.clone(),
            match date.timing {
                ExerciseTiming::BeforeScheduledCash => owner::NativeStopTiming::BeforeScheduledCash,
                ExerciseTiming::AfterScheduledCash => owner::NativeStopTiming::AfterScheduledCash,
            },
            if point.principal == 0. {
                policy
                    .request()
                    .policy
                    .rights_only_termination
                    .as_ref()
                    .map(|r| r.review.clone())
            } else {
                None
            },
        )?]);
    }
    Ok(Vec::new())
}
fn pricing_role(law: &stochastic::Request) -> Result<PricingRole, String> {
    let fitted_policy = stochastic::fit_training(law)?;
    pricing_role_with_policy(law, fitted_policy)
}
fn pricing_role_with_policy(
    law: &stochastic::Request,
    fitted_policy: stochastic::NativeTrainedPolicy,
) -> Result<PricingRole, String> {
    if !encoded_equal(law, fitted_policy.request())? {
        return Err("original generated fitted policy source identity differs".into());
    }
    let tape = stochastic::generate_tape(law, &law.evaluation)?;
    let mut sources = Vec::new();
    for i in 0..law.evaluation.paths {
        check()?;
        let raw =
            stochastic::generated_path(law, &law.evaluation, &tape, i, CreditMeasure::Pricing)?;
        let prefix = owner::evaluate_economic_with_measure(&raw, CreditMeasure::Pricing)?;
        let stops = first_stop(&fitted_policy, law, &raw, prefix.carry(), &tape, i)?;
        drop(prefix);
        let result = owner::evaluate_economic_with_stops(&raw, CreditMeasure::Pricing, &stops)?;
        let cash = economic_cash(result.carry())?;
        if raw.counted_work.is_some() && cash.len() > stochastic::cash_event_bound(&raw)? {
            return Err("native cash output exceeds counted source inventory".into());
        }
        sources.push(EconomicSource {
            raw_request: raw,
            native_stops: stops,
            cash,
            weight: 1. / law.evaluation.paths as f64,
        });
    }
    Ok(PricingRole {
        raw_law: law.clone(),
        fitted_policy,
        sources,
    })
}
pub fn prepare(r: &Request) -> Result<Prepared, String> {
    if r.oas_calibration.is_some() {
        return Err("generated OAS calibration requires original graph quote authority".into());
    }
    prepare_internal(r, None)
}
pub(crate) fn prepare_with_original_pricing(
    r: &Request,
    original: PricingRole,
) -> Result<Prepared, String> {
    if r.oas_calibration.is_some() {
        return Err("generated calibrated preparation requires sealed resolved controls".into());
    }
    if !encoded_equal(&r.pricing_training, &original.raw_law)?
        || !encoded_equal(&r.pricing_training, original.fitted_policy.request())?
    {
        return Err("calibrated original pricing authority differs".into());
    }
    prepare_internal(r, Some(original))
}
fn prepare_internal(r: &Request, original: Option<PricingRole>) -> Result<Prepared, String> {
    let selected = selected_request(r)?;
    let r = selected.as_ref();
    let admission = admit(r)?;
    let original_pricing = if let Some(original) = original {
        original
    } else {
        pricing_role(&r.pricing_training)?
    };
    // Acquisition expectations originate in the explicitly retained original
    // source, including its scenario/path/fixing identities. Current scenario
    // labels never replace this authority.
    let acquisition_law = acquisition_law(r)?;
    let original_tape = stochastic::generate_tape(&acquisition_law, &acquisition_law.evaluation)?;
    let mut original_physical_sources = Vec::new();
    for i in 0..r.original_physical.evaluation.paths {
        check()?;
        let mut raw = physical::generated_path(
            &acquisition_law,
            physical::original(r),
            &original_tape,
            i,
            true,
        )?;
        for draw in &mut raw.performing_draw_tranches {
            draw.source.original = draw.source.current.clone();
        }
        raw.source.original = raw.source.current.clone();
        refresh_counted_source(&mut raw)?;
        let prefix = owner::evaluate_physical_prefix_for_acquisition(&raw)?;
        let stops = first_stop(
            &original_pricing.fitted_policy,
            &acquisition_law,
            &raw,
            prefix.carry(),
            &original_tape,
            i,
        )?;
        drop(prefix);
        let stopped = owner::evaluate_physical_prefix_with_stops_for_acquisition(&raw, &stops)?;
        let cash = economic_cash(stopped.carry())?;
        if raw.counted_work.is_some() && cash.len() > stochastic::cash_event_bound(&raw)? {
            return Err("native cash output exceeds counted source inventory".into());
        }
        original_physical_sources.push(EconomicSource {
            raw_request: raw,
            native_stops: stops,
            cash,
            weight: 1. / r.original_physical.evaluation.paths as f64,
        });
    }
    let inputs: Vec<_> = original_physical_sources
        .iter()
        .map(|p| (&p.raw_request, p.weight, p.native_stops.as_slice()))
        .collect();
    let original_acquisition_proof =
        owner::NativeOriginalAcquisitionProof::compile_generated(&inputs)?;
    // Original reviewed actions are validated by the genuine full owner too;
    // they never alter acquisition cash or fit continuation.
    for source in &original_physical_sources {
        if source.raw_request.reviewed_credit_actions.is_some() {
            owner::evaluate_with_native_original_acquisition(
                &source.raw_request,
                &source.native_stops,
                &original_acquisition_proof,
            )?;
        }
    }
    let mut current_pricing = Vec::new();
    for law in &r.current_pricing {
        current_pricing.push(pricing_role(law)?);
    }
    let mut current_physical = Vec::new();
    for law in &r.current_physical {
        let tape = stochastic::generate_tape(law, &law.evaluation)?;
        let policy = &current_pricing
            .iter()
            .find(|p| {
                p.raw_law.parallel_bump == 0.
                    && p.raw_law.source.source.current.scenario_id
                        == law.source.source.current.scenario_id
            })
            .ok_or("native base Pricing policy missing")?
            .fitted_policy;
        let mut sources = Vec::new();
        for (i, original) in original_physical_sources
            .iter()
            .enumerate()
            .take(law.evaluation.paths)
        {
            check()?;
            let mut raw =
                physical::generated_path(law, physical::current(r, law)?, &tape, i, false)?;
            physical::retain_original_draws(&mut raw, &original.raw_request)?;
            raw.source.original = original.raw_request.source.original.clone();
            refresh_counted_source(&mut raw)?;
            let prefix = owner::evaluate_physical_prefix_for_acquisition(&raw)?;
            let stops = first_stop(policy, law, &raw, prefix.carry(), &tape, i)?;
            drop(prefix);
            let projection = owner::evaluate_with_native_original_acquisition(
                &raw,
                &stops,
                &original_acquisition_proof,
            )?;
            let cash = economic_cash(projection.carry())?;
            if raw.counted_work.is_some() && cash.len() > stochastic::cash_event_bound(&raw)? {
                return Err("native cash output exceeds counted source inventory".into());
            }
            let book_points = projection.accounting().groups().iter().map(|g| BookPoint {
                year:g.year, principal:g.next.principal,
                interest_income_lines:g.postings.iter().filter(|p|p.posting.account == portfolio_model_core::credit_accounting::AccountCode::InterestIncome).map(|p|p.posting.amount).collect(),
            }).collect();
            drop(projection);
            sources.push(PhysicalSource {
                weight: 1. / law.evaluation.paths as f64,
                raw_request: raw,
                native_stops: stops,
                cash,
                book_points,
            });
        }
        current_physical.push(PhysicalRole {
            raw_law: law.clone(),
            sources,
        });
    }
    let result = Prepared {
        schema: SCHEMA,
        raw_request: r.clone(),
        admission,
        original_pricing,
        original_physical_sources,
        original_acquisition_proof,
        current_pricing,
        current_physical,
        physical_outcomes_used_for_policy_fit: false,
    };
    strict_json::serialized_size(&result, MAX_BYTES)?;
    check()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn request() -> Request {
        let q: stochastic::Request = serde_json::from_slice(include_bytes!(
            "../../../../../../examples/models/commercial-generated-exercise-1.json"
        ))
        .unwrap();
        let mut p_json = serde_json::to_value(&q).unwrap();
        fn physical(v: &mut serde_json::Value) {
            match v {
                serde_json::Value::Object(o) => {
                    if let Some(measure) = o.get_mut("measure") {
                        if *measure == "pricing" {
                            *measure = serde_json::Value::String("physical".into());
                        }
                    }
                    for val in o.values_mut() {
                        physical(val);
                    }
                }
                serde_json::Value::Array(a) => {
                    for val in a {
                        physical(val)
                    }
                }
                _ => {}
            }
        }
        physical(&mut p_json);
        p_json["annual_covariance"] = serde_json::json!([0.]);
        p_json["identity"] = serde_json::json!({"id":"explicit-physical-law","revision":"1"});
        let p: stochastic::Request = serde_json::from_value(p_json).unwrap();
        Request {
            schema: SCHEMA.into(),
            admission_tier: "bounded_generated_policy_replay-1".into(),
            pricing_training: q.clone(),
            original_physical: p.clone(),
            current_physical: vec![p],
            current_pricing: vec![q],
            oas_calibration: None,
            physical_path_inputs: None,
        }
    }
    #[test]
    fn generated_q_policy_replays_separate_physical_law_original_acquisition_and_book() {
        let mut r = request();
        for bump in [-0.001, 0.001] {
            let mut q = r.pricing_training.clone();
            q.parallel_bump = bump;
            r.current_pricing.push(q);
        }
        let result = prepare(&r).unwrap();
        assert_eq!(result.current_pricing.len(), 3);
        assert!(!result.physical_outcomes_used_for_policy_fit);
        assert_eq!(result.current_physical[0].sources.len(), 128);
        let projection = result.current_physical[0].sources[0]
            .materialize(&result.original_acquisition_proof)
            .unwrap();
        assert!(result.current_physical[0]
            .sources
            .iter()
            .all(|s| s.raw().source.acquisition.cost == 100.));
        assert!(projection.original_cash_quote().to_bits() == 100_f64.to_bits());
        assert!(projection
            .carry()
            .carry_points()
            .iter()
            .all(|p| p.effective_interest == 0.));

        assert!(result.admission.retained_bytes_upper <= MAX_BYTES);
    }
    #[test]
    fn physical_distribution_cannot_change_frozen_q_training_or_imply_missing_original_anchor() {
        let r = request();
        let policy = stochastic::fit_training(&r.pricing_training).unwrap();
        let mut other = r.clone();
        other.current_physical[0].drivers[0].long_run_mean = 0.001;
        let same = stochastic::fit_training(&other.pricing_training).unwrap();
        assert_eq!(
            strict_json::encode(&policy, 1 << 20).unwrap(),
            strict_json::encode(&same, 1 << 20).unwrap()
        );
        other.current_physical[0].source.source.acquisition.cost += 1.;
        assert!(admit(&other).unwrap_err().contains("retained original"));
        let mut mismatch = r;
        mismatch.current_physical[0].evaluation.credit_seed += 1;
        assert!(admit(&mismatch).is_err());
    }
}

/// Source catalogs and expanded Physical owners coexist with the consuming
/// generated journal. Shared proof/compact state retain their old reservations.
pub fn physical_journal_retained_upper(r: &Request) -> Result<usize, String> {
    let selected = selected_request(r)?;
    let r = selected.as_ref();
    let original_upper = physical::upper(&r.original_physical, physical::original(r), true)?;
    let mut bytes = add(
        mul(
            physical::expanded_compact(&r.original_physical, &original_upper)?,
            2,
        )?
        .checked_sub(shared_authority_lower(r, &r.original_physical, true)?)
        .ok_or("shared original authority reservation differs")?,
        physical::retained_inputs(r)?,
    )?;
    for law in &r.current_physical {
        let upper = physical::upper(law, physical::current(r, law)?, false)?;
        bytes = add(bytes, physical::expanded_compact(law, &upper)?)?;
        bytes = add(
            bytes,
            mul(upper.4, law.evaluation.paths)?
                .checked_sub(shared_authority_lower(r, law, false)?)
                .ok_or("shared Physical authority reservation differs")?,
        )?;
        bytes = add(
            bytes,
            mul(
                mul(upper.2, add(512, mul(upper.3, 32)?)?)?,
                law.evaluation.paths,
            )?,
        )?;
        bytes = add(bytes, mul(law.evaluation.paths, 8192)?)?;
    }
    Ok(bytes)
}

#[derive(Debug)]
pub struct PhysicalSourceCapacity {
    pub work_upper: usize,
    pub retained_bytes_upper: usize,
    pub full_credit_end_year: f64,
    pub raw_recipe_bytes_upper: usize,
}
/// Maximum one genuine selected Physical owner, including every stopped source
/// and reviewed action. Omission preserves the historical template reservation.
pub fn current_physical_source_capacity(
    r: &Request,
    law: &stochastic::Request,
) -> Result<PhysicalSourceCapacity, String> {
    let selected = selected_request(r)?;
    let r = selected.as_ref();
    let mut selected_law = law.clone();
    if r.physical_path_inputs
        .as_ref()
        .is_some_and(|p| p.source_work_policy.is_some())
    {
        bind_counted_source(&mut selected_law.source)?;
    }
    let law = &selected_law;
    let inputs = physical::current(r, law)?;
    let mut result = PhysicalSourceCapacity {
        work_upper: 0,
        retained_bytes_upper: 0,
        full_credit_end_year: 0.,
        raw_recipe_bytes_upper: 0,
    };
    let count = if inputs.is_some() {
        law.evaluation.paths
    } else {
        1
    };
    for index in 0..count {
        check()?;
        let raw = if inputs.is_some() {
            physical::source(law, inputs, index, false)?
        } else {
            (*law.source).clone()
        };
        let raw_bytes = raw_recipe_upper(&raw)?;
        let admitted = if inputs.is_some() {
            let mut expanded = law.clone();
            *expanded.source = raw;
            stochastic::admitted_source(&expanded, CreditMeasure::Physical)?
        } else {
            owner::admit(&raw)?
        };
        result.work_upper = result.work_upper.max(admitted.joined_work_upper);
        result.retained_bytes_upper = result.retained_bytes_upper.max(admitted.joined_bytes_upper);
        result.full_credit_end_year = result
            .full_credit_end_year
            .max(admitted.full_credit_end_year);
        result.raw_recipe_bytes_upper = result.raw_recipe_bytes_upper.max(raw_bytes);
    }
    Ok(result)
}
