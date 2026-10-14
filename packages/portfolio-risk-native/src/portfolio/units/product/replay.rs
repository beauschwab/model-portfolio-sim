use super::*;
use crate::market::analysis::curves::SuppliedCurve;
use portfolio_model_core::{
    cashflow::CashflowMeasure,
    dated_income::{self, DatedIncomeRequest, Identity, IncomeEvent},
};
pub(super) struct Cash {
    pub event_id: String,
    pub ordinal: i32,
    pub principal: f64,
    pub interest: f64,
}
pub(super) fn cash(p: &Value, product: &str) -> Result<Vec<Cash>, String> {
    let side = if product == "deposit" { -1. } else { 1. };
    let mut out = Vec::new();
    for e in array(p, "cashflows")? {
        let principal = admission::money(
            e["principal"]
                .as_f64()
                .ok_or("native cash principal missing")?
                * side,
        )?;
        let interest = admission::money(
            e[if product == "deposit" {
                "cash_interest"
            } else {
                "interest"
            }]
            .as_f64()
            .ok_or("native cash interest missing")?
                * side,
        )?;
        if principal < 0. || (product != "deposit" && e["fees"].as_f64() != Some(0.)) {
            return fail("draw/fee cash has no admitted income owner");
        }
        out.push(Cash {
            event_id: e["event_id"]
                .as_str()
                .ok_or("native cash source identity missing")?
                .into(),
            ordinal: integer(&e["ordinal"])?,
            principal,
            interest,
        });
    }
    Ok(out)
}
pub(super) fn opening(raw: &Value, product: &str) -> Result<f64, String> {
    let value = if product == "deposit" {
        array(
            &histories(raw, product)?[0]["physical"][0]["request"],
            "segments",
        )?
        .iter()
        .map(|s| {
            s["opening_balance"]
                .as_f64()
                .ok_or("deposit opening missing")
        })
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum()
    } else {
        let c = contract(raw, product);
        if product == "consumer_installment" {
            c["opening_principal"]
                .as_f64()
                .ok_or("consumer opening missing")?
        } else {
            array(
                c,
                if product == "card" {
                    "opening_principal"
                } else {
                    "opening_investor_principal"
                },
            )?
            .iter()
            .map(|v| v.as_f64().ok_or("opening amount missing"))
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .sum()
        }
    };
    if admission::money(value)? <= 0. {
        return fail("positive performing purchase principal required");
    }
    Ok(value)
}
fn earned(p: &Value, product: &str, left: i32, right: i32) -> Result<f64, String> {
    let mut total = 0.;
    for e in array(
        p,
        if product == "deposit" {
            "interest"
        } else {
            "recognition"
        },
    )? {
        let a = integer(&e["start_ordinal"])?;
        let b = integer(&e["end_ordinal"])?;
        let x = e[if product == "deposit" {
            "amount"
        } else {
            "interest"
        }]
        .as_f64()
        .ok_or("native earned source missing")?;
        let overlap = right.min(b) - left.max(a);
        if overlap > 0 {
            if b <= a {
                return fail("recognition source interval must increase");
            }
            total = admission::money(total + x * overlap as f64 / (b - a) as f64)?;
        }
    }
    Ok(total)
}
pub(super) fn income(
    u: &ProductUnit,
    p: &Value,
    product: &str,
    principal: f64,
    reporting: &[i32],
) -> Result<(Value, Vec<IncomeState>), String> {
    let cash = cash(p, product)?;
    let quote = admission::money(u.quote_price_per_face_unit * u.unit_face)?;
    let y = u.income_policy.original_annual_effective_rate;
    let mut pv = 0.;
    let mut full = u.purchase_ordinal;
    let mut dates: BTreeSet<_> = reporting
        .iter()
        .copied()
        .filter(|d| *d >= u.purchase_ordinal)
        .collect();
    for c in &cash {
        if c.ordinal <= u.purchase_ordinal {
            return fail("same-asof action cash needs reconciled opening owner");
        }
        full = full.max(c.ordinal);
        dates.insert(c.ordinal);
        pv = admission::money(
            pv + (c.principal + c.interest)
                * (-y.ln_1p() * (c.ordinal - u.purchase_ordinal) as f64 / 365.).exp(),
        )?;
    }
    for e in array(
        p,
        if product == "deposit" {
            "interest"
        } else {
            "recognition"
        },
    )? {
        for name in ["start_ordinal", "end_ordinal"] {
            let d = integer(&e[name])?;
            if d >= u.purchase_ordinal {
                dates.insert(d);
                full = full.max(d);
            }
        }
    }
    if (pv - quote).abs() > 1e-10 * quote.abs().max(1.) {
        return fail("retained original effective yield does not reconcile native physical full cash to actual purchase quote; no Q-derived yield or implicit modification");
    }
    dates.remove(&u.purchase_ordinal);
    let end = full.max(
        *dates
            .iter()
            .next_back()
            .ok_or("unit income dates missing")?,
    );
    let mut previous = u.purchase_ordinal;
    let mut events = Vec::with_capacity(dates.len());
    let source = Identity {
        id: u.unit_id.clone(),
        revision: u.income_policy.review.revision.clone(),
    };
    for day in dates {
        portfolio_compute_control::checkpoint()?;
        let mut pc = 0.;
        let mut ic = 0.;
        for c in cash.iter().filter(|e| e.ordinal == day) {
            pc = admission::money(pc + c.principal)?;
            ic = admission::money(ic + c.interest)?;
        }
        events.push(IncomeEvent {
            event_id: format!("{}:income:{day}", u.unit_id),
            source: source.clone(),
            payment_ordinal: day,
            contractual_interest_earned: earned(p, product, previous, day)?,
            principal_cash: pc,
            interest_cash: ic,
            integral_fee_cash: 0.,
            nonintegral_fee_cash: 0.,
            yield_change: None,
        });
        previous = day;
    }
    if events.len() > dated_income::MAX_EVENTS {
        return fail("native retained-yield event work admission exceeded");
    }
    let req = DatedIncomeRequest {
        schema: dated_income::SCHEMA.into(),
        contract: source,
        original_yield_anchor: u.income_policy.original_yield_anchor.clone(),
        policy: u.income_policy.review.clone(),
        currency: p["identity"]["currency"]
            .as_str()
            .ok_or("income currency missing")?
            .into(),
        monetary_unit: "currency_units".into(),
        measure: CashflowMeasure::Physical,
        as_of_ordinal: u.purchase_ordinal,
        full_contract_end_ordinal: end,
        opening_principal: principal,
        opening_accrued_interest: 0.,
        opening_gross_carrying_value: quote,
        original_annual_effective_rate: y,
        opening_annual_effective_rate: y,
        events,
    };
    let out = dated_income::evaluate(&req)?;
    let native = serde_json::to_value(&out).map_err(|e| e.to_string())?;
    let rows = array(&native, "rows")?;
    let mut cumulative = 0.;
    let mut map = BTreeMap::new();
    map.insert(
        u.purchase_ordinal,
        IncomeState {
            ordinal: u.purchase_ordinal,
            principal,
            accrued_interest: 0.,
            gross_carrying_value: quote,
            book_adjustment: quote - principal,
            effective_interest_to_date: 0.,
        },
    );
    for row in rows {
        cumulative = admission::money(
            cumulative
                + row["effective_interest"]
                    .as_f64()
                    .ok_or("income effective amount missing")?,
        )?;
        let day = integer(&row["payment_ordinal"])?;
        map.insert(
            day,
            IncomeState {
                ordinal: day,
                principal: row["closing_principal"]
                    .as_f64()
                    .ok_or("income principal missing")?,
                accrued_interest: row["closing_accrued_interest"]
                    .as_f64()
                    .ok_or("income accrued missing")?,
                gross_carrying_value: row["closing_gross_carrying_value"]
                    .as_f64()
                    .ok_or("income carrying missing")?,
                book_adjustment: row["closing_book_adjustment"]
                    .as_f64()
                    .ok_or("income basis missing")?,
                effective_interest_to_date: cumulative,
            },
        );
    }
    let terminal = map.get(&full).ok_or("full cash terminal state missing")?;
    let scale = cash
        .iter()
        .map(|c| c.principal.abs() + c.interest.abs())
        .sum::<f64>()
        .max(1.);
    if terminal.principal.abs() > 1e-10 * scale
        || terminal.accrued_interest.abs() > 1e-10 * scale
        || terminal.gross_carrying_value.abs() > 1e-10 * scale
    {
        return fail("full terminal principal/accrual/book closure fails; reviewed retirement policy required");
    }
    let states = reporting
        .iter()
        .map(|day| {
            if *day < u.purchase_ordinal {
                Ok(IncomeState {
                    ordinal: *day,
                    principal: 0.,
                    accrued_interest: 0.,
                    gross_carrying_value: 0.,
                    book_adjustment: 0.,
                    effective_interest_to_date: 0.,
                })
            } else {
                map.get(day)
                    .cloned()
                    .ok_or("report income state missing".into())
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((
        json!({"raw_income_request":req,"native_income_result":out,"complete_source_projection":p,"full_cash_terminal_ordinal":full}),
        states,
    ))
}
pub(super) fn price(
    p: &Value,
    product: &str,
    purchase: i32,
    day: i32,
    curve: &SuppliedCurve,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    curve.validate()?;
    let anchor = curve
        .value_validated((day - purchase) as f64 / 365.)?
        .discount_factor;
    let mut value = 0.;
    let side = if product == "deposit" { -1. } else { 1. };
    for e in cash(p, product)?.iter().filter(|e| e.ordinal > day) {
        let df = curve
            .value_validated((e.ordinal - purchase) as f64 / 365.)?
            .discount_factor;
        value = admission::money(
            value
                + side * (e.principal + e.interest) * df / anchor
                    * (-(oas + bump) * (e.ordinal - day) as f64 / 365.).exp(),
        )?;
    }
    Ok(value)
}
pub(super) type Rights = BTreeMap<(String, String, i32, &'static str), f64>;
pub(super) fn rights(
    sources: &[Value],
    product: &str,
    outer_weight: f64,
) -> Result<Rights, String> {
    let mut out = BTreeMap::new();
    for source in sources {
        let role = source["role"].as_str().ok_or("source role missing")?;
        if role == "physical" {
            continue;
        }
        let scenario = source["scenario_id"]
            .as_str()
            .ok_or("source scenario missing")?;
        let weight = source["weight"].as_f64().ok_or("source weight missing")? * outer_weight;
        for c in cash(&source["projection"], product)? {
            for (name, amount) in [("principal", c.principal), ("interest", c.interest)] {
                if amount != 0. {
                    let key = (scenario.into(), role.into(), c.ordinal, name);
                    let old = out.get(&key).copied().unwrap_or(0.);
                    out.insert(key, admission::money(old + weight * amount)?);
                }
            }
        }
    }
    Ok(out)
}
pub(super) fn same_rights(a: &Rights, b: &Rights) -> Result<(), String> {
    let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
    for k in keys {
        let x = a.get(k).copied().unwrap_or(0.);
        let y = b.get(k).copied().unwrap_or(0.);
        if (x - y).abs() > 1e-9_f64.max(1e-12 * x.abs().max(y.abs())) {
            return fail(
                "purchase conditioned Q/bump branches change original full pricing cash rights",
            );
        }
    }
    Ok(())
}
fn trim_steps(h: &Value, product: &str, day: i32) -> Value {
    if product == "deposit" {
        let r = &h["request"];
        let mut trimmed = r.clone();
        trimmed["drivers"] = Value::Array(
            r["drivers"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|d| d["ordinal"].as_i64().unwrap() < day as i64)
                .cloned()
                .collect(),
        );
        trimmed["identity"]["measure"] = json!("prefix");
        trimmed["identity"]["assumption_id"] = json!("prefix");
        trimmed["identity"]["assumption_rationale"] = json!("prefix");
        return trimmed;
    }
    if product == "mortgage" {
        return json!({"path":h["path"].as_array().unwrap().iter().filter(|p|p["start_ordinal"].as_i64().unwrap()<day as i64).collect::<Vec<_>>(),"arm_resets":h["arm_resets"].as_array().unwrap().iter().filter(|p|p["ordinal"].as_i64().unwrap()<=day as i64).collect::<Vec<_>>()});
    }
    json!(h["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["ordinal"].as_i64().unwrap() <= day as i64)
        .collect::<Vec<_>>())
}
fn prefix(p: &Value, product: &str, day: i32) -> Result<Value, String> {
    let points: Vec<_> = array(p, "points")?
        .iter()
        .filter(|p| p["ordinal"].as_i64().is_some_and(|d| d <= day as i64))
        .collect();
    let events: Vec<_> = array(p, "cashflows")?
        .iter()
        .filter(|p| p["ordinal"].as_i64().is_some_and(|d| d <= day as i64))
        .collect();
    let entitlements: Vec<_> = p["entitlements"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|p| {
                    p["entitlement_ordinal"]
                        .as_i64()
                        .is_some_and(|d| d <= day as i64)
                })
                .collect()
        })
        .unwrap_or_default();
    let statements: Vec<_> = p["statements"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|p| p["ordinal"].as_i64().is_some_and(|d| d <= day as i64))
                .collect()
        })
        .unwrap_or_default();
    // Installment event points can be grouped differently by future events. Exact past
    // raw steps+known contract prove hidden state; accrued amount is independently integrated.
    let points = if product == "consumer_installment" {
        Vec::new()
    } else {
        points
    };
    let asof = integer(&p["points"][0]["ordinal"])?; // First event point need not be purchase for installments.
    let start = if product == "consumer_installment" {
        array(p, "recognition")?
            .first()
            .map(|r| integer(&r["start_ordinal"]))
            .transpose()?
            .unwrap_or(asof)
    } else {
        asof
    };
    Ok(
        json!({"points":points,"cash":events,"entitlements":entitlements,"statements":statements,"earned_through_age":earned(p,product,start,day)?}),
    )
}
pub(super) fn check_prefix(
    raw: &Value,
    branch: &Value,
    source: &Value,
    result: &Value,
    product: &str,
    day: i32,
) -> Result<(), String> {
    let scenario = source["scenario_id"]
        .as_str()
        .ok_or("source scenario missing")?;
    let path = source["path_id"].as_str().ok_or("source path missing")?;
    let actual = histories(raw, product)?
        .iter()
        .find(|s| s["scenario_id"] == scenario)
        .ok_or("source history absent")?["physical"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["path_id"] == path)
        .ok_or("source path absent")?;
    let branched = &histories(branch, product)?[0];
    let selected = &branched["physical"][0];
    let mut expected = actual.clone();
    expected["weight"] = json!(1.);
    if selected != &expected {
        return fail("branch physical path differs from complete actual native source");
    }
    let expected = prefix(&source["projection"], product, day)?;
    let raw_prefix = trim_steps(actual, product, day);
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        for h in array(branched, role)? {
            if trim_steps(h, product, day) != raw_prefix {
                return fail(
                    "supplied pricing branch changes physical-prefix actions/drivers/policies",
                );
            }
            let p = result["native_sources"]
                .as_array()
                .ok_or("age result sources missing")?
                .iter()
                .find(|p| p["role"] == role && p["path_id"] == h["path_id"])
                .ok_or("age native source identity missing")?;
            if prefix(&p["projection"], product, day)? != expected {
                return fail("native pricing prefix state/cash/accrual/entitlement differs from actual physical age");
            }
        }
    }
    Ok(())
}
