use portfolio_model_core::strict_json::serialized_equal_checked;
use portfolio_model_core::strict_json::serialized_matches_checked;
use serde::{Serialize, Serializer};
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

fn compare(a: &Value, b: &Value, each: usize, combined: usize) -> Result<bool, String> {
    serialized_equal_checked(a, b, each, combined, || Ok(()))
}

#[test]
fn retained_bytes_compare_without_reencoding_reference_and_preserve_exact_categories_and_limits() {
    let values = [
        json!(-0.),
        json!(0.),
        json!(0),
        json!(null),
        json!("\n非😀"),
        json!({"state":[1.,true,{"rights":[],"amount":-0.}]}),
        json!([1, 2]),
        json!([1, 2, 3]),
    ];
    for a in &values {
        for b in &values {
            let expected = serde_json::to_vec(a).unwrap();
            let actual = serde_json::to_vec(b).unwrap();
            assert_eq!(
                serialized_matches_checked(&expected, b, 4096, || Ok(())).unwrap(),
                expected == actual
            );
            assert!(
                serialized_matches_checked(&expected, b, expected.len() - 1, || Ok(())).is_err()
            );
        }
    }
    let expected = serde_json::to_vec(&json!("x")).unwrap();
    assert!(serialized_matches_checked(&expected, &"xxxxxxxx", expected.len(), || Ok(())).is_err());
    assert!(!serialized_matches_checked(b"invalid-json", &json!(null), 100, || Ok(())).unwrap());
}

#[test]
fn streamed_comparison_retains_late_serializer_failure_and_every_long_write_cancellation() {
    struct Fails;
    impl Serialize for Fails {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeSeq;
            let mut seq = s.serialize_seq(Some(2))?;
            seq.serialize_element(&1)?;
            Err(serde::ser::Error::custom("late streamed refusal"))
        }
    }
    assert!(serialized_matches_checked(b"[2,0]", &Fails, 100, || Ok(()))
        .unwrap_err()
        .contains("late streamed refusal"));
    let text = "x".repeat(100_000);
    let expected = serde_json::to_vec(&text).unwrap();
    let mut probes = 0;
    assert!(
        serialized_matches_checked(&expected, &text, expected.len(), || {
            probes += 1;
            Ok(())
        })
        .unwrap()
    );
    assert!(probes >= 6);
    for stop in 1..=probes {
        let mut seen = 0;
        assert!(
            serialized_matches_checked(&expected, &text, expected.len(), || {
                seen += 1;
                if seen == stop {
                    Err("stream comparison cancelled".into())
                } else {
                    Ok(())
                }
            })
            .unwrap_err()
            .contains("stream comparison cancelled")
        );
        assert_eq!(seen, stop);
    }
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
#[test]
fn counted_retained_comparison_matches_independent_encodings_and_pays_complete_mismatches() {
    let original = serde_json::json!({"head":-0.0,"tail":[1,2.0,"雪\n"]});
    let bytes = serde_json::to_vec(&original).unwrap();
    for changed in [
        original.clone(),
        serde_json::json!({"head":0.0,"tail":[1,2.0,"雪\n"]}),
        serde_json::json!({"head":-0.0,"tail":[1,2.0,"changed\n"]}),
    ] {
        let expected = serde_json::to_vec(&changed).unwrap();
        let mut charged = 0;
        let (same, work) = portfolio_model_core::strict_json::serialized_matches_counted(
            &bytes,
            &changed,
            1024,
            |n| {
                charged += n;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(same, bytes == expected);
        assert_eq!(work.encoded_bytes, expected.len());
        assert_eq!(work.total_work, work.traversal_work + 2 * expected.len());
        assert_eq!(charged, work.total_work);
    }
}

#[test]
fn counted_retained_comparison_refuses_unfunded_tail_overrun_and_cancelled_work() {
    let original = serde_json::json!({"first":1,"tail":"x".repeat(65536)});
    let bytes = serde_json::to_vec(&original).unwrap();
    let (_, work) = portfolio_model_core::strict_json::serialized_matches_counted(
        &bytes,
        &original,
        bytes.len(),
        |_| Ok(()),
    )
    .unwrap();
    for limit in [0, 1, work.total_work - 1] {
        let mut spent = 0;
        assert!(
            portfolio_model_core::strict_json::serialized_matches_counted(
                &bytes,
                &original,
                bytes.len(),
                |n| {
                    if n > limit - spent {
                        return Err("unfunded comparison".into());
                    }
                    spent += n;
                    Ok(())
                }
            )
            .unwrap_err()
            .contains("unfunded comparison")
        );
        assert!(spent <= limit);
    }
    assert!(
        portfolio_model_core::strict_json::serialized_matches_counted(b"[]", &original, 2, |_| Ok(
            ()
        ))
        .is_err()
    );
    assert!(
        portfolio_model_core::strict_json::serialized_matches_counted(
            &bytes,
            &original,
            bytes.len(),
            |_| Err("COMPUTE_CANCELLED".into())
        )
        .unwrap_err()
        .contains("COMPUTE_CANCELLED")
    );
}

#[test]
fn counted_mismatch_still_observes_a_late_serializer_failure() {
    struct Late;
    impl Serialize for Late {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeSeq;
            let mut sequence = serializer.serialize_seq(Some(2))?;
            sequence.serialize_element(&"different")?;
            Err(serde::ser::Error::custom("late counted failure"))
        }
    }
    let error = portfolio_model_core::strict_json::serialized_matches_counted(
        b"[\"original\",1]",
        &Late,
        1024,
        |_| Ok(()),
    )
    .unwrap_err();
    assert!(error.contains("late counted failure"));
}
