//! Selected new-route wire domains. Historical shared Serde parsers are unchanged.
//! Run on duplicate-preserving original JSON before typed preparation/generation.
use portfolio_compute_control::checkpoint;
use serde_json::Value;

const MEASURE: &[&str] = &["contractual", "pricing", "physical"];
const DAY_COUNT: &[&str] = &["act360", "act365_fixed", "act_act_isda"];
const TIMING: &[&str] = &["before_scheduled_payments", "after_scheduled_payments"];

fn field(v: &Value, key: &str, path: &str, domain: &[&str]) -> Result<(), String> {
    checkpoint()?;
    match v.get(key).and_then(Value::as_str) {
        Some(s) if domain.contains(&s) => Ok(()),
        _ => Err(format!(
            "{path}.{key} must be an explicit enum string in {}",
            domain.join(",")
        )),
    }
}
fn each(
    v: &Value,
    key: &str,
    path: &str,
    f: impl Fn(&Value, &str) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(rows) = v.get(key).and_then(Value::as_array) {
        for (i, row) in rows.iter().enumerate() {
            checkpoint()?;
            f(row, &format!("{path}.{key}[{i}]"))?;
        }
    }
    // Missing/non-array containers are refused by the subsequent typed parser.
    Ok(())
}
fn observations(v: &Value, path: &str) -> Result<(), String> {
    each(v, "observations", path, |o, p| {
        let s = &o["source"];
        let p = format!("{p}.source");
        field(s, "kind", &p, &["observed", "conditional_projection"])?;
        if s["kind"] == "conditional_projection" {
            field(s, "measure", &p, MEASURE)?;
        }
        Ok(())
    })
}
fn swap(v: &Value, path: &str) -> Result<(), String> {
    field(v, "side", path, &["payer", "receiver"])?;
    field(v, "payment_timing", path, TIMING)?;
    each(v, "fixed_periods", path, |v, p| {
        field(v, "day_count", p, DAY_COUNT)
    })?;
    each(v, "floating_periods", path, |v, p| {
        field(v, "day_count", p, DAY_COUNT)?;
        let c = &v["coupon"];
        let cp = format!("{p}.coupon");
        field(c, "kind", &cp, &["fixed", "indexed"])?;
        if c["kind"] == "indexed" {
            field(
                &c["index"],
                "kind",
                &format!("{cp}.index"),
                &["term", "overnight", "published_average"],
            )?;
            field(
                c,
                "calculation",
                &cp,
                &[
                    "term_simple",
                    "daily_simple",
                    "daily_compounded",
                    "balance_compounded",
                ],
            )?;
            field(c, "margin_treatment", &cp, &["simple", "compounded"])?;
            observations(c, &cp)?;
        }
        Ok(())
    })
}
fn option(v: &Value, path: &str) -> Result<(), String> {
    field(v, "measure", path, MEASURE)?;
    field(v, "side", path, &["long", "short"])?;
    field(
        v,
        "settlement",
        path,
        &["collateralized_cash_price", "physical_delivery"],
    )?;
    each(v, "states", path, |v, p| {
        swap(&v["underlying"], &format!("{p}.underlying"))
    })
}
fn trade(v: &Value, path: &str) -> Result<(), String> {
    field(
        v,
        "kind",
        path,
        &["swap", "expired_european", "european_swaption"],
    )?;
    match v["kind"].as_str() {
        Some("swap") => swap(&v["request"], &format!("{path}.request")),
        Some("european_swaption") => option(&v["request"], &format!("{path}.request")),
        _ => {
            option(&v["terminal"], &format!("{path}.terminal"))?;
            if v.get("delivered_underlying").is_some_and(|v| !v.is_null()) {
                swap(
                    &v["delivered_underlying"],
                    &format!("{path}.delivered_underlying"),
                )?;
            }
            Ok(())
        }
    }
}
fn derivative(v: &Value, path: &str) -> Result<(), String> {
    let op = &v["operation"];
    let p = format!("{path}.operation");
    field(op, "kind", &p, &["swap", "european_swaption", "netting"])?;
    let r = &op["request"];
    let p = format!("{p}.request");
    match op["kind"].as_str() {
        Some("swap") => swap(r, &p),
        Some("european_swaption") => option(r, &p),
        _ => {
            field(r, "measure", &p, MEASURE)?;
            field(
                &r["policy"],
                "collateral_interest_day_count",
                &format!("{p}.policy"),
                DAY_COUNT,
            )?;
            each(r, "paths", &p, |v, p| {
                each(v, "nodes", p, |v, p| {
                    field(v, "payment_timing", p, TIMING)?;
                    each(v, "trades", p, trade)
                })?;
                if let Some(mpor) = v
                    .get("default")
                    .and_then(|d| d.get("mpor"))
                    .filter(|m| !m.is_null())
                {
                    let mp = format!("{p}.default.mpor");
                    field(mpor, "claim_measure", &mp, MEASURE)?;
                    field(mpor, "carry_policy", &mp, &["nominal_no_interest"])?;
                    field(
                        mpor,
                        "exercise_rights_policy",
                        &mp,
                        &["preserve_contractual_european_to_closeout"],
                    )?;
                    each(mpor, "physical_index_paths", &mp, observations)?;
                    each(mpor, "settlement_instructions", &mp, |v, p| {
                        field(
                            &v["disposition"],
                            "kind",
                            &format!("{p}.disposition"),
                            &["retain_to_closeout", "settle_on_node"],
                        )
                    })?;
                }
                Ok(())
            })
        }
    }
}
fn saved(v: &Value, path: &str) -> Result<(), String> {
    each(&v["book"], "trades", &format!("{path}.book"), trade)?;
    each(
        &v["book"],
        "legacy_hedges",
        &format!("{path}.book"),
        |v, p| field(v, "side", p, &["payer", "receiver"]),
    )?;
    let physical = &v["physical"];
    let p = format!("{path}.physical");
    field(physical, "measure", &p, MEASURE)?;
    field(physical, "basis", &p, &["modeled_physical_scenario"])?;
    let policy = &physical["policy"];
    let pp = format!("{p}.policy");
    field(policy, "custody", &pp, &["segregated_custodian_memo_only"])?;
    field(
        policy,
        "default",
        &pp,
        &["reviewed_modeled_closeout_shortfall_in_earnings"],
    )?;
    for key in ["xva", "modeled_funding_cost"] {
        field(policy, key, &pp, &["report_only"])?;
    }
    each(policy, "trades", &pp, |v, p| {
        field(
            v,
            "designation",
            p,
            &[
                "undesignated_trading",
                "undesignated_economic_hedge",
                "fair_value_hedge",
                "cash_flow_hedge",
                "net_investment_hedge",
            ],
        )?;
        field(
            v,
            "mark_recognition",
            p,
            &["clean_value_changes_in_earnings"],
        )?;
        field(
            v,
            "coupon_recognition",
            p,
            &[
                "settle_derivative_carrying",
                "contractual_income_at_due_with_mark_offset",
            ],
        )
    })?;
    derivative(&physical["derivative"], &format!("{p}.derivative"))?;
    each(physical, "physical_index_paths", &p, observations)?;
    if physical
        .get("pricing_xva_reference")
        .is_some_and(|v| !v.is_null())
    {
        derivative(
            &physical["pricing_xva_reference"],
            &format!("{p}.pricing_xva_reference"),
        )?;
    }
    Ok(())
}
pub(crate) fn validate_owners(v: &Value) -> Result<(), String> {
    each(v, "owners", "request", |v, p| {
        each(v, "runs", p, |v, p| {
            saved(&v["saved"], &format!("{p}.saved"))
        })
    })
}
pub(super) fn validate(v: &Value) -> Result<(), String> {
    field(
        &v["dated"]["recognition_policy"],
        "fees",
        "dated.recognition_policy",
        &["immediate_earnings"],
    )?;
    let d = &v["dated"]["dated"];
    each(d, "owners", "dated.dated", |v, p| {
        field(v, "direction", p, &["receipt", "payment"])?;
        field(
            v,
            "ownership",
            p,
            &["contractual_dated_native_state", "fully_scenario_generated"],
        )
    })?;
    each(d, "sources", "dated.dated", |v, p| {
        each(
            &v["schedule"],
            "events",
            &format!("{p}.schedule"),
            |v, p| field(v, "measure", p, MEASURE),
        )
    })?;
    validate_owners(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw() -> Value {
        serde_json::from_str(include_str!(
            "../../../../tests/fixtures/derivative-mixed.json"
        ))
        .unwrap()
    }
    #[test]
    fn documented_strings_accept_without_projection() {
        validate(&raw()).unwrap();
    }
    #[test]
    fn historical_shared_parser_is_unchanged() {
        let old: portfolio_model_core::cashflow::CashflowMeasure =
            serde_json::from_value(serde_json::json!({"physical":null})).unwrap();
        assert_eq!(
            old,
            portfolio_model_core::cashflow::CashflowMeasure::Physical
        );
        let mut selected = raw();
        selected["owners"][0]["runs"][0]["saved"]["physical"]["measure"] =
            serde_json::json!({"physical":null});
        assert!(validate(&selected).is_err());
    }
    #[test]
    fn aliases_null_and_unknown_refuse_before_typed_projection() {
        let paths = [
            "/owners/0/runs/0/saved/physical/measure",
            "/owners/0/runs/0/saved/physical/basis",
            "/owners/0/runs/0/saved/physical/policy/custody",
            "/owners/0/runs/0/saved/physical/policy/trades/0/designation",
            "/owners/0/runs/0/saved/physical/derivative/operation/request/measure",
            "/owners/0/runs/0/saved/book/trades/0/request/side",
            "/dated/recognition_policy/fees",
            "/dated/dated/owners/0/direction",
        ];
        for p in paths {
            for form in 0..4 {
                let mut v = raw();
                let selected = v.pointer_mut(p).unwrap();
                let name = selected.as_str().unwrap().to_owned();
                *selected = match form {
                    0 => serde_json::json!({name:null}),
                    1 => Value::Null,
                    2 => serde_json::json!(false),
                    _ => serde_json::json!("unknown"),
                };
                assert!(
                    validate(&v).unwrap_err().contains("explicit enum string"),
                    "{p}"
                );
            }
        }
    }
}
