//! Explicit finite complete-transfer work authority. No caller numeric budget.
use super::*;
use indexed::work::{self, ContractWorkSummary, Op, WorkMeter};

pub const POLICY: &str = "counted_finite_transfer_contract_factor_work-1";
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SourceWorkPolicy {
    #[serde(rename = "counted_finite_transfer_contract_factor_work-1")]
    CountedFiniteTransfer,
}
#[derive(Debug, Clone, Copy)]
pub(crate) enum Role {
    Origin,
    Original,
    Current,
}
/// Native-only immutable borrow of the selected complete raw acquisition. It
/// accepts no serialized state, monetary factor or caller numeric allowance.
pub(crate) struct Authority<'a> {
    owner: &'a source_income::Request,
    role: Role,
}
impl<'a> Authority<'a> {
    pub(crate) fn selected(owner: &'a source_income::Request, role: Role) -> Option<Self> {
        owner.transfer.as_ref()?.source_work_policy?;
        Some(Self { owner, role })
    }
    pub(crate) fn raw(&self) -> &indexed::ContractPathRequest {
        match self.role {
            Role::Origin => &self.owner.transfer.as_ref().unwrap().contract_origin,
            Role::Original => &self.owner.original,
            Role::Current => &self.owner.current,
        }
    }
    pub(crate) fn information_asof(&self) -> i32 {
        self.owner.transfer.as_ref().unwrap().economic_ordinal
    }
    pub(crate) fn verify(
        &self,
        raw: &indexed::ContractPathRequest,
        transferred: bool,
    ) -> Result<usize, String> {
        let expected = strict_json::encode(self.raw(), 1 << 20)?;
        let received = strict_json::encode(raw, 1 << 20)?;
        let scanned = expected
            .len()
            .checked_add(received.len())
            .ok_or("finite identity scan overflow")?;
        if transferred == matches!(self.role, Role::Origin)
            || self
                .owner
                .transfer
                .as_ref()
                .is_none_or(|t| t.source_work_policy.is_none())
            || (!matches!(self.role, Role::Origin) && raw.asof_ordinal != self.information_asof())
            || expected != received
        {
            return Err("foreign/stale finite transfer source/role/information authority".into());
        }
        Ok(scanned)
    }
    pub(crate) fn summary(
        &self,
        raw: &indexed::ContractPathRequest,
        queries: usize,
        observable: bool,
        terminal: bool,
    ) -> Result<ContractWorkSummary, String> {
        let mut summary = ContractWorkSummary::census(raw, queries, observable, terminal)?;
        summary.policy = POLICY;
        summary.upper.input_bytes = summary
            .upper
            .input_bytes
            .checked_mul(3)
            .ok_or("finite identity work overflow")?;
        Ok(summary)
    }
    pub(crate) fn verification_bytes(&self) -> Result<usize, String> {
        // Both complete canonical equality buffers, allocator capacity/slack.
        indexed::work::request_bytes(self.raw())?
            .checked_mul(4)
            .ok_or_else(|| "finite authority bytes overflow".into())
    }
}
/// Actual bare full-origin source build: no second NativeFactors compilation,
/// no fabricated segment extraction and no legacy floor in the selected tier.
pub(crate) fn build_origin(
    owner: &source_income::Request,
) -> Result<indexed::ContractPathResult, String> {
    let authority =
        Authority::selected(owner, Role::Origin).ok_or("selected finite origin policy missing")?;
    let raw = authority.raw();
    let verified_bytes = authority.verify(raw, false)?;
    let summary = authority.summary(raw, 0, true, false)?;
    let meter = WorkMeter::new(&summary);
    work::bump(Some(&meter), Op::InputBytes, verified_bytes)?;
    let full = indexed::build_transfer_path_counted(
        raw,
        authority.information_asof(),
        &meter,
        None,
        None,
    )?;
    work::bump(
        Some(&meter),
        Op::OutputBytes,
        strict_json::serialized_size(&full, 32 << 20)?,
    )?;
    meter.check()?;
    #[cfg(test)]
    {
        ORIGIN_BUILD_CALLS.with(|v| v.set(v.get() + 1));
        work::export_receipt(raw, &summary, meter.observed());
    }
    Ok(full)
}
#[cfg(test)]
thread_local! {pub(crate) static ORIGIN_BUILD_CALLS:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commercial_credit_compiler as owner;
    use serde_json::json;
    fn raw() -> owner::Request {
        serde_json::from_str(include_str!(
            "../../../../../../examples/models/commercial-secondary-entitlement-1.json"
        ))
        .unwrap()
    }
    fn selected() -> owner::Request {
        let mut r = raw();
        r.source.transfer.as_mut().unwrap().source_work_policy =
            Some(SourceWorkPolicy::CountedFiniteTransfer);
        r
    }
    #[test]
    fn selected_complete_origin_and_two_buyers_are_all_metered_once() {
        let before = owner::admit(&raw()).unwrap();
        let r = selected();
        let after = owner::admit(&r).unwrap();
        assert!(after.original_source_work < before.original_source_work);
        assert!(after.current_source_work > 0);
        println!(
            "finite source owner work before={},after={}",
            before.joined_work_upper, after.joined_work_upper
        );
        ORIGIN_BUILD_CALLS.with(|v| v.set(0));
        crate::commercial_factor_source::owner::SOURCE_CALLS.with(|v| v.set(0));
        let actual = owner::evaluate(&r).unwrap();
        assert_eq!(ORIGIN_BUILD_CALLS.with(|v| v.get()), 1);
        assert_eq!(
            crate::commercial_factor_source::owner::SOURCE_CALLS.with(|v| v.get()),
            2
        );
        let actual = serde_json::to_value(actual).unwrap();
        assert_eq!(
            actual["current_source"]["admission"]["counted_work"]["policy"],
            POLICY
        );
        assert_eq!(actual["secondary_acquisition"]["principal"], 80.);
        assert_eq!(
            actual["secondary_acquisition"]["coupons"][0]["opening_entitlement_principal"],
            100.
        );
        let economic = owner::evaluate_economic_with_measure(
            &r,
            portfolio_model_core::credit::CreditMeasure::Physical,
        )
        .unwrap();
        for source in [&economic.original_source, &economic.current_source] {
            let measured = source.counted_operations().unwrap();
            let summary = source.admission().counted_work.as_ref().unwrap();
            assert!(measured.work().unwrap() <= summary.upper.work().unwrap());
            assert!(measured.input_bytes > 0 && measured.output_bytes > 0);
            assert_eq!(measured.periods, source.raw().periods.len());
        }
    }
    #[test]
    fn borrowing_authority_rejects_foreign_source_role_and_clock() {
        let r = selected();
        let authority = Authority::selected(&r.source, Role::Current).unwrap();
        authority.verify(&r.source.current, true).unwrap();
        assert!(authority.verify(&r.source.current, false).is_err());
        let mut changed = r.source.current.clone();
        changed.asof_ordinal += 1;
        assert!(authority.verify(&changed, true).is_err());
        let mut changed = r.source.current.clone();
        changed.contract.revision.push('x');
        assert!(authority.verify(&changed, true).is_err());
        let origin = Authority::selected(&r.source, Role::Origin).unwrap();
        assert!(origin.verify(&r.source.current, false).is_err());
        assert!(Authority::selected(&raw().source, Role::Origin).is_none());
        let mut mixed = selected();
        mixed.counted_work = Some(Box::new(
            work::GeneratedCatalogAuthority::bind(&mixed).unwrap(),
        ));
        assert!(owner::admit(&mixed)
            .unwrap_err()
            .contains("generated counted secondary"));
    }
    #[test]
    fn omitted_null_and_private_authority_injection_preserve_historical_receipts() {
        let old = raw();
        let mut value = serde_json::to_value(&old).unwrap();
        value["source"]["transfer"]["source_work_policy"] = serde_json::Value::Null;
        let explicit: owner::Request = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            strict_json::encode(&old, 1 << 20).unwrap(),
            strict_json::encode(&explicit, 1 << 20).unwrap()
        );
        assert_eq!(
            serde_json::to_value(owner::admit(&old).unwrap()).unwrap(),
            serde_json::to_value(owner::admit(&explicit).unwrap()).unwrap()
        );
        value["source"]["transfer"]["source_work_policy"] = json!({"work_upper":1});
        assert!(serde_json::from_value::<owner::Request>(value).is_err());
        let mut value = serde_json::to_value(&old).unwrap();
        value["finite_work_authority"] = json!({"work_upper":1});
        assert!(serde_json::from_value::<owner::Request>(value).is_err());
    }
    #[test]
    fn selector_uses_existing_transfer_layout_padding() {
        #[allow(dead_code)]
        struct HistoricalTransfer {
            schema: String,
            review: term::Identity,
            contract_origin: indexed::ContractPathRequest,
            economic_ordinal: i32,
            recognition_ordinal: i32,
            settlement_ordinal: i32,
            coincident_order: String,
            conveyance: String,
            loan_clean_cost: f64,
            transaction_cost: f64,
            transaction_cost_policy: String,
            coupon_allocations: Vec<super::super::CouponAllocation>,
            regime: super::super::Regime,
        }
        assert_eq!(
            std::mem::size_of::<Transfer>(),
            std::mem::size_of::<HistoricalTransfer>()
        );
        assert_eq!(std::mem::size_of::<Option<SourceWorkPolicy>>(), 1);
    }
    #[test]
    fn genuine_joint_graph_charges_every_full_origin_and_both_buyer_builds() {
        fn select(v: &mut serde_json::Value) {
            match v {
                serde_json::Value::Object(o) => {
                    if o.get("schema").and_then(|s| s.as_str()) == Some(SCHEMA) {
                        o.insert("source_work_policy".into(), json!(POLICY));
                    }
                    for value in o.values_mut() {
                        select(value);
                    }
                }
                serde_json::Value::Array(a) => {
                    for value in a {
                        select(value);
                    }
                }
                _ => {}
            }
        }
        let mut raw:serde_json::Value=serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"),"/../portfolio-risk/tests/fixtures/commercial-secondary-prepaid-exercise-candidate-refusal.json"))).unwrap();
        select(&mut raw);
        let graph: crate::commercial_graph::Request =
            serde_json::from_value(raw["commercial"]["units"][0]["graph"].clone()).unwrap();
        let bound = crate::commercial_graph::audit_admitted_work(&graph).unwrap();
        owner::COUNTED_OWNER_WORK.with(|v| v.set(0));
        owner::OWNER_EVALUATIONS.with(|v| v.set(0));
        ORIGIN_BUILD_CALLS.with(|v| v.set(0));
        crate::commercial_factor_source::owner::SOURCE_CALLS.with(|v| v.set(0));
        let actual = crate::commercial_graph::analyze(&graph).unwrap();
        let owners = owner::OWNER_EVALUATIONS.with(|v| v.get());
        assert!(owners > 10);
        assert_eq!(ORIGIN_BUILD_CALLS.with(|v| v.get()), owners);
        assert_eq!(
            crate::commercial_factor_source::owner::SOURCE_CALLS.with(|v| v.get()),
            2 * owners
        );
        assert!(owner::COUNTED_OWNER_WORK.with(|v| v.get()) <= bound);
        assert_eq!(actual.admitted_work_upper, bound);
        println!("finite joint graph owners={owners},origin_builds={},buyer_builds={},charged_owner_work={},graph_work={bound}",ORIGIN_BUILD_CALLS.with(|v|v.get()),crate::commercial_factor_source::owner::SOURCE_CALLS.with(|v|v.get()),owner::COUNTED_OWNER_WORK.with(|v|v.get()));
    }
    #[test]
    fn finite_census_counts_actual_calendar_and_escaped_identity_queries() {
        let mut r: owner::Request = serde_json::from_str(include_str!(
            "../../../../../../examples/models/commercial-secondary-acquisition-1.json"
        ))
        .unwrap();
        r.source.transfer.as_mut().unwrap().source_work_policy =
            Some(SourceWorkPolicy::CountedFiniteTransfer);
        let at = r.source.current.asof_ordinal;
        let raw = &mut r.source.current;
        raw.path.id = "\\".repeat(128);
        raw.path.revision = "\\".repeat(128);
        raw.periods[0].period_id = "\\".repeat(128);
        raw.periods[0].day_count = portfolio_model_core::cashflow::ActualDayCount::ActActIsda;
        raw.periods[0].end_ordinal = at + 800;
        raw.periods[0].payment_ordinal = at + 800;
        raw.tail_end_ordinal = at + 800;
        let old =
            NativeFactors::compile_transfer(raw.clone(), r.factor_policy.clone(), 8, raw.measure)
                .unwrap();
        let authority = Authority::selected(&r.source, Role::Current).unwrap();
        let raw = &r.source.current;
        let selected = NativeFactors::compile_finite_transfer(
            raw.clone(),
            r.factor_policy.clone(),
            8,
            raw.measure,
            &authority,
            None,
        )
        .unwrap();
        assert_eq!(
            strict_json::encode(old.source(), 1 << 20).unwrap(),
            strict_json::encode(selected.source(), 1 << 20).unwrap()
        );
        selected.earned(0., 799. / 365.).unwrap();
        selected.cumulative_coupon_factors(799. / 365.).unwrap();
        let ops = selected.counted_operations().unwrap();
        assert_eq!(ops.earned_calls, 1);
        assert_eq!(ops.canonical_calls, 1);
        assert!(ops.calendar >= 5);
        assert!(ops.input_bytes > strict_json::serialized_size(raw, 1 << 20).unwrap() * 2);
        assert!(ops.work().unwrap() <= selected.admission().work_upper);
    }
}
