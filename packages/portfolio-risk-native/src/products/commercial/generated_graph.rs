//! Explicit generated-policy tier of the same commercial graph. Native trained
//! Pricing policies and separately supplied Physical laws retain source ownership.
use crate::{
    advanced_graph::{OasTarget, Role, Scenario},
    commercial_generated_policy as generated, commercial_graph as graph,
    commercial_stochastic_exercise as stochastic,
};
use portfolio_model_core::strict_json;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeSet;
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn equal<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    Ok(strict_json::encode(a, 4 << 20)? == strict_json::encode(b, 4 << 20)?)
}
pub fn admit(
    r: &graph::Request,
    p: &graph::Position,
    g: &generated::Request,
) -> Result<generated::Admission, String> {
    if r.positions.len() != 1
        || p.exercise.is_some()
        || !p.histories.is_empty()
        || !p.original_pricing.is_empty()
        || !p.original_physical.is_empty()
    {
        return Err("generated commercial graph requires one exclusive generated policy owner and no supplied-history/finite exercise overlap".into());
    }
    let base_oas = match (&p.oas_target, &g.oas_calibration) {
        (OasTarget::Locked { base_oas, .. }, None) => *base_oas,
        (OasTarget::CalibratePositiveCash { .. }, Some(_)) => g.pricing_training.fixed_base_oas,
        _ => return Err("generated graph requires locked original OAS or explicit original quote calibration controls".into()),
    };
    if base_oas.to_bits() != g.pricing_training.fixed_base_oas.to_bits()
        || g.pricing_training.parallel_bump.to_bits() != 0_f64.to_bits()
        || !equal(
            &r.original_scenario.collateral_curve,
            &g.pricing_training.discount_curve,
        )?
        || g.current_physical.len() != r.scenarios.len()
        || g.current_pricing.len() != r.scenarios.len() * 3
    {
        return Err("generated original OAS/curve or complete graph scenario roles differ".into());
    }
    for (role, law) in std::iter::once((Role::Pricing, &g.pricing_training))
        .chain(std::iter::once((Role::Physical, &g.original_physical)))
        .chain(g.current_physical.iter().map(|law| (Role::Physical, law)))
        .chain(g.current_pricing.iter().map(|law| (Role::Pricing, law)))
    {
        graph::validate_history(
            r,
            p,
            &graph::History {
                path_id: law
                    .source
                    .credit
                    .paths
                    .first()
                    .ok_or("generated template path absent")?
                    .path_id
                    .clone(),
                weight: 1.,
                source: (*law.source).clone(),
            },
            role,
        )?;
    }
    let mut keys = BTreeSet::new();
    for scene in &r.scenarios {
        if g.current_physical
            .iter()
            .filter(|law| law.source.source.current.scenario_id == scene.scenario_id)
            .count()
            != 1
        {
            return Err("generated graph needs one separate Physical law per scenario".into());
        }
        for bump in [0., -r.bump_bp * 1e-4, r.bump_bp * 1e-4] {
            let laws = g
                .current_pricing
                .iter()
                .filter(|law| {
                    law.source.source.current.scenario_id == scene.scenario_id
                        && law.parallel_bump.to_bits() == bump.to_bits()
                })
                .collect::<Vec<_>>();
            if laws.len() != 1
                || !equal(&laws[0].discount_curve, &scene.collateral_curve)?
                || !keys.insert((scene.scenario_id.as_str(), bump.to_bits()))
            {
                return Err(
                    "generated graph needs exact base/down/up Pricing laws and scenario curves"
                        .into(),
                );
            }
        }
    }
    let mut upper = generated::admit(g)?;
    let add = |a: usize, b: usize| a.checked_add(b).ok_or("generated graph capacity overflow");
    let mul = |a: usize, b: usize| a.checked_mul(b).ok_or("generated graph capacity overflow");
    // NativeSource labels/stops and compact projection Maps coexist with the
    // sealed recipes. No second JSON copy of the raw source is retained.
    upper.retained_bytes_upper = add(
        upper.retained_bytes_upper,
        mul(add(upper.physical_paths, upper.pricing_paths)?, 8192)?,
    )?;
    for law in &g.current_physical {
        upper.retained_bytes_upper = add(
            upper.retained_bytes_upper,
            mul(
                generated::raw_recipe_upper(&law.source)?,
                law.evaluation.paths,
            )?,
        )?;
    }
    // Public original proof JSON coexists with the private immutable native proof.
    let proof_raw = strict_json::serialized_size(&*g.original_physical.source, 1 << 20)?;
    let proof_cash = mul(
        g.original_physical.source.source.current.periods.len() + 36,
        1536,
    )?;
    upper.retained_bytes_upper = add(
        upper.retained_bytes_upper,
        mul(
            add(mul(proof_raw, 20)?, proof_cash)?,
            g.original_physical.evaluation.paths,
        )?,
    )?;
    if let Some(policy) = &g.oas_calibration {
        let (work, bytes) = stochastic::calibration::admitted(&g.pricing_training, policy)?;
        let (original_work, _) = stochastic::admit(&g.pricing_training)?;
        // Preparation receives the sealed fitted policy and actual stopped Q
        // sources; it does not generate those samples again.
        upper.work_upper = add(
            upper.work_upper,
            work.checked_sub(original_work)
                .ok_or("calibration work ownership differs")?,
        )?;
        // Calibration finishes before prepared P/Q roles and proofs exist.
        // Only the accepted policy/receipt coexist with graph preparation.
        upper.retained_bytes_upper =
            add(upper.retained_bytes_upper, 1 << 20)?.max(add(bytes, 1 << 20)?);
        if upper.work_upper > 4_000_000_000 {
            return Err(format!(
                "generated graph joined calibration/preparation exceeds4B: work={}",
                upper.work_upper
            ));
        }
    }
    if upper.retained_bytes_upper > 128 << 20 {
        return Err("generated graph compact receipt/native recipe coexistence exceeds128MiB before generation".into());
    }
    Ok(upper)
}
fn source(
    p: &graph::Position,
    scene: &Scenario,
    role: Role,
    s: generated::EconomicSource,
) -> graph::NativeSource {
    let (raw, stops, cash, weight) = s.into_parts();
    graph::NativeSource {
        position_id: p.position_id.clone(),
        scenario_id: scene.scenario_id.clone(),
        run_id: scene.run_id.clone(),
        path_id: raw.credit.paths[0].path_id.clone(),
        path_key: raw.credit.paths[0].path_key,
        weight,
        role,
        projection: json!({"schema":"commercial-generated-source-1","source_binding":"retained_source_request-1","native_stops":stops,"credit_measure":if role==Role::Physical{"physical"}else{"pricing"}}),
        source_request: raw,
        cash,
        physical_projection: None,
        generated_projection: None,
    }
}
fn receipt(
    p: &graph::Position,
    scene: &Scenario,
    role: Role,
    policy: &stochastic::NativeTrainedPolicy,
) -> Result<graph::ExerciseReceipt, String> {
    Ok(graph::ExerciseReceipt {
        position_id: p.position_id.clone(),
        scenario_id: scene.scenario_id.clone(),
        role,
        policy: serde_json::to_value(policy).map_err(|e| e.to_string())?,
    })
}
fn weighted(
    sources: &[graph::NativeSource],
    scene: &Scenario,
    asof: i32,
    mark: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    sources.iter().try_fold(0., |sum, s| {
        graph::money(
            sum + s.weight * graph::price(s, &scene.collateral_curve, asof, mark, oas, bump)?,
        )
    })
}
pub fn analyze(
    r: &graph::Request,
    p: &graph::Position,
    g: &generated::Request,
    work: usize,
) -> Result<graph::ResultData, String> {
    check()?;
    let (prepared, base_oas, calibration) = if let Some(policy) = &g.oas_calibration {
        let solved = stochastic::calibration::solve(
            &g.pricing_training,
            policy,
            p.original_signed_cash_quote,
        )?;
        let mut selected = g.clone();
        selected.oas_calibration = None;
        selected.pricing_training.fixed_base_oas = solved.base_oas;
        selected.original_physical.fixed_base_oas = solved.base_oas;
        for law in selected
            .current_pricing
            .iter_mut()
            .chain(selected.current_physical.iter_mut())
        {
            law.fixed_base_oas = solved.base_oas;
        }
        (
            generated::prepare_with_original_pricing(&selected, solved.original_pricing)?,
            solved.base_oas,
            Some(solved.receipt),
        )
    } else {
        let OasTarget::Locked { base_oas, .. } = p.oas_target else {
            unreachable!("admission owns locked OAS")
        };
        (generated::prepare(g)?, base_oas, None)
    };
    let mut exercise_policies = vec![receipt(
        p,
        &r.original_scenario,
        Role::Pricing,
        &prepared.original_pricing.fitted_policy,
    )?];
    let mut native_sources = prepared
        .original_pricing
        .sources
        .into_iter()
        .map(|s| source(p, &r.original_scenario, Role::Pricing, s))
        .collect::<Vec<_>>();
    let original = weighted(
        &native_sources,
        &r.original_scenario,
        r.as_of_ordinal,
        r.as_of_ordinal,
        base_oas,
        0.,
    )?;
    if (original - p.original_signed_cash_quote).abs()
        > 1e-9 + 1e-12 * p.original_signed_cash_quote.abs()
    {
        return Err("locked original generated OAS does not reconcile separately retained original Q fitted-policy cash quote".into());
    }
    let original_physical_sources = prepared
        .original_physical_sources
        .into_iter()
        .map(|s| source(p, &r.original_scenario, Role::Physical, s))
        .collect();
    let original_proof =
        std::sync::Arc::new(std::sync::Mutex::new(prepared.original_acquisition_proof));
    let original_acquisition_proofs = vec![serde_json::to_value(
        &*original_proof
            .lock()
            .map_err(|_| "generated original proof owner poisoned")?,
    )
    .map_err(|e| e.to_string())?];
    for physical in prepared.current_physical {
        let scene = r
            .scenarios
            .iter()
            .find(|s| s.scenario_id == physical.raw_law.source.source.current.scenario_id)
            .ok_or("generated Physical graph scene absent")?;
        for s in physical.sources {
            let raw = s.raw().clone();
            let cash = s.cash().to_vec();
            let projection = json!({"schema":"commercial-generated-source-1","source_binding":"retained_source_request-1","native_stops":s.stops(),"credit_measure":"physical"});
            native_sources.push(graph::NativeSource {
                position_id: p.position_id.clone(),
                scenario_id: scene.scenario_id.clone(),
                run_id: scene.run_id.clone(),
                path_id: raw.credit.paths[0].path_id.clone(),
                path_key: raw.credit.paths[0].path_key,
                weight: s.weight,
                role: Role::Physical,
                source_request: raw,
                cash,
                projection,
                physical_projection: None,
                generated_projection: Some((Box::new(s), original_proof.clone())),
            });
        }
    }
    for pricing in prepared.current_pricing {
        let scene = r
            .scenarios
            .iter()
            .find(|s| s.scenario_id == pricing.raw_law.source.source.current.scenario_id)
            .ok_or("generated Pricing graph scene absent")?;
        let role = if pricing.raw_law.parallel_bump.to_bits() == 0_f64.to_bits() {
            Role::Pricing
        } else if pricing.raw_law.parallel_bump < 0. {
            Role::PricingDown
        } else {
            Role::PricingUp
        };
        exercise_policies.push(receipt(p, scene, role, &pricing.fitted_policy)?);
        native_sources.extend(
            pricing
                .sources
                .into_iter()
                .map(|s| source(p, scene, role, s)),
        );
    }
    // Original Pricing sources have original-scene metadata and remain distinct
    // even when supplied original/current scenario labels are equal.
    let original_count = g.pricing_training.evaluation.paths;
    let mut metrics = Vec::new();
    for scene in &r.scenarios {
        for day in &r.reporting_ordinals {
            check()?;
            let mut values = [0.; 3];
            for s in native_sources[original_count..]
                .iter()
                .filter(|s| s.scenario_id == scene.scenario_id)
            {
                let (index, bump) = match s.role {
                    Role::Pricing => (0, 0.),
                    Role::PricingDown => (1, -r.bump_bp * 1e-4),
                    Role::PricingUp => (2, r.bump_bp * 1e-4),
                    Role::Physical => continue,
                };
                values[index] = graph::money(
                    values[index]
                        + s.weight
                            * graph::price(
                                s,
                                &scene.collateral_curve,
                                r.as_of_ordinal,
                                *day,
                                base_oas,
                                bump,
                            )?,
                )?;
            }
            metrics.push(graph::Metric {
                position_id: p.position_id.clone(),
                scenario_id: scene.scenario_id.clone(),
                ordinal: *day,
                pv: values[0],
                dv01: graph::money((values[1] - values[2]) / (2. * r.bump_bp))?,
            });
        }
    }
    let scope = if calibration.is_some() {
        "explicit retained-Q generated original quote calibration with per-trial continuation refit and final native stopped cash replay; selected original OAS retained in paired current Pricing risk and separate Physical acquisition proofs; supplied laws do not establish empirical accuracy, general convergence or mixed scale acceptance"
    } else {
        "explicit bounded generated policy replay tier: native Q training and independent Q evaluation, paired Q CRN at locked original OAS, separately supplied P laws and sealed stopped original P acquisition proof; compact source receipts retain genuine native current P accounting owners; no original OAS calibration, observed empirical evidence or complete mixed scale acceptance"
    };
    let result = graph::ResultData {
        schema: graph::SCHEMA,
        raw_source: r.clone(),
        original_oas: vec![graph::Oas {
            position_id: p.position_id.clone(),
            base_oas,
            original_signed_cash_quote: p.original_signed_cash_quote,
        }],
        native_sources,
        metrics,
        admitted_work_upper: work,
        shared_draws_retained: true,
        physical_accounting_source_retained: false,
        generated_physical_recipes_retained: Some(true),
        generated_oas_calibration: calibration,
        empirical_validation: false,
        scope,
        exercise_policies,
        original_physical_sources,
        original_acquisition_proofs,
    };
    strict_json::serialized_size(&result, 32 << 20)?;
    check()?;
    Ok(result)
}
