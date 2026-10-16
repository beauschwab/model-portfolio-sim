//! Explicit immutable per-path Physical source catalogs and reviewed actions.
//! Catalogs declare source recipes, never monetary checkpoints or stopping rules.
use super::*;
use portfolio_model_core::commercial_coupon_accounting::reviews::ReviewedActions;

pub const SCHEMA: &str = "commercial-generated-physical-path-inputs-1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecipe {
    pub identity: crate::term_enhancements::Identity,
    pub performing_draw_tranches: Vec<owner::PerformingDrawTranche>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathInputs {
    pub path_key: u64,
    pub review: crate::term_enhancements::Identity,
    pub source_recipe_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_credit_actions: Option<ReviewedActions>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleInputs {
    pub source_recipes: Vec<SourceRecipe>,
    pub paths: Vec<PathInputs>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioInputs {
    pub scenario_id: String,
    pub inputs: RoleInputs,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalPathInputs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_work_policy: Option<SourceWorkPolicy>,
    pub schema: String,
    pub identity: crate::term_enhancements::Identity,
    pub original_physical: RoleInputs,
    pub current_physical: Vec<ScenarioInputs>,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceWorkPolicy {
    #[serde(rename = "counted_native_contract_factor_work-1")]
    CountedNativeContractFactorWork,
}
fn identity(id: &crate::term_enhancements::Identity) -> Result<(), String> {
    crate::term_enhancements::identity(id, &mut 0)
}
pub(super) fn same_draw_catalog(a: &owner::Request, b: &owner::Request) -> Result<(), String> {
    if a.performing_draw_tranches.len() != b.performing_draw_tranches.len() {
        return Err("generated Q/P canonical draw catalog length differs".into());
    }
    let observable_origin = a.source.current.asof_ordinal;
    for (a, b) in a
        .performing_draw_tranches
        .iter()
        .zip(&b.performing_draw_tranches)
    {
        if a.review != b.review
            || a.draw_ordinal != b.draw_ordinal
            || a.eligible_origins != b.eligible_origins
            || !encoded_equal(&a.source.acquisition, &b.source.acquisition)?
        {
            return Err(
                "generated Q/P draw ordering, eligibility or original acquisition anchor differs"
                    .into(),
            );
        }
        same_contract_source(&a.source.current, &b.source.current, observable_origin)?;
    }
    Ok(())
}
pub(super) fn original(r: &Request) -> Option<&RoleInputs> {
    r.physical_path_inputs
        .as_ref()
        .map(|p| &p.original_physical)
}
pub(super) fn current<'a>(
    r: &'a Request,
    law: &stochastic::Request,
) -> Result<Option<&'a RoleInputs>, String> {
    r.physical_path_inputs
        .as_ref()
        .map(|p| {
            p.current_physical
                .iter()
                .find(|p| p.scenario_id == law.source.source.current.scenario_id)
                .map(|p| &p.inputs)
                .ok_or_else(|| "generated Physical scenario path inputs absent".into())
        })
        .transpose()
}
pub(super) fn source(
    law: &stochastic::Request,
    inputs: Option<&RoleInputs>,
    index: usize,
    original: bool,
) -> Result<owner::Request, String> {
    if let Some(authority) = &law.source.counted_work {
        authority.verify(&law.source)?;
    }
    let mut raw = (*law.source).clone();
    raw.credit.paths[0].path_key = index as u64 + 1;
    raw.credit.paths[0].path_id = format!("generated-{index}");
    raw.credit.draw_stream = portfolio_model_core::credit_path::DrawStream::CounterSplitmix64V1 {
        stream_id: law.evaluation.randomness_domain.id.clone(),
        stream_revision: law.evaluation.randomness_domain.revision.clone(),
        seed: law.evaluation.credit_seed,
    };
    if let Some(inputs) = inputs {
        let path = inputs
            .paths
            .get(index)
            .ok_or("generated Physical path input absent")?;
        if path.path_key != index as u64 + 1 {
            return Err("generated Physical path key/order differs".into());
        }
        raw.performing_draw_tranches = inputs
            .source_recipes
            .get(path.source_recipe_index)
            .ok_or("generated Physical source recipe absent")?
            .performing_draw_tranches
            .clone();
        raw.reviewed_credit_actions = path.reviewed_credit_actions.clone();
    }
    if original {
        raw.source.current = raw.source.original.clone();
        for draw in &mut raw.performing_draw_tranches {
            draw.source.current = draw.source.original.clone();
        }
    }
    refresh_counted_source(&mut raw)?;
    Ok(raw)
}
fn validate_role(law: &stochastic::Request, inputs: &RoleInputs) -> Result<(), String> {
    if inputs.paths.len() != law.evaluation.paths
        || inputs.source_recipes.is_empty()
        || inputs.source_recipes.len() > 512
    {
        return Err(
            "generated Physical inputs require complete bounded path and recipe coverage".into(),
        );
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut used = std::collections::BTreeSet::new();
    for recipe in &inputs.source_recipes {
        identity(&recipe.identity)?;
        if !ids.insert((
            recipe.identity.id.as_str(),
            recipe.identity.revision.as_str(),
        )) {
            return Err("duplicate generated Physical source recipe identity".into());
        }
    }
    ids.clear();
    for (index, path) in inputs.paths.iter().enumerate() {
        check()?;
        identity(&path.review)?;
        if path.path_key != index as u64 + 1
            || !ids.insert((path.review.id.as_str(), path.review.revision.as_str()))
        {
            return Err(
                "generated Physical path identities require exact ordered unique coverage".into(),
            );
        }
        used.insert(path.source_recipe_index);
        let raw = source(law, Some(inputs), index, false)?;
        same_draw_catalog(&law.source, &raw)?;
        if let Some(actions) = &raw.reviewed_credit_actions {
            if actions
                .allowance_reviews
                .iter()
                .any(|a| a.path_key != path.path_key)
                || actions
                    .chargeoffs
                    .iter()
                    .any(|a| a.path_key != path.path_key)
                || actions
                    .legal_waivers
                    .iter()
                    .any(|a| a.path_key != path.path_key)
            {
                return Err(
                    "generated reviewed actions must name their exact supplied path key".into(),
                );
            }
        }
    }
    if used.len() != inputs.source_recipes.len() {
        return Err("generated Physical source catalog contains unused recipes".into());
    }
    Ok(())
}
pub(super) fn validate(r: &Request) -> Result<(), String> {
    if r.original_physical.source.reviewed_credit_actions.is_some()
        || r.current_physical
            .iter()
            .any(|p| p.source.reviewed_credit_actions.is_some())
    {
        return Err("generated reviewed credit actions require explicit per-path inputs".into());
    }
    let Some(inputs) = &r.physical_path_inputs else {
        return Ok(());
    };
    if inputs.schema != SCHEMA || inputs.current_physical.len() != r.current_physical.len() {
        return Err("explicit versioned complete generated Physical path inputs required".into());
    }
    identity(&inputs.identity)?;
    validate_role(&r.original_physical, &inputs.original_physical)?;
    let mut scenes = std::collections::BTreeSet::new();
    for (scene, law) in inputs.current_physical.iter().zip(&r.current_physical) {
        if scene.scenario_id != law.source.source.current.scenario_id
            || !scenes.insert(&scene.scenario_id)
        {
            return Err("generated Physical scenario source ordering differs".into());
        }
        validate_role(law, &scene.inputs)?;
        for index in 0..law.evaluation.paths {
            let original = source(
                &r.original_physical,
                Some(&inputs.original_physical),
                index,
                true,
            )?;
            let current = source(law, Some(&scene.inputs), index, false)?;
            if owner::original_draw_bindings(&original)? != owner::original_draw_bindings(&current)?
            {
                return Err(
                    "current Physical per-path original tranche source/anchor changed".into(),
                );
            }
        }
    }
    Ok(())
}
/// Worst simultaneously live full source and stochastic role for this selected
/// explicit catalog. Every path's review/recipe is admitted before generation.
pub(super) fn upper(
    law: &stochastic::Request,
    inputs: Option<&RoleInputs>,
    original: bool,
) -> Result<(usize, usize, usize, usize, usize), String> {
    if inputs.is_none() {
        let (work, bytes) = stochastic::admitted_law(law, CreditMeasure::Physical)?;
        let a = owner::admit(&law.source)?;
        return Ok((
            work,
            bytes,
            a.accounting_groups_upper,
            a.lots_per_path_upper,
            raw_recipe_upper(&law.source)?,
        ));
    }
    let mut result = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut source_work = 0usize;
    let features = stochastic::feature_bound(law)?;
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
    let per_path_overhead = mul(
        256,
        add(
            mul(law.time_ordinals.len(), law.drivers.len() + 2)?,
            mul(
                law.policy.exercise_dates.len(),
                add(add(features, cash)?, 192)?,
            )?,
        )?,
    )?;
    for index in 0..law.evaluation.paths {
        check()?;
        let raw = source(law, inputs, index, original)?;
        let mut expanded = law.clone();
        *expanded.source = raw;
        let (_, bytes) = stochastic::admitted_law(&expanded, CreditMeasure::Physical)?;
        let a = stochastic::admitted_source(&expanded, CreditMeasure::Physical)?;
        // P generates evaluation paths and applies a sealed Q forecast; it
        // neither simulates Q training paths nor solves another regression.
        // Two genuine owners cover current prefix+stopped book. The caller's
        // original-role multiplier also covers proof and reviewed book replay.
        source_work = add(
            source_work,
            add(mul(a.joined_work_upper, 2)?, per_path_overhead)?,
        )?;
        result.1 = result.1.max(bytes);
        result.2 = result.2.max(a.accounting_groups_upper);
        result.3 = result.3.max(a.lots_per_path_upper);
        result.4 = result.4.max(raw_recipe_upper(&expanded.source)?);
    }
    result.0 = source_work;
    Ok(result)
}
pub(super) fn generated_path(
    law: &stochastic::Request,
    inputs: Option<&RoleInputs>,
    tape: &stochastic::Tape,
    index: usize,
    original: bool,
) -> Result<owner::Request, String> {
    let raw = source(law, inputs, index, original)?;
    stochastic::generated_path_from_source(
        law,
        &law.evaluation,
        tape,
        index,
        CreditMeasure::Physical,
        raw,
    )
}
pub(super) fn retain_original_draws(
    raw: &mut owner::Request,
    original: &owner::Request,
) -> Result<(), String> {
    if raw.performing_draw_tranches.len() != original.performing_draw_tranches.len() {
        return Err("generated original draw catalog mismatch".into());
    }
    for (draw, anchor) in raw
        .performing_draw_tranches
        .iter_mut()
        .zip(&original.performing_draw_tranches)
    {
        draw.source.original = anchor.source.original.clone();
    }
    Ok(())
}

pub(super) fn retained_inputs(r: &Request) -> Result<usize, String> {
    r.physical_path_inputs.as_ref().map_or(Ok(0), |p| {
        mul(strict_json::serialized_size(p, 1 << 20)?, 20)
    })
}
pub(super) fn expanded_compact(
    law: &stochastic::Request,
    upper: &(usize, usize, usize, usize, usize),
) -> Result<usize, String> {
    let template = raw_recipe_upper(&law.source)?;
    add(
        compact_upper(law)?,
        mul(upper.4.saturating_sub(template), law.evaluation.paths)?,
    )
}
