# Independent tagged product-event publication gate

`analytics.product_event_reference.verify(raw, result, check)` is an independent
Python verification oracle for `product-mixed-product-credit-1`. It is not an
execution backend. Production source generation and daily financial execution
remain Rust-owned.

The oracle reconstructs the original performing dated backbook and candidate
sources, the tagged installment/card physical histories, frozen original-yield
income, explicit acquisition and initial-allowance reviews, and reviewed
default/chargeoff/gross-recovery/cost intents. It merges source events by actual
date into one pooled cash and GL state. It never concatenates standalone journals
or accepts producer source arrays or reconciliation flags as economic evidence.

`product-event-owner-1` retains original owner/run/path identities and tagged
`{kind: installment|card, request: raw}` inputs. Each unweighted complete source is
verified before applying its physical weight and allocated-face factor once.
Zero weight or allocation never removes an incomplete claim or cash/right/basis
tail. Complete consumer/card source receipts, statement indices, commitment
retirement, original anchors and acquisition transaction lineage are compared.

The gate compares every ordered journal row, trial balance, closing statement,
dated right, source event, acquisition, exposure, daily earnings/cash report and
reporting observation, plus `product_sources`, `product_daily`,
`product_lineage` and `product_acquisition_lineage`. Dates, indices, protocol and
work/capacity identities remain exact integers; financial comparisons retain
the existing absolute1e-9 plus relative1e-12 tolerance. Input/result objects stay
immutable. Cancellation is checked throughout admission and reconstruction.

Admission independently charges raw JSON32MiB, tagged primitive1MiB, source
generation work, actual escaped source string metadata, a conservative1080-day
replay reservation and the complete performing-source capacity against unchanged
8 million work/128MiB budgets. The result32MiB cap is separately checked. Bounds
include zero-weight and zero-allocation paths. These are explicit component
admission limits, not aggregate process RSS or60,000-position acceptance.

This route admits supplied physical acquired performing installment/card sources
with exclusive reviewed repayment/default/recovery policies and zero generic
credit/fee/mark overlays. Card grace, draws, fees and unsupported ownership reject.
Initial allowance requires an explicit reviewed expense/contra posting. The
unchanged principal-plus-basis guard still rejects the par-cost card fixture with
P0/Apositive/Bnegative; the richer artifact basis-rights proof is excluded.
Pricing measure, empirical calibration, accounting-standard approval, joint
stochastic adoption and inferred regulatory default classifications are excluded.

The publication receipt is reproducible and contains `verified`,
`mixed_product_credit_only`, `ordered_mixed_journal_recomputed` and
`complete_cash_rights_basis_tail_verified`, with `empirical_validation` and
`dynamic_credit_acceptance` false. Native producer verification flags remain
false. API persisted verification and immutable publication are separate gates.

Current evidence:84 checks pass (20.13 seconds), comprising67 independent checks
and17 fresh actual production DEBUG Decision CLI checks. They include complete raw-source hand
cash, ordered-table tampering, strict float/date/index identities, incomplete
zero-weight tails, once-only weights, future acquisitions, positive/negative
original-yield interest-only closure, same-day and net-zero recovery, cancellation,
escaped capacity, and exact old consumer-route financial compatibility. The native
cases independently check every emitted table for the canonical mixed book and
nine accepted variants, and reject seven unsupported inputs. Canonical complete
closing cash/equity is999.2506993647357. The explicit128MiB budget is preserved;
four complete14-day product paths reserve72,724,480 bytes and292,156 work units,
plus the full performing preparation and replay. The independent receipt does
is a source/journal receipt, separate from artifact identity and persisted
publication verification. The rebuilt official Decision release also passes all
84 checks within a 300-check consumer/performing/card regression group. The
28 new durable primitive/mixed/capital/failure-fence checks and 37 browser cases
pass; the complete bundle passes 1,627 engine and 445 API tests. The separate
public card primitive93-check suite passes against the official Risk release.
