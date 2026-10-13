use portfolio_ledger_native::{dated_store::RecognitionSegment, deposit_fifo_derivation as new};
use portfolio_model_core::deposit_state as source;
// Historical output-order control, not an independent financial oracle.
fn old(s: &source::DepositStateResult) -> Vec<Vec<RecognitionSegment>> {
    let mut remaining: Vec<_> = s.interest.iter().map(|i| i.amount).collect();
    s.cashflows
        .iter()
        .map(|c| {
            let mut needed = -c.cash_interest;
            let full = matches!(c.kind.as_str(), "interest_payment" | "contractual_maturity");
            let mut segments = Vec::new();
            for (i, o) in s.interest.iter().enumerate() {
                if needed <= 0. && !full {
                    break;
                }
                if remaining[i] <= 0. {
                    continue;
                }
                if o.end_ordinal > c.ordinal {
                    assert!(portfolio_model_core::deposit_continuation::near(needed, 0.));
                    break;
                }
                let allocated = if full {
                    remaining[i]
                } else {
                    needed.min(remaining[i])
                };
                if allocated > 0. {
                    if let Some(p) = segments.last() {
                        let p: &RecognitionSegment = p;
                        if p.end_ordinal < o.start_ordinal {
                            segments.push(RecognitionSegment {
                                start_ordinal: p.end_ordinal,
                                end_ordinal: o.start_ordinal,
                                accrued_interest: 0.,
                                book_amortization: 0.,
                            });
                        }
                    }
                    segments.push(RecognitionSegment {
                        start_ordinal: o.start_ordinal,
                        end_ordinal: o.end_ordinal,
                        accrued_interest: -allocated,
                        book_amortization: 0.,
                    });
                }
                remaining[i] -= allocated;
                needed -= allocated;
            }
            assert!(portfolio_model_core::deposit_continuation::near(needed, 0.));
            segments
        })
        .collect()
}
#[test]
fn all_32_native_sources_preserve_all_cash_segments_exactly() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../portfolio-model-core/tests/fixtures/deposit-principal");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|r| r.unwrap().path())
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 32);
    for path in paths {
        let raw: source::DepositStateRequest =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let s = source::simulate(&raw).unwrap();
        let p = new::compile(&s, raw.as_of_ordinal + raw.horizon_days as i32, &mut || {
            Ok(())
        })
        .unwrap();
        let actual: Vec<_> = p
            .cash
            .iter()
            .map(|r| p.segments[r.segment_start..r.segment_end].to_vec())
            .collect();
        assert_eq!(
            serde_json::to_vec(&actual).unwrap(),
            serde_json::to_vec(&old(&s)).unwrap()
        );
        assert!(p.segments.len() <= p.segment_bound);
        assert!(p.capacity_used <= p.capacity_reserved);
        assert!(p.work_used <= p.work_reserved);
    }
}
#[test]
fn incomplete_source_end_is_refused_even_with_zero_right_cash() {
    let raw: source::DepositStateRequest = serde_json::from_slice(include_bytes!(
        "../../portfolio-model-core/tests/fixtures/deposit-principal/canonical-original.json"
    ))
    .unwrap();
    let s = source::simulate(&raw).unwrap();
    assert!(new::compile(&s, raw.as_of_ordinal, &mut || Ok(())).is_err());
}
#[test]
fn preflight_cancellation_cannot_return_partial_arena() {
    let raw: source::DepositStateRequest = serde_json::from_slice(include_bytes!(
        "../../portfolio-model-core/tests/fixtures/deposit-principal/canonical-original.json"
    ))
    .unwrap();
    let s = source::simulate(&raw).unwrap();
    assert!(new::compile(&s, i32::MAX, &mut || Err("cancelled".into()))
        .unwrap_err()
        .contains("cancelled"));
}
