use portfolio_ledger_native::{dated_ledger::CompiledDatedLedger, mortgage_pending_owner as owner};
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/mortgage_mixed_pending.json")).unwrap()
}
fn request(v: Value) -> owner::Request {
    serde_json::from_value(v).unwrap()
}
fn evaluate(v: Value) -> Result<owner::ResultData, String> {
    owner::evaluate(request(v))
}
fn rows<'a>(r: &'a owner::ResultData, table: &str) -> Vec<&'a Value> {
    r.ledger_messages
        .iter()
        .filter(|m| m["table"] == table)
        .flat_map(|m| m["rows"].as_array().unwrap())
        .collect()
}
fn journal(r: &owner::ResultData) -> Vec<Value> {
    let mut out = Vec::new();
    for m in r.ledger_messages.iter().filter(|m| m["table"] == "journal") {
        let c = m["columns"].as_object().unwrap();
        let d = m["dictionary"].as_array().unwrap();
        let n = c["day"].as_array().unwrap().len();
        for i in 0..n {
            let mut v = serde_json::Map::new();
            for (k, a) in c {
                let x = &a[i];
                v.insert(
                    k.clone(),
                    if matches!(
                        k.as_str(),
                        "scenario" | "account" | "event" | "gl_account" | "instrument_id"
                    ) {
                        d[x.as_u64().unwrap() as usize].clone()
                    } else {
                        x.clone()
                    },
                );
            }
            out.push(Value::Object(v));
        }
    }
    out
}
fn preflight(v: Value) -> Result<owner::Admission, String> {
    let r = request(v);
    let d = CompiledDatedLedger::compile(r.dated.clone(), 512, 8_000_000)?;
    owner::audit(&r, &d)
}
#[test]
fn one_source_two_explicit_runs_separate_books_cash_and_earnings() {
    let r = evaluate(raw()).unwrap();
    assert_eq!(r.sources.len(), 1);
    assert_eq!(r.admission.bindings, 2);
    assert_eq!(r.admission.work_units, 7_124_923);
    assert!(r.admission.reserved_bytes < 32 * 1024 * 1024);
    let summaries = rows(&r, "summary");
    assert_eq!(summaries.len(), 2);
    for s in summaries {
        assert!((s["final_cash"].as_f64().unwrap() - 1050.2131168226465).abs() < 1e-9);
        assert!((s["final_equity"].as_f64().unwrap() - 1050.2131168226465).abs() < 1e-9);
    }
    let lineage = rows(&r, "mortgage_lineage");
    assert_eq!(
        lineage
            .iter()
            .filter(|x| x["native_group_index"] == 0)
            .count(),
        2
    );
    assert!(
        !r.production_adoption
            && !r.mixed_ledger_adoption
            && !r.capital_ftp_eligible
            && !r.closing_capital_eligible
    );
}
#[test]
fn interim_exact_gross_active_and_pending_default_are_in_gl() {
    let mut v = raw();
    v["reporting_end_ordinal"] = json!(739622);
    v["sources"][0]["request"]["source"]["contract"]["path"][0]["credit_hazards_per_year"][0][3] =
        json!(0.2);
    let r = evaluate(v).unwrap();
    let books = rows(&r, "mortgage_daily");
    let b = books
        .iter()
        .find(|x| x["scenario"] == "baseline" && x["ordinal"] == 739622)
        .unwrap();
    let slots = b["unweighted_book"]["slots"].as_array().unwrap();
    assert!(slots[3]["principal"].as_f64().unwrap() > 0.);
    assert!(slots[3]["recognized_interest"].as_f64().unwrap() > 0.);
    let journal = journal(&r);
    assert!(journal
        .iter()
        .any(|x| x["gl_account"] == "mortgage_pending_default_principal"));
    assert!(journal
        .iter()
        .any(|x| x["gl_account"] == "credit_default_principal"));
    assert!(rows(&r, "path").iter().any(|x| x["day"] == 5));
    for x in rows(&r, "mortgage_closing") {
        assert!(x["full_tail_complete"].as_bool().unwrap());
    }
}
#[test]
fn prepay_pending_assets_and_reviewed_income_remain_separate() {
    let mut v = raw();
    v["sources"][0]["request"]["source"]["contract"]["path"][0]["turnover_hazard_per_year"] =
        json!(0.2);
    let r = evaluate(v).unwrap();
    let j = journal(&r);
    assert!(j
        .iter()
        .any(|x| x["gl_account"] == "mortgage_pending_prepay_principal"));
    assert!(j
        .iter()
        .any(|x| x["gl_account"] == "pnl:mortgage_reviewed_realization"));
}
#[test]
fn zero_allocation_keeps_complete_source_and_no_monetary_group_transaction() {
    let mut v = raw();
    v["owners"][0]["allocated_face"] = json!(0.);
    let r = evaluate(v).unwrap();
    for s in rows(&r, "summary") {
        assert!((s["final_cash"].as_f64().unwrap() - 1050.05).abs() < 1e-9);
    }
    for l in rows(&r, "mortgage_lineage") {
        assert!(l["transaction_number"].is_null());
    }
    assert!(r.sources[0]
        .native
        .closing
        .borrower_principal
        .iter()
        .all(|x| x.abs() < 1e-9));
}
#[test]
fn half_face_applies_once() {
    let mut v = raw();
    v["owners"][0]["allocated_face"] = json!(50.);
    let r = evaluate(v).unwrap();
    for s in rows(&r, "summary") {
        assert!(
            (s["final_cash"].as_f64().unwrap() - (1050.05 + 0.16311682264652916 * 0.5)).abs()
                < 1e-9
        );
    }
}
#[test]
fn future_purchase_has_no_early_money_or_rights() {
    let mut v = raw();
    for k in ["contract", "original_contract"] {
        let c = &mut v["sources"][0]["request"]["source"][k];
        c["as_of_ordinal"] = json!(739622);
        c["payment_dates"] = json!([739652]);
        c["path"][0]["start_ordinal"] = json!(739622);
        for t in c["complete_recovery_tails"].as_array_mut().unwrap() {
            for (i, x) in t.as_array_mut().unwrap().iter_mut().enumerate() {
                x["lag_days"] = json!(5 + 10 * i);
            }
        }
    }
    let s = &mut v["sources"][0]["request"]["source"];
    s["acquisition"]["purchase_ordinal"] = json!(739622);
    s["policy"]["economic_principal_disposal_lag_days"] = json!(15);
    let r = evaluate(v).unwrap();
    for b in rows(&r, "mortgage_daily")
        .into_iter()
        .filter(|b| b["ordinal"].as_i64().unwrap() < 739622)
    {
        assert_eq!(b["acquired"], false);
        assert!(b["unweighted_book"].is_null());
    }
    assert_eq!(
        rows(&r, "mortgage_lineage")
            .iter()
            .filter(|x| x["native_group_index"] == 0 && x["ordinal"] == 739622)
            .count(),
        2
    );
}
#[test]
fn two_distinct_raw_sources_reject_joined_budget_even_with_zero_weight() {
    let mut v = raw();
    let mut s = v["sources"][0].clone();
    s["source_id"] = json!("second-source");
    s["request"]["source"]["path_id"] = json!("second-path");
    v["sources"].as_array_mut().unwrap().push(s);
    for r in v["owners"][0]["runs"].as_array_mut().unwrap() {
        r["paths"]
            .as_array_mut()
            .unwrap()
            .push(json!({"path_id":"second-path","source_id":"second-source","source_weight":0.}));
    }
    assert!(preflight(v).unwrap_err().contains("joined8Mwork/32MiB"));
}
#[test]
fn registry_identity_and_full_coverage_reject_without_inference() {
    for case in 0..4 {
        let mut v = raw();
        match case {
            0 => v["owners"][0]["runs"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ()),
            1 => {
                v["owners"][0]["runs"][1]["paths"][0]["source_id"] = json!("missing");
                None
            }
            2 => {
                let s = v["sources"][0].clone();
                v["sources"].as_array_mut().unwrap().push(s);
                None
            }
            _ => {
                v["owners"][0]["source_identity"]["revision"] = json!("changed");
                None
            }
        };
        assert!(preflight(v).is_err());
    }
}
#[test]
fn old_generic_loan_overlay_is_refused() {
    for key in ["rate", "annual_pd", "monthly_runoff", "commitment"] {
        let mut v = raw();
        v["dated"]["specification"]["positions"][1][key] = json!(0.1);
        assert!(preflight(v).is_err(), "{key}");
    }
}
#[test]
fn zero_allocation_never_prunes_unclosed_source() {
    let mut v = raw();
    v["owners"][0]["allocated_face"] = json!(0.);
    v["sources"][0]["request"]["source"]["contract"]["current_payment"] = json!(1.);
    assert!(evaluate(v).is_err());
}
#[test]
fn cancellation_before_compile_publishes_nothing() {
    let c = portfolio_compute_control::Control::new(30_000).unwrap();
    c.cancel();
    let r = portfolio_compute_control::scope(Some(c), || evaluate(raw()));
    assert_eq!(r.err().unwrap(), "COMPUTE_CANCELLED");
    assert!(portfolio_compute_control::checkpoint().is_ok());
}
#[test]
fn strict_duplicate_nested_raw_specification_is_not_collapsed() {
    let s = include_str!("fixtures/mortgage_mixed_pending.json").replacen(
        "\"horizon_days\": 60",
        "\"horizon_days\":60,\"horizon_days\":60",
        1,
    );
    assert!(serde_json::from_str::<owner::Request>(&s).is_err());
}
#[test]
fn metadata_only_dated_capacity_sizes() {
    use portfolio_ledger_native::dated_store::*;
    println!(
        "sizes event={} owner={} run={} provenance={} segment={} boundary={} arc={}",
        std::mem::size_of::<StoredEvent>(),
        std::mem::size_of::<StoredOwner>(),
        std::mem::size_of::<StoredRun>(),
        std::mem::size_of::<StoredProvenance>(),
        std::mem::size_of::<RecognitionSegment>(),
        std::mem::size_of::<AccrualBoundary>(),
        std::mem::size_of::<std::sync::Arc<str>>()
    );
    let r = request(raw());
    let d = CompiledDatedLedger::compile(r.dated, 512, 8_000_000).unwrap();
    println!(
        "canonical dated store capacity {}",
        d.store.retained_capacity_bytes()
    );
}

#[test]
fn singleton_object_enum_aliases_reject_in_new_proof_only() {
    for case in 0..4 {
        let mut v = raw();
        match case {
            0 => v["dated"]["recognition_policy"]["fees"] = json!({"immediate_earnings":null}),
            1 => v["dated"]["dated"]["owners"][0]["direction"] = json!({"receipt":null}),
            2 => {
                v["dated"]["dated"]["owners"][0]["ownership"] =
                    json!({"contractual_dated_native_state":null})
            }
            _ => {
                v["dated"]["dated"]["sources"][0]["schedule"]["events"][0]["measure"] =
                    json!({"contractual":null})
            }
        }
        assert!(serde_json::from_value::<owner::Request>(v)
            .unwrap_err()
            .to_string()
            .contains("documented string enum"));
    }
}

#[test]
fn liability_backbook_cash_and_signed_basis_retire_without_direction_change() {
    let mut v = raw();
    let d = &mut v["dated"];
    d["specification"]["accounts"][0]["equity"] = json!(949.);
    d["specification"]["positions"][0]["kind"] = json!("deposit");
    d["specification"]["positions"][0]["book_adjustment"] = json!(1.);
    d["dated"]["owners"][0]["direction"] = json!("payment");
    let e = &mut d["dated"]["sources"][0]["schedule"]["events"][0]["components"];
    e["principal"] = json!(-50.);
    e["cash_interest"] = json!(-0.05);
    e["accrued_interest"] = json!(-0.05);
    e["book_amortization"] = json!(1.);
    let s = &mut d["dated"]["accrual_plans"][0]["segments"][0];
    s["accrued_interest"] = json!(-0.05);
    s["book_amortization"] = json!(1.);
    let r = evaluate(v).unwrap();
    for s in rows(&r, "summary") {
        assert!((s["final_cash"].as_f64().unwrap() - 950.1131168226465).abs() < 1e-9);
        assert!((s["final_equity"].as_f64().unwrap() - 950.1131168226465).abs() < 1e-9);
    }
}
