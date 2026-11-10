//! Compact generated service-rights recipes and separate acquired cost policy.
//! Full fee journals exist only beside the currently expanded native loan owner.
use super::*;
use crate::advanced_graph::Role;
use crate::products::commercial::fee_source::{self as source, adoption};
use portfolio_model_core::{credit_accounting::Identity, nonintegral_fee as fee};
use std::sync::Arc;

pub const ACQUISITION_SCHEMA: &str = "commercial-generated-fee-acquisition-1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioReleases {
    pub scenario_id: String,
    pub acquisition_releases: Vec<adoption::Release>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acquisition {
    pub schema: String,
    pub source_work_policy: String,
    pub identity: Identity,
    pub original_signed_fee_quote: f64,
    pub total_purchase_cost: f64,
    pub allocations: Vec<adoption::Allocation>,
    pub original_physical_releases: Vec<adoption::Release>,
    pub physical_releases_by_scenario: Vec<ScenarioReleases>,
}
pub(crate) struct Context {
    law: Box<stochastic::FeeLaw>,
    allocations: Vec<adoption::Allocation>,
    releases: Vec<adoption::Release>,
}
impl Context {
    pub(crate) fn pricing(law: &stochastic::Request) -> Option<Arc<Self>> {
        law.nonintegral_fees.as_ref().map(|law| {
            Arc::new(Self {
                law: law.clone(),
                allocations: Vec::new(),
                releases: Vec::new(),
            })
        })
    }
    pub(crate) fn physical(
        r: &Request,
        law: &stochastic::Request,
        original: bool,
    ) -> Result<Option<Arc<Self>>, String> {
        let Some(fees) = &law.nonintegral_fees else {
            return Ok(None);
        };
        let acquired = r
            .fee_acquisition
            .as_ref()
            .ok_or("generated fee acquisition absent")?;
        let releases = if original {
            &acquired.original_physical_releases
        } else {
            &acquired
                .physical_releases_by_scenario
                .iter()
                .find(|h| h.scenario_id == law.source.source.current.scenario_id)
                .ok_or("generated Physical fee release coverage absent")?
                .acquisition_releases
        };
        Ok(Some(Arc::new(Self {
            law: fees.clone(),
            allocations: acquired.allocations.clone(),
            releases: releases.clone(),
        })))
    }
    fn history(&self, raw: &owner::Request) -> adoption::History {
        adoption::History {
            path_id: raw.credit.paths[0].path_id.clone(),
            policy: self.law.policy.clone(),
            acquisition_releases: self.releases.clone(),
        }
    }
    fn bind(
        &self,
        raw: &owner::Request,
        carry: &portfolio_model_core::credit_path_income::ResultData,
        stops: &[owner::NativeStop],
    ) -> Result<adoption::Projection, String> {
        #[cfg(test)]
        stochastic::fees::record_binding(raw);
        let mut paths = source::bind_carry(&self.law.policy, raw, carry, stops)?;
        if paths.len() != 1 {
            return Err("one exact generated native fee source required".into());
        }
        adoption::from_bound_parts(
            self.law.discount_spread,
            &self.allocations,
            &self.history(raw),
            paths.remove(0),
        )
    }
}
#[derive(Serialize)]
pub struct IncomePoint {
    pub year: f64,
    pub income_lines: Vec<f64>,
}
pub struct Projection {
    context: Arc<Context>,
    path_id: String,
    cash: Vec<EconomicCash>,
    income_points: Vec<IncomePoint>,
}
impl Serialize for Projection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        #[derive(Serialize)]
        struct History<'a> {
            path_id: &'a str,
            policy: &'a source::Policy,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            acquisition_releases: &'a Vec<adoption::Release>,
        }
        let mut out = serializer.serialize_struct("GeneratedFeeProjection", 8)?;
        out.serialize_field("schema", "commercial-generated-fee-source-1")?;
        out.serialize_field(
            "raw_fee_history",
            &History {
                path_id: &self.path_id,
                policy: &self.context.law.policy,
                acquisition_releases: &self.context.releases,
            },
        )?;
        out.serialize_field("cash", &self.cash)?;
        out.serialize_field("discount_spread", &self.context.law.discount_spread)?;
        out.serialize_field("allocations", &self.context.allocations)?;
        out.serialize_field("income_points", &self.income_points)?;
        out.serialize_field("full_native_fee_tail_replayed", &true)?;
        out.serialize_field("loan_original_cost_yield_unchanged", &true)?;
        out.end()
    }
}
impl Projection {
    pub(crate) fn build(
        context: Option<&Arc<Context>>,
        raw: &owner::Request,
        carry: &portfolio_model_core::credit_path_income::ResultData,
        stops: &[owner::NativeStop],
    ) -> Result<Option<Self>, String> {
        let Some(context) = context else {
            return Ok(None);
        };
        let bound = context.bind(raw, carry, stops)?;
        let mut income_points = Vec::new();
        if raw.source.current.measure == portfolio_model_core::cashflow::CashflowMeasure::Physical {
            for (year, group) in bound
                .bound_path
                .native_years()
                .iter()
                .zip(bound.bound_path.source().groups())
            {
                let income_lines: Vec<_> = group
                    .postings
                    .iter()
                    .filter(|p| {
                        matches!(
                            p.account,
                            fee::Account::FeeIncome
                                | fee::Account::FeeWriteoffExpense
                                | fee::Account::FeeRecoveryIncome
                                | fee::Account::FeeAcquisitionExpense
                        )
                    })
                    .map(|p| p.amount)
                    .collect();
                if !income_lines.is_empty() {
                    income_points.push(IncomePoint {
                        year: *year,
                        income_lines,
                    });
                }
            }
        }
        Ok(Some(Self {
            context: context.clone(),
            path_id: raw.credit.paths[0].path_id.clone(),
            cash: bound.cash,
            income_points,
        }))
    }
    pub fn cash(&self) -> &[EconomicCash] {
        &self.cash
    }
    pub fn income_period(&self, left: f64, right: f64) -> Result<f64, String> {
        let mut income = 0.;
        for point in self
            .income_points
            .iter()
            .filter(|p| (p.year > left || left == 0. && p.year == 0.) && p.year <= right)
        {
            for value in &point.income_lines {
                income = crate::commercial_graph::money(income - value)?;
            }
        }
        Ok(income)
    }
    pub fn materialize(
        &self,
        raw: &owner::Request,
        carry: &portfolio_model_core::credit_path_income::ResultData,
        stops: &[owner::NativeStop],
    ) -> Result<adoption::Projection, String> {
        self.context.bind(raw, carry, stops)
    }
    pub fn price(
        &self,
        curve: &crate::model_market::curves::SuppliedCurve,
        asof: i32,
        mark: i32,
        bump: f64,
        include_at_mark: bool,
    ) -> Result<f64, String> {
        let year = f64::from(mark - asof) / 365.;
        let anchor = curve.value_validated(year)?.discount_factor;
        let mut sum = 0.;
        let mut correction = 0.;
        for c in self
            .cash
            .iter()
            .filter(|c| c.year > year || include_at_mark && c.year == year)
        {
            check()?;
            let value = crate::commercial_graph::money(
                c.total()? * curve.value_validated(c.year)?.discount_factor / anchor
                    * (-(self.context.law.discount_spread + bump) * (c.year - year)).exp(),
            )?;
            let adjusted = value - correction;
            let next = crate::commercial_graph::money(sum + adjusted)?;
            correction = (next - sum) - adjusted;
            sum = next;
        }
        Ok(sum)
    }
}
pub(crate) fn compatible(q: &stochastic::Request, p: &stochastic::Request) -> Result<(), String> {
    match (&q.nonintegral_fees, &p.nonintegral_fees) {
        (None, None) => Ok(()),
        (Some(q), Some(p)) if encoded_equal(q, p)? => Ok(()),
        _ => Err("generated roles require identical explicit service contracts/exit policy and separate fee spread; Physical outcomes cannot supply a Pricing fee law".into()),
    }
}
pub(crate) fn audit(r: &Request) -> Result<(usize, usize), String> {
    let Some(a) = &r.fee_acquisition else {
        if std::iter::once(&r.pricing_training)
            .chain(std::iter::once(&r.original_physical))
            .chain(r.current_pricing.iter())
            .chain(r.current_physical.iter())
            .any(|l| l.nonintegral_fees.is_some())
        {
            return Err(
                "generated service rights require separate reviewed acquisition ownership".into(),
            );
        }
        return Ok((0, 0));
    };
    if r.physical_path_inputs.is_some() {
        return Err("generated fee rights with selected draw/review catalogs need a separate jointly admitted fee catalog".into());
    }
    if a.schema != ACQUISITION_SCHEMA
        || a.source_work_policy != "counted_native_contract_factor_work-1"
        || a.allocations.is_empty()
        || a.allocations.len() > 64
        || a.physical_releases_by_scenario.len() != r.current_physical.len()
    {
        return Err("explicit complete generated fee acquisition tier required".into());
    }
    crate::products::term::projection::label(&a.identity.id)?;
    crate::products::term::projection::label(&a.identity.revision)?;
    let law = r
        .pricing_training
        .nonintegral_fees
        .as_ref()
        .ok_or("generated Pricing fee law absent")?;
    let mut ids = std::collections::BTreeSet::new();
    let mut cost = 0.;
    for allocation in &a.allocations {
        crate::products::term::projection::label(&allocation.review.id)?;
        crate::products::term::projection::label(&allocation.review.revision)?;
        if !ids.insert(&allocation.fee_id)
            || !law
                .policy
                .fees
                .iter()
                .any(|f| f.fee_id == allocation.fee_id)
        {
            return Err("generated unique fee allocation coverage differs".into());
        }
        let fee = law
            .policy
            .fees
            .iter()
            .find(|f| f.fee_id == allocation.fee_id)
            .ok_or("generated fee allocation absent")?;
        if allocation.allocated_cost != 0.
            && allocation.allocated_cost.signum() != fee.entitlement.signum()
        {
            return Err(
                "generated signed fee acquisition allocation differs from asset/liability contract"
                    .into(),
            );
        }
        cost = crate::commercial_graph::money(cost + allocation.allocated_cost)?;
    }
    if ids.len() != law.policy.fees.len()
        || !near(cost, a.original_signed_fee_quote)
        || !a.total_purchase_cost.is_finite()
        || a.total_purchase_cost <= 0.
    {
        return Err("generated separate signed fee quote/allocation mismatch".into());
    }
    let mut scenarios = std::collections::BTreeSet::new();
    for releases in &a.physical_releases_by_scenario {
        if !scenarios.insert(&releases.scenario_id)
            || !r
                .current_physical
                .iter()
                .any(|l| l.source.source.current.scenario_id == releases.scenario_id)
        {
            return Err("generated fee release scenario coverage differs".into());
        }
    }
    let mut work = 0usize;
    let mut bytes = mul(strict_json::serialized_size(a, 1 << 20)?, 20)?;
    for (law, original) in std::iter::once((&r.original_physical, true))
        .chain(r.current_physical.iter().map(|l| (l, false)))
    {
        let context =
            Context::physical(r, law, original)?.ok_or("generated Physical fee law absent")?;
        let history = context.history(&law.source);
        adoption::validate_releases_parts(&a.allocations, &history, &law.source, Role::Physical)?;
        // Cost compilation replays the native service groups, then acquired cost
        // events. Count both event arrays and the full bound for every P path.
        let events = add(
            add(source::event_bound(&history.policy), a.allocations.len())?,
            history.acquisition_releases.len(),
        )?;
        work = add(work, mul(mul(events, 1024)?, law.evaluation.paths)?)?;
        bytes = add(bytes, mul(add(events, 1)?, 4096)?)?;
    }
    Ok((work, bytes))
}
pub(crate) fn compact_upper(law: &stochastic::Request) -> Result<usize, String> {
    let Some(fees) = &law.nonintegral_fees else {
        return Ok(0);
    };
    // Cash and income lines retain two numeric tapes; an Arc header/path label
    // exists per recipe, while the shared immutable policy body is charged once.
    let per = add(mul(source::event_bound(&fees.policy), 2048)?, 1024)?;
    add(
        mul(per, law.evaluation.paths)?,
        mul(strict_json::serialized_size(fees, 1 << 20)?, 20)?,
    )
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-12 * b.abs()
}
