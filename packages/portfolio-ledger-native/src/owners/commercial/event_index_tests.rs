//! Frozen former daily Vec/sort ordering as an independent index oracle.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone)]
struct Raw<'a> {
    scenario: &'a str,
    day: usize,
    year: f64,
    event: Scheduled,
}
fn legacy(rows: &[Raw<'_>], scenario: &str, day: usize) -> Vec<Scheduled> {
    // This intentionally retains the old owned key/daily collection and full
    // (fractional year, Loan-before-Fee, binding, group) comparison.
    let mut map = BTreeMap::<(String, usize), Vec<&Raw<'_>>>::new();
    for row in rows {
        map.entry((row.scenario.to_string(), row.day))
            .or_default()
            .push(row);
    }
    let mut rows = map
        .get(&(scenario.to_string(), day))
        .cloned()
        .unwrap_or_default();
    rows.sort_by(|a, b| {
        let key = |row: &Raw<'_>| match row.event {
            Scheduled::Loan(binding, group) => (row.year, 0, binding, group),
            Scheduled::Fee(binding, group) => (row.year, 1, binding, group),
        };
        let a = key(a);
        let b = key(b);
        a.0.total_cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });
    rows.into_iter().map(|r| r.event).collect()
}
fn build<'a>(rows: &[Raw<'a>]) -> (Index<'a>, Index<'a>) {
    let (mut loans, mut fees) = (Index::default(), Index::default());
    for row in rows {
        let index = match row.event {
            Scheduled::Loan(..) => &mut loans,
            Scheduled::Fee(..) => &mut fees,
        };
        let timeline = index.register(row.scenario).unwrap();
        index.push(timeline, row.day, row.year, row.event).unwrap();
    }
    loans.seal().unwrap();
    fees.seal().unwrap();
    (loans, fees)
}
fn check(rows: &[Raw<'_>]) {
    let (loans, fees) = build(rows);
    for scene in ["baseline", "stress", "stress-long-同じ😀", "unknown", ""] {
        for day in 0..=32 {
            let mut actual = OrderedEvents::new(&loans, &fees, scene, day);
            let expected = legacy(rows, scene, day);
            assert_eq!(actual.len(), expected.len());
            for (index, event) in expected.iter().enumerate() {
                assert_eq!(actual.len(), expected.len() - index);
                assert_eq!(actual.next(), Some(*event));
            }
            assert_eq!(actual.len(), 0);
            assert_eq!(actual.next(), None);
            assert_eq!(actual.next(), None);
        }
    }
    for inventory in [loans.inventory().unwrap(), fees.inventory().unwrap()] {
        assert_eq!(inventory.owned_scenario_string_bytes, 0);
        assert_eq!(inventory.daily_schedule_vector_allocations, 0);
        assert!(inventory.sort_comparisons <= inventory.sort_comparison_upper);
        assert!(inventory.sort_metadata_copies <= inventory.sort_metadata_copy_upper);
        assert!(!inventory.complete_source_sim_codec_bounds);
    }
}

#[test]
fn immutable_order_matches_old_daily_sort_for_scenarios_fractional_ties_and_empty_days() {
    let mut rows = Vec::new();
    for n in 0..257 {
        let scenario = ["baseline", "stress", "stress-long-同じ😀"][n % 3];
        let day = (n * 7) % 31;
        let year = match n % 9 {
            0 => -0.0,
            1 => 0.0,
            _ => day as f64 / 365. + (n % 5) as f64 / 1_000_000.,
        };
        let event = if n % 2 == 0 {
            Scheduled::Loan(n % 11, n)
        } else {
            Scheduled::Fee(n % 7, n)
        };
        rows.push(Raw {
            scenario,
            day,
            year,
            event,
        });
    }
    check(&rows);
    rows.reverse();
    check(&rows);
    rows.rotate_left(113);
    check(&rows);
    check(&[]);
}

#[test]
fn coincident_loans_precede_fees_with_binding_and_group_ties_preserved() {
    let rows = [
        Scheduled::Fee(0, 0),
        Scheduled::Loan(1, 0),
        Scheduled::Fee(1, 0),
        Scheduled::Loan(0, 1),
        Scheduled::Loan(0, 0),
    ]
    .map(|event| Raw {
        scenario: "baseline",
        day: 0,
        year: 0.,
        event,
    });
    let (loans, fees) = build(&rows);
    assert_eq!(
        OrderedEvents::new(&loans, &fees, "baseline", 0).collect::<Vec<_>>(),
        [
            Scheduled::Loan(0, 0),
            Scheduled::Loan(0, 1),
            Scheduled::Loan(1, 0),
            Scheduled::Fee(0, 0),
            Scheduled::Fee(1, 0)
        ]
    );
}

#[test]
fn explicit_merge_sort_actual_operations_fit_closed_bounds_at_every_small_shape() {
    for size in 0..=256 {
        for pattern in 0..3 {
            let mut rows = (0..size)
                .map(|i| Entry {
                    day: match pattern {
                        0 => i,
                        1 => size - i,
                        _ => (i * 71) % 257,
                    },
                    year: (i % 7) as f64,
                    event: Scheduled::Loan(i % 13, i),
                })
                .collect::<Vec<_>>();
            let mut expected = rows.clone();
            expected.sort_by(ordered);
            let work = sort(&mut rows).unwrap();
            let upper = sort_upper(size).unwrap();
            assert_eq!(
                rows.iter().map(|r| r.event).collect::<Vec<_>>(),
                expected.iter().map(|r| r.event).collect::<Vec<_>>()
            );
            assert!(work.comparisons <= upper.comparison_upper);
            assert!(work.copies <= upper.metadata_copy_upper);
        }
    }
    assert!(sort_upper(usize::MAX).is_err());
}

#[test]
fn source_labels_are_borrowed_and_sealing_prevents_mutable_replay_indices() {
    let label = String::from("a reviewed scenario whose source owns this label");
    let mut index = Index::default();
    let first = index.register(&label).unwrap();
    assert_eq!(index.timelines[0].scenario.as_ptr(), label.as_ptr());
    assert_eq!(index.register(&label).unwrap(), first);
    assert!(index
        .push(first, 0, f64::NAN, Scheduled::Loan(0, 0))
        .is_err());
    assert!(index.push(99, 0, 0., Scheduled::Loan(0, 0)).is_err());
    index
        .push(first, 2, 2. / 365., Scheduled::Loan(0, 0))
        .unwrap();
    index.seal().unwrap();
    assert!(index
        .push(first, 3, 3. / 365., Scheduled::Loan(0, 1))
        .is_err());
    assert!(index.register("another").is_err());
    assert!(index.seal().is_err());
    assert_eq!(index.inventory().unwrap().entries, 1);
}

#[test]
fn daily_binary_boundaries_match_independent_partition_search_with_closed_step_limits() {
    for size in 0..=257 {
        let rows = (0..size)
            .map(|i| Entry {
                day: i / 3,
                year: 0.,
                event: Scheduled::Loan(0, i),
            })
            .collect::<Vec<_>>();
        for day in 0..=size + 1 {
            for inclusive in [false, true] {
                let mut comparisons = 0;
                let index = boundary(&rows, day, inclusive, || comparisons += 1);
                assert_eq!(
                    index,
                    rows.partition_point(|r| r.day < day || (inclusive && r.day == day))
                );
                assert!(comparisons <= search_upper(size));
            }
        }
    }
}

#[test]
fn expired_or_cancelled_native_control_refuses_sort_without_changing_metadata() {
    use portfolio_compute_control::{scope, Control};
    let mut rows = vec![
        Entry {
            day: 2,
            year: 0.,
            event: Scheduled::Loan(0, 0),
        },
        Entry {
            day: 1,
            year: 0.,
            event: Scheduled::Loan(0, 1),
        },
    ];
    let control = Control::new(1000).unwrap();
    control.cancel();
    let result = scope(Some(control), || sort(&mut rows));
    assert!(result.err().unwrap().contains("CANCELLED"));
    assert_eq!(rows[0].day, 2);
}
