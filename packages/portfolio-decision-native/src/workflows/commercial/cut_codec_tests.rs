use super::*;
use serde::Serialize;
use serde_json::{json, Value};

fn expected(v: &Value) -> usize {
    match v {
        Value::Null | Value::Bool(_) => 1,
        Value::Number(_) => 33,
        Value::String(s) => 1 + s.len(),
        Value::Array(xs) => 1 + xs.iter().map(expected).sum::<usize>(),
        Value::Object(xs) => {
            1 + xs
                .iter()
                .map(|(k, v)| 1 + k.len() + expected(v))
                .sum::<usize>()
        }
    }
}
fn compare<T: Serialize>(v: &T) {
    let ordinary = serde_json::to_vec(v).unwrap();
    let mut paid = Vec::new();
    let mut charged = 0;
    let work = write(&mut paid, v, |n| {
        charged += n;
        Ok(())
    })
    .unwrap();
    assert_eq!(ordinary, paid);
    assert_eq!(work, charged);
    let value: Value = serde_json::from_slice(&ordinary).unwrap();
    assert_eq!(work, expected(&value));
}
#[test]
fn complete_json_bytes_and_independent_traversal_formula_match() {
    compare(
        &json!({"escaped\n𐀀": [null, true, false, -0.0, 1.7976931348623157e308,
        -9223372036854775808_i64, 18446744073709551615_u64, "\"\\\t\r\n\u{001f}é😀"], "nested": {"last": 3}}),
    );
    compare(&"x".repeat(1 << 20));
    compare(&'😀');
    compare(&Some((None::<u64>, 123_u64, "text")));
}

#[test]
fn diagnostic_value_is_prepaid_and_preserves_the_complete_json_tree() {
    let raw = json!({"full": [null, -0.0, "é", {"tail": true}], "fee": -12.5});
    let mut charged = 0;
    let (value, work) = diagnostic_value(&raw, |n| {
        charged += n;
        Ok(())
    })
    .unwrap();
    assert_eq!(value, raw);
    assert_eq!(work, expected(&raw));
    assert_eq!(charged, work);
    for limit in [0, 1, work - 1] {
        let mut spent = 0;
        assert!(diagnostic_value(&raw, |n| {
            if n > limit - spent {
                return Err("diagnostic work exhausted".into());
            }
            spent += n;
            Ok(())
        })
        .unwrap_err()
        .contains("diagnostic work exhausted"));
        assert!(spent <= limit);
    }
}
#[test]
fn typed_structs_enums_tuples_options_and_flatten_preserve_wire_and_work() {
    #[derive(Serialize)]
    enum E {
        Unit,
        New(u64),
        Tuple(u64, String),
        Struct { field: bool },
    }
    #[derive(Serialize)]
    struct Inner {
        a: u64,
        b: Option<String>,
    }
    #[derive(Serialize)]
    struct Outer {
        text: String,
        #[serde(flatten)]
        inner: Inner,
        enumeration: E,
    }
    for enumeration in [
        E::Unit,
        E::New(3),
        E::Tuple(4, "é".into()),
        E::Struct { field: true },
    ] {
        compare(&Outer {
            text: "full".into(),
            inner: Inner { a: 7, b: None },
            enumeration,
        });
    }
    #[derive(Serialize)]
    struct Tuple(u64, String);
    #[derive(Serialize)]
    struct New(u64);
    #[derive(Serialize)]
    struct Unit;
    compare(&Tuple(5, "tail".into()));
    compare(&New(9));
    compare(&Unit);
}
#[test]
fn exhausted_budget_refuses_before_large_string_scan_or_forwarding() {
    let large = "𐀀\n".repeat(1 << 18);
    let mut output = Vec::new();
    let error = write(&mut output, &large, |n| {
        if n > 4 {
            Err("prepaid fixture exhaustion".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(error.contains("prepaid fixture exhaustion"));
    assert!(output.is_empty());
    let mut remaining: usize = 1;
    let error = write(&mut output, &vec![large], |n| {
        remaining = remaining
            .checked_sub(n)
            .ok_or("prepaid fixture exhaustion")?;
        Ok(())
    })
    .unwrap_err();
    assert!(error.contains("prepaid fixture exhaustion"));
    assert_eq!(output, b"[");
}
#[test]
fn unbounded_display_and_opaque_json_refuse_before_formatting() {
    struct Display;
    impl std::fmt::Display for Display {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            panic!("unfunded Display visited")
        }
    }
    impl Serialize for Display {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.collect_str(self)
        }
    }
    let mut output = Vec::new();
    assert!(write(&mut output, &Display, |_| Ok(()))
        .unwrap_err()
        .contains("unbounded Display"));
    let raw = serde_json::value::RawValue::from_string("{\"unpaid\":1}".into()).unwrap();
    assert!(write(&mut output, &raw, |_| Ok(()))
        .unwrap_err()
        .contains("opaque JSON"));
    assert!(output.is_empty());
}
#[test]
fn cancellation_writer_failures_and_nesting_refuse_without_success_receipt() {
    let mut output = Vec::new();
    assert!(write(&mut output, &json!([1, 2]), |_| Err(
        "COMPUTE_CANCELLED".into()
    ))
    .unwrap_err()
    .contains("COMPUTE_CANCELLED"));
    assert!(output.is_empty());
    let mut value = json!(true);
    for _ in 0..130 {
        value = json!([value]);
    }
    assert!(write(&mut output, &value, |_| Ok(()))
        .unwrap_err()
        .contains("nesting exceeds128"));
    struct Failed;
    impl Write for Failed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("fixture writer failed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(write(&mut Failed, &json!({"money":1}), |_| Ok(()))
        .unwrap_err()
        .contains("fixture writer failed"));
}

#[test]
fn nonfinite_and_oversized_number_domains_refuse_before_emitting_null_or_digits() {
    let mut output = Vec::new();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(write(&mut output, &value, |_| Ok(()))
            .unwrap_err()
            .contains("nonfinite"));
    }
    assert!(write(&mut output, &u128::MAX, |_| Ok(())).is_err());
    assert!(write(&mut output, &i128::MIN, |_| Ok(())).is_err());
    assert!(output.is_empty());
    compare(&(u64::MAX as u128));
    compare(&(i64::MIN as i128));
}
