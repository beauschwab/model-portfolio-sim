use super::*;
use portfolio_model_core::limits::MAX_MONETARY_AMOUNT;
use serde::Serialize;
use std::io::{self, Write};
pub(super) fn money(x: f64) -> Result<f64, String> {
    if x.is_finite() && x.abs() <= MAX_MONETARY_AMOUNT {
        Ok(x)
    } else {
        fail("finite bounded signed money required")
    }
}
fn label(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        fail("bounded source identity required")
    } else {
        Ok(())
    }
}
struct Count {
    bytes: usize,
    limit: usize,
}
impl Write for Count {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        self.bytes = self.bytes.saturating_add(b.len());
        if self.bytes > self.limit {
            return Err(io::Error::other("product unit byte admission exceeded"));
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn size<T: Serialize + ?Sized>(v: &T, limit: usize) -> Result<usize, String> {
    let mut c = Count { bytes: 0, limit };
    serde_json::to_writer(&mut c, v).map_err(|e| e.to_string())?;
    Ok(c.bytes)
}
pub(super) fn retained<T: Serialize>(v: &T) -> Result<(), String> {
    if retained_size(v)? > MAX_RETAINED_BYTES {
        fail("conservative retained bytes exceeded")
    } else {
        Ok(())
    }
}
pub(super) fn retained_size<T: Serialize + ?Sized>(v: &T) -> Result<usize, String> {
    Ok(size(v, MAX_RETAINED_BYTES)?.saturating_mul(4))
}
fn zero(v: &Value) -> bool {
    match v {
        Value::Number(n) => n.as_f64() == Some(0.),
        Value::Array(a) => a.iter().all(zero),
        _ => false,
    }
}
fn opening_component(v: &Value) -> Result<f64, String> {
    let amount = money(v.as_f64().ok_or("unit opening principal missing")?)?;
    if amount < 0. {
        return fail("nonnegative unit opening principal required");
    }
    Ok(amount)
}
fn opening_total(mut values: impl Iterator<Item = Result<f64, String>>) -> Result<f64, String> {
    values.try_fold(0., |total, value| money(total + value?))
}
fn reconcile_face(face: f64, principal: f64) -> Result<(), String> {
    // Same absolute-plus-relative monetary tolerance as independent amount replay;
    // permits summation roundoff, never an arbitrary notional normalization.
    if (face - principal).abs() > 1e-9 + 1e-12 * principal.abs() {
        return fail("unit_face must equal contractual opening principal in every history");
    }
    Ok(())
}
fn admit_face(raw: &Value, product: &str, face: f64) -> Result<(), String> {
    if product == "deposit" {
        for set in histories(raw, product)? {
            for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                for history in array(set, role)? {
                    portfolio_compute_control::checkpoint()?;
                    let principal = opening_total(
                        array(&history["request"], "segments")?
                            .iter()
                            .map(|segment| opening_component(&segment["opening_balance"])),
                    )?;
                    reconcile_face(face, principal)?;
                }
            }
        }
    } else {
        let c = contract(raw, product);
        let principal = if product == "consumer_installment" {
            opening_component(&c["opening_principal"])?
        } else {
            let field = if product == "card" {
                "opening_principal"
            } else {
                "opening_investor_principal"
            };
            opening_total(array(c, field)?.iter().map(opening_component))?
        };
        reconcile_face(face, principal)?;
    }
    Ok(())
}
fn steps(v: &Value, day: i32, card: bool) -> Result<(), String> {
    for step in array(v, "steps")? {
        if integer(&step["ordinal"])? <= day {
            return fail("same-asof actions require separately reconciled opening ownership");
        }
        let a = &step["action"];
        let kind = a["kind"].as_str().ok_or("unit action kind missing")?;
        if !(kind == "payment"
            || (!card && kind == "payoff")
            || (card && kind == "set_limit" && zero(&a["amount"])))
        {
            return fail("unsupported draw/default/nonaccrual/fee/modification transition");
        }
    }
    Ok(())
}
pub(super) fn performing(raw: &Value, product: &str) -> Result<(), String> {
    let day = integer(&raw["as_of_ordinal"])?;
    let sets = histories(raw, product)?;
    if product == "deposit" {
        for set in sets {
            for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                for h in array(set, role)? {
                    let r = &h["request"];
                    if !array(r, "migrations")?.is_empty() {
                        return fail("unsupported deposit migration");
                    }
                    for d in array(r, "drivers")? {
                        if !zero(&d["external_inflows"]) {
                            return fail("unsupported deposit new external inflow");
                        }
                    }
                    for seg in array(r, "segments")? {
                        if !zero(&seg["opening_accrued_interest"]) {
                            return fail("zero opening accrual required");
                        }
                        if seg["maturity_ordinal"]
                            .as_i64()
                            .is_some_and(|m| m <= day as i64)
                            || array(seg, "interest_payment_ordinals")?
                                .iter()
                                .any(|d| d.as_i64().is_some_and(|v| v <= day as i64))
                        {
                            return fail(
                                "same-asof deposit settlement requires reconciled opening",
                            );
                        }
                    }
                }
            }
        }
        return Ok(());
    }
    let c = contract(raw, product);
    if product == "consumer_installment" {
        if c["initial_state"] != "performing"
            || !zero(&c["opening_accrued_interest"])
            || !zero(&c["opening_unrecognized_interest"])
            || !zero(&c["opening_fees"])
        {
            return fail("performing zero-accrual/fee opening required");
        }
        for set in sets {
            for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                for h in array(set, role)? {
                    steps(h, day, false)?;
                }
            }
        }
    } else if product == "card" {
        if c["initial_state"] != "performing"
            || c["initial_purchase_grace"] != false
            || c["regain_grace_after_full_statement_payment"] != false
        {
            return fail("card grace/recognition policy requires separate reviewed ownership");
        }
        for name in [
            "opening_billed_interest",
            "opening_unbilled_interest",
            "opening_unrecognized_billed_interest",
            "opening_fees",
            "opening_contingent_purchase_interest",
        ] {
            if !zero(&c[name]) {
                return fail("card zero accrued/fee/contingent opening required");
            }
        }
        for set in sets {
            for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                for h in array(set, role)? {
                    steps(h, day, true)?;
                }
            }
        }
    } else {
        if c["rate"]["kind"] != "fixed"
            || !zero(&c["opening_contractual_interest"])
            || !zero(&c["opening_borrower_interest"])
            || !zero(&c["opening_investor_interest"])
        {
            return fail("mortgage fixed-rate zero accrued opening required; ARM modification ownership open");
        }
        for name in [
            "opening_probability",
            "opening_borrower_principal",
            "opening_investor_principal",
        ] {
            if array(c, name)?.iter().skip(1).any(|v| !zero(v)) {
                return fail("mortgage performing origin only; credit migration ownership open");
            }
        }
        if c["borrower_collection_fraction"][0].as_f64() != Some(1.) {
            return fail("mortgage partial collection/nonaccrual ownership open");
        }
        if c["ownership"]["kind"] == "whole_loan"
            && c["ownership"]["recognized_accrual_fraction"][0].as_f64() != Some(1.)
        {
            return fail("whole-loan recognition fraction needs reviewed income ownership");
        }
        if !zero(&c["refinance"]["incentive_slope"]) {
            return fail("mortgage refinancing/basis retirement ownership open");
        }
        for set in sets {
            for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                for h in array(set, role)? {
                    if !array(h, "arm_resets")?.is_empty() {
                        return fail("mortgage ARM resets require reviewed yield policy");
                    }
                    for p in array(h, "path")? {
                        if !zero(&p["credit_hazards_per_year"])
                            || !zero(&p["turnover_hazard_per_year"])
                        {
                            return fail("mortgage credit/prepay/migration ownership open");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
fn context(raw: &Value) -> Value {
    let mut m = serde_json::Map::new();
    for name in [
        "schema",
        "as_of_ordinal",
        "currency",
        "reporting_ordinals",
        "bump_bp",
        "terminal_policy",
        "tail_policy",
        "cash_policy",
        "cash_timing",
    ] {
        if let Some(v) = raw.get(name) {
            m.insert(name.into(), v.clone());
        }
    }
    let id = &raw["identity"];
    m.insert(
        "pairing_identity".into(),
        json!([
            id["model_revision"],
            id["paired_history_source"],
            id["paired_history_revision"],
            id["randomness_domain"]
        ]),
    );
    m.insert(
        "oas_target".into(),
        raw["positions"][0]["oas_target"].clone(),
    );
    Value::Object(m)
}
// Bound source inspection and income interval/cash cross products, including
// rows which happen to become zero under a supplied path.
fn source_cost(raw: &Value, product: &str, with_income: bool) -> Result<(usize, usize), String> {
    let mut work = 0usize;
    let mut events = 0usize;
    for set in histories(raw, product)? {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for path in array(set, role)? {
                let c = if product == "deposit" {
                    &path["request"]
                } else {
                    contract(raw, product)
                };
                let days = c["horizon_days"].as_u64().ok_or("unit horizon absent")? as usize;
                let (recognition, cash, dates) = if product == "deposit" {
                    let n = array(c, "segments")?.len();
                    (
                        days * n,
                        3 * days * n,
                        days + array(raw, "reporting_ordinals")?.len(),
                    )
                } else if product == "mortgage" {
                    let k = array(c, "payment_dates")?.len();
                    (
                        3 * days,
                        2 * k,
                        days + 2 * k + array(raw, "reporting_ordinals")?.len(),
                    )
                } else if product == "card" {
                    let k = array(path, "steps")?.len() + 1;
                    (
                        3 * days,
                        k,
                        days + k + array(raw, "reporting_ordinals")?.len(),
                    )
                } else {
                    let k = array(c, "payment_dates")?.len() + array(path, "steps")?.len() + 1;
                    (k, k, 2 * k + array(raw, "reporting_ordinals")?.len())
                };
                let scan = (recognition + cash + dates)
                    .checked_mul(64)
                    .ok_or("unit source work overflow")?;
                work = work.checked_add(scan).ok_or("unit source work overflow")?;
                if with_income && role == "physical" {
                    work = work
                        .checked_add(
                            dates
                                .checked_mul(recognition + cash)
                                .and_then(|x| x.checked_mul(4))
                                .ok_or("unit income work overflow")?,
                        )
                        .ok_or("unit income work overflow")?;
                    events = events
                        .checked_add(cash)
                        .ok_or("unit funding event overflow")?;
                }
            }
        }
    }
    Ok((work, events))
}
pub(super) fn validate(r: &ProductUnitsRequest) -> Result<(Vec<String>, usize), String> {
    portfolio_compute_control::checkpoint()?;
    if r.schema != SCHEMA
        || r.monetary_unit != "currency_units"
        || r.units.is_empty()
        || r.units.len() > 256
        || !(2..=121).contains(&r.reporting_ordinals.len())
    {
        return fail("version/units/shape admission required");
    }
    for s in [
        &r.currency,
        &r.base_scenario_id,
        &r.identity.model_revision,
        &r.identity.grid_revision,
        &r.identity.crn_id,
        &r.identity.input_revision,
    ] {
        label(s)?;
    }
    for &d in &r.reporting_ordinals {
        validate_ordinal(d)?;
    }
    if r.reporting_ordinals.windows(2).any(|v| v[0] >= v[1]) {
        return fail("actual reporting dates must strictly increase");
    }
    let input = size(r, MAX_INPUT_BYTES)?;
    let mut work = input.saturating_mul(4);
    let mut ids = BTreeSet::new();
    let mut templates = BTreeSet::new();
    let mut pairs = BTreeSet::new();
    let mut scenario_ids = Vec::new();
    let mut common_scenarios = None;
    let mut full_events = 0usize;
    for u in &r.units {
        portfolio_compute_control::checkpoint()?;
        for s in [
            &u.unit_id,
            &u.template_id,
            &u.accounting_category,
            &u.capital_policy_revision,
            &u.income_policy.review.id,
            &u.income_policy.review.revision,
            &u.income_policy.original_yield_anchor.id,
            &u.income_policy.original_yield_anchor.revision,
        ] {
            label(s)?;
        }
        if !ids.insert(&u.unit_id)
            || !pairs.insert((&u.template_id, u.purchase_ordinal))
            || !r.reporting_ordinals[..r.reporting_ordinals.len() - 1].contains(&u.purchase_ordinal)
            || money(u.unit_face)? <= 0.
            || money(u.quote_price_per_face_unit)? <= 0.
            || !(-0.99..=10.).contains(&u.income_policy.original_annual_effective_rate)
            || u.age_branches.len() > 16384
        {
            return fail("unit purchase/quote/yield/branch admission exceeded");
        }
        templates.insert(&u.template_id);
        money(u.quote_price_per_face_unit * u.unit_face)?;
        if templates.len() > 32 {
            return fail("template admission exceeded");
        }
        for w in [
            u.weights.hqla_l2a,
            u.weights.outflow30,
            u.weights.asf,
            u.weights.rsf,
            u.weights.rwa,
        ] {
            if !w.is_finite() || !(0.0..=100.).contains(&w) {
                return fail("supplied weight outside bounds");
            }
        }
        let raw = u.purchase_graph.raw()?;
        let product = u.purchase_graph.product();
        admit_face(&raw, product, u.unit_face)?;
        if array(&raw, "positions")?.len() != 1
            || raw["positions"][0]["position_id"] != u.unit_id
            || raw["currency"] != r.currency
            || integer(&raw["as_of_ordinal"])? != u.purchase_ordinal
            || raw["base_scenario_id"] != r.base_scenario_id
            || raw["positions"][0]["oas_target"]["kind"] != "locked"
        {
            return fail("single-position actual purchase scope and original locked OAS required");
        }
        let expected: Vec<_> = r
            .reporting_ordinals
            .iter()
            .copied()
            .filter(|d| *d >= u.purchase_ordinal)
            .collect();
        if raw["reporting_ordinals"] != json!(expected) {
            return fail("purchase graph actual report grid mismatch");
        }
        let scenes = array(&raw, "scenarios")?;
        let scene_identity: Vec<_> = scenes
            .iter()
            .map(|s| json!([s["scenario_id"], s["run_id"], s["severity"]]))
            .collect();
        if let Some(ref original) = common_scenarios {
            if original != &scene_identity {
                return fail("unit scenario/market contexts differ");
            }
        } else {
            scenario_ids = scenes
                .iter()
                .map(|s| {
                    s["scenario_id"]
                        .as_str()
                        .map(str::to_owned)
                        .ok_or("scenario identity missing".to_owned())
                })
                .collect::<Result<_, _>>()?;
            common_scenarios = Some(scene_identity);
        }
        performing(&raw, product)?;
        work = work
            .checked_add(u.purchase_graph.work()?)
            .ok_or("unit work overflow")?;
        let (extra, events) = source_cost(&raw, product, true)?;
        work = work.checked_add(extra).ok_or("unit source work overflow")?;
        full_events = full_events
            .checked_add(events + scenes.len())
            .ok_or("unit aggregate funding events overflow")?;
        let sets = histories(&raw, product)?;
        let mut keys = BTreeSet::new();
        for set in sets {
            for p in array(set, "physical")? {
                for &d in &expected[..expected.len() - 1] {
                    keys.insert((
                        d,
                        set["scenario_id"]
                            .as_str()
                            .ok_or("scenario missing")?
                            .to_owned(),
                        p["path_id"]
                            .as_str()
                            .ok_or("physical path missing")?
                            .to_owned(),
                    ));
                }
            }
        }
        if keys.len() != u.age_branches.len() {
            return fail("complete exact physical-path/age/scenario branch coverage required");
        }
        let ctx = context(&raw);
        for b in &u.age_branches {
            if !keys.remove(&(b.ordinal, b.scenario_id.clone(), b.physical_path_id.clone()))
                || b.graph.product() != product
            {
                return fail("duplicate/unknown conditioned branch");
            }
            let br = b.graph.raw()?;
            admit_face(&br, product, u.unit_face)?;
            if context(&br) != ctx
                || array(&br, "positions")?.len() != 1
                || br["positions"][0]["position_id"] != u.unit_id
                || array(&br, "scenarios")?.len() != 1
                || br["scenarios"][0]
                    != *scenes
                        .iter()
                        .find(|s| s["scenario_id"] == b.scenario_id)
                        .ok_or("branch scenario unknown")?
                || br["base_scenario_id"] != b.scenario_id
            {
                return fail("conditioned graph market/OAS/context mismatch");
            }
            let hs = histories(&br, product)?;
            if hs.len() != 1
                || hs[0]["scenario_id"] != b.scenario_id
                || array(&hs[0], "physical")?.len() != 1
                || hs[0]["physical"][0]["path_id"] != b.physical_path_id
                || hs[0]["physical"][0]["weight"].as_f64() != Some(1.)
            {
                return fail("conditioned branch must identify one actual physical path");
            }
            if product != "deposit" && contract(&raw, product) != contract(&br, product) {
                return fail("branch must retain complete original known purchase contract");
            }
            performing(&br, product)?;
            work = work
                .checked_add(b.graph.work()?)
                .ok_or("unit branch work overflow")?;
            work = work
                .checked_add(source_cost(&br, product, false)?.0)
                .ok_or("unit comparison work overflow")?;
            if work > MAX_WORK {
                return fail("aggregate product-unit work exceeded before generation");
            }
        }
    }
    let count = scenario_ids
        .len()
        .checked_mul(r.units.len())
        .and_then(|v| v.checked_mul((r.reporting_ordinals.len() - 1) * 10))
        .ok_or("unit dense shape overflow")?;
    let funding_count = scenario_ids
        .len()
        .checked_mul(r.units.len())
        .and_then(|x| x.checked_mul(full_events + r.reporting_ordinals.len()))
        .ok_or("unit fulltail funding work overflow")?;
    work = work
        .checked_add((count + funding_count) * 8)
        .ok_or("unit output work overflow")?;
    if count > MAX_VALUES
        || funding_count > MAX_VALUES
        || full_events > MAX_EVENTS
        || work > MAX_WORK
    {
        return fail("aggregate product-unit dense/work admission exceeded");
    }
    Ok((scenario_ids, work))
}
