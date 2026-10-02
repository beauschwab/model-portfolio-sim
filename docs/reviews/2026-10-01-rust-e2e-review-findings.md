# Full Rust process review: confirmed findings

Historical read-only review before the tape/cohort feature. All three findings
are fixed in 0.29.1; see 2026-10-01-cohort-followthrough.md for regression evidence.
The descriptions below preserve the original reproductions. The focused existing workflow/decision/graph/
controller suite passed 50 tests with 2 skips. Separate reproductions found:

1. **P1 â€” Refuse cache admission after eviction exhausts recency lists.**
   `packages/portfolio-risk-native/src/graph_cache.rs:149-157` admits based on
   currently shared identities, then evicts those identities. The incoming
   protected result may no longer fit even in an empty cache, and `expect`
   panics. A one-loan public pricing request with one path, one-month horizon,
   `max_bytes=3250` and `max_entries=8` reproduces this on the reviewed build.
   Subsequent requests using the same handle fail with `graph cache poisoned`.

2. **P2 â€” Enforce the workflow deadline through persisted replay/publication.**
   `analytics/owned_workflow.py:89-101` checks cancellation after `_finalize`,
   but not the elapsed deadline. With timeout 2 seconds and an injected
   3-second finalization delay, the workflow returned a published manifest at
   3.257 seconds. Carry an absolute deadline into finalization and check it
   before the atomic publication operation.

3. **P2 â€” Skip the asset-cap replay row when its RHS is null.**
   `strategy/decision.py:144-147` unconditionally constructs a cap check even
   though Rust accepts `max_total_assets=None`. A native DecisionSession with
   that value and a separate finite commercial ALL_ASSET bound solves, then
   raises `TypeError: bad operand type for abs(): 'NoneType'` in Python replay.

The review traced raw inputs, dependency caches, fixed-OAS pricing, transactional
decision edits, C++ HiGHS invocation, accounting capture, saved mapping, daily
replay, and immutable publication. The full 60,000-position acceptance workload
was not rerun as part of this review.
