# Native commercial cut state component (2026-10-16)

This component moves the actual native daily Sim after its reconciled cut day
into a distinct resumed Sim. It retains the original calendar, gross loan/fee
rights, cash, basis, policy counters, native owner tracking and dictionary
identity. It never accepts a caller monetary checkpoint, repeats acquisition,
resets source time or reconstructs a fresh opening from financial JSON.

The component emits protocol 8. A balanced cut-opening journal exposes the
carried GL without reposting it into runtime money; subsequent transactions
continue the original sequence. Existing full-origin protocols 6/7 retain their
execution path. Native prior-publication verification remains false: a durable
coordinator needs separate independent source and persisted-publication evidence.

Revision validation requires identical surviving specifications, positions,
owner order, allocations, original binding identities and observed ordered loan
and fee groups. New candidates must have zero-opening exclusive commercial AC
ownership and an actual native acquisition after the cut. Complete horizons and
all non-position specification fields must match. Reverse runs and incompatible
ownership refuse. The existing source coordinator must retain original pricing,
physical, effective-yield and purchase anchors; this component cannot infer them.

Both source owners coexist during revision checking and replay. The native
component admits at most 8 million work units and 32 MiB of retained and temporary
capacity. Its work includes both numeric owner bounds, raw specification scans
and eight times both retained source bounds for canonical size/encoding/equality
passes. Capacity includes twice both retained owner bounds for comparison buffers,
4096 bytes per revised position and twenty times both raw specification sizes.
Overflow and oversize refuse before comparison allocation or prefix output.
The outer coordinator must separately admit graph preparation, retained owners,
transport/output and this component before either source generation. These are
component reservations, not an aggregate process RSS guarantee.

Validation on the pinned component/dependency tree: all 208 native ledger tests
pass; all-target/all-feature Clippy passes with warnings denied. Eight new cut
controls cover exact future ordered journal, trial balance and closing parity at
multiple cuts; new future acquisition; zero/half/full allocation; signed live
prepaid fee rights; immutable history/allocation/calendar and past acquisition
refusals; checked work/capacity overflow; and cancellation after visible output
with no completion trailer. Hand-constructed loan cash reconciles to 1001, and
signed fee cases to 1000 + allocation*(1 + signed fee). Two report controls retain
acquired fee liabilities and their positive valuation offsets as gross liabilities.
Historical full-output compatibility remains in the passing native suite.

The source receipt pins 303 source, dependency and fixture inputs, rechecked after
both gates. Source-freeze SHA256:
`f64c5ccc9c9db03bc92cb4559a87ca5f4bba4591272795fd361f6d54ed89711e`.
The contained logs and receipt are in
`artifacts/cut-ledger-component/`; installed production artifacts were not replaced.

This is component acceptance. Raw graph/coordinator admission, independent source
and persisted replay, durable final publication, mixed saved-book/cohort/strategy
adoption, observed validation and complete issue #15/#25 scale gates remain open.
No full product completion, accounting/regulatory approval or deployment is claimed.
