//! Fixed finite graph allocations. All original and current sources count,
//! including zero weight and pricing bumps. No monetary pruning is permitted.
use super::*;
use std::mem::size_of;
type R<T> = Result<T, String>;
fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b)
        .ok_or("fixed graph storage overflow".into())
}
fn mul(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b)
        .ok_or("fixed graph storage overflow".into())
}
fn sum(v: impl IntoIterator<Item = usize>) -> R<usize> {
    v.into_iter().try_fold(0, add)
}
fn vector<T>(n: usize) -> R<usize> {
    if n == 0 {
        Ok(0)
    } else {
        mul(add(mul(n, 3)?, 8)?, size_of::<T>())
    }
}
fn empty<T>(v: &Vec<T>) -> R<usize> {
    if !v.is_empty() {
        return Err("fixed graph has an uninventoried owner".into());
    }
    mul(v.capacity(), size_of::<T>())
}
fn absent<T>(v: &Option<T>) -> R<()> {
    if v.is_some() {
        Err("fixed graph has an uninventoried owner".into())
    } else {
        Ok(())
    }
}
fn scalar<T>(_: &T) -> R<()> {
    if std::mem::needs_drop::<T>() {
        Err("fixed graph scalar acquired ownership".into())
    } else {
        Ok(())
    }
}
#[derive(Debug, Serialize)]
pub struct FixedConstructionCapacity {
    pub source_count: usize,
    pub physical_source_count: usize,
    pub physical_cash_upper: usize,
    pub source_bytes_upper: usize,
    pub graph_bytes_upper: usize,
    pub total_bytes_upper: usize,
}
pub fn fixed_construction_capacity(r: &Request) -> R<FixedConstructionCapacity> {
    let raw = owner::finite_graph_raw_layout(r)?;
    let mut source_count = 0;
    let mut physical_source_count = 0;
    let mut physical_cash_upper = 0;
    let mut source_bytes_upper = 0;
    let mut wrapper_bytes = 0;
    for p in &r.positions {
        if p.fees
            .as_ref()
            .is_some_and(|f| !f.original_physical.is_empty())
        {
            return Err(
                "fixed fee graph requires separate original Physical acquisition inventory".into(),
            );
        }
        if !p.original_physical.is_empty() {
            return Err(
                "fixed graph requires separate original Physical acquisition inventory".into(),
            );
        }
        if p.histories
            .iter()
            .any(|h| !r.scenarios.iter().any(|s| s.scenario_id == h.scenario_id))
        {
            return Err("fixed graph history scenario absent".into());
        }
        wrapper_bytes = add(
            wrapper_bytes,
            sum([
                vector::<NativeSource>(p.original_pricing.len())?,
                vector::<Rc<owner::EconomicResult>>(p.original_pricing.len())?,
                vector::<crate::products::commercial::exercise::PricingView<'_>>(
                    p.original_pricing.len(),
                )?,
            ])?,
        )?;
        let histories = p
            .original_pricing
            .iter()
            .map(|h| (h, Role::Pricing, &r.original_scenario, true))
            .chain(
                p.histories
                    .iter()
                    .flat_map(|set| {
                        let scene = r
                            .scenarios
                            .iter()
                            .find(|s| s.scenario_id == set.scenario_id);
                        roles(set)
                            .into_iter()
                            .flat_map(move |(role, hs)| hs.iter().map(move |h| (h, role, scene)))
                    })
                    .map(|(h, role, scene)| {
                        (h, role, scene.unwrap_or(&r.original_scenario), false)
                    }),
            );
        for (h, role, scene, original) in histories {
            check()?;
            source_count = add(source_count, 1)?;
            let physical = role == Role::Physical;
            let bound = owner::fixed_construction_capacity(
                &h.source,
                if physical {
                    CreditMeasure::Physical
                } else {
                    CreditMeasure::Pricing
                },
            )?;
            source_bytes_upper = add(source_bytes_upper, bound.total_bytes_upper)?;
            if let Some(fees) = &p.fees {
                let history = fees.history(&scene.scenario_id, role, &h.path_id, original)?;
                let fee_bound =
                    crate::products::commercial::fee_source::fixed_capacity::construction(
                        history,
                        &fees.allocations,
                    )?;
                source_bytes_upper = add(source_bytes_upper, fee_bound)?;
                if physical {
                    physical_cash_upper = add(
                        physical_cash_upper,
                        crate::products::commercial::fee_source::fixed_capacity::events(
                            history,
                            fees.allocations.len(),
                        )?,
                    )?;
                }
            }
            // One possible cash row for every source boundary/jump and all 32
            // complete default recoveries, regardless of value or participation.
            let cash = mul(
                h.source.credit.paths.len(),
                sum([
                    h.source.source.current.periods.len(),
                    h.source.credit.max_jumps_per_path,
                    32,
                ])?,
            )?;
            if physical {
                physical_source_count = add(physical_source_count, 1)?;
                physical_cash_upper = add(physical_cash_upper, cash)?;
            }
            let source_raw = owner::finite_raw_layout(&h.source)?;
            wrapper_bytes = add(
                wrapper_bytes,
                sum([
                    source_raw.cloned_heap_bytes,
                    p.position_id.len(),
                    scene.scenario_id.len(),
                    scene.run_id.len(),
                    h.path_id.len(),
                    // Projection's owner inline storage is already in its source
                    // bound. Rc contributes two counters; the original bundle
                    // additionally retains one Rc handle per source.
                    2 * size_of::<usize>(),
                    size_of::<Rc<owner::EconomicResult>>(),
                    size_of::<crate::products::commercial::exercise::PricingView<'_>>(),
                    vector::<EconomicCash>(cash)?,
                    mul(cash, 2 * 256)?,
                    mul(cash, size_of::<EconomicCash>())?,
                ])?,
            )?;
        }
    }
    let metric_count = mul(
        mul(r.positions.len(), r.scenarios.len())?,
        r.reporting_ordinals.len(),
    )?;
    let graph_bytes_upper = sum([
        size_of::<ResultData>(),
        raw.retained_with_one_clone_bytes,
        wrapper_bytes,
        vector::<NativeSource>(source_count)?,
        vector::<Oas>(r.positions.len())?,
        mul(r.positions.len(), 256)?,
        vector::<Metric>(metric_count)?,
        mul(metric_count, 512)?,
        // Graph admission sets/maps and identity checks use borrowed keys.
        // <= one pinned BTree/hash node per source, position or scenario.
        mul(
            sum([source_count, r.positions.len(), r.scenarios.len(), 1])?,
            4096,
        )?,
    ])?;
    let total_bytes_upper = add(source_bytes_upper, graph_bytes_upper)?;
    if total_bytes_upper > MAX_BYTES {
        return Err("fixed graph construction exceeds128MiB before generation".into());
    }
    Ok(FixedConstructionCapacity {
        source_count,
        physical_source_count,
        physical_cash_upper,
        source_bytes_upper,
        graph_bytes_upper,
        total_bytes_upper,
    })
}

impl NativeSource {
    pub fn fixed_retained_capacity(&self) -> R<usize> {
        let Self {
            position_id,
            scenario_id,
            run_id,
            path_id,
            path_key,
            weight,
            role,
            source_request,
            cash,
            fee_projection,
            generated_fee_projection,
            projection,
            generated_projection,
        } = self;
        scalar(path_key)?;
        scalar(weight)?;
        scalar(role)?;
        let fee_bytes = fee_projection
            .as_ref()
            .map_or(Ok(0), |f| f.fixed_retained_capacity())?;
        absent(generated_fee_projection)?;
        absent(generated_projection)?;
        let projection = match &projection.0 {
            ProjectionOwner::Physical(v) => v.fixed_retained_capacity()?,
            ProjectionOwner::Pricing(v) => {
                add(v.fixed_retained_capacity()?, 2 * size_of::<usize>())?
            }
            ProjectionOwner::Generated(_) => {
                return Err("fixed graph rejects generated projection".into())
            }
        };
        let cash_labels = cash.iter().try_fold(0, |total, c| {
            let EconomicCash {
                event_id,
                kind,
                year,
                principal,
                interest,
                fees,
            } = c;
            scalar(year)?;
            scalar(principal)?;
            scalar(interest)?;
            scalar(fees)?;
            add(total, add(event_id.capacity(), kind.capacity())?)
        })?;
        sum([
            size_of::<Self>(),
            position_id.capacity(),
            scenario_id.capacity(),
            run_id.capacity(),
            path_id.capacity(),
            owner::finite_raw_layout(source_request)?.retained_heap_bytes,
            mul(cash.capacity(), size_of::<EconomicCash>())?,
            cash_labels,
            projection,
            fee_bytes,
        ])
    }
}
impl ResultData {
    pub fn fixed_retained_capacity(&self) -> R<usize> {
        let Self {
            schema,
            raw_source,
            original_oas,
            native_sources,
            metrics,
            admitted_work_upper,
            shared_draws_retained,
            physical_accounting_source_retained,
            generated_physical_recipes_retained,
            generated_oas_calibration,
            empirical_validation,
            scope,
            exercise_policies,
            original_physical_sources,
            original_acquisition_proofs,
        } = self;
        scalar(schema)?;
        scalar(admitted_work_upper)?;
        scalar(shared_draws_retained)?;
        scalar(physical_accounting_source_retained)?;
        scalar(empirical_validation)?;
        scalar(scope)?;
        absent(generated_physical_recipes_retained)?;
        absent(generated_oas_calibration)?;
        let sources = native_sources.iter().try_fold(0, |n, s| {
            add(n, s.fixed_retained_capacity()? - size_of::<NativeSource>())
        })?;
        let anchors = original_oas.iter().try_fold(0, |n, a| {
            let Oas {
                position_id,
                base_oas,
                original_signed_cash_quote,
            } = a;
            scalar(base_oas)?;
            scalar(original_signed_cash_quote)?;
            add(n, position_id.capacity())
        })?;
        let labels = metrics.iter().try_fold(0, |n, m| {
            let Metric {
                position_id,
                scenario_id,
                ordinal,
                pv,
                dv01,
            } = m;
            scalar(ordinal)?;
            scalar(pv)?;
            scalar(dv01)?;
            add(n, add(position_id.capacity(), scenario_id.capacity())?)
        })?;
        sum([
            size_of::<Self>(),
            owner::finite_graph_raw_layout(raw_source)?.retained_heap_bytes,
            mul(original_oas.capacity(), size_of::<Oas>())?,
            anchors,
            mul(native_sources.capacity(), size_of::<NativeSource>())?,
            sources,
            mul(metrics.capacity(), size_of::<Metric>())?,
            labels,
            empty(exercise_policies)?,
            empty(original_physical_sources)?,
            empty(original_acquisition_proofs)?,
        ])
    }
}
