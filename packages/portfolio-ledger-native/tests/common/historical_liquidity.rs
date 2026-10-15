use serde_json::{json, Value};

pub fn historical_with_current_liquidity_contract(historical: &[u8]) -> Vec<u8> {
    // These inputs supply no liquidity profile. The frozen runner predates
    // explicit unconfigured statuses and null headroom. Keep every other byte,
    // including ordered journals, financial amounts and serialized columns.
    let mut expected = Vec::new();
    for line in historical.split_inclusive(|b| *b == b'\n') {
        assert_eq!(line.last(), Some(&b'\n'));
        let mut record: Value = serde_json::from_slice(line).unwrap();
        if !matches!(record["table"].as_str(), Some("path" | "summary")) {
            expected.extend_from_slice(line);
            continue;
        }
        assert_eq!(
            serde_json::to_vec(&record).unwrap(),
            &line[..line.len() - 1],
            "historical output must round-trip without changing any original bytes"
        );
        match record["table"].as_str() {
            Some("path") => {
                for row in record["rows"].as_array_mut().unwrap() {
                    let row = row.as_object_mut().unwrap();
                    for ratio in ["lcr", "nsfr"] {
                        let headroom = format!("{ratio}_headroom");
                        assert!(row[&headroom].is_number());
                        row.insert(headroom, Value::Null);
                        assert!(row
                            .insert(format!("{ratio}_status"), json!("unconfigured"))
                            .is_none());
                    }
                }
            }
            Some("summary") => {
                for row in record["rows"].as_array_mut().unwrap() {
                    let row = row.as_object_mut().unwrap();
                    for name in [
                        "lcr_applicability",
                        "nsfr_applicability",
                        "lcr_status",
                        "nsfr_status",
                    ] {
                        assert!(row.insert(name.into(), json!("unconfigured")).is_none());
                    }
                    assert!(row
                        .insert("htm_reclassification_day".into(), Value::Null)
                        .is_none());
                }
            }
            _ => {}
        }
        expected.extend(serde_json::to_vec(&record).unwrap());
        expected.push(b'\n');
    }
    expected
}
