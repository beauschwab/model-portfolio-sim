//! Reviewed actual-purchase performing product units; no legacy unit replacement.
mod admission;
mod replay;
mod types;
use crate::{advanced_graph, card_graph, deposit_graph, mortgage_graph};
use portfolio_model_core::cashflow::validate_ordinal;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
pub use types::*;
pub const SCHEMA: &str = "product-units-1";
pub const MAX_WORK: usize = 32_000_000;
pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_VALUES: usize = 1_000_000;
pub const MAX_EVENTS: usize = 250_000;
impl GraphRequest {
    fn product(&self) -> &'static str {
        match self {
            Self::ConsumerInstallment(_) => "consumer_installment",
            Self::Card(_) => "card",
            Self::Mortgage(_) => "mortgage",
            Self::Deposit(_) => "deposit",
        }
    }
    fn raw(&self) -> Result<Value, String> {
        match self {
            Self::ConsumerInstallment(r) => serde_json::to_value(r),
            Self::Card(r) => serde_json::to_value(r),
            Self::Mortgage(r) => serde_json::to_value(r),
            Self::Deposit(r) => serde_json::to_value(r),
        }
        .map_err(|e| e.to_string())
    }
    fn work(&self) -> Result<usize, String> {
        match self {
            Self::ConsumerInstallment(r) => advanced_graph::audit_admitted_work(r),
            Self::Card(r) => card_graph::audit_admitted_work(r),
            Self::Mortgage(r) => mortgage_graph::audit_admitted_work(r),
            Self::Deposit(r) => deposit_graph::audit_admitted_work(r),
        }
    }
    fn run(&self) -> Result<Value, String> {
        match self {
            Self::ConsumerInstallment(r) => serde_json::to_value(advanced_graph::analyze(r)?),
            Self::Card(r) => serde_json::to_value(card_graph::analyze(r)?),
            Self::Mortgage(r) => serde_json::to_value(mortgage_graph::analyze(r)?),
            Self::Deposit(r) => serde_json::to_value(deposit_graph::analyze(r)?),
        }
        .map_err(|e| e.to_string())
    }
}
pub fn prepare(r: &ProductUnitsRequest) -> Result<ProductUnitsResult, String> {
    Ok(prepare_reusing(r, None)?.0)
}

/// Retained sources enter only through the native strategy's private Prepared.
/// Full admission still covers the complete candidate, including reused units.
pub(crate) fn prepare_reusing(
    r: &ProductUnitsRequest,
    previous: Option<&ProductUnitsResult>,
) -> Result<(ProductUnitsResult, usize, bool), String> {
    let (scenarios, work) = admission::validate(r)?;
    let h = r.reporting_ordinals.len() - 1;
    let n = r.units.len();
    let s = scenarios.len();
    let mut values = vec![0.; s * n * 10 * h];
    let mut receipts = Vec::with_capacity(n);
    let mut cash = Vec::new();
    let mut retained = admission::retained_size(r)?.saturating_add(values.capacity() * 8);
    let previous = match previous {
        Some(p) => {
            let old = shared_context(&p.raw_source);
            let new = shared_context(r);
            if old == new && p.shape == [s, n, 10, h] && p.scenario_ids == scenarios {
                Some(p)
            } else {
                None
            }
        }
        None => None,
    };
    let full_rebuild = previous.is_none();
    let mut units_rebuilt = 0;
    for (index, u) in r.units.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        let cash_start = cash.len();
        if let Some(p) = previous {
            if serde_json::to_value(u).map_err(|e| e.to_string())?
                == serde_json::to_value(&p.raw_source.units[index]).map_err(|e| e.to_string())?
            {
                for si in 0..s {
                    let at = (si * n + index) * 10 * h;
                    values[at..at + 10 * h].copy_from_slice(&p.values[at..at + 10 * h]);
                }
                receipts.push(p.units[index].clone());
                cash.extend(
                    p.funding
                        .cash_events
                        .iter()
                        .filter(|e| e.unit_index == index)
                        .cloned(),
                );
                retained = retained
                    .saturating_add(admission::retained_size(receipts.last().unwrap())?)
                    .saturating_add(admission::retained_size(&cash[cash_start..])?);
                if cash.len() > MAX_EVENTS || retained > MAX_RETAINED_BYTES {
                    return fail("aggregate retained product source admission exceeded");
                }
                continue;
            }
        }
        units_rebuilt += 1;
        let raw = u.purchase_graph.raw()?;
        let result = u.purchase_graph.run()?;
        let product = u.purchase_graph.product();
        let side = if product == "deposit" { -1. } else { 1. };
        let locked = raw["positions"][0]["oas_target"]["base_oas"]
            .as_f64()
            .ok_or("missing frozen purchase OAS")?;
        let sources = result["native_sources"]
            .as_array()
            .ok_or("missing native unit sources")?;
        let quote = admission::money(u.quote_price_per_face_unit * u.unit_face)?;
        let base_metric = result["metrics"]
            .as_array()
            .ok_or("unit base metrics missing")?
            .iter()
            .find(|m| m["scenario_id"] == r.base_scenario_id)
            .ok_or("unit base metric missing")?;
        let base_price = base_metric[if product == "deposit" {
            "liability_pv"
        } else {
            "pv"
        }]
        .as_f64()
        .ok_or("base purchase price missing")?;
        if (base_price - quote).abs() > 1e-10 * quote.abs().max(1.) {
            return fail("locked original purchase OAS does not reconcile base pricing cash with actual purchase quote");
        }
        let original_rights = replay::rights(sources, product, 1.)?;
        let mut purchase_branches = BTreeMap::new();
        let mut incomes = Vec::new();
        let mut marks = Vec::new();
        for (scenario_index, id) in scenarios.iter().enumerate() {
            cash.push(FundingEvent {
                scenario_id: id.clone(),
                unit_index: index,
                path_id: "purchase".into(),
                source_event_id: format!("purchase:{}", u.unit_id),
                ordinal: u.purchase_ordinal,
                source_weight: 1.,
                purchase_cash: -side * u.quote_price_per_face_unit,
                principal_cash: 0.,
                interest_cash: 0.,
                fee_cash: 0.,
                bank_cash: -side * u.quote_price_per_face_unit,
            });
            for source in sources
                .iter()
                .filter(|p| p["role"] == "physical" && p["scenario_id"] == *id)
            {
                portfolio_compute_control::checkpoint()?;
                let path = source["path_id"]
                    .as_str()
                    .ok_or("unit path identity missing")?;
                let weight = source["weight"]
                    .as_f64()
                    .ok_or("unit path weight missing")?;
                let projection = &source["projection"];
                let opening = replay::opening(&raw, product)?;
                let (income, states) =
                    replay::income(u, projection, product, opening, &r.reporting_ordinals)?;
                for (period, pair) in states.windows(2).enumerate() {
                    if r.reporting_ordinals[period] < u.purchase_ordinal {
                        continue;
                    }
                    let effective =
                        pair[1].effective_interest_to_date - pair[0].effective_interest_to_date;
                    let principal = pair[0].principal / u.unit_face;
                    let at = ((scenario_index * n + index) * 10) * h + period;
                    let metrics = [
                        side * effective / u.unit_face,
                        principal,
                        0.,
                        u.weights.hqla_l2a * principal,
                        u.weights.outflow30 * principal,
                        u.weights.asf * principal,
                        u.weights.rsf * principal,
                        u.weights.rwa * principal,
                        if side > 0. { principal } else { 0. },
                        if side < 0. { principal } else { 0. },
                    ];
                    for (m, x) in metrics.into_iter().enumerate() {
                        values[at + m * h] = admission::money(values[at + m * h] + weight * x)?;
                    }
                }
                incomes.push(IncomeReceipt {
                    scenario_id: id.clone(),
                    path_id: path.into(),
                    source_weight: weight,
                    opening_states: states,
                    native_income: income,
                });
                for event in replay::cash(projection, product)? {
                    let principal = side * event.principal * weight / u.unit_face;
                    let interest = side * event.interest * weight / u.unit_face;
                    cash.push(FundingEvent {
                        scenario_id: id.clone(),
                        unit_index: index,
                        path_id: path.into(),
                        source_event_id: event.event_id,
                        ordinal: event.ordinal,
                        source_weight: weight,
                        purchase_cash: 0.,
                        principal_cash: principal,
                        interest_cash: interest,
                        fee_cash: 0.,
                        bank_cash: admission::money(principal + interest)?,
                    });
                }
                for (period, &day) in r.reporting_ordinals[..h]
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| **d >= u.purchase_ordinal)
                {
                    let branch = u
                        .age_branches
                        .iter()
                        .find(|b| {
                            b.ordinal == day && b.scenario_id == *id && b.physical_path_id == path
                        })
                        .ok_or("missing exact physical conditioned age branch")?;
                    let branch_raw = branch.graph.raw()?;
                    let branch_result = branch.graph.run()?;
                    replay::check_prefix(&raw, &branch_raw, source, &branch_result, product, day)?;
                    if day == u.purchase_ordinal {
                        let branch_sources = branch_result["native_sources"]
                            .as_array()
                            .ok_or("purchase branch sources absent")?;
                        for (key, value) in replay::rights(branch_sources, product, weight)? {
                            let old = purchase_branches.get(&key).copied().unwrap_or(0.);
                            purchase_branches.insert(key, admission::money(old + value)?);
                        }
                    }
                    let scenario = &branch_raw["scenarios"][0];
                    let curve = serde_json::from_value(scenario["collateral_curve"].clone())
                        .map_err(|e| e.to_string())?;
                    let bump = branch_raw["bump_bp"].as_f64().ok_or("unit bump missing")?;
                    let mut pv = 0.;
                    let mut down = 0.;
                    let mut up = 0.;
                    for p in branch_result["native_sources"]
                        .as_array()
                        .ok_or("unit age sources missing")?
                    {
                        let role = p["role"].as_str().ok_or("unit age role missing")?;
                        if role == "physical" {
                            continue;
                        }
                        let w = p["weight"].as_f64().ok_or("age weight missing")?;
                        let spread = if role == "pricing_down" {
                            -bump * 1e-4
                        } else if role == "pricing_up" {
                            bump * 1e-4
                        } else {
                            0.
                        };
                        let value = replay::price(
                            &p["projection"],
                            product,
                            u.purchase_ordinal,
                            day,
                            &curve,
                            locked,
                            spread,
                        )?;
                        match role {
                            "pricing" => pv = admission::money(pv + w * value)?,
                            "pricing_down" => down = admission::money(down + w * value)?,
                            "pricing_up" => up = admission::money(up + w * value)?,
                            _ => return Err("unknown age role".into()),
                        }
                    }
                    let dv = (down - up) / (2. * bump);
                    let offset = ((scenario_index * n + index) * 10 + 2) * h + period;
                    values[offset] = admission::money(values[offset] + weight * dv / u.unit_face)?;
                    marks.push(AgeReceipt {
                        ordinal: day,
                        scenario_id: id.clone(),
                        physical_path_id: path.into(),
                        physical_weight: weight,
                        pv_per_face: pv / u.unit_face,
                        dv01_per_face: dv / u.unit_face,
                        locked_purchase_oas: locked,
                        complete_native_result: branch_result,
                    });
                }
            }
        }
        replay::same_rights(&original_rights, &purchase_branches)?;
        if cash.len() > MAX_EVENTS {
            return Err("product-unit full cash event admission exceeded".into());
        }
        receipts.push(UnitReceipt {
            unit_id: u.unit_id.clone(),
            template_id: u.template_id.clone(),
            purchase_ordinal: u.purchase_ordinal,
            unit_face: u.unit_face,
            side,
            quote_price_per_face_unit: u.quote_price_per_face_unit,
            locked_purchase_oas: locked,
            accounting_category: u.accounting_category.clone(),
            capital_policy_revision: u.capital_policy_revision.clone(),
            complete_purchase_result: result,
            income: incomes,
            age_marks: marks,
        });
        retained = retained
            .saturating_add(admission::retained_size(receipts.last().unwrap())?)
            .saturating_add(admission::retained_size(&cash[cash_start..])?);
        if retained > MAX_RETAINED_BYTES {
            return fail("aggregate retained product source admission exceeded");
        }
    }
    let funding = funding(cash, scenarios.clone(), n, &r.reporting_ordinals)?;
    let out=ProductUnitsResult{schema:SCHEMA.into(),identity:r.identity.clone(),raw_source:r.clone(),currency:r.currency.clone(),monetary_unit:r.monetary_unit.clone(),base_scenario_id:r.base_scenario_id.clone(),scenario_ids:scenarios,reporting_ordinals:r.reporting_ordinals.clone(),shape:[s,n,10,h],metrics:crate::aged_units::METRICS.iter().map(|s|(*s).into()).collect(),values,units:receipts,funding,admitted_work:work,income_policy:"reviewed frozen original annual effective yield on native physical repayment/runoff cash; gross contractual recognition remains separate; no default/draw/nonaccrual/migration/new funding/modification/fee transitions".into(),scope:"actual supplied purchase and exact physical-prefix conditioned supplied pricing histories; coefficient preparation only, no solver, daily candidate acceptance, empirical calibration or 60000-position acceptance".into()};
    admission::retained(&out)?;
    portfolio_compute_control::checkpoint()?;
    Ok((out, units_rebuilt, full_rebuild))
}

fn shared_context(r: &ProductUnitsRequest) -> Value {
    // Exhaustive destructuring forces future shared request fields into this
    // invalidation contract. Do not clone every retained raw unit just to
    // compare the small shared context.
    let ProductUnitsRequest {
        schema,
        identity,
        currency,
        monetary_unit,
        reporting_ordinals,
        base_scenario_id,
        units: _,
    } = r;
    json!({"schema":schema,"identity":identity,"currency":currency,
        "monetary_unit":monetary_unit,"reporting_ordinals":reporting_ordinals,
        "base_scenario_id":base_scenario_id})
}
fn funding(
    mut events: Vec<FundingEvent>,
    scenarios: Vec<String>,
    n: usize,
    reporting: &[i32],
) -> Result<ProductFunding, String> {
    let mut dates: BTreeSet<i32> = reporting.iter().copied().collect();
    for e in &events {
        validate_ordinal(e.ordinal)?;
        dates.insert(e.ordinal);
    }
    let ordinals: Vec<_> = dates.into_iter().collect();
    let f = ordinals.len();
    let count = scenarios
        .len()
        .checked_mul(n)
        .and_then(|v| v.checked_mul(f))
        .ok_or("unit funding shape overflow")?;
    if count > MAX_VALUES || events.len() > MAX_EVENTS {
        return Err("product unit fulltail funding shape admission exceeded".into());
    }
    let mut demand = vec![0.; count];
    // Stable sort retains input path/event order for exact same-day source reduction.
    events.sort_by_key(|e| e.ordinal);
    let mut balances = vec![0.; scenarios.len() * n];
    let mut cursor = 0;
    let map: BTreeMap<_, _> = scenarios
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    for (d, &day) in ordinals.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        while cursor < events.len() && events[cursor].ordinal <= day {
            let e = &events[cursor];
            let i = map[e.scenario_id.as_str()] * n + e.unit_index;
            balances[i] = admission::money(balances[i] + e.bank_cash)?;
            cursor += 1;
        }
        for (i, &cash) in balances.iter().enumerate() {
            demand[i * f + d] = -cash;
        }
    }
    Ok(ProductFunding{scenario_ids:scenarios,ordinals,shape:[balances.len()/n,n,f],demand,cash_events:events,policy:"scenario-specific native physical cash including full purchase/repayment/runoff tails; gross pretax end-of-day netting; no taxes, distributions or discretionary reinvestment".into()})
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    v[key]
        .as_array()
        .ok_or_else(|| format!("unit {key} array missing"))
}
fn integer(v: &Value) -> Result<i32, String> {
    let i = v.as_i64().ok_or("unit date must be integer")?;
    i32::try_from(i).map_err(|_| "unit date overflow".into())
}
fn histories<'a>(v: &'a Value, product: &str) -> Result<&'a Vec<Value>, String> {
    array(
        if product == "consumer_installment" {
            &v["positions"][0]["product"]
        } else {
            &v["positions"][0]
        },
        "histories",
    )
}
fn contract<'a>(v: &'a Value, product: &str) -> &'a Value {
    if product == "consumer_installment" {
        &v["positions"][0]["product"]["contract"]
    } else {
        &v["positions"][0]["contract"]
    }
}
fn fail<T>(message: &str) -> Result<T, String> {
    Err(format!("product units: {message}"))
}

/// Pre-generation full typed source admission, returning exact charged work.
pub fn audit_source_work(r: &ProductUnitsRequest) -> Result<usize, String> {
    admission::validate(r).map(|v| v.1)
}
