use portfolio_model_core::strict_json::serialized_equal_checked;
use serde::{Serialize, Serializer};
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

fn compare(a: &Value, b: &Value, each: usize, combined: usize) -> Result<bool, String> {
    serialized_equal_checked(a, b, each, combined, || Ok(()))
}

#[test]
fn complete_serialized_values_match_the_independent_two_encoding_oracle() {
    let values = [
        Value::Null,
        json!(false),
        json!(true),
        json!(0),
        json!(0.),
        json!(-0.),
        json!(u64::MAX),
        json!(i64::MIN),
        json!(f64::MIN_POSITIVE),
        json!(f64::MAX),
        json!(""),
        json!("x\"\\\n\r\t\u{0}\u{1f}é😀"),
        json!([]),
        json!([0, -0., null, {"late": [true, "fee"]}]),
        json!({}),
        json!({"cash": -0., "rights": [{"amount": 1., "state": "default"}]}),
        json!({"cash": 0., "rights": [{"amount": 1., "state": "default"}]}),
        json!({"cash": -0., "rights": [{"amount": 2., "state": "default"}]}),
    ];
    for a in &values {
        for b in &values {
            let left = serde_json::to_vec(a).unwrap();
            let right = serde_json::to_vec(b).unwrap();
            assert_eq!(
                compare(a, b, 4096, 8192).unwrap(),
                left == right,
                "{a:?} vs {b:?}"
            );
        }
    }
}

#[test]
fn original_operand_and_combined_limits_are_both_enforced_before_staging() {
    let a = json!("é\n");
    let size = serde_json::to_vec(&a).unwrap().len();
    assert!(compare(&a, &a, size, 2 * size).unwrap());
    assert!(compare(&a, &a, size - 1, 2 * size).is_err());
    assert!(compare(&a, &a, size, 2 * size - 1)
        .unwrap_err()
        .contains("combined byte admission"));
    let short = json!(0);
    assert!(!compare(&short, &a, size, size + 1).unwrap());
    assert!(!compare(&a, &short, size, size + 1).unwrap());
}

struct Observed<'a> {
    name: &'a str,
    text: &'a str,
    calls: Rc<RefCell<Vec<&'a str>>>,
    fail_second: bool,
    change_second: Option<&'a str>,
}
impl Serialize for Observed<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let second = self.calls.borrow().contains(&self.name);
        self.calls.borrow_mut().push(self.name);
        if second && self.fail_second {
            return Err(serde::ser::Error::custom("late serializer refusal"));
        }
        serializer.serialize_str(if second {
            self.change_second.unwrap_or(self.text)
        } else {
            self.text
        })
    }
}

#[test]
fn only_smaller_operand_is_staged_and_mismatch_still_traverses_both_fully() {
    for small_on_left in [false, true] {
        let calls = Rc::new(RefCell::new(vec![]));
        let small = Observed {
            name: "small",
            text: "a",
            calls: calls.clone(),
            fail_second: false,
            change_second: None,
        };
        let large = Observed {
            name: "large",
            text: "abcdef",
            calls: calls.clone(),
            fail_second: false,
            change_second: None,
        };
        let (a, b) = if small_on_left {
            (&small, &large)
        } else {
            (&large, &small)
        };
        assert!(!serialized_equal_checked(a, b, 100, 200, || Ok(())).unwrap());
        assert_eq!(&*calls.borrow(), &[a.name, b.name, "small", "large"]);
    }
}

#[test]
fn late_serializer_errors_and_changed_sizes_cannot_be_accepted_as_mismatch() {
    for fail_second in [false, true] {
        for changed in ["", "a much longer representation"] {
            let calls = Rc::new(RefCell::new(vec![]));
            let a = Observed {
                name: "a",
                text: "short",
                calls: calls.clone(),
                fail_second,
                change_second: Some(changed),
            };
            let b = Observed {
                name: "b",
                text: "different",
                calls,
                fail_second: false,
                change_second: None,
            };
            assert!(serialized_equal_checked(&a, &b, 100, 200, || Ok(())).is_err());
            // Also exercise a serializer error after the retained operand has
            // already compared unequal, during the complete streamed traversal.
            let calls = Rc::new(RefCell::new(vec![]));
            let a = Observed {
                calls: calls.clone(),
                ..a
            };
            let b = Observed {
                calls,
                fail_second: true,
                ..b
            };
            let a = Observed {
                fail_second: false,
                change_second: None,
                ..a
            };
            assert!(serialized_equal_checked(&a, &b, 100, 200, || Ok(()))
                .unwrap_err()
                .contains("late serializer refusal"));
        }
    }
}

#[test]
fn every_cancellation_probe_can_abort_including_long_unescaped_string_writes() {
    let text = "x".repeat(100_000);
    let mut probes = 0;
    assert!(
        serialized_equal_checked(&text, &text, 110_000, 220_000, || {
            probes += 1;
            Ok(())
        })
        .unwrap()
    );
    assert!(probes >= 18, "long writes must have bounded probes");
    for stop in 1..=probes {
        let mut seen = 0;
        let error = serialized_equal_checked(&text, &text, 110_000, 220_000, || {
            seen += 1;
            if seen == stop {
                Err("comparison cancelled".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(error.contains("comparison cancelled"), "{error}");
        assert_eq!(seen, stop);
    }
}
