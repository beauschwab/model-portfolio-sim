//! Complete ordinary fixed graph/coefficients/funding construction inventory.
use super::*;
use std::mem::{needs_drop, size_of};
type R<T> = Result<T, String>;
fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b).ok_or("fixed unit storage overflow".into())
}
fn mul(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b).ok_or("fixed unit storage overflow".into())
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
fn scalar<T>(_: &T) -> R<()> {
    if needs_drop::<T>() {
        Err("fixed unit scalar acquired ownership".into())
    } else {
        Ok(())
    }
}
fn strings(v: &Vec<String>) -> R<usize> {
    v.iter()
        .try_fold(mul(v.capacity(), size_of::<String>())?, |n, s| {
            add(n, s.capacity())
        })
}
#[derive(Debug, Serialize)]
pub struct FixedConstructionCapacity {
    pub source_count: usize,
    pub cash_events_upper: usize,
    pub funding_dates_upper: usize,
    pub graph_bytes_upper: usize,
    pub unit_bytes_upper: usize,
    pub total_bytes_upper: usize,
}
pub fn fixed_construction_capacity(r: &Request) -> R<FixedConstructionCapacity> {
    let raw = crate::commercial_credit_compiler::finite_units_raw_layout(r)?;
    if r.units.is_empty() || r.units.len() > 32 || !(2..=121).contains(&r.reporting_ordinals.len())
    {
        return Err("fixed unit construction requires bounded units and reporting dates".into());
    }
    let s = r.units[0].graph.scenarios.len();
    let n = r.units.len();
    let h = r.reporting_ordinals.len() - 1;
    let coefficients = if r
        .units
        .iter()
        .any(|u| u.graph.positions.iter().any(|p| p.fees.is_some()))
    {
        11
    } else {
        10
    };
    let mut graph_bytes_upper = 0;
    let mut cash_events_upper = mul(s, n)?; // unconditional acquisition row per scenario/unit
    let mut source_count = 0;
    for u in &r.units {
        if u.graph.scenarios.len() != s || u.graph.positions.len() != 1 {
            return Err(
                "fixed unit construction requires common scenarios and one position".into(),
            );
        }
        let bound = graph::fixed_construction_capacity(&u.graph)?;
        graph_bytes_upper = add(graph_bytes_upper, bound.total_bytes_upper)?;
        cash_events_upper = add(cash_events_upper, bound.physical_cash_upper)?;
        source_count = add(source_count, bound.source_count)?;
    }
    let funding_dates_upper = add(r.reporting_ordinals.len(), cash_events_upper)?;
    let funding_values = mul(mul(s, n)?, funding_dates_upper)?;
    if cash_events_upper > 250_000 || funding_values > 1_000_000 {
        return Err(
            "fixed complete funding bound exceeds existing output limits before generation".into(),
        );
    }
    let unit_bytes_upper = sum([
        size_of::<Prepared>(),
        raw.retained_with_one_clone_bytes,
        vector::<graph::ResultData>(n)?,
        vector::<UnitIdentity>(n)?,
        mul(n, 4 * 256)?,
        // Result and Funding each own scenarios. Source scenario labels are
        // <=256 bytes. Purchase event ID adds its fixed prefix to the unit ID.
        mul(2, add(vector::<String>(s)?, mul(s, 256)?)?)?,
        vector::<&str>(10)?,
        mul(mul(mul(mul(s, n)?, coefficients)?, h)?, size_of::<f64>())?,
        vector::<CashEvent>(cash_events_upper)?,
        mul(cash_events_upper, 3 * 256 + 9)?,
        mul(cash_events_upper, size_of::<CashEvent>())?,
        vector::<i32>(funding_dates_upper)?,
        mul(funding_values, size_of::<f64>())?,
        mul(mul(s, n)?, size_of::<f64>())?,
        // Dates set, scenario map and validation sets have borrowed/scalar
        // entries. This reserve exceeds their pinned BTree/hash node layouts.
        mul(sum([funding_dates_upper, s, n, 16])?, 4096)?,
    ])?;
    let total_bytes_upper = add(graph_bytes_upper, unit_bytes_upper)?;
    if total_bytes_upper > 128 << 20 {
        return Err("fixed unit construction exceeds128MiB before generation".into());
    }
    Ok(FixedConstructionCapacity {
        source_count,
        cash_events_upper,
        funding_dates_upper,
        graph_bytes_upper,
        unit_bytes_upper,
        total_bytes_upper,
    })
}
impl Prepared {
    pub fn fixed_retained_capacity(&self) -> R<usize> {
        let Self { source } = self;
        source.fixed_retained_capacity()
    }
}
impl ResultData {
    pub fn fixed_retained_capacity(&self) -> R<usize> {
        let Self {
            schema,
            raw_source,
            scenario_ids,
            units,
            shape,
            metrics,
            values,
            fee_income_values,
            funding,
            native_graphs,
            admitted_work_upper,
            scope,
        } = self;
        scalar(schema)?;
        scalar(shape)?;
        scalar(admitted_work_upper)?;
        scalar(scope)?;
        let labels = units.iter().try_fold(0, |n, u| {
            let UnitIdentity {
                unit_id,
                template_id,
                purchase_ordinal,
                purchase_period,
                side,
                accounting_category,
                capital_policy_revision,
                quote_price_per_face_unit,
            } = u;
            scalar(purchase_ordinal)?;
            scalar(purchase_period)?;
            scalar(side)?;
            scalar(quote_price_per_face_unit)?;
            add(
                n,
                sum([
                    unit_id.capacity(),
                    template_id.capacity(),
                    accounting_category.capacity(),
                    capital_policy_revision.capacity(),
                ])?,
            )
        })?;
        let graphs = native_graphs.iter().try_fold(0, |n, g| {
            add(
                n,
                g.fixed_retained_capacity()? - size_of::<graph::ResultData>(),
            )
        })?;
        let Funding {
            scenario_ids: funding_scenarios,
            ordinals,
            shape: funding_shape,
            demand,
            cash_events,
            policy,
        } = funding;
        scalar(funding_shape)?;
        scalar(policy)?;
        let cash_labels = cash_events.iter().try_fold(0, |n, c| {
            let CashEvent {
                scenario_id,
                unit_index,
                path_id,
                source_event_id,
                year,
                ordinal,
                source_weight,
                purchase_cash,
                principal_cash,
                interest_cash,
                fee_cash,
                bank_cash,
            } = c;
            scalar(unit_index)?;
            scalar(year)?;
            scalar(ordinal)?;
            scalar(source_weight)?;
            scalar(purchase_cash)?;
            scalar(principal_cash)?;
            scalar(interest_cash)?;
            scalar(fee_cash)?;
            scalar(bank_cash)?;
            add(
                n,
                sum([
                    scenario_id.capacity(),
                    path_id.capacity(),
                    source_event_id.capacity(),
                ])?,
            )
        })?;
        sum([
            size_of::<Self>(),
            crate::commercial_credit_compiler::finite_units_raw_layout(raw_source)?
                .retained_heap_bytes,
            strings(scenario_ids)?,
            mul(units.capacity(), size_of::<UnitIdentity>())?,
            labels,
            mul(metrics.capacity(), size_of::<&str>())?,
            mul(values.capacity(), size_of::<f64>())?,
            mul(fee_income_values.capacity(), size_of::<f64>())?,
            mul(native_graphs.capacity(), size_of::<graph::ResultData>())?,
            graphs,
            strings(funding_scenarios)?,
            mul(ordinals.capacity(), size_of::<i32>())?,
            mul(demand.capacity(), size_of::<f64>())?,
            mul(cash_events.capacity(), size_of::<CashEvent>())?,
            cash_labels,
        ])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_fee_owners_and_income_coefficients_fit_fixed_inventory() {
        let raw: Value = serde_json::from_slice(include_bytes!(
            "../../../../../examples/models/commercial-candidate-holding-journal-1.json"
        ))
        .unwrap();
        for side in ["completed_candidate", "revised_candidate"] {
            let r: Request = serde_json::from_value(raw[side]["commercial"].clone()).unwrap();
            let cap = fixed_construction_capacity(&r).unwrap();
            let prepared = prepare(&r).unwrap();
            assert!(prepared.fixed_retained_capacity().unwrap() <= cap.total_bytes_upper);
            assert!(!prepared.source().fee_income_values.is_empty());
            assert!(prepared.source().funding.cash_events.len() <= cap.cash_events_upper);
            assert!(prepared.source().funding.ordinals.len() <= cap.funding_dates_upper);
            let mut count = 0;
            for (unit, graph) in r.units.iter().zip(&prepared.source().native_graphs) {
                assert!(
                    graph.fixed_retained_capacity().unwrap()
                        <= super::graph::fixed_construction_capacity(&unit.graph)
                            .unwrap()
                            .total_bytes_upper
                );
                for source in &graph.native_sources {
                    let fee = source.fee_projection.as_ref().unwrap();
                    let bound =
                        crate::products::commercial::fee_source::fixed_capacity::construction(
                            &fee.raw_fee_history,
                            &fee.allocations,
                        )
                        .unwrap();
                    assert!(fee.fixed_retained_capacity().unwrap() <= bound);
                    count += 1;
                }
            }
            assert!(count > 0);
        }
    }
    #[test]
    fn complete_cut_graph_and_unit_owners_fit_prospective_inventory() {
        let raw: Value = serde_json::from_slice(include_bytes!(
            "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
        ))
        .unwrap();
        let mut count = 0;
        for side in ["completed_candidate", "revised_candidate"] {
            let r: Request = serde_json::from_value(raw[side]["commercial"].clone()).unwrap();
            let cap = fixed_construction_capacity(&r).unwrap();
            count += cap.source_count;
            let prepared = prepare(&r).unwrap();
            assert!(prepared.fixed_retained_capacity().unwrap() <= cap.total_bytes_upper);
            assert!(prepared.source().funding.cash_events.len() <= cap.cash_events_upper);
            assert!(prepared.source().funding.ordinals.len() <= cap.funding_dates_upper);
            for (u, g) in r.units.iter().zip(&prepared.source().native_graphs) {
                assert!(
                    g.fixed_retained_capacity().unwrap()
                        <= graph::fixed_construction_capacity(&u.graph)
                            .unwrap()
                            .total_bytes_upper
                );
            }
        }
        assert_eq!(count, 27);
    }
}
