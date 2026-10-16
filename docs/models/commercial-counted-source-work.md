# Counted native commercial source work

The explicit `physical_path_inputs.source_work_policy` value
`counted_native_contract_factor_work-1` selects structural factor admission for a
complete generated Physical catalog. It changes admission, never coupon arithmetic,
cash, original yield/OAS, CRN, sample counts, observability or monetary tolerances.
Omission preserves the historical 250,000-work factor floor, 36-row generic cash
reserve, serialized results and admission formulas. A direct raw credit/source
request cannot select this tier or supply a numeric work allowance.

The native-only `GeneratedCatalogAuthority` binds the exact serialized compiler
request after catalog/source/clock expansion. `serde(skip)` plus unknown-field
rejection prevents raw authority injection. Source, original anchor, clock,
draw date and path-key changes invalidate it. Legitimate native path generation
and original-source alignment create a fresh binding after their edits. A token
from one source cannot reduce another source's bound.

## Reusable structural census

`term/contracts/indexed/work.rs` owns checked-integer `ContractWorkSummary`,
`FactorUseBound` and `WorkMeter`. `NativeFactors` uses the same summary for fixed,
indexed, main and independently acquired draw sources, for economic credit and
full accounting replay. There is no special fixture allowance.

Let P be coupon periods, O indexed observations, S native factor segments (one
per fixed period or indexed observation), q the admitted earned queries
(two per credit sojourn), and A the sum of squared observation counts within
observable indexed periods. Let B be the structurally bounded source JSON size.
Every number reserves 32 bytes; strings use actual escaped UTF-8 bytes and JSON
keys/punctuation are counted. Generated path/model identities reserve the full
128-byte escaped identity domain before realized numerical generation.

For observable prefixes set c=3q; otherwise c=q. Selected terminal ownership with
q>0 adds S+1 canonical calls and one terminal scan. Observable sources with q>0
reserve h=2q+68+66S pending queries, otherwise h=0. These reserves include native
prefix calls inside earned queries and terminal decomposition. Call counts and
loop counts are independently metered; a second query family cannot spend a
first family's allowance.

| Operation family | Safe upper count |
| --- | ---: |
| Input byte scans | B |
| Output/descriptor byte scans | 8B + 8192(P+S+1) |
| Period/observation/segment construction | P, O, S |
| Stable payment ordering | P squared |
| Coupon/audit maps and observable observation binding | 2P squared + 2PS + A |
| Whole builder validation | 4P squared + 32 |
| Earned descriptor scans | q(3S+P) |
| Canonical descriptor/audit scans | c(PS+S+P) |
| Pending fragment scans | hS |
| Terminal decomposition | S squared + PS when selected |

Map/tree operations reserve height at most P and identity comparisons at most
128 bytes. Hash-based validation reserves worst-case pair comparisons, rather
than assuming uniform hashes. The builder's validation counter records that
whole worst-case component; sort comparisons, descriptor/query loops, emitted
segments, serialized bytes and actual calendar calls are counted at their
execution sites. Output strings that repeat per audit/segment fit the 8192-byte
per-record envelope (bounded 128-byte control-free term keys and fixed numeric
schemas); the repeated top-level source metadata fits the separate 8B term.

Work is `16*input_bytes + output_bytes + 128*loop_steps + 256*calendar_cells`.
Call counters enforce method multiplicity; their bodies are already included in
loop/calendar counts. Every multiplication/addition checks overflow. All meters
are live, fail closed before returning an over-budget result, and retain their
own independent counters. The source's existing 8-million-work/32-MiB and joined
carry/book gates remain.

## Calendar cells

For ACT/360 or ACT/365F a calendar span reserves one cell. ACT/ACT ISDA reserves
`ceil(span_days/365)+2`, within the unchanged 36,600-day domain. Builder cells
cover every full coupon and both interest/observation spans of every indexed
observation, even when a span is unnecessary for the chosen convention.

The per-query calendar bound sums one full interest-span allowance plus one
partial-day cell per fixed/unshifted segment. A legacy shifted indexed segment
reserves interest plus observation allowances plus two partial cells. An
observable shifted segment reserves twice each allowance plus two partial
cells. Builder cells plus `(2q+c)*query_cells` cover legacy calls; observable
calls use builder cells plus `c*query_cells` because earned is implemented by
canonical-prefix differences. Runtime meters count actual Gregorian January
splits, not a constant-time assumption for long ACT/ACT periods.

## Genuine owner passes and degree

A generated Pricing role creates Ntrain training owners and two Neval evaluation
owners (observed prefix and accepted stopped full tail). Prospective payoffs use
the retained legal/principal state; they do not generate another source.
Original Physical paths reserve four owners, including original acquisition
proof and any original reviewed-book validation. Current Physical paths reserve
two owners. Every owner constructs both original/current main sources and both
original/current sources of every declared draw. No actual pass is removed
because a particular fixture emits no coupon/default cash.

Original OAS calibration collects paths once, refits retained Pricing data for
each trial, and replays accepted stopped outcomes once. Source construction is
not repeated for every trial, while fitting, discounting and candidate/graph
scenario/bump work remain charged. Linear/quadratic/cubic column counts use the
selected polynomial degree and canonical feature inventory; all Physical laws
must match the Pricing regression policy. Tests count actual owners and source
constructions and compare their summed owner admission to the reserved work.

The expanded credit DTO has its own structural size C, including daily service
intervals and metadata. Selected owner work replaces only the existing raw-byte
term with `24R+4C`, where R is the compiler-request structural upper: 8R is the
ordinary source scan and 16R covers exact authority encoding/comparison. Actual
carry raw bytes must fit C. Joined owner bytes reserve `20*(R+C+native_stop_bytes)`
plus authority and book bytes; all existing economic, factor, carry, accounting,
reviewed-action and full-tail work remains charged.

## Retention and actual sharing

Each source adds the exact `size_of(ContractWorkSummary)+size_of(WorkMeter)`
(including the summary duplicated inside the meter) to retained admission.
A source always has P>=1 and S>=1; its existing 4096-byte source cells additionally
cover complete inline source/owner headers. A native test checks the complete
`NativeFactors` header fits one such cell. The new compiler pointer, full native
stop header, bounded event/review/global-generation labels, String capacity and
allocation headers are checked against the existing 8192-byte recipe block;
per-observation model/path expansion retains its independent 2048-byte reserve.

Each selected raw recipe additionally reserves the authority body's actual byte
length, its boxed/Arc headers and the compiler pointer. Retained law authorities,
Q training/evaluation recipes, original proof/raw bindings, current NativeSource
clones and maximum live expanded accounting owner coexistence are admitted.

A deduction is permitted only for two sealed clones whose bodies satisfy
`Arc::ptr_eq`. It subtracts merely a structural *lower* bound on the second
body (full key/punctuation bytes, empty-string/numeric minimum values), leaving
all clone headers, summaries and remaining bytes charged. Independently
allocated equal JSON, different sources or different path bodies get no sharing
deduction. The original proof remains the same sealed Arc-backed authority.
This is a component/phase capacity bound, not an aggregate process RSS guarantee.

## Complete cash inventory

The selected tier uses one shared bound for actual extracted `EconomicCash`:
`Ptotal + D + T + 4 + (terminal ? Stotal*(T+1) : 0)`.
Ptotal/Stotal include main and all draw sources; D is each draw acquisition;
T is the longest supplied complete recovery tail, including zero-hazard terms.
Ordinary default recovery rows already aggregate across amounts into that tail.
Independently retained terminal coupon fragments can each produce T recoveries
and one settlement; their product is mandatory. The four residual rows cover
native terminal/default transfer, maturity/exit and conversion funding. All
actual extracted Q/original/current stopped cash checks this bound. Signed
multidraw defaults, staggered settlement, each supplied recovery lag and
before/after-cash exercise have native emitted-row controls. Omission retains its
historical generic 36 reservation.

## Evidence and limits

Native controls cover private/stale authority, fixed/indexed exact numerical
parity, ACT/ACT cells, shared versus independently allocated bodies, every
operation-family overrun, byte escaping, genuine owner/source pass counts,
calibration for each degree, and signed/default/stopped cash multiplicity.
Independent Python structural controls compare actual native operation receipts
against a separate census. Independent financial replay reconstructs raw source
cash, original acquisition proofs, ordered journal postings and immutable
Parquet partitions for the minimal two-scenario, 128-path-per-scenario draw
candidate. Its validated native receipt reserves **3,263,234,384 work units**
and **108,267,865 bytes** against unchanged 4B/128MiB limits (256 Physical
paths, complete day-30 outer journal, day-1 draw repaid on day2). The larger 512-path selection still refuses before publication;
the omitted minimal candidate retains its exact historical 4,800,566,464 work
refusal. Caps, sample minima, source identities and deadline/publication guards
remain unchanged.

These synthetic controls establish bounded native ownership and independent
replay for this selected tier. They do not establish empirical calibration,
accounting/legal approval, deployment, broad secondary acquisition rights or
complete issue-15/25 mixed-book scale acceptance. Rebuild the native artifacts
and restart deployed workers before selecting the additive policy.


At the isolated 2026-10-16 checkpoint, all 145 native risk library tests,
strict risk and decision all-target Clippy checks, 27 generated Physical/candidate
persisted controls, 82 observable/stochastic/calibration controls, 8 independent
census/DTO controls, and architecture/native/HTTP/MCP contract checks passed.
The standalone analysis/journal checks used matching isolated debug binaries,
not replacement production artifacts. Native source identity over 410 ordered
Cargo/source inputs was
`05526e9b7ba2d862e571b080218cfcbc78bd7535171d0813433ffa301b10e85d`;
engine Python `source_identity()` was
`47dc205115a3efb95d88e2923402b47acb7ef88ca88739ad1b41c9fd23f907bf`.
Each native input contributes its relative UTF-8 path then LF-normalized bytes;
the native set covers Cargo manifests/locks and src Rust files in risk, core,
compute-control, ledger and decision crates. The executable remains separately
pinned and hashed; these source hashes do not replace artifact fences.
