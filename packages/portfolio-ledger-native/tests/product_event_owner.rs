use portfolio_ledger_native::{
    daily,
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest},
    product_event_owner as product,
};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
struct ResultData {
    product_sources: Value,
    ledger_messages: Vec<Value>,
}
#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        let mut v = self.0.lock().unwrap();
        if v.len()
            .checked_add(b.len())
            .is_none_or(|n| n > 32 * 1024 * 1024)
        {
            return Err(io::Error::other("test capture bound"));
        }
        v.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn compile(v: &Value) -> Result<(CompiledDatedLedger, product::Compiled), String> {
    let raw = serde_json::from_value::<DatedLedgerRequest>(v["dated"].clone())
        .map_err(|e| e.to_string())?;
    let dated = CompiledDatedLedger::compile(raw, 512, 8_000_000)?;
    let r=serde_json::from_value::<product::Request>(json!({"schema":product::SCHEMA,"reporting_end_ordinal":v["reporting_end_ordinal"],"owners":v["product_owners"]})).map_err(|e|e.to_string())?;
    let owners = product::Compiled::compile(r, &dated)?;
    Ok((dated, owners))
}
fn evaluate(v: &Value) -> Result<ResultData, String> {
    let (dated, owners) = compile(v)?;
    let product_sources = serde_json::to_value(&owners.sources).map_err(|e| e.to_string())?;
    let buffer = Arc::new(Mutex::new(Vec::new()));
    daily::run_dated_with_products(dated, owners, Box::new(Capture(buffer.clone())))?;
    let bytes = buffer.lock().unwrap();
    let ledger_messages = bytes
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice(b).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ResultData {
        product_sources,
        ledger_messages,
    })
}
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/product_event_owner.json")).unwrap()
}
fn run(v: Value) -> Result<ResultData, String> {
    evaluate(&v)
}
fn rows(r: &ResultData, table: &str) -> Vec<Value> {
    r.ledger_messages
        .iter()
        .filter(|m| m["table"] == table)
        .flat_map(|m| m["rows"].as_array().unwrap().clone())
        .collect()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a}!={b}")
}
fn journals(r: &ResultData) -> Vec<Value> {
    let mut dict = Vec::<String>::new();
    let mut out = Vec::new();
    for m in &r.ledger_messages {
        if m["table"] != "journal" {
            continue;
        }
        dict.extend(
            m["dictionary"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_owned()),
        );
        let c = &m["columns"];
        for i in 0..c["value"].as_array().unwrap().len() {
            let mut row = json!({});
            for k in ["transaction_number", "day", "value"] {
                row[k] = c[k][i].clone();
            }
            for k in [
                "scenario",
                "account",
                "event",
                "gl_account",
                "instrument_id",
            ] {
                row[k] = json!(dict[c[k][i].as_u64().unwrap() as usize]);
            }
            out.push(row);
        }
    }
    out
}
#[test]
fn all_three_sources_share_one_gl_and_actual_cash() {
    let r = run(raw()).unwrap();
    assert_eq!(r.product_sources.as_array().unwrap().len(), 4);
    let cash = rows(&r, "trial_balance")
        .into_iter()
        .find(|x| x["scenario"] == "baseline" && x["gl_account"] == "cash")
        .unwrap();
    close(cash["balance"].as_f64().unwrap(), 948.05);
    let j = journals(&r);
    assert!(j.iter().any(|x| x["instrument_id"] == "p"));
    assert!(j.iter().any(|x| x["instrument_id"] == "credit-loan"));
    assert!(j.iter().any(|x| x["instrument_id"] == "credit-card"));
    let mut tx = std::collections::BTreeMap::new();
    for x in j {
        *tx.entry((
            x["scenario"].as_str().unwrap().to_owned(),
            x["transaction_number"].as_u64().unwrap(),
        ))
        .or_insert(0.) += x["value"].as_f64().unwrap();
    }
    for v in tx.values() {
        close(*v, 0.);
    }
}
#[test]
fn allocation_and_probability_apply_once() {
    let mut v = raw();
    v["product_owners"][1]["allocated_face"] = json!(50.);
    for r in v["product_owners"][1]["runs"].as_array_mut().unwrap() {
        let mut other = r["paths"][0].clone();
        r["paths"][0]["source_weight"] = json!(0.25);
        other["source_weight"] = json!(0.75);
        other["path_id"] = json!("second-card-path");
        other["request"]["request"]["path_id"] = json!("second-card-path");
        r["paths"].as_array_mut().unwrap().push(other);
    }
    let r = run(v).unwrap();
    let cash = rows(&r, "trial_balance")
        .into_iter()
        .find(|x| x["scenario"] == "baseline" && x["gl_account"] == "cash")
        .unwrap();
    close(cash["balance"].as_f64().unwrap(), 973.05);
    let acquisition = rows(&r, "product_acquisition_lineage");
    assert_eq!(
        acquisition
            .iter()
            .filter(|x| x["position_id"] == "credit-card")
            .count(),
        4
    );
}
#[test]
fn full_raw_card_receipts_and_actual_claims_survive() {
    let r = run(raw()).unwrap();
    let card = &r
        .product_sources
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["native"]["kind"] == "card")
        .unwrap()["native"];
    assert_eq!(card["kind"], "card");
    assert_eq!(
        card["result"]["commitment_source"]["funded_contingent_draw"],
        json!(0.)
    );
    assert!(card["result"]["raw_request"]["termination"].is_object());
    assert_eq!(
        card["result"]["closing"]["claims"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(rows(&r, "product_daily")
        .iter()
        .any(|x| x["product_receipts"]["kind"] == "card"));
}
#[test]
fn card_draw_fee_grace_and_ccf_overrides_reject_whole_source() {
    for kind in 0..4 {
        let mut v = raw();
        let raw = &mut v["product_owners"][1]["runs"][0]["paths"][0]["request"]["request"];
        match kind {
            0 => raw["contract"]["initial_purchase_grace"] = json!(true),
            1 => raw["contract"]["opening_fees"] = json!([1., 0., 0.]),
            2 => {
                raw["contract"]["steps"][0]["action"] =
                    json!({"kind":"draw","class":"purchase","amount":1.})
            }
            _ => raw["policy"]["commitment"] = json!("ccf-1"),
        };
        assert!(run(v).is_err());
    }
}
#[test]
fn zero_weight_path_still_validates_card_ownership() {
    let mut v = raw();
    let owner = &mut v["product_owners"][1];
    for r in owner["runs"].as_array_mut().unwrap() {
        let mut p = r["paths"][0].clone();
        p["path_id"] = json!("invalid-zero-path");
        p["source_weight"] = json!(0.);
        p["request"]["request"]["path_id"] = json!("invalid-zero-path");
        p["request"]["request"]["contract"]["initial_purchase_grace"] = json!(true);
        r["paths"].as_array_mut().unwrap().push(p);
    }
    assert!(run(v).is_err());
}
#[test]
fn exact_run_anchors_original_terms_and_owner_inventory_reject() {
    for k in 0..5 {
        let mut v = raw();
        match k {
            0 => v["product_owners"][1]["runs"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap(),
            1 => {
                v["product_owners"][1]["runs"][1]["paths"][0]["request"]["request"]["acquisition"]
                    ["cost"] = json!(101.)
            }
            2 => {
                v["product_owners"][1]["runs"][1]["paths"][0]["request"]["request"]["contract"]
                    ["rates"]["purchase"] = json!(0.2)
            }
            3 => v["product_owners"][1]["position_id"] = json!("credit-loan"),
            _ => v["product_owners"][1]["runs"][0]["paths"][0]["request"]["intents"] = json!([]),
        };
        assert!(run(v).is_err());
    }
}
#[test]
fn incomplete_card_claim_tail_and_reporting_clock_reject() {
    for k in 0..2 {
        let mut v = raw();
        for r in v["product_owners"][1]["runs"].as_array_mut().unwrap() {
            if k == 0 {
                r["paths"][0]["request"]["request"]["chargeoffs"] = json!([]);
            } else {
                r["paths"][0]["request"]["request"]["reporting_end_ordinal"] = json!(738925);
            }
        }
        assert!(run(v).is_err());
    }
}
#[test]
fn cancellation_admits_no_result() {
    let token = portfolio_compute_control::Control::new(10000).unwrap();
    token.cancel();
    assert!(portfolio_compute_control::scope(Some(token), || run(raw())).is_err());
}

#[test]
fn statement_receipts_are_exact_date_indexed_without_stale_tail_replay() {
    let (_, owners) = compile(&raw()).unwrap();
    let source = owners
        .sources
        .iter()
        .position(|s| matches!(s.native, product::source::Native::Card(_)))
        .unwrap();
    let product::source::Native::Card(card) = &owners.sources[source].native else {
        unreachable!()
    };
    for statement in &card.statement_lineage {
        let day = (statement.ordinal - card.raw_request.acquisition.purchase_ordinal) as usize;
        let receipt = owners.daily_receipts(source, day);
        let rows = receipt["statement_lineage"].as_array().unwrap();
        assert!(rows
            .iter()
            .any(|r| r["source_index"] == statement.source_index));
    }
    let tail = owners.daily_receipts(source, card.raw_request.contract.horizon_days as usize + 1);
    assert!(tail["statement_lineage"].as_array().unwrap().is_empty());
    assert!(
        owners.daily_receipts(source, usize::MAX)["statement_lineage"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn consumer_bridge_borrows_original_receipt_without_tagged_wire_change() {
    use portfolio_ledger_native::consumer_intent_owner as old;
    let v = raw();
    let (dated, _) = compile(&v).unwrap();
    let mut owner = v["product_owners"][0].clone();
    for run in owner["runs"].as_array_mut().unwrap() {
        for path in run["paths"].as_array_mut().unwrap() {
            path["request"] = path["request"]["request"].clone();
        }
    }
    let request = serde_json::from_value(json!({"schema":old::SCHEMA,"reporting_end_ordinal":v["reporting_end_ordinal"],"owners":[owner]})).unwrap();
    let old = old::Compiled::compile(request, &dated).unwrap();
    let bridge = product::OwnerSources::Consumer(&old);
    assert_eq!(bridge.wire_format(), product::WireFormat::Consumer);
    assert_eq!(bridge.source_count(), old.sources.len());
    for index in 0..old.sources.len() {
        assert!(std::ptr::eq(
            bridge.native(index).opening(),
            &old.sources[index].native.acquisition_opening
        ));
        assert_eq!(
            bridge.emit_source(index),
            serde_json::to_value(&old.sources[index]).unwrap()
        );
        assert!(bridge.emit_source(index)["native"]["kind"].is_null());
        assert_eq!(
            serde_json::to_value(bridge.native(index).raw().acquisition).unwrap(),
            serde_json::to_value(&old.sources[index].native.raw_request.acquisition).unwrap()
        );
        assert_eq!(
            bridge.native(index).product_event_ids(0),
            serde_json::to_value(&old.sources[index].native.lineage[0].product_event_ids).unwrap()
        );
    }
}

#[test]
fn zero_allocated_unsplit_basis_and_nonphysical_histories_reject() {
    for pricing in [false, true] {
        let mut v = raw();
        let owner = &mut v["product_owners"][1];
        owner["allocated_face"] = json!(0.);
        for run in owner["runs"].as_array_mut().unwrap() {
            let request = &mut run["paths"][0]["request"]["request"];
            if pricing {
                request["contract"]["identity"]["measure"] = json!("pricing");
            } else {
                request["contract"]["steps"] = json!([{"ordinal":738926,"action":{"kind":"payment","amount":100.}},{"ordinal":738927,"action":{"kind":"payment","amount":1.}},{"ordinal":738927,"action":{"kind":"set_limit","amount":0.}}]);
                request["contract"]["cycles"] =
                    json!([{"statement_ordinal":738926,"due_ordinal":738931}]);
                request["acquisition"]["cost"] = json!(100.);
                request["acquisition"]["original_annual_effective_rate"] = json!(0.);
                request["termination"] = json!({"kind":"reviewed_zero_rights_closure","ordinal":738927,"review":{"id":"explicit-zero-rights-close","revision":"r1"}});
                request["opening_allowance"] = json!(0.);
                request["allowance_reviews"] = json!([]);
                request["chargeoffs"] = json!([]);
            }
        }
        if !pricing {
            owner["initial_allowance_review"] = Value::Null;
        }
        let error = match compile(&v) {
            Err(error) => error,
            Ok(_) => panic!("unsupported raw source must reject even at zero allocation"),
        };
        if pricing {
            assert!(error.contains("physical raw histories"), "{error}");
        } else {
            assert!(error.contains("principal plus basis"), "{error}");
        }
    }
}
