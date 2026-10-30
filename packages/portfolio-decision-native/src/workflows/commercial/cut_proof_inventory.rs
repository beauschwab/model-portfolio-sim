//! Refusal-only compact-cut inventory preparation. No public request selector,
//! financial preparation, Sim execution, codec or publication uses this module.
//! Native projection envelopes are whitelisted; deeper source/layout and original
//! Q ownership gates remain explicitly incomplete rather than producer authority.
use portfolio_ledger_native::daily;
use portfolio_risk_native::{commercial_graph as graph, commercial_units as units};
use serde::Serialize;
use serde_json::{Map, Value};
type R<T> = Result<T, String>;

#[derive(Default, Debug, Serialize)]
pub struct ProjectionEnvelopeInventory {
    pub graph_uses: usize,
    pub original_pricing_expected_uses: usize,
    pub current_source_expected_uses: usize,
    pub current_physical_uses: usize,
    pub current_pricing_uses: usize,
    pub projection_cells: usize,
    pub projection_fields: usize,
    pub projection_string_utf8_bytes: usize,
    pub observed_value_array_capacity_slots: usize,
    pub observed_value_string_capacity_bytes: usize,
    pub source_map_allocator_layout_proved: bool,
    pub complete_projection_field_mapping: bool,
    pub original_pricing_proofs_retained: bool,
}
fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b)
        .ok_or("cut proof inventory overflow".into())
}
fn tick(work: usize) -> R<()> {
    portfolio_compute_control::checkpoint()?;
    daily::charge_cut_census_work(work)
}
fn mul(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b)
        .ok_or("cut proof inventory overflow".into())
}
fn at<'a>(map: &'a Map<String, Value>, key: &str) -> R<&'a Value> {
    // BTree/ordered-map lookup can compare the requested key with multiple
    // retained keys. Reserve every possible key comparison before the lookup.
    tick(mul(add(map.len(), 1)?, add(key.len(), 1)?)?)?;
    map.get(key)
        .ok_or_else(|| format!("cut proof missing native field: {key}"))
}

fn object<'a>(
    value: &'a Value,
    required: &[&str],
    optional: &[&str],
    name: &str,
) -> R<&'a Map<String, Value>> {
    tick(1)?;
    let map = value
        .as_object()
        .ok_or_else(|| format!("cut proof {name} object required"))?;
    for key in map.keys() {
        tick(add(
            1,
            mul(add(required.len(), optional.len())?, add(key.len(), 1)?)?,
        )?)?;
        if !required.contains(&key.as_str()) && !optional.contains(&key.as_str()) {
            return Err(format!("cut proof unmapped {name} field: {key}"));
        }
    }
    for key in required {
        at(map, key)?;
    }
    Ok(map)
}
fn schema(map: &Map<String, Value>, value: &str) -> R<()> {
    tick(value.len())?;
    if at(map, "schema")?.as_str() != Some(value) {
        return Err("cut proof unsupported native projection version".into());
    }
    Ok(())
}
fn flag(map: &Map<String, Value>, name: &str, expected: Option<bool>) -> R<()> {
    tick(1)?;
    let actual = at(map, name)?
        .as_bool()
        .ok_or_else(|| format!("cut proof strict bool required: {name}"))?;
    if expected.is_some_and(|v| v != actual) {
        return Err(format!("cut proof authority flag differs: {name}"));
    }
    Ok(())
}
fn envelope(value: &Value, physical: bool) -> R<()> {
    let required = if physical {
        &[
            "schema",
            "raw_request",
            "admission",
            "selected_accounting_owner",
            "original_source",
            "current_source",
            "original_cash_quote",
            "secondary_acquisition",
            "prepared_credit_request",
            "carry",
            "accounting",
            "original_anchor_refitted",
            "prospective_accounting_rate_policy",
            "mixed_ledger_adoption",
            "empirical_adoption",
            "accounting_approval",
        ][..]
    } else {
        &[
            "secondary_acquisition",
            "original_source",
            "current_source",
            "carry",
            "admission",
            "selected_accounting_owner",
            "physical_journal_eligible",
        ][..]
    };
    let map = object(
        value,
        required,
        &["native_stops", "draw_sources"],
        "projection",
    )?;
    if physical {
        schema(map, "commercial-source-credit-accounting-proof-1")?;
    }
    if physical {
        for name in [
            "original_anchor_refitted",
            "mixed_ledger_adoption",
            "empirical_adoption",
            "accounting_approval",
        ] {
            flag(map, name, Some(false))?;
        }
        flag(map, "prospective_accounting_rate_policy", None)?;
    } else {
        flag(map, "physical_journal_eligible", Some(false))?;
    }
    let selected = if physical {
        "reviewed_coupon_book-1"
    } else {
        "economic_carry_only-1"
    };
    tick(selected.len())?;
    if at(map, "selected_accounting_owner")?.as_str() != Some(selected) {
        return Err("cut proof native selected owner version differs".into());
    }
    for role in ["original_source", "current_source"] {
        let factors = object(
            at(map, role)?,
            &["raw", "source", "policy", "admission", "segments"],
            &[],
            "factor envelope",
        )?;
        let source = object(
            at(factors, "source")?,
            &[
                "schema",
                "model_version",
                "contract",
                "assumption",
                "path",
                "schedule",
                "audit",
                "principal_after_reporting",
                "disclosure",
            ],
            &[],
            "factor source",
        )?;
        schema(source, "term-enhancements-1")?;
        tick("term-contracts-1".len())?;
        if at(source, "model_version")?.as_str() != Some("term-contracts-1") {
            return Err("cut proof unsupported term model version".into());
        }
    }
    let carry = object(
        at(map, "carry")?,
        &[
            "schema",
            "raw_request",
            "admission",
            "credit",
            "carry_points",
            "complete_recovery_receipts",
            "native_factor_provider_selected",
            "earned_origin_contributions",
            "complete_coupon_settlement_ownership",
            "original_anchor_refitted",
            "default_interest_stopped",
            "corporate_source_binding",
            "mixed_ledger_adoption",
            "accounting_approval",
        ],
        &[
            "native_source_cash",
            "native_coupon_recoveries",
            "tail_census",
        ],
        "carry envelope",
    )?;
    schema(carry, "credit-path-income-proof-1")?;
    if physical {
        let book = object(
            at(map, "accounting")?,
            &[
                "schema",
                "policy",
                "groups",
                "closing",
                "complete_native_recovery_tail_retained",
                "claims_fully_closed",
                "actual_risk_source_binding",
                "mixed_ledger_adoption",
                "accounting_approval",
            ],
            &[],
            "book envelope",
        )?;
        schema(book, "commercial-coupon-accounting-proof-1")?;
    }
    Ok(())
}
fn contract_subset(
    raw: &portfolio_risk_native::term_enhancements::indexed::ContractPathRequest,
) -> R<()> {
    use portfolio_risk_native::term_enhancements::indexed::{
        ContractPathRequest, ContractPeriod, Coupon, PrincipalPlan,
    };
    let ContractPathRequest {
        schema: _,
        model_version: _,
        contract: _,
        assumption: _,
        path: _,
        position_id: _,
        scenario_id: _,
        currency: _,
        monetary_unit: _,
        direction: _,
        measure: _,
        asof_ordinal: _,
        reporting_end_ordinal: _,
        tail_end_ordinal: _,
        observed_publication_cutoff_unix_seconds: _,
        opening_principal: _,
        principal_plan: _,
        periods: _,
    } = raw;
    match &raw.principal_plan {
        PrincipalPlan::Supplied {
            principal_payments: _,
        }
        | PrincipalPlan::DatedSuppliedPeriodOpening {
            principal_events: _,
        } => {}
        PrincipalPlan::RetainedPayment {
            original_payment: _,
            terminal: _,
        } => return Err("cut proof retained-payment source layout not yet mapped".into()),
    }
    for period in &raw.periods {
        tick(1)?;
        let ContractPeriod {
            period_id: _,
            start_ordinal: _,
            end_ordinal: _,
            payment_ordinal: _,
            day_count: _,
            coupon: _,
            fee: _,
        } = period;
        match &period.coupon {
            Coupon::Fixed { annual_rate: _ } => {}
            Coupon::Indexed {
                index: _,
                calendar: _,
                complete_calendar_mapping: _,
                calculation: _,
                margin_treatment: _,
                margin: _,
                index_bounds: _,
                all_in_bounds: _,
                reset_cutoff_unix_seconds: _,
                lookback_business_days: _,
                observation_shift: _,
                observations: _,
            } => return Err("cut proof indexed coupon source layout not yet mapped".into()),
        }
    }
    Ok(())
}
fn request_subset(raw: &portfolio_risk_native::commercial_credit_compiler::Request) -> R<()> {
    tick(1)?;
    let transfer = raw
        .source
        .transfer
        .as_ref()
        .filter(|t| t.source_work_policy.is_some())
        .ok_or("cut proof explicitly counted finite transfer required")?;
    if !raw.performing_draw_tranches.is_empty()
        || raw.reviewed_credit_actions.is_some()
        || raw.factor_policy.terminal_obligations.is_some()
    {
        return Err("cut proof draw/review/pending source layout not yet mapped".into());
    }
    contract_subset(&raw.source.original)?;
    contract_subset(&raw.source.current)?;
    contract_subset(&transfer.contract_origin)
}
fn count_value(value: &Value, result: &mut ProjectionEnvelopeInventory) -> R<()> {
    tick(1)?;
    result.projection_cells = add(result.projection_cells, 1)?;
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
        Value::String(s) => {
            tick(s.len())?;
            result.projection_string_utf8_bytes =
                add(result.projection_string_utf8_bytes, s.len())?;
            result.observed_value_string_capacity_bytes =
                add(result.observed_value_string_capacity_bytes, s.capacity())?;
        }
        Value::Array(values) => {
            result.observed_value_array_capacity_slots = add(
                result.observed_value_array_capacity_slots,
                values.capacity(),
            )?;
            for value in values {
                count_value(value, result)?;
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                tick(add(1, key.len())?)?;
                result.projection_fields = add(result.projection_fields, 1)?;
                result.projection_string_utf8_bytes =
                    add(result.projection_string_utf8_bytes, key.len())?;
                result.observed_value_string_capacity_bytes =
                    add(result.observed_value_string_capacity_bytes, key.capacity())?;
                count_value(value, result)?;
            }
        }
    }
    Ok(())
}

/// Inspect already prepared, genuine sources without retaining/encoding another
/// receipt. Counts are diagnostic. No source work or unknown allocator is waived.
pub fn inventory_prepared(prepared: &units::Prepared) -> R<ProjectionEnvelopeInventory> {
    if daily::cut_census_work_spent().is_none() {
        return Err("cut proof inventory requires a remaining-work ticket".into());
    }
    let source = prepared.source();
    // Public native structs are exhaustively named. Their private child owner
    // visitors are not accessible here and remain a separate explicit gate.
    let units::ResultData {
        schema: _,
        raw_source: _,
        scenario_ids: _,
        units: _,
        shape: _,
        metrics: _,
        values: _,
        fee_income_values: _,
        funding: _,
        native_graphs: _,
        admitted_work_upper: _,
        scope: _,
    } = source;
    let mut result = ProjectionEnvelopeInventory::default();
    for g in &source.native_graphs {
        let graph::ResultData {
            schema: _,
            raw_source: _,
            original_oas: _,
            native_sources: _,
            metrics: _,
            admitted_work_upper: _,
            shared_draws_retained: _,
            physical_accounting_source_retained: _,
            generated_physical_recipes_retained: _,
            generated_oas_calibration: _,
            empirical_validation: _,
            scope: _,
            exercise_policies: _,
            original_physical_sources: _,
            original_acquisition_proofs: _,
        } = g;
        tick(1)?;
        result.graph_uses = add(result.graph_uses, 1)?;
        if g.schema != "commercial-product-graph-1"
            || g.generated_physical_recipes_retained.is_some()
            || g.generated_oas_calibration.is_some()
            || !g.exercise_policies.is_empty()
            || !g.original_physical_sources.is_empty()
            || !g.original_acquisition_proofs.is_empty()
        {
            return Err(
                "cut proof inventory selected finite nonoptional transfer subset required".into(),
            );
        }
        for p in &g.raw_source.positions {
            let graph::Position {
                position_id: _,
                credit_identity: _,
                oas_target: _,
                original_signed_cash_quote: _,
                original_pricing: _,
                original_physical: _,
                histories: _,
                exercise: _,
                generated_policy: _,
                fees: _,
            } = p;
            if p.exercise.is_some()
                || p.generated_policy.is_some()
                || !p.original_physical.is_empty()
            {
                return Err(
                    "cut proof optional/generated original proof mapping incomplete".into(),
                );
            }
            result.original_pricing_expected_uses = add(
                result.original_pricing_expected_uses,
                p.original_pricing.len(),
            )?;
            for h in &p.original_pricing {
                request_subset(&h.source)?;
            }
        }
        for s in &g.native_sources {
            tick(1)?;
            request_subset(&s.source_request)?;
            let physical = match s.role {
                portfolio_risk_native::advanced_graph::Role::Physical => true,
                portfolio_risk_native::advanced_graph::Role::Pricing
                | portfolio_risk_native::advanced_graph::Role::PricingDown
                | portfolio_risk_native::advanced_graph::Role::PricingUp => false,
            };
            result.current_source_expected_uses = add(result.current_source_expected_uses, 1)?;
            if physical {
                result.current_physical_uses = add(result.current_physical_uses, 1)?;
            } else {
                result.current_pricing_uses = add(result.current_pricing_uses, 1)?;
            }
            envelope(&s.projection, physical)?;
            count_value(&s.projection, &mut result)?;
        }
    }
    // Original Q sources are actually dropped by graph::analyze after OAS;
    // current final Preparations do not retain their complete source proofs.
    Ok(result)
}

pub fn require_complete_inventory(_: &ProjectionEnvelopeInventory) -> R<()> {
    Err("cut proof inventory incomplete: original Pricing proof lifetime, recursive native Value field mapping, raw-derived source/allocator and complete Sim/output census are unproved".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::commercial::{candidates, cut_mixed};
    fn raw() -> cut_mixed::Request {
        serde_json::from_str(include_str!(
            "../../../../portfolio-risk/tests/fixtures/commercial-short-secondary-cut-refusal.json"
        ))
        .unwrap()
    }
    fn inspect(
        r: &cut_mixed::Request,
    ) -> R<(
        ProjectionEnvelopeInventory,
        ProjectionEnvelopeInventory,
        daily::TypedCutPrefixInventory,
    )> {
        let a = candidates::admit_finite_preparation(&r.completed_candidate)?;
        let b = candidates::admit_finite_preparation(&r.revised_candidate)?;
        let mut observed = None;
        candidates::with_finite_compiled(
            &r.completed_candidate,
            (b.work_upper, b.retained_bytes_upper),
            &mut |old_spec, old, old_owner| {
                candidates::with_finite_compiled(
                    &r.revised_candidate,
                    (a.work_upper, a.retained_bytes_upper),
                    &mut |new_spec, new, new_owner| {
                        daily::with_cut_census_budget(5_198_020, || {
                            let x = inventory_prepared(old)?;
                            let y = inventory_prepared(new)?;
                            let prefix = daily::commercial_cut_typed_prefix_inventory(
                                old_spec,
                                new_spec,
                                old_owner,
                                new_owner,
                                (r.cut_ordinal - r.completed_candidate.as_of_ordinal) as usize,
                            )?;
                            observed = Some((x, y, prefix));
                            Ok(())
                        })
                    },
                )?;
                Ok(())
            },
        )?;
        observed.ok_or("diagnostic source inspection absent".into())
    }
    #[test]
    fn unknown_or_missing_native_projection_field_and_version_refuse() {
        daily::with_cut_census_budget(100_000,||{
            let mut value=serde_json::json!({"secondary_acquisition":{},"original_source":{},"current_source":{},"carry":{},"admission":{},
                "selected_accounting_owner":"economic_carry_only-1","physical_journal_eligible":false,"unmapped_new_money":1.});
            assert!(envelope(&value,false).unwrap_err().contains("unmapped"));
            value.as_object_mut().unwrap().remove("unmapped_new_money");value.as_object_mut().unwrap().remove("carry");
            assert!(envelope(&value,false).unwrap_err().contains("missing"));
            let map=serde_json::json!({"schema":"future-source-version"});assert!(schema(map.as_object().unwrap(),"known").is_err());Ok(())
        }).unwrap();
        assert!(require_complete_inventory(&ProjectionEnvelopeInventory::default()).is_err());
    }
    #[test]
    fn genuine_y_sources_prefix_and_partial_layouts_never_grant_admission() {
        let observed = inspect(&raw()).unwrap();
        let (x, y, prefix) = &observed;
        assert_eq!(
            x.original_pricing_expected_uses + y.original_pricing_expected_uses,
            3
        );
        assert_eq!(
            x.current_source_expected_uses + y.current_source_expected_uses,
            24
        );
        assert!(!x.original_pricing_proofs_retained && !x.complete_projection_field_mapping);
        assert!(require_complete_inventory(x).is_err());
        assert!(require_complete_inventory(y).is_err());
        assert_eq!(prefix.prefix_events_per_side, 8);
        assert_eq!(prefix.prefix_loan_events_per_side, 4);
        assert_eq!(prefix.prefix_fee_events_per_side, 4);
        assert_eq!(prefix.completed.compiled_work_formula, 61_504);
        assert_eq!(prefix.revised.compiled_work_formula, 107_136);
        assert!(
            prefix.immutable_specification_verified
                && !prefix.complete_sim_bound_available
                && !prefix.raw_source_capacity_bound_available
        );
        assert!(prefix.specification.compiled_ownership_verified);
        println!(
            "CUT_PROOF_DIAGNOSTIC={}",
            serde_json::to_string(&observed).unwrap()
        );
    }
    #[test]
    fn zero_allocations_retain_all_sources_and_foreign_allocation_refuses() {
        let mut r = raw();
        for candidate in [&mut r.completed_candidate, &mut r.revised_candidate] {
            for a in &mut candidate.allocation {
                a.face = 0.;
            }
        }
        let (a, b, p) = inspect(&r).unwrap();
        assert_eq!(
            a.current_source_expected_uses + b.current_source_expected_uses,
            24
        );
        assert_eq!(p.prefix_events_per_side, 8);
        assert_eq!(p.completed.loan_groups, 12);
        assert_eq!(p.revised.loan_groups, 20);
        assert_eq!(p.completed.compiled_work_formula, 61_504);
        assert_eq!(p.revised.compiled_work_formula, 107_136);
        assert!(require_complete_inventory(&a).is_err());
        r.revised_candidate.allocation[0].face = 1.;
        assert!(inspect(&r)
            .unwrap_err()
            .contains("surviving binding identity differs"));
    }
    #[test]
    fn unicode_source_refusal_retained_and_empty_component_lookups_are_charged() {
        let mut r = raw();
        let name = "\u{1f4a8}".repeat(120);
        // Day1 has no fee events, but the fee-map lookup still clones its key.
        for candidate in [&mut r.completed_candidate, &mut r.revised_candidate] {
            candidate.run_mapping[1].ledger_scenario_id = name.clone();
            candidate.specification["scenarios"][0]["name"] = Value::String(name.clone());
        }
        // Keep the genuine larger-label source refusal; do not shrink that case
        // or treat a source-less component control as financial acceptance.
        let error = inspect(&r).unwrap_err();
        assert_eq!(error,"commercial candidate joined 32M/128MiB preflight exceeds bound before graph generation: work=42982268,bytes=27811456,physical_retained=0,one_owner=0");
        let mut raw_spec = r.completed_candidate.specification.clone();
        raw_spec.as_object_mut().unwrap().remove("version");
        let mut spec: portfolio_ledger_native::types::Spec =
            serde_json::from_value(raw_spec).unwrap();
        spec.positions.clear();
        let a =
            portfolio_ledger_native::commercial_intent_owner::Compiled::new(&spec, vec![]).unwrap();
        let b =
            portfolio_ledger_native::commercial_intent_owner::Compiled::new(&spec, vec![]).unwrap();
        let p = daily::with_cut_census_budget(100_000, || {
            daily::commercial_cut_typed_prefix_inventory(&spec, &spec, &a, &b, 1)
        })
        .unwrap();
        assert_eq!(name.len(), 480);
        assert_eq!(p.prefix_events_per_side, 0);
        assert_eq!(
            p.scenario_clone_bytes_upper,
            4 * 2 * ("baseline".len() + name.len())
        );
        assert_eq!(
            p.accessor_string_workspace_bytes_upper,
            2 * (std::mem::size_of::<String>() + 2 * name.len())
        );
        assert_eq!(p.accessor_lookup_work_upper, 4 * (2 * 480 + 2) * 4);
        assert!(
            p.actual_census_ticket_work
                > p.ordering_comparison_steps_upper + p.accessor_lookup_work_upper
        );
        assert!(!p.complete_sim_bound_available);
    }
}
