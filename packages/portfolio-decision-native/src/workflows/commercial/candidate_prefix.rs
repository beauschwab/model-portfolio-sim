//! Complete borrowed raw-prefix comparisons. Views are private serializer inputs,
//! never prepared financial owners or a caller checkpoint. Admission is unchanged.
use super::candidates;
use portfolio_compute_control::checkpoint;
use portfolio_model_core::strict_json;
use portfolio_risk_native::{
    commercial_continuation as continuation,
    commercial_fee_source::{self as fees, adoption as fee},
    commercial_graph as graph, commercial_units as units,
};
use serde::{
    ser::{SerializeMap, SerializeSeq, SerializeStruct},
    Serialize,
};
use serde_json::Value;

type R<T> = Result<T, String>;
fn same<A: Serialize, B: Serialize>(a: &A, b: &B) -> R<()> {
    checkpoint()?;
    if strict_json::encode(a, 32 << 20)? != strict_json::encode(b, 32 << 20)? {
        return Err(
            "candidate holding immutable original anchor or observed prefix changed".into(),
        );
    }
    Ok(())
}
fn same_cut<A: Serialize, B: Serialize>(a: &A, b: &B) -> R<()> {
    checkpoint()?;
    if strict_json::encode(a, 2 << 20)? != strict_json::encode(b, 2 << 20)? {
        return Err(
            "native cut immutable owner/original anchor or observed raw prefix differs".into(),
        );
    }
    Ok(())
}
macro_rules! fields {
    ($out:ident, $($name:ident),+ $(,)?) => { $( $out.serialize_field(stringify!($name), $name)?; )+ };
}

struct CommercialView<'a> {
    old: &'a units::Request,
    current: &'a units::Request,
    calendar: bool,
}
struct UnitView<'a> {
    old: &'a units::Unit,
    current: &'a units::Unit,
    calendar: bool,
}
struct GraphView<'a> {
    old: &'a graph::Request,
    current: &'a graph::Request,
    calendar: bool,
}
struct PositionView<'a> {
    old: &'a graph::Position,
    current: &'a graph::Position,
    calendar: bool,
}
struct FeeView<'a> {
    old: &'a fee::Selection,
    current: &'a fee::Selection,
}
macro_rules! sequence {
    ($name:ident, $item:ty, $view:ident) => {
        struct $name<'a> {
            old: &'a [$item],
            current: &'a [$item],
            calendar: bool,
        }
        impl Serialize for $name<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                if self.old.len() != self.current.len() {
                    return Err(serde::ser::Error::custom("borrowed prefix shape changed"));
                }
                let mut out = serializer.serialize_seq(Some(self.old.len()))?;
                for (old, current) in self.old.iter().zip(self.current) {
                    out.serialize_element(&$view {
                        old,
                        current,
                        calendar: self.calendar,
                    })?;
                }
                out.end()
            }
        }
    };
}
sequence!(UnitsView, units::Unit, UnitView);
sequence!(PositionsView, graph::Position, PositionView);

impl Serialize for CommercialView<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let units::Request {
            schema,
            identity,
            currency,
            reporting_ordinals,
            base_scenario_id,
            units,
        } = self.current;
        let mut out = serializer.serialize_struct("Request", 6)?;
        fields!(out, schema, identity, currency);
        out.serialize_field(
            "reporting_ordinals",
            if self.calendar {
                &self.old.reporting_ordinals
            } else {
                reporting_ordinals
            },
        )?;
        fields!(out, base_scenario_id);
        let retained = units
            .get(..self.old.units.len())
            .ok_or_else(|| serde::ser::Error::custom("borrowed unit coverage changed"))?;
        out.serialize_field(
            "units",
            &UnitsView {
                old: &self.old.units,
                current: retained,
                calendar: self.calendar,
            },
        )?;
        out.end()
    }
}
impl Serialize for UnitView<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let units::Unit {
            unit_id,
            template_id,
            unit_face,
            accounting_category,
            capital_policy_revision,
            weights,
            graph,
        } = self.current;
        let mut out = serializer.serialize_struct("Unit", 7)?;
        fields!(
            out,
            unit_id,
            template_id,
            unit_face,
            accounting_category,
            capital_policy_revision,
            weights
        );
        out.serialize_field(
            "graph",
            &GraphView {
                old: &self.old.graph,
                current: graph,
                calendar: self.calendar,
            },
        )?;
        out.end()
    }
}
impl Serialize for GraphView<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let graph::Request {
            schema,
            identity,
            as_of_ordinal,
            currency,
            reporting_ordinals,
            base_scenario_id,
            bump_bp,
            original_scenario,
            scenarios,
            positions,
        } = self.current;
        let mut out = serializer.serialize_struct("Request", 10)?;
        fields!(out, schema, identity, as_of_ordinal, currency);
        out.serialize_field(
            "reporting_ordinals",
            if self.calendar {
                &self.old.reporting_ordinals
            } else {
                reporting_ordinals
            },
        )?;
        fields!(out, base_scenario_id, bump_bp, original_scenario, scenarios);
        out.serialize_field(
            "positions",
            &PositionsView {
                old: &self.old.positions,
                current: positions,
                calendar: self.calendar,
            },
        )?;
        out.end()
    }
}
impl Serialize for PositionView<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let graph::Position {
            position_id,
            credit_identity,
            oas_target,
            original_signed_cash_quote,
            original_pricing,
            original_physical,
            histories: _,
            exercise,
            generated_policy,
            fees,
        } = self.current;
        // Calendar folding is handled by GraphView; a position has no grid.
        let _calendar_already_folded = self.calendar;
        let mut out = serializer.serialize_struct("Position", 11)?;
        fields!(
            out,
            position_id,
            credit_identity,
            oas_target,
            original_signed_cash_quote,
            original_pricing
        );
        if !original_physical.is_empty() {
            fields!(out, original_physical);
        }
        out.serialize_field("histories", &self.old.histories)?;
        if exercise.is_some() {
            fields!(out, exercise);
        }
        if generated_policy.is_some() {
            fields!(out, generated_policy);
        }
        if let Some(current) = fees {
            let old = self
                .old
                .fees
                .as_ref()
                .ok_or_else(|| serde::ser::Error::custom("borrowed fee ownership changed"))?;
            out.serialize_field("fees", &FeeView { old, current })?;
        }
        out.end()
    }
}
impl Serialize for FeeView<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let fee::Selection {
            schema,
            identity,
            discount_spread,
            original_signed_fee_quote,
            total_purchase_cost,
            allocations,
            original_pricing,
            original_physical,
            histories: _,
        } = self.current;
        let mut out = serializer.serialize_struct("Selection", 9)?;
        fields!(
            out,
            schema,
            identity,
            discount_spread,
            original_signed_fee_quote,
            total_purchase_cost,
            allocations,
            original_pricing
        );
        if !original_physical.is_empty() {
            fields!(out, original_physical);
        }
        out.serialize_field("histories", &self.old.histories)?;
        out.end()
    }
}
struct FeePolicyView<'a> {
    old: &'a fees::Policy,
    current: &'a fees::Policy,
}
impl Serialize for FeePolicyView<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let fees::Policy {
            transfer,
            schema,
            policy,
            complete_end_ordinal,
            fees,
            events: _,
            default,
            prepayment,
            exercise,
            maturity,
            coincident_order,
        } = self.current;
        let mut out = serializer.serialize_struct("Policy", 11)?;
        if transfer.is_some() {
            fields!(out, transfer);
        }
        fields!(out, schema, policy, complete_end_ordinal, fees);
        out.serialize_field("events", &self.old.events)?;
        fields!(
            out,
            default,
            prepayment,
            exercise,
            maturity,
            coincident_order
        );
        out.end()
    }
}

struct SpecificationView<'a> {
    old: &'a Value,
    current: &'a Value,
    retained: Option<usize>,
}
impl Serialize for SpecificationView<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Some(count) = self.retained else {
            return self.current.serialize(serializer);
        };
        let map = self
            .current
            .as_object()
            .ok_or_else(|| serde::ser::Error::custom("native cut placeholder array absent"))?;
        let extra = !map.contains_key("horizon_days") && self.old.get("horizon_days").is_some();
        let mut out = serializer.serialize_map(Some(map.len() + usize::from(extra)))?;
        let mut horizon_pending = extra;
        for (key, value) in map {
            if horizon_pending && key.as_str() > "horizon_days" {
                out.serialize_entry("horizon_days", &self.old["horizon_days"])?;
                horizon_pending = false;
            }
            match key.as_str() {
                "positions" => {
                    let values =
                        value
                            .as_array()
                            .and_then(|v| v.get(..count))
                            .ok_or_else(|| {
                                serde::ser::Error::custom(
                                    "native cut surviving placeholder coverage differs",
                                )
                            })?;
                    out.serialize_entry(key, values)?;
                }
                "horizon_days" if self.old.get(key).is_some() => {
                    out.serialize_entry(key, &self.old[key])?
                }
                _ => out.serialize_entry(key, value)?,
            }
        }
        // The historical fold inserts a supplied old horizon even when absent
        // from the revised raw map; retain that behavior without cloning it.
        if horizon_pending {
            out.serialize_entry("horizon_days", &self.old["horizon_days"])?;
        }
        out.end()
    }
}
struct MetadataView<'a> {
    old: &'a candidates::Request,
    current: &'a candidates::Request,
    calendar: bool,
}
impl Serialize for MetadataView<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let candidates::Request {
            schema,
            commercial: _,
            allocation,
            as_of_ordinal,
            reporting_end_ordinal,
            owners,
            run_mapping,
            specification,
            verified_opening_identity,
        } = self.current;
        let mut out = serializer.serialize_struct("Request", 8)?;
        fields!(out, schema);
        let n = self.old.commercial.units.len();
        out.serialize_field(
            "allocation",
            if self.calendar {
                &allocation[..n]
            } else {
                allocation.as_slice()
            },
        )?;
        fields!(out, as_of_ordinal);
        out.serialize_field(
            "reporting_end_ordinal",
            if self.calendar {
                &self.old.reporting_end_ordinal
            } else {
                reporting_end_ordinal
            },
        )?;
        out.serialize_field(
            "owners",
            if self.calendar {
                &owners[..n]
            } else {
                owners.as_slice()
            },
        )?;
        fields!(out, run_mapping);
        out.serialize_field(
            "specification",
            &SpecificationView {
                old: &self.old.specification,
                current: specification,
                retained: self.calendar.then_some(n),
            },
        )?;
        fields!(out, verified_opening_identity);
        out.end()
    }
}

fn loan_prefix(a: &graph::History, b: &graph::History, cut: i32) -> R<()> {
    same(&(&a.path_id, a.weight), &(&b.path_id, b.weight))?;
    if cut <= a.source.source.current.asof_ordinal {
        return same(&a.source, &b.source);
    }
    continuation::validate_raw_prefix_sources(&a.source, &b.source, cut)?;
    Ok(())
}
fn fee_prefix(a: &fee::History, b: &fee::History, cut: i32) -> R<()> {
    same(&a.path_id, &b.path_id)?;
    same(
        &a.policy,
        &FeePolicyView {
            old: &a.policy,
            current: &b.policy,
        },
    )?;
    same(
        &a.policy
            .events
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
        &b.policy
            .events
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
    )?;
    same(
        &a.acquisition_releases
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
        &b.acquisition_releases
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
    )
}

/// `calendar` implements exactly the existing mixed-cut append/calendar fold.
/// False preserves complete-origin holding coverage. No monetary source is built.
pub(super) fn validate(
    a: &candidates::Request,
    b: &candidates::Request,
    cut: i32,
    calendar: bool,
) -> R<()> {
    let n = a.commercial.units.len();
    if calendar {
        if b.commercial.units.len() < n || b.allocation.len() < n || b.owners.len() < n {
            return Err("native cut cannot remove surviving units/allocation/owners".into());
        }
        same_cut(&a.as_of_ordinal, &b.as_of_ordinal)?;
        same_cut(
            &a.commercial
                .reporting_ordinals
                .iter()
                .filter(|d| **d <= cut)
                .collect::<Vec<_>>(),
            &b.commercial
                .reporting_ordinals
                .iter()
                .filter(|d| **d <= cut)
                .collect::<Vec<_>>(),
        )?;
        for (old, new) in a.commercial.units.iter().zip(&b.commercial.units) {
            same_cut(
                &old.graph
                    .reporting_ordinals
                    .iter()
                    .filter(|d| **d <= cut)
                    .collect::<Vec<_>>(),
                &new.graph
                    .reporting_ordinals
                    .iter()
                    .filter(|d| **d <= cut)
                    .collect::<Vec<_>>(),
            )?;
        }
        let positions = b
            .specification
            .get("positions")
            .and_then(Value::as_array)
            .ok_or("native cut placeholder array absent")?;
        if positions.len() < n {
            return Err("native cut surviving placeholder coverage differs".into());
        }
    } else if n != b.commercial.units.len() {
        return Err("candidate holding unit coverage changed".into());
    }
    for (old, new) in a.commercial.units.iter().zip(&b.commercial.units) {
        if old.graph.positions.len() != new.graph.positions.len() {
            return Err("candidate holding position coverage changed".into());
        }
        for (p, q) in old.graph.positions.iter().zip(&new.graph.positions) {
            if p.histories.len() != q.histories.len() {
                return Err("candidate holding scenario coverage changed".into());
            }
            for (x, y) in p.histories.iter().zip(&q.histories) {
                same(&x.scenario_id, &y.scenario_id)?;
                for (xs, ys) in [
                    (&x.physical, &y.physical),
                    (&x.pricing, &y.pricing),
                    (&x.pricing_down, &y.pricing_down),
                    (&x.pricing_up, &y.pricing_up),
                ] {
                    if xs.len() != ys.len() {
                        return Err("candidate holding path coverage changed".into());
                    }
                    for (x, y) in xs.iter().zip(ys) {
                        loan_prefix(x, y, cut)?;
                    }
                }
            }
            match (&p.fees, &q.fees) {
                (None, None) => {}
                (Some(x), Some(y)) => {
                    if x.histories.len() != y.histories.len() {
                        return Err("candidate holding fee scenario coverage changed".into());
                    }
                    for (x, y) in x.histories.iter().zip(&y.histories) {
                        same(&x.scenario_id, &y.scenario_id)?;
                        for (xs, ys) in [
                            (&x.physical, &y.physical),
                            (&x.pricing, &y.pricing),
                            (&x.pricing_down, &y.pricing_down),
                            (&x.pricing_up, &y.pricing_up),
                        ] {
                            if xs.len() != ys.len() {
                                return Err("candidate holding fee path coverage changed".into());
                            }
                            for (x, y) in xs.iter().zip(ys) {
                                fee_prefix(x, y, cut)?;
                            }
                        }
                    }
                }
                _ => return Err("candidate holding fee ownership changed".into()),
            }
        }
    }
    same(
        &a.commercial,
        &CommercialView {
            old: &a.commercial,
            current: &b.commercial,
            calendar,
        },
    )?;
    same(
        &MetadataView {
            old: a,
            current: a,
            calendar: false,
        },
        &MetadataView {
            old: a,
            current: b,
            calendar,
        },
    )?;
    if calendar
        && b.commercial
            .units
            .iter()
            .skip(n)
            .any(|u| u.graph.as_of_ordinal <= cut)
    {
        return Err("native cut appended candidate acquisition must follow cut".into());
    }
    Ok(())
}
