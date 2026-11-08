# Ongoing liquidity cashflows: local implementation and validation

This change completes the selected ongoing economic-cashflow follow-through under #20/#51.
The explicit `liquidity-lifecycle-1` lane now recognizes held-swap marks and coupons,
reviews/settles current-population VM, and accrues/pays reviewed facility interest.
Rust owns preparation, execution, allocation and independent verification. The service
transports and publishes the result; React displays verified monthly amounts.
See [the contract](../models/liquidity-lifecycle-contract.md#ongoing-derivative-and-facility-cashflows-20--51).

## Behavior and independent evidence

The committed `packages/portfolio-decision-native/tests/fixtures/lifecycle-ongoing.json`
starts the lifecycle owner with cash 1,000, draws/repays 100, pays a swap coupon of -4,
and incurs 0.30 of facility interest. Its lifecycle closing cash is 995.70, with coupon
income -4 and mark P&L +4 reported separately. The separate dated backbook also earns
0.30; allocation therefore retains -3.70 of combined external NII in the hold baseline.
The draw/repay package adds 0.30 of funding cost without inventing coupon or mark income.

The independent verifier reconstructs source values, coupons, daily drawn principal,
gross accrued interest, margin targets, ordered postings and reports. A changed reviewed
rate, missing coupon coverage, fabricated monthly income or mark P&L, and an overflowing
VM settlement date refuse. Successful termination owns the same-day coupon once;
failed termination retains the contractual cash path and blocks dependent margin returns.
Segregated VM return releases its cash restriction; only new posted margin needs borrowing.
ACT/360 and ACT/365F, signed rates, unpaid rights and same-day draw/repay are explicit cases.

## Validation

| Gate | Result |
|---|---|
| Core lifecycle execution, margin and policy regressions | 20 passed |
| Independent verifier, including frozen streams and tamper cases | 15 passed |
| Native acquisition, margin and allocation regressions | 15 passed |
| New release `lifecycle_ongoing` suite | 9 passed |
| Rust service with `fault-injection` and the live producer | 31 executed checks passed; large-scale fault test unconfigured |
| Browser lifecycle flow and ongoing monthly display | Both passed; ongoing flow repeated after the final verifier date guard |
| Web TypeScript/Vite build | Passed; existing large-chunk warning |
| Rust formatting and strict Clippy | Core, verifier, decision (release/all targets) and service (`s3,fault-injection`) passed |
| Repository architecture, native ABI and HTTP generated contracts | Passed |

The live test submits the committed raw fixture, independently verifies and publishes it,
restarts the API, reopens its receipt and published journal table, and confirms that a
missing final coupon produces `native_error` with no published manifest. Existing live
saved-book, funding-policy and allocation tests also pass. The live research-IM smoke
harness ran at 1 and 64 backbook positions; that is not a new large-scale acceptance claim.

Local execution used SQLite and local immutable artifacts on Windows. PostgreSQL/S3 and
the opt-in large-scale fault harness were not configured. The browser ran only Vite and
the Rust service/worker; the deliberately disabled Python API caused unrelated dashboard
proxy errors. The worktree's linked font dependency used a fallback font in development;
the production Vite build bundled the font assets successfully. Python was used only for
existing static repository boundary/contract checks, not feature execution or verification.

An initial parallel build exhausted available memory. Targeted release builds were rerun
with a single Cargo worker, preserving the release profile, equations and tolerances.

Artifacts used by the final live/browser checks (SHA-256):

- Native release producer: `88d1fc6818b5f5cde737257ca565e72479f9c62e67efc94f97e69971036afbcf`.
- Rust test service (`fault-injection` build): `f3f73218048b12d32d7a7d55a33f3580d7c444d3f9512ba11a28990e7eb4f026`.
- Committed ongoing fixture: `02ed367b91657cdc71eddc6a19f3296b87654adb7aa99183a0600642f303139e`.

The browser evidence is retained locally under `apps/web/test-results/`.
The live CI job now runs the ongoing release suite and the shared service/browser tests.

## Acceptance boundaries

This is local implementation and validation, not deployment or worker restart. The supplied
coupon paths and fixed facility rates are reviewed assumptions, not calibrated histories.
Provider margin certification, formal ASC 815 adoption, interim capital/FTP, and complete
mixed graph/credit/solver/journal scale acceptance remain separate issue gates. Pending VM
windows cannot overlap population changes; no missing fixings, tail payments or formal
accounting treatment are inferred. Historical requests without the new arrays retain their
economics. Rebuild/restart the deployed native producer and service before explicit adoption.
