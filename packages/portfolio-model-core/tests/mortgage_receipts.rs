use portfolio_model_core::mortgage_receipts::*;
use portfolio_model_core::{mortgage_sequential_carry, mortgage_state};
#[derive(Default)]
struct Count {
    admitted: usize,
    intervals: usize,
    boundaries: usize,
    links: usize,
    complete: usize,
}
impl MortgageReceiptSink for Count {
    fn admit(&mut self, v: AdmissionView<'_>) -> Result<(), String> {
        self.admitted = v.interval_count;
        Ok(())
    }
    fn interval(&mut self, v: IntervalView<'_>) -> Result<(), String> {
        assert_eq!(v.interval_index, self.intervals);
        self.intervals += 1;
        Ok(())
    }
    fn recovery(&mut self, _: RecoveryView<'_>) -> Result<(), String> {
        Ok(())
    }
    fn prepayment(&mut self, _: PrepaymentView<'_>) -> Result<(), String> {
        Ok(())
    }
    fn scheduled(&mut self, _: ScheduledView<'_>) -> Result<(), String> {
        Ok(())
    }
    fn boundary(&mut self, v: BoundaryView<'_>) -> Result<(), String> {
        assert_eq!(v.interval_index, self.boundaries);
        self.boundaries += 1;
        Ok(())
    }
    fn cash_link(&mut self, _: CashLinkView<'_>) -> Result<(), String> {
        self.links += 1;
        Ok(())
    }
    fn complete(&mut self, _: &mortgage_state::MortgageResult) -> Result<(), String> {
        self.complete += 1;
        Ok(())
    }
}
fn raw() -> mortgage_state::MortgageRequest {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mortgage_receipts/request.json")).unwrap();
    serde_json::from_value(v["contract"].clone()).unwrap()
}
#[test]
fn cancellation_aborts_before_receipt_admission() {
    let mut count = Count::default();
    assert!(mortgage_state::project_with_receipts(
        &raw(),
        &mut count,
        &mut || Err("cancel".into())
    )
    .is_err());
    assert_eq!(count.admitted, 0);
}
fn compare_owner_port(mut raw: serde_json::Value, mut expected: serde_json::Value) {
    raw["schema"] = mortgage_sequential_carry::SCHEMA.into();
    let r: mortgage_sequential_carry::Request = serde_json::from_value(raw).unwrap();
    fn update(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::String(s) => {
                *s = match s.as_str() {
                    "mortgage-sequential-carry-draft-1" => mortgage_sequential_carry::SCHEMA.into(),
                    "mortgage-coupled-carry-draft-1" => "mortgage-coupled-carry-1".into(),
                    "mortgage-interval-receipt-draft-1" => "mortgage-loop-interval-1".into(),
                    BASE_FINANCIAL_SOURCE_SHA256 => generator_source_sha256().into(),
                    _ => s.clone(),
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(update),
            serde_json::Value::Object(o) => o.values_mut().for_each(update),
            _ => {}
        }
    }
    update(&mut expected);
    for (source, key) in [
        ("source", "current_source_receipt_hash"),
        ("original_source", "original_source_receipt_hash"),
    ] {
        use sha2::{Digest, Sha256};
        expected[key] = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&expected[source]).unwrap())
        )
        .into();
    }
    expected["admission"]["work_bound"] =
        (expected["admission"]["work_bound"].as_u64().unwrap() + source_code_bytes() as u64).into();
    expected["admission"]["generator_code_hash_bytes"] = (source_code_bytes() as u64).into();
    let schema_delta = ("mortgage-sequential-carry-draft-1".len()
        - mortgage_sequential_carry::SCHEMA.len()) as u64;
    let count = expected["admission"]["retained_row_bound"]
        .as_u64()
        .unwrap()
        + expected["admission"]["interval_count"].as_u64().unwrap()
        + expected["admission"]["original_interval_count"]
            .as_u64()
            .unwrap()
        + 4;
    expected["admission"]["retained_byte_bound"] = (expected["admission"]["retained_byte_bound"]
        .as_u64()
        .unwrap()
        - schema_delta * count)
        .into();
    let actual = serde_json::to_value(mortgage_sequential_carry::evaluate(&r).unwrap()).unwrap();
    fn same(a: &serde_json::Value, b: &serde_json::Value, path: &str) {
        match (a, b) {
            (serde_json::Value::Object(x), serde_json::Value::Object(y)) => {
                assert_eq!(
                    x.keys().collect::<Vec<_>>(),
                    y.keys().collect::<Vec<_>>(),
                    "{path}"
                );
                for (k, v) in x {
                    same(v, &y[k], &format!("{path}.{k}"));
                }
            }
            (serde_json::Value::Array(x), serde_json::Value::Array(y)) => {
                assert_eq!(x.len(), y.len(), "{path}");
                for (i, (v, w)) in x.iter().zip(y).enumerate() {
                    same(v, w, &format!("{path}[{i}]"));
                }
            }
            _ => assert_eq!(a, b, "{path}"),
        }
    }
    same(&actual, &expected, "result");
}
fn proof_files(dir: &str) -> Vec<serde_json::Value> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mortgage_receipts")
        .join(dir);
    let mut files = std::fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "json") && p.file_name().unwrap() != "manifest.json"
        })
        .collect::<Vec<_>>();
    files.sort();
    files
        .into_iter()
        .map(|p| serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap())
        .collect()
}
#[test]
fn raw_owner_port_matches_all_frozen_receipts_and_rejections() {
    let files = proof_files("sequential-native");
    assert_eq!(files.len(), 32);
    for case in files {
        if let Some(expected) = case.get("native_result") {
            compare_owner_port(case["raw_request"].clone(), expected.clone());
        } else {
            let mut raw = case["raw_request"].clone();
            raw["schema"] = mortgage_sequential_carry::SCHEMA.into();
            let request = serde_json::from_value(raw).unwrap();
            let error = if case["stage"] == "sequential_admission" {
                mortgage_sequential_carry::admit(&request).unwrap_err()
            } else {
                mortgage_sequential_carry::evaluate(&request).unwrap_err()
            };
            assert_eq!(
                error,
                case["native_error"].as_str().unwrap(),
                "{}",
                case["native_test"]
            );
        }
    }
}
#[test]
fn old_actual_generator_cases_preserve_none_and_borrowed_bytes() {
    let mut count = 0;
    for case in proof_files("native") {
        if case["stage"] != "actual_interval_source" {
            continue;
        }
        count += 1;
        let r: mortgage_state::MortgageRequest =
            serde_json::from_value(case["raw_request"].clone()).unwrap();
        let expected = if let Some(value) = case.get("native_result") {
            Ok(serde_json::from_value::<mortgage_state::MortgageResult>(
                value["native_projection"].clone(),
            )
            .unwrap())
        } else {
            Err(case["native_error"].as_str().unwrap().to_owned())
        };
        let none = mortgage_state::project(&r);
        let mut sink = Count::default();
        let captured = mortgage_state::project_with_receipts(&r, &mut sink, &mut || Ok(()));
        match expected {
            Ok(v) => {
                let bytes = serde_json::to_vec(&v).unwrap();
                assert_eq!(serde_json::to_vec(&none.unwrap()).unwrap(), bytes);
                assert_eq!(serde_json::to_vec(&captured.unwrap()).unwrap(), bytes);
            }
            Err(e) => {
                assert_eq!(none.unwrap_err(), e);
                assert_eq!(captured.unwrap_err(), e);
            }
        }
    }
    assert!(count >= 8);
}

#[test]
fn generator_digest_binds_actual_compiled_receipt_and_native_dependencies() {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    let mut count = 0;
    for name in [
        "mortgage_state.rs",
        "mortgage_receipts.rs",
        "consumer_contract.rs",
        "credit_operator.rs",
        "credit.rs",
        "amortization.rs",
        "cashflow.rs",
        "floating_rate.rs",
        "limits.rs",
    ] {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join(name),
        )
        .unwrap();
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
        count += 16 + name.len() + bytes.len();
    }
    assert_eq!(format!("{:x}", hash.finalize()), generator_source_sha256());
    assert_ne!(
        generator_source_sha256(),
        BASE_FINANCIAL_SOURCE_SHA256.to_ascii_lowercase()
    );
    assert_eq!(count, source_code_bytes());
    let r: mortgage_sequential_carry::Request =
        serde_json::from_str(include_str!("fixtures/mortgage_receipts/request.json")).unwrap();
    assert_eq!(
        mortgage_sequential_carry::admit(&r)
            .unwrap()
            .generator_code_hash_bytes,
        count
    );
}
