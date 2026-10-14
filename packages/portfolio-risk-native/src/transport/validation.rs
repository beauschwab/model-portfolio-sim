//! Operation-specific shape and input validation before native dispatch.
use super::abi_operations::*;
use crate::math::arrays::View;

pub(crate) fn validate(op: u32, a: &[View<'_>]) {
    let count = input_count(op).expect("unsupported operation");
    if op != BEHAVIORAL {
        assert_eq!(a.len(), count);
    }
    let same = |indices: &[usize]| {
        for &i in indices {
            assert_eq!(a[i].shape, a[indices[0]].shape);
        }
    };
    match op {
        LMM => {
            assert!(a[3].shape.iter().all(|&n| n > 0));
        }
        CORPORATE | CD => {
            let off = if op == CORPORATE { 3 } else { 2 };
            same(&(0..off).collect::<Vec<_>>());
            assert!(a[0].shape[0] > 0 && a[0].shape[1] > 0 && a[0].shape[2] == 1);
            assert!(!a[off].v.is_empty());
            assert_eq!(a[off].ix(0), 0);
            assert_eq!(a[off].ix(a[off].v.len() - 1), a[off + 1].v.len());
            same(&((off + 1)..if op == CORPORATE { 12 } else { 9 }).collect::<Vec<_>>());
            for i in if op == CORPORATE { 12..17 } else { 9..13 } {
                assert_eq!(a[i].v.len() + 1, a[off].v.len());
            }
        }
        MORTGAGE | MORTGAGE_STRESS | MORTGAGE_BATCH => {
            same(&[0, 1, 2, 3]);
            let start = if op == MORTGAGE_BATCH { 15 } else { 13 };
            // nine per-pool vectors (terms, original HPI, static and speed multipliers) and OAS
            same(&(start..start + 10).collect::<Vec<_>>());
            if op == MORTGAGE_BATCH {
                assert_eq!(a[4].v.len(), a[0].shape[0]);
                assert_eq!(a[25].v.len(), a[start].v.len());
            }
            if op == MORTGAGE_STRESS {
                same(&[25, 26]);
                assert_eq!(a[25].shape[..2], [a[13].v.len(), a[0].shape[0]]);
                assert!(a[23].ix(0) > 0);
            }
        }
        DEPOSIT | DEPOSIT_STRESS => {
            same(&[0, 1, 2, 3]);
            same(&[6, 7, 8, 9, 10, 11, 12, 13, 16]);
            // terminal treatment code is the last input (deposit-terminal-1)
            assert!(a[a.len() - 1].ix(0) <= 2);
            if op == DEPOSIT_STRESS {
                assert_eq!(a[19].shape[..2], [a[6].v.len(), a[0].shape[0]]);
                assert!(a[17].ix(0) > 0);
            }
        }
        CSR_OAS | CSR_PV => {
            assert!(!a[0].v.is_empty());
            assert_eq!(a[3].v.len() + 1, a[0].v.len());
        }
        BEHAVIORAL => {
            assert!(!a.is_empty());
            let mode = a[0].ix(0);
            assert!(mode <= 3);
            assert_eq!(a.len(), if mode == 0 { 5 } else { 4 });
            if mode == 0 {
                assert_eq!(a[1].shape[1], 4);
                assert_eq!(a[2].shape, [6, a[1].shape[2], 1]);
                assert_eq!(a[3].v.len(), 11);
            } else {
                assert_eq!(a[1].shape[2], 1);
            }
            if mode == 2 {
                same(&[1, 2]);
            }
        }
        SWAPTION => {
            assert_eq!(a[2].shape[1..], [5, 1]);
            assert_eq!(a[1].shape, [a[0].shape[0], a[0].shape[2], 1]);
            assert_eq!(a[3].at(0), a[0].shape[0] as f64);
        }
        PROGRAM => {
            assert!(a[0].shape[0] > 0 && a[0].shape[1] >= a[1].v.len());
        }
        INCOME => {
            assert_eq!(a[1].v.len(), a[0].shape[0]);
            assert!(a[2].ix(0) <= a[0].shape[1]);
        }
        ACCRUAL => {
            same(&[1, 2, 3]);
            assert!(!a[0].v.is_empty());
            assert_eq!(a[0].ix(0), 0);
            assert_eq!(a[0].ix(a[0].v.len() - 1), a[3].v.len());
            for w in a[0].v.windows(2) {
                assert!(w[0] <= w[1]);
            }
        }
        MONEY_MARKET => {
            same(&[1, 2, 3]);
            assert!(a[0].shape[0] > 0 && a[4].ix(0) <= a[0].shape[1]);
        }
        VOLATILITY => {
            assert_eq!(a[0].shape[1..], [4, 1]);
            assert_eq!(a[1].v.len(), 4);
            assert_eq!(a[3].v.len(), a[2].v.len() + 1);
            assert_eq!(a[4].shape[0], a[2].v.len());
        }
        CURVE => {
            assert_eq!(a[0].v.len(), a[1].v.len());
        }
        RATE_PATHS | MORTGAGE_PATHS => {
            assert_eq!(a[0].v.len(), a[1].v.len());
            assert_eq!(a[2].v.len(), 4);
            assert_eq!(a[3].shape[1..], [a[4].shape[2], 1]);
            assert_eq!(a[5].v.len(), 3);
            if op == MORTGAGE_PATHS {
                assert_eq!(a[6].shape, [6, 2, 1]);
                assert_eq!(a[7].v.len(), 11);
                assert_eq!(a[9].v.len(), 4);
                assert_eq!(a[12].v.len(), 3);
                for i in [10, 11] {
                    assert_eq!(a[i].shape, [a[4].shape[0], a[4].shape[1], 1]);
                }
            }
        }
        PARALLEL_SHOCK => {
            same(&[0, 1, 2]);
            assert_eq!(a[0].shape[2], 1);
            assert_eq!(a[3].v.len(), 6);
            assert_eq!(a[4].v.len(), 11);
        }
        FIT_CC => {
            assert_eq!(a[0].shape[1..], [11, 1]);
        }
        FIT_PS => {
            assert_eq!(a[0].shape[1..], [1, 1]);
        }
        FACTOR_LOADINGS => {}
        FIT_VOL => {
            assert_eq!(a[0].shape[1..], [3, 1]);
            assert_eq!(a[1].shape[1..], [1, 1]);
            assert_eq!(a[2].shape[1..], [1, 1]);
            assert_eq!(a[3].shape[0], a[1].v.len());
            assert_eq!(a[3].shape[2], 1);
            assert_eq!(a[5].v.len(), 2);
        }
        FIT_DEPOSIT => assert_eq!(a[0].shape[1..], [2, 1]),
        SHARED_DRAWS => {
            assert_eq!(a[0].shape[1..], [1, 1]);
            assert!(a[0]
                .v
                .iter()
                .all(|&x| x >= 0. && x <= u32::MAX as f64 && x.fract() == 0.));
            assert_eq!(a[1].v.len(), 3);
        }
        MORTGAGE_RISK | MORTGAGE_FORWARD_STRESS => {
            assert_eq!(a[2].shape[1..], [3, 1]);
            assert_eq!(a[3].shape[1..], [11, 1]);
            assert_eq!(a[5].shape[1..], [13, 1]);
            assert_eq!(a[8].v.len(), 17);
            assert_eq!(a[16].v.len(), 2);
            assert!(a[7]
                .v
                .iter()
                .all(|&v| v >= 0. && v <= u32::MAX as f64 && v.fract() == 0.));
        }
        MORTGAGE_CACHE => {
            assert_eq!(a[0].v.len(), 1);
            assert!(a[0].at(0) == 0. || a[0].at(0) == 1.);
        }
        INTERACTIVE => {
            assert_eq!(a[0].shape[1], 10);
            assert!((1..=360).contains(&a[0].shape[2]));
            assert_eq!(a[0].shape[0], a[1].v.len());
            assert!(a[1].v.iter().all(|v| v.is_finite() && *v >= 0.));
            assert!(a[0].v.iter().all(|v| v.is_finite()));
            assert!(a[2].v.is_empty() || a[2].v.len() == 13);
            if !a[2].v.is_empty() {
                assert!(a[2].v.iter().all(|v| v.is_finite()));
                assert!(a[2].at(0) > 0.);
                assert!((0.0..1.0).contains(&a[2].at(10)));
            }
        }
        _ => unreachable!(),
    }
}
