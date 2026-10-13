# Native loan and separate fee journal

Local0.29.17 adds explicitly selected `commercial-fee-journal-1`, transported by
`analytics.model_contracts.analyze_commercial_fees` to `portfolio-strategy` and by
durable `/models/runs` jobs. It joins original-tranche service fees to the same
physical commercial loan paths and existing daily Sim. Loan principal, original
cost/EIR, contractual coupons, credit draws, allowance and LGD retain their own
owners. Separate fees are never silently included in the loan quote or basis.

The request supplies `loan` (the existing `commercial-credit-ledger-1` request)
and one `commercial-native-fees-1` policy per commercial source, in source order.
Each fee retains its exact position, original tranche, service, classification
review and source identity. Supplied events contain only service earning, cash
and permitted prepaid refunds. Caller default/exercise/waiver/recovery money is
rejected. The complete supplied tape must be valid before loan generation.

Rust generates each loan owner once, then binds fee dispositions to that owner's
actual first default/prepayment event or explicit reviewed maturity event. A
performing contract without a reviewed maturity event does not invent one merely
because its principal settled. Sealed conditional exercise owners are recognized
by exact path/event identity in the binder; this public joint request does not
yet accept an exercise graph or acquired exercise candidate source.

Every exit category supplies a separate review and explicit cancellation,
unpaid-earned waiver, prepaid-refund and recovery policy. At the exit, let E be
signed earned service, W signed previously waived service and K net service cash.
For entitlement sign s, eligible unpaid earning is max((E-W-K)s,0)s and eligible
prepaid cash is max((K-E+W)s,0)s. Apply separately supplied fractions to those
rights. Fee recovery applies only to the newly waived fee, with independent
fractions and actual-day lags; loan LGD and coupon recovery cannot supply it.
Recovery fractions total at most one. Refund policy must agree with fee terms.

The required `native_exit_before_coincident_service_cash-1` policy retires eligible
future events and posts reviewed dispositions before service/cash at the identical
native clock. Native years remain unchanged. Integer ACT365 dates retain their
exact calendar day; fractional events use the first day-end at or after their
native timestamp. A just-later floating-point year cannot round onto the preceding
day. Loan groups precede fee groups at an identical global clock; original group
and binding order then resolves ties. No timestamp nudges or tolerance changes
are used.

The one Sim posts separate fee receivable/payable, deferred income/prepaid expense,
income, writeoff and recovery accounts. It checks every absolute gross fee snapshot
against accumulated postings and reconciles them alongside loan stocks. Source
weight and face allocation are applied exactly once. Zero participation retains
the complete unweighted source policy. Separate signed fee rights remain gross.
Complete fee cash and recovery tails survive principal settlement and the original
report date. The root retains that reporting cutoff; the loan receipt resolves
the complete simulation horizon. Loan receipt admission covers the loan slice;
root admission additionally charges fee preparation, postings and decoded output.

Protocol7 completion and `commercial_fee_lineage`, `commercial_fee_daily` and
`commercial_fee_closing` accompany the existing journal/reports and loan lineage.
Historical protocol6 and all earlier schemas remain separately readable unchanged.
Independent verification recomputes the raw loan credit/carry/accounting owners,
binds raw fee policies, replays ordered loan and fee postings, checks gross daily
and closing stocks and derives every pooled cash/equity/trial-balance report.
The same check runs after persistence. Original source/command/executable identity,
integer native timeout, queue deadline, cancellation, lease/token/revision/attempt
and final SQL object rereads remain mandatory. Rehashed alterations cannot publish.

Input is bounded to2MiB and encoded output to32MiB. Each fee policy admits64 fees,
512 supplied events,256 physical paths and1080 days. The joined loan/source/fee/
journal tier retains8M work and128MiB capacity; the compiled daily component
retains8M/32MiB. These are admission budgets rather than aggregate RSS guarantees.
Every source and exit-policy bound is checked before loan path generation; final
actual complete-tail and serialized-output checks remain required.

This lane handles exclusive loan owners and cash accounts with separately owned
original service fees. Issuer securities use their existing candidate route;
draw/secondary fee ownership, earned refunds, acquisition fee allocation,
conditional exercise/candidate/holding adoption, fee Pricing graphs/units,
capital/FTP, saved/cohort adoption, daily in-arrears indexed earning and generated
streaming remain separate work. Native flags explicitly retain those boundaries.
Synthetic controls establish numerical identities, not observed predictive
accuracy or reviewed accounting/regulatory approval. Complete15/25 gates and
full60000-position mixed graph/credit/solver/journal acceptance remain required.

Rebuild matching product, ledger and dependent decision/workflow artifacts,
restart deployed API/workers and explicitly rebuild selected libraries/sessions
before additive execution. Local validation performs no deployment or restart.
