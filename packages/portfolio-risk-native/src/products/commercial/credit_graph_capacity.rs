//! Complete fixed graph/unit input ownership, including all repeated history
//! requests and spare vector capacity. No economic source generation occurs.
use super::{absent, add, inline, mul, Heap, Layout, RawLayout, R};
use crate::market::analysis::curves::{CurveRole, SuppliedCurve};
use crate::{advanced_graph, aged_units, commercial_graph as graph, commercial_units as units};
use std::mem::size_of;

macro_rules! fields {
    ($ty:path, [$($owned:ident),*], [$($scalar:ident),*]) => {
        impl Layout for $ty {
            fn heap(&self) -> R<Heap> {
                let Self { $($owned,)* $($scalar,)* } = self;
                $(inline($scalar)?;)*
                let result = Heap::default();
                $(let result = result.add($owned.heap()?)?;)*
                Ok(result)
            }
        }
    };
}
fields!(
    advanced_graph::GraphIdentity,
    [
        model_revision,
        input_revision,
        paired_history_source,
        paired_history_revision,
        randomness_domain
    ],
    []
);
fields!(
    advanced_graph::Scenario,
    [scenario_id, run_id, collateral_curve],
    [severity]
);
fields!(
    SuppliedCurve,
    [
        curve_id,
        source_id,
        source_revision,
        currency,
        role,
        times_years,
        discount_factors
    ],
    [right_tail]
);
impl Layout for CurveRole {
    fn heap(&self) -> R<Heap> {
        match self {
            Self::Collateral { collateral_index } => collateral_index.heap(),
            Self::Index { index_id } => index_id.heap(),
        }
    }
}
impl Layout for advanced_graph::OasTarget {
    fn heap(&self) -> R<Heap> {
        match self {
            Self::Locked {
                base_oas,
                source_id,
                source_revision,
            } => {
                inline(base_oas)?;
                source_id.heap()?.add(source_revision.heap()?)
            }
            Self::CalibratePositiveCash {
                target_cash_price,
                source_id,
                source_revision,
            } => {
                inline(target_cash_price)?;
                source_id.heap()?.add(source_revision.heap()?)
            }
        }
    }
}
fields!(graph::History, [path_id, source], [weight]);
fields!(
    graph::Histories,
    [scenario_id, physical, pricing, pricing_down, pricing_up],
    []
);
impl Layout for graph::Position {
    fn heap(&self) -> R<Heap> {
        let Self {
            position_id,
            credit_identity,
            oas_target,
            original_signed_cash_quote,
            original_pricing,
            original_physical,
            histories,
            exercise,
            generated_policy,
            fees,
        } = self;
        inline(credit_identity)?;
        inline(original_signed_cash_quote)?;
        absent(exercise)?;
        absent(generated_policy)?;
        absent(fees)?;
        position_id
            .heap()?
            .add(oas_target.heap()?)?
            .add(original_pricing.heap()?)?
            .add(original_physical.heap()?)?
            .add(histories.heap()?)
    }
}
fields!(
    graph::Request,
    [
        schema,
        identity,
        currency,
        reporting_ordinals,
        base_scenario_id,
        original_scenario,
        scenarios,
        positions
    ],
    [as_of_ordinal, bump_bp]
);
fields!(
    aged_units::AgedIdentity,
    [model_revision, grid_revision, crn_id, input_revision],
    []
);
fields!(
    aged_units::Weights,
    [],
    [hqla_l2a, outflow30, asf, rsf, rwa]
);
fields!(
    units::Unit,
    [
        unit_id,
        template_id,
        accounting_category,
        capital_policy_revision,
        weights,
        graph
    ],
    [unit_face]
);
fields!(
    units::Request,
    [
        schema,
        identity,
        currency,
        reporting_ordinals,
        base_scenario_id,
        units
    ],
    []
);

fn layout<T: Layout>(r: &T) -> R<RawLayout> {
    let heap = r.heap()?;
    Ok(RawLayout {
        inline_bytes: size_of::<T>(),
        retained_heap_bytes: heap.retained,
        cloned_heap_bytes: heap.cloned,
        retained_with_one_clone_bytes: add(
            mul(2, size_of::<T>())?,
            add(heap.retained, heap.cloned)?,
        )?,
    })
}
pub fn finite_graph_raw_layout(r: &graph::Request) -> R<RawLayout> {
    layout(r)
}
pub fn finite_units_raw_layout(r: &units::Request) -> R<RawLayout> {
    layout(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_candidate_graph_wrappers_count_complete_history_clones() {
        let r: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
        ))
        .unwrap();
        for side in ["completed_candidate", "revised_candidate"] {
            let request: units::Request =
                serde_json::from_value(r[side]["commercial"].clone()).unwrap();
            let raw = finite_units_raw_layout(&request).unwrap();
            let cloned = finite_units_raw_layout(&request.clone()).unwrap();
            assert_eq!(raw.cloned_heap_bytes, cloned.retained_heap_bytes);
            for unit in &request.units {
                let raw = finite_graph_raw_layout(&unit.graph).unwrap();
                let cloned = finite_graph_raw_layout(&unit.graph.clone()).unwrap();
                assert_eq!(raw.cloned_heap_bytes, cloned.retained_heap_bytes);
            }
        }
    }
}
