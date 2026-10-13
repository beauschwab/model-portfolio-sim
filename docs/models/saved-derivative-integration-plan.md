# Saved derivative adoption: implementation requirements

This is the remaining implementation plan for issue #20, not an implemented
schema or an acceptance receipt. The current native saved runner explicitly
rejects nonempty hedge books in `portfolio-decision-native/src/owned_workflow.rs`.
The rejection remains correct until the complete source and journal mapping exists.

## Current source boundaries

The historical `hedge_lifecycle::Swap` records notional, payer/receiver side,
fixed rate, maturity and floating spread. Its generated six-month fixed and
three-month floating legs use historical calendar/day-count conventions.
It contains no trade identifier, legal entity, counterparty, CSA, settlement
evidence or independent observed index-fixing source. A netting-set label cannot
recover these missing terms.

The separate `derivative_contract` owners already value complete dated raw swaps,
European cash/physical settlement and explicitly reviewed CSA/MPOR paths.
`derivative_accounting` already prepares balanced Physical journal intents from
complete daily raw marks, actual contractual fixing/election policies and opening
carrying/collateral anchors. These owners must supply the upgraded saved route;
the adapter must not recreate coupon, option, margin or loss calculations.

## Required native integration

1. Add an explicitly selected, immutable saved-trade source contract. Bind each
   saved-book row by its frozen book revision, complete raw content and row slot;
   identical economic rows still require distinct complete trade identities.
   Require exact one-to-one coverage and reject duplicate, missing or surplus
   mappings before pricing. A selected contractual upgrade retains its own
   reviewed revision; it does not silently reinterpret historical saved economics.
2. Require complete entity/currency/counterparty/netting/legal/CSA identities,
   contractual leg/reset/settlement terms, original opening anchors, and separate
   Physical/Pricing sources. Preserve observed-fixing information cutoffs and
   explicit future projection identity. Pricing projections cannot become
   physical settlement evidence. Legal netting and financial statement offsetting
   remain separately reviewed policies.
3. Integrate the existing raw derivative owners into the native pricing graph.
   Cache keys include the complete contract, curves, supplied histories, legal
   policy, measure, CRN and original valuation identity. Paired risk bumps share
   draws; any original spread/OAS anchor remains immutable. Holdout observations
   must not change training calibration. European settlement/exercise retains
   complete cash or delivered-leg tails exactly once.
4. Adopt derivative accounting intents as one exclusive owner in the actual
   shared daily Sim. Generic credit, coupon and maturity overlays must bypass
   that owner. Post opening cash/equity once for the combined book. Retain gross
   received/posted VM, segregated IM, unpaid contractual obligations and delivered
   ownership through default and complete closeout; margin principal transfers
   are not trading losses. Preserve each original ordered source transaction and
   reconcile the mixed GL to independently rebuilt raw derivative sources.
5. Prepare native strategy coefficients and Treasury exposure/flows from these
   verified owners. Constraint-only interactive evaluation stays coefficient-only.
   Capital and FTP use explicitly reviewed source/policy inputs; regulatory/XVA
   outputs cannot be inferred from a legal label, an economic loss memo or a
   physical journal. Journal and source permissions remain separate.
6. Queue the complete immutable input/code/native identity with one original
   deadline. Retain bounded source/output partitions, actual producer completion,
   independent raw and persisted reconstruction, and transaction-bound
   revision/attempt/token/lease/cancellation/source/body-hash checks. Reopening a
   completed source is provenance; a new job owns a new finite accepted deadline.

## Evidence required before closing the issue

All original issue #20 criteria remain required: independent par and signed-rate
swap schedules, payer/receiver parity, annuity/tenor and European exercise
convergence, cash versus physical settlement, legal netting versus gross buckets,
collateral lag/outage/segregation and default closeout conservation. Add missing
and duplicate saved mappings, zero/signed positions, same-day mixed acquisitions,
unpaid maturity tails, physical delivery and no second margin-loss posting.

Compare original default-free legacy output bytes independently. Run new native
graph/risk/strategy and actual combined journal checks, rebuilt default process
tests with Python financial callbacks forbidden, durable worker/restart/persisted
tamper/publication tests, and the actual saved-book UI flow. The combined
60,000-position graph/credit/solver/journal acceptance remains a separate required
scope; small source tests and component budgets cannot establish it.

## Primary research and remaining model limits

[BCBS counterparty-risk guidance, paragraphs 40.43 and 40.66](https://www.bis.org/committees/bcbs/basel-consolidated-guidelines/module/cri/40)
addresses reviewed enforceability, collateral-access delays, contractual netting,
material risk-factor correlations and MPOR collateral dynamics. Paragraph 40.70
addresses exposure backtesting against historical markets. These support the
explicit source and validation requirements above; they do not certify this
repository, supply jurisdiction-specific legal opinions or prescribe its
accounting policies. The existing supplied-path owners still need joint scenario
generation and observed exposure validation before predictive claims.

The ISDA 2021 Definitions InfoHub identifies confirmation templates and versioned
contractual conventions. Its search metadata was checked on 2026-10-03; direct
InfoHub and public PDF retrieval returned an error/403 in that check, so no new
formula or settlement convention is inferred from unread definitions. The full
admitted European/Bermudan boundary and unsupported own-default/DVA, dynamic IM,
cross-currency and formal hedge-accounting policies must remain explicit until
implemented and independently validated.
