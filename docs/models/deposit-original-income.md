# Original-yield deposit dated income

`deposit-graph-income-ledger-1` is a separate reviewed purchase-date accounting
source. The existing `deposit-graph-dated-ledger-1` par/simple-interest lane keeps
its behavior and serialized contract. See [the complete synthetic example](../../examples/models/deposit-graph-income-ledger-1.json).

The request retains the raw deposit graph, owners, explicit run mapping, ledger
specification, recognition policy and one `income_anchors` entry per position.
Each anchor supplies purchase date, quote identity/revision, price per 100,
original principal, original gross carrying value, original annual effective yield
and its own identity/revision. Purchase must equal graph as-of date. The original
quote must reconcile carrying value to face times price/100. Seasoned opening
states require an independently replayed purchase-to-date history and remain
unsupported here. The anchor is not a current market mark and is never replaced
with pricing-measure PV or calibrated OAS.

The capture policy is explicitly `deposit_daily_original_eir-1` with
`original_expected_cash_deposit-1`, rejecting fees. Basis revision policy is
`reject_changed_cash_expectations-1`; terminal policy is `reject_nonzero-1`.
The complete original physical expected bank payment schedule discounted at the
supplied original yield must equal original carrying value. Every physical
scenario must retain exactly the same original dated cash rights. Different
pricing histories and scenario market valuation remain separate and preserve the
graph's base OAS. Changed physical cash expectations are rejected rather than
treated as a catch-up, prospective yield revision or terminal gain/loss. No yield
is solved from Q. There is no empirical or stochastic calibration claim for the
supplied paired histories.

## Recognition and bank liability direction

The native source reuses existing deposit state projection once and retains its
complete contractual cash and daily earned-interest origins. Original performing
runoff/closure may be included in the purchase-time expected schedule. It cannot
be scaled again by legacy flight stress. Migration, external inflow, opening
accrued interest, negative physical paid rates/floors or original yield, fees and
modifications require additional ownership and are rejected in this route.

For positive liability carrying value `C`, annual effective original yield `y`,
and actual calendar interval `dt` days, effective expense is
`E = C * ((1+y)^(dt/365) - 1)`. The core `dated_income` recurrence uses stable
`log1p`/`expm1`, with one interval per actual day and a separate day-zero cash
action if present. Positive contractual earned interest `I` increases accrued
liability. Positive bank-payment magnitudes `Pcash` and `Icash` reduce principal
and carrying value. The inherited generic income result field `cash_received`
contains that positive payment magnitude in this liability wrapper; the retained
physical graph/capture contains actual signed negative bank cash.

The rollforward is `Cnext = C + E - Pcash - Icash`. Basis change is `E-I`, separate
from contractual accrued rights. Signed captured earned interest is `-I`, and
signed book recognition is `-(E-I)`, so ledger income is `-E` and bank expense is
`E`. Opening positive book adjustment is `C-face`, with negative GL polarity for
the liability. Cash settlement releases earned payable once; zero-cash daily
basis events do not invent payments. Every daily basis origin, including exact
zero, retains stable identity rather than depending on a numerical zero test.

Full original cash, principal, contractual accrued rights, carrying value and
book basis must close within existing monetary tolerances. The source adds no
terminal cash, catch-up or closing adjustment. Both discount and premium quotes
are admitted when the original supplied yield/cash/quote identities reconcile.
The current synthetic premium example uses face 1,000, paid contractual interest
one per day for 31 days, original yield 0.10 and initial carrying value
`1031/(1.1^(31/365))`. Day-30 effective expense is
`C*(1.1^(30/365)-1)`, unpaid contractual interest is 30, and cash is still `C`.
Day 31 pays 1,031 and leaves zero principal, interest payable and basis.

## Retained source and independent gates

`deposit_graph_capture::income::capture_original_income` returns capture schema
`deposit-original-income-capture-1`, original cash origins and `income_sources`.
Each income source retains position/scenario/run, its complete original anchor,
the derived native `dated-income-1` request and the complete native result.
`deposit_ledger::income::prepare` returns a fully admitted dated ledger plus source
receipt `deposit-graph-income-ledger-source-1` with raw request, graph, capture,
cash origins and income sources. Normalization policy is
`native_original_eir_deposit_opening_anchors-1`; conflicting explicit opening
anchors reject, and the raw ledger specification remains immutable.

`deposit_income_reference` independently reconstructs physical deposit cash and
earned origins, the original quote/yield consistency, each scalar income interval,
signed book events and full terminal closure. It compares the entire capture,
including extra-event rejection, and the prepared opening/specification. It sets
`physical_deposit_source_recomputed`, `original_deposit_income_recomputed`,
`original_yield_anchors_retained` and `accrual_origins_retained`. Pricing graph and
empirical verification remain false. Existing independent persisted dated journal
and report replay is a separate gate; API publication repeats source and stored
partition verification under the unchanged deadline/cancellation and identity
fences. No Python financial execution backend is introduced.

Native and independent fixtures include premium/discount hand values, original
competing runoff, cash-versus-earned-versus-effective expense, complete closure,
zero origins, explicit anchor preservation, changed expectations and source
tamper rejection. Daily income events are bounded to 4,096, complete tails to
4,095 days, additional income work to 250,000 daily intervals, and combined
recognition segments to 250,000. Existing graph/capture/ledger input, retained-byte
and daily work budgets also apply with cooperative checkpoints. Component limits
are not aggregate RSS or combined 60,000-position acceptance evidence.

The [IFRS Interpretations Committee February 2022 update](https://www.ifrs.org/news-and-events/updates/ifric/2022/ifric-update-february-2022/)
discusses original effective yield and initial expected cash estimates for a bank
financial liability. The [IASB September 2025 staff paper](https://www.ifrs.org/content/dam/ifrs/meetings/2025/september/iasb/ap11b-subsequent-changes-effective-interest-rate.pdf)
distinguishes floating-rate re-estimation and other cash-flow estimate changes.
These are primary context for keeping policies explicit; the staff paper is not
an issued standard, and this implementation asserts no accounting-standard
compliance. Reviewed revised expectations, liability derecognition, seasoned
replay, migration/inflow/fees/negative ownership, empirical fit and detailed
reverse journals remain open requirements.
