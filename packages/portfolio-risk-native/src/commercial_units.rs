//! Immutable native commercial coefficients from owned Physical journal sources
//! and paired Pricing cash. Evaluation cannot accept replacement coefficients.
use crate::{
    aged_units::{AgedIdentity, Weights, METRICS},
    commercial_graph as graph,
    product_strategy::{Allocation, UnitIdentity},
};
use portfolio_model_core::strict_json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
pub const SCHEMA: &str = "commercial-units-1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    pub unit_id: String,
    pub template_id: String,
    pub unit_face: f64,
    pub accounting_category: String,
    pub capital_policy_revision: String,
    pub weights: Weights,
    pub graph: graph::Request,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub identity: AgedIdentity,
    pub currency: String,
    pub reporting_ordinals: Vec<i32>,
    pub base_scenario_id: String,
    pub units: Vec<Unit>,
}
#[derive(Clone, Serialize)]
pub struct CashEvent {
    pub scenario_id: String,
    pub unit_index: usize,
    pub path_id: String,
    pub source_event_id: String,
    pub year: f64,
    pub ordinal: i32,
    pub source_weight: f64,
    pub purchase_cash: f64,
    pub principal_cash: f64,
    pub interest_cash: f64,
    pub fee_cash: f64,
    pub bank_cash: f64,
}
#[derive(Serialize)]
pub struct Funding {
    pub scenario_ids: Vec<String>,
    pub ordinals: Vec<i32>,
    pub shape: [usize; 3],
    pub demand: Vec<f64>,
    pub cash_events: Vec<CashEvent>,
    pub policy: &'static str,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_source: Request,
    pub scenario_ids: Vec<String>,
    pub units: Vec<UnitIdentity>,
    pub shape: [usize; 4],
    pub metrics: Vec<&'static str>,
    pub values: Vec<f64>,
    pub funding: Funding,
    pub native_graphs: Vec<graph::ResultData>,
    pub admitted_work_upper: usize,
    pub scope: &'static str,
}
/// Private retained source; no caller-produced financial coefficient admission.
#[derive(Serialize)]
pub struct Prepared {
    source: ResultData,
}
impl Prepared {
    pub fn source(&self) -> &ResultData {
        &self.source
    }
}
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn money(v: f64) -> Result<f64, String> {
    graph::money(v)
}
fn label(v: &str) -> Result<(), String> {
    crate::advanced_product::label(v)
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-12 * b.abs()
}
/// Identical exact-native-day versus subday end-of-day attribution as Sim.
pub fn settlement_ordinal(asof: i32, year: f64) -> Result<i32, String> {
    if !year.is_finite() || !(0. ..=100.).contains(&year) {
        return Err("commercial actual cash time outside horizon".into());
    }
    let elapsed = year * 365.;
    let integer = elapsed.round() as i32;
    let mut day = if (f64::from(integer) / 365.).to_bits() == year.to_bits() {
        integer
    } else {
        elapsed.ceil() as i32
    };
    if year > f64::from(day) / 365. {
        day += 1;
    }
    let out = asof
        .checked_add(day)
        .ok_or("commercial actual cash date overflow")?;
    portfolio_model_core::cashflow::validate_ordinal(out)?;
    Ok(out)
}
pub fn audit_source_work(r: &Request) -> Result<usize, String> {
    check()?;
    if r.schema != SCHEMA
        || r.units.is_empty()
        || r.units.len() > 32
        || !(2..=121).contains(&r.reporting_ordinals.len())
        || r.reporting_ordinals.windows(2).any(|x| x[0] >= x[1])
    {
        return Err(
            "explicit commercial units and actual increasing reporting grid required".into(),
        );
    }
    for v in [
        &r.currency,
        &r.base_scenario_id,
        &r.identity.model_revision,
        &r.identity.input_revision,
        &r.identity.grid_revision,
        &r.identity.crn_id,
    ] {
        label(v)?;
    }
    for day in &r.reporting_ordinals {
        portfolio_model_core::cashflow::validate_ordinal(*day)?;
    }
    let raw = strict_json::serialized_size(r, 32 << 20)?;
    let mut work = raw.checked_mul(8).ok_or("commercial unit work overflow")?;
    let mut ids = BTreeSet::new();
    let mut common = None;
    for u in &r.units {
        check()?;
        for v in [
            &u.unit_id,
            &u.template_id,
            &u.accounting_category,
            &u.capital_policy_revision,
        ] {
            label(v)?;
        }
        if u.graph
            .positions
            .iter()
            .any(|p| p.exercise.is_some() && p.original_physical.is_empty())
        {
            return Err("commercial exercise units require an independently supplied original stopped Physical acquisition cash/yield anchor; analytical continuation alone cannot establish acquired coefficients".into());
        }
        if !ids.insert(&u.unit_id)
            || money(u.unit_face)? <= 0.
            || u.graph.positions.len() != 1
            || u.graph.positions[0].position_id != u.unit_id
            || u.graph.currency != r.currency
            || u.graph.base_scenario_id != r.base_scenario_id
            || !r.reporting_ordinals[..r.reporting_ordinals.len() - 1]
                .contains(&u.graph.as_of_ordinal)
            || !(matches!(
                u.graph.positions[0].oas_target,
                crate::advanced_graph::OasTarget::Locked { .. }
            ) || (matches!(
                u.graph.positions[0].oas_target,
                crate::advanced_graph::OasTarget::CalibratePositiveCash { .. }
            ) && u.graph.positions[0]
                .generated_policy
                .as_ref()
                .is_some_and(|g| g.oas_calibration.is_some())))
        {
            return Err("commercial unit needs one known purchase position, positive reconciled face and retained original OAS".into());
        }
        let expected: Vec<_> = r
            .reporting_ordinals
            .iter()
            .copied()
            .filter(|d| *d >= u.graph.as_of_ordinal)
            .collect();
        if u.graph.reporting_ordinals != expected {
            return Err("commercial purchase/report grid mismatch".into());
        }
        let scenes = u
            .graph
            .scenarios
            .iter()
            .map(|s| json!([s.scenario_id, s.run_id, s.severity]))
            .collect::<Vec<_>>();
        if common.as_ref().is_some_and(|old| *old != scenes) {
            return Err("commercial units require identical scenario identities/order".into());
        }
        common.get_or_insert(scenes);
        for w in [
            u.weights.hqla_l2a,
            u.weights.outflow30,
            u.weights.asf,
            u.weights.rsf,
            u.weights.rwa,
        ] {
            if !w.is_finite() || !(0. ..=100.).contains(&w) {
                return Err(
                    "commercial unit regulatory weights require explicit bounded supplied policy"
                        .into(),
                );
            }
        }
        for h in &u.graph.positions[0].histories {
            for histories in [&h.physical, &h.pricing, &h.pricing_down, &h.pricing_up] {
                for p in histories {
                    if !near(p.source.source.current.opening_principal, u.unit_face) {
                        return Err(
                            "commercial unit face differs from actual source opening principal"
                                .into(),
                        );
                    }
                }
            }
        }
        for h in &u.graph.positions[0].original_physical {
            if !near(h.source.source.current.opening_principal, u.unit_face) {
                return Err(
                    "commercial unit face differs from original Physical acquisition principal"
                        .into(),
                );
            }
        }
        if let Some(generated) = &u.graph.positions[0].generated_policy {
            for law in std::iter::once(&generated.pricing_training)
                .chain(std::iter::once(&generated.original_physical))
                .chain(&generated.current_physical)
                .chain(&generated.current_pricing)
            {
                if !near(law.source.source.current.opening_principal, u.unit_face)
                    || !near(law.source.source.original.opening_principal, u.unit_face)
                {
                    return Err("generated commercial unit face differs from immutable original/current law source principal".into());
                }
            }
        }
        work = work
            .checked_add(graph::audit_admitted_work(&u.graph)?)
            .ok_or("commercial unit joined work overflow")?;
    }
    let cells =
        r.units.len() * r.units[0].graph.scenarios.len() * 10 * (r.reporting_ordinals.len() - 1);
    work = work
        .checked_add(cells * 128)
        .ok_or("commercial units output overflow")?;
    let max_work = if r.units.iter().any(|u| {
        u.graph
            .positions
            .iter()
            .any(|p| p.generated_policy.is_some())
    }) {
        4_000_000_000
    } else {
        32_000_000
    };
    if cells > 1_000_000 || work > max_work {
        return Err(
            "commercial units joined 32M work/dense admission exceeded before generation".into(),
        );
    }
    Ok(work)
}
pub fn prepare(r: &Request) -> Result<Prepared, String> {
    let work = audit_source_work(r)?;
    let scenarios = r.units[0]
        .graph
        .scenarios
        .iter()
        .map(|s| s.scenario_id.clone())
        .collect::<Vec<_>>();
    let s = scenarios.len();
    let n = r.units.len();
    let h = r.reporting_ordinals.len() - 1;
    let mut values = vec![0.; s * n * 10 * h];
    let mut units = Vec::new();
    let mut native_graphs = Vec::new();
    let mut cash = Vec::new();
    for (ui, u) in r.units.iter().enumerate() {
        check()?;
        let g = graph::analyze(&u.graph)?;
        let purchase = u.graph.as_of_ordinal;
        let period = r.reporting_ordinals[..h]
            .iter()
            .position(|d| *d == purchase)
            .ok_or("commercial purchase report boundary absent")?;
        let quote = g.original_oas[0].original_signed_cash_quote / u.unit_face;
        for source in g
            .native_sources
            .iter()
            .filter(|s| s.role == crate::advanced_graph::Role::Physical)
        {
            let acquisition = &source.source_request.source.acquisition;
            if !near(acquisition.cost / u.unit_face, quote) {
                return Err("commercial Pricing quote and independently supplied Physical acquisition cost differ".into());
            }
            let scene = scenarios
                .iter()
                .position(|s| *s == source.scenario_id)
                .ok_or("commercial scenario source absent")?;
            for (pi, days) in r
                .reporting_ordinals
                .windows(2)
                .enumerate()
                .filter(|(_, d)| d[0] >= purchase)
            {
                check()?;
                let left = f64::from(days[0] - purchase) / 365.;
                let right = f64::from(days[1] - purchase) / 365.;
                let (native_principal, nii) = source.physical_period(left, right)?;
                let principal = money(native_principal / u.unit_face)?;
                let at = ((scene * n + ui) * 10) * h + pi;
                let metrics = [
                    nii / u.unit_face,
                    principal,
                    0.,
                    u.weights.hqla_l2a * principal,
                    u.weights.outflow30 * principal,
                    u.weights.asf * principal,
                    u.weights.rsf * principal,
                    u.weights.rwa * principal,
                    principal,
                    0.,
                ];
                for (mi, value) in metrics.iter().enumerate() {
                    values[at + mi * h] = money(values[at + mi * h] + source.weight * value)?;
                }
            }
            for c in &source.cash {
                let principal = source.weight * c.principal / u.unit_face;
                let interest = source.weight * c.interest / u.unit_face;
                let fees = source.weight * c.fees / u.unit_face;
                cash.push(CashEvent {
                    scenario_id: source.scenario_id.clone(),
                    unit_index: ui,
                    path_id: source.path_id.clone(),
                    source_event_id: c.event_id.clone(),
                    year: c.year,
                    ordinal: settlement_ordinal(purchase, c.year)?,
                    source_weight: source.weight,
                    purchase_cash: 0.,
                    principal_cash: principal,
                    interest_cash: interest,
                    fee_cash: fees,
                    bank_cash: money(principal + interest + fees)?,
                });
            }
        }
        for (si, id) in scenarios.iter().enumerate() {
            cash.push(CashEvent {
                scenario_id: id.clone(),
                unit_index: ui,
                path_id: "purchase".into(),
                source_event_id: format!("purchase:{}", u.unit_id),
                year: 0.,
                ordinal: purchase,
                source_weight: 1.,
                purchase_cash: -quote,
                principal_cash: 0.,
                interest_cash: 0.,
                fee_cash: 0.,
                bank_cash: -quote,
            });
            for m in g.metrics.iter().filter(|m| m.scenario_id == *id) {
                if let Some(pi) = r.reporting_ordinals[..h]
                    .iter()
                    .position(|d| *d == m.ordinal)
                {
                    values[((si * n + ui) * 10 + 2) * h + pi] = money(m.dv01 / u.unit_face)?;
                }
            }
        }
        units.push(UnitIdentity {
            unit_id: u.unit_id.clone(),
            template_id: u.template_id.clone(),
            purchase_ordinal: purchase,
            purchase_period: period,
            side: 1.,
            accounting_category: u.accounting_category.clone(),
            capital_policy_revision: u.capital_policy_revision.clone(),
            quote_price_per_face_unit: quote,
        });
        native_graphs.push(g);
    }
    let mut dates: BTreeSet<_> = r.reporting_ordinals.iter().copied().collect();
    for c in &cash {
        dates.insert(c.ordinal);
    }
    let dates = dates.into_iter().collect::<Vec<_>>();
    let f = dates.len();
    if s * n * f > 1_000_000 || cash.len() > 250_000 {
        return Err("commercial full-tail funding output bound exceeded".into());
    }
    cash.sort_by_key(|e| e.ordinal);
    let mut balances = vec![0.; s * n];
    let mut demand = vec![0.; s * n * f];
    let mut cursor = 0;
    let scene_map: BTreeMap<_, _> = scenarios
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    for (di, day) in dates.iter().enumerate() {
        check()?;
        while cursor < cash.len() && cash[cursor].ordinal <= *day {
            let e = &cash[cursor];
            let ix = scene_map[e.scenario_id.as_str()] * n + e.unit_index;
            balances[ix] = money(balances[ix] + e.bank_cash)?;
            cursor += 1;
        }
        for (ix, v) in balances.iter().enumerate() {
            demand[ix * f + di] = -v;
        }
    }
    let source=ResultData{schema:SCHEMA,raw_source:r.clone(),scenario_ids:scenarios.clone(),units,shape:[s,n,10,h],metrics:METRICS.to_vec(),values,
        funding:Funding{scenario_ids:scenarios,ordinals:dates,shape:[s,n,f],demand,cash_events:cash,policy:"actual end-of-day cash attribution identical to native daily Sim; complete native source/prepayment/draw/default/recovery cash tails, separately reviewed acquisition cash, no expected-loss cash or implicit reinvestment"},native_graphs,admitted_work_upper:work,
        scope:"commercial credit NII from authoritative Physical InterestIncome postings; live/default principal amounts and supplied regulatory weights; unconditional supplied paired Pricing marks at frozen purchase OAS; no automatic regulatory eligibility, empirical calibration or Physical exercise policy inference"};
    strict_json::serialized_size(&source, 32 << 20)?;
    check()?;
    Ok(Prepared { source })
}
/// Complete ordered face allocations use retained coefficients and source cash;
/// zero participation preserves full unweighted source evidence.
pub fn evaluate(p: &Prepared, allocation: &[Allocation]) -> Result<Value, String> {
    evaluate_retained(p, allocation, true)
}
/// Interactive evaluation reduces immutable prepared tensors only. Full source
/// cash replay is reserved for preparation/publication and solver acceptance.
pub fn evaluate_coefficients(p: &Prepared, allocation: &[Allocation]) -> Result<Value, String> {
    evaluate_retained(p, allocation, false)
}
fn evaluate_retained(
    p: &Prepared,
    allocation: &[Allocation],
    replay_cash: bool,
) -> Result<Value, String> {
    check()?;
    let r = &p.source;
    let [s, n, m, h] = r.shape;
    if allocation.len() != n {
        return Err("complete commercial ordered allocation required".into());
    }
    let mut faces = Vec::new();
    let mut face = 0.;
    let mut cost = 0.;
    for (a, u) in allocation.iter().zip(&r.units) {
        if a.unit_id != u.unit_id || money(a.face)? < 0. {
            return Err("commercial allocation identity/order/domain mismatch".into());
        }
        faces.push(a.face);
        face = money(face + a.face)?;
        cost = money(cost + a.face * u.quote_price_per_face_unit)?;
    }
    let mut scenarios = Vec::new();
    let mut worst = f64::INFINITY;
    for si in 0..s {
        check()?;
        let mut values = vec![0.; m * h];
        for (ui, x) in faces.iter().enumerate() {
            for mi in 0..m {
                for hi in 0..h {
                    let ix = mi * h + hi;
                    values[ix] =
                        money(values[ix] + x * r.values[((si * n + ui) * m + mi) * h + hi])?;
                }
            }
        }
        let nii = money(values[..h].iter().sum())?;
        worst = worst.min(nii);
        let f = r.funding.ordinals.len();
        let mut cash = Vec::new();
        let mut total = 0.;
        for (di, day) in r.funding.ordinals.iter().enumerate() {
            let expected = faces.iter().enumerate().try_fold(0., |sum, (ui, x)| {
                money(sum + x * r.funding.demand[(si * n + ui) * f + di])
            })?;
            if !replay_cash {
                cash.push(json!({"ordinal":day,"cumulative_bank_cash":-expected,"funding_demand":expected}));
                continue;
            }
            let mut purchase = 0.;
            let mut principal = 0.;
            let mut interest = 0.;
            let mut fees = 0.;
            for e in r
                .funding
                .cash_events
                .iter()
                .filter(|e| e.scenario_id == r.scenario_ids[si] && e.ordinal == *day)
            {
                let x = faces[e.unit_index];
                purchase = money(purchase + x * e.purchase_cash)?;
                principal = money(principal + x * e.principal_cash)?;
                interest = money(interest + x * e.interest_cash)?;
                fees = money(fees + x * e.fee_cash)?;
            }
            total = money(total + purchase + principal + interest + fees)?;
            if !near(expected, -total) {
                return Err("commercial retained funding/source cash replay mismatch".into());
            }
            cash.push(json!({"ordinal":day,"purchase_cash":purchase,"principal_cash":principal,"interest_cash":interest,"fee_cash":fees,"cumulative_bank_cash":total,"funding_demand":expected}));
        }
        scenarios.push(json!({"scenario_id":r.scenario_ids[si],"shape":[m,h],"values":values,"incremental_external_nii":nii,"cash":cash}));
    }
    check()?;
    Ok(
        json!({"schema":"commercial-strategy-replay-1","units":r.units,"allocated_face":faces,"total_asset_face":face,"total_asset_purchase_cost":cost,"worst_incremental_external_nii":worst,"scenarios":scenarios,"source_cash_replayed":replay_cash,"scope":"retained native financial coefficients; full source cash replay only at solver/build acceptance; no repricing or credit/accounting execution"}),
    )
}
