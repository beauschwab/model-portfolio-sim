# Issue #67: current-contract preservation review

This change reorganizes instructions only. It changes no financial runtime,
model, input schema, output tolerance, saved economics or adoption status.
The root contract has 149 physical lines; CI caps it at 150, including blanks.

## Reviewed checklist

- [x] Production Rust/Python-reference/HiGHS ownership, explicit migration of saved
  Python settings, native failure and graph-entry rules map to root lines 5–14.
- [x] Layer ownership, nested instructions, product extension order, UI tokens,
  revisioned storage and architecture maintenance remain in the root contract.
- [x] Fixed OAS/CRN, unchanged tolerances, explicit independent oracles, frozen
  constants and coefficient-only evaluation remain mandatory.
- [x] Immutable source/artifact identity, independent raw/persisted replay,
  complete gross tails, original deadlines and final SQL authority fences remain.
- [x] Product-specific obligations and exclusions map below to current root rules
  and their linked detailed schemas. Superseded intermediate ownership/defaults,
  ABI versions and “still open” claims are preserved as history, not restated as
  current behavior. Each selected schema's own admission limits still apply.
- [x] Undated engine invariants, extension recipes, frame schemas and change gates
  remain in their active source files. Operator request/evaluation semantics are
  consolidated into the mirrored `references/current-runtime.md`.
- [x] All 348 archived version sections match their original committed bytes,
  including duplicate sections, ordering and wording. The manifest records the
  base commit, original line, archive line and SHA-256 for every section.
- [x] Operator entrypoints and all four reference files match their packaged
  mirrors; a local `.skill` distribution is rebuilt for validation.
- [x] Root edits alone trigger CI; the line check runs before dependency setup,
  and the full `make check-architecture` target runs after setup.

## Current rule map

A row applies to every occurrence of that former version heading across the nine
sources in the archive manifest. The current root lines are normative summaries;
follow their linked current model/schema documents for detailed bounds and wire
contracts. The archive is for provenance only. Source-only workflow details are
also in `packages/portfolio-risk/skills/references/current-runtime.md`.

| Former section (all occurrences) | Current root rules |
|---|---|
| Acquired consumer-credit event ownership (local 0.29.7) | [L81](../../AGENTS.md#L81) |
| Balance-sheet stress (0.22.0) | [L121](../../AGENTS.md#L121), [L42](../../AGENTS.md#L42) |
| Capital coverage and funds transfer pricing (0.29.2) | [L115](../../AGENTS.md#L115), [L117](../../AGENTS.md#L117) |
| Cohort accuracy and reliability (0.29.1) | [L137](../../AGENTS.md#L137) |
| Commercial follow-through (local 0.29.11) | [L99](../../AGENTS.md#L99), [L101](../../AGENTS.md#L101) |
| Commercial source, credit and coefficients (local 0.29.10) | [L99](../../AGENTS.md#L99), [L101](../../AGENTS.md#L101) |
| Complete-origin commercial holding journals (local 0.29.14) | [L103](../../AGENTS.md#L103) |
| Complete-origin commercial holding windows (local 0.29.13) | [L103](../../AGENTS.md#L103) |
| Conditional forecast income (v0.20.1) | [L33](../../AGENTS.md#L33) |
| Corporate exercise publication (local 0.29.9) | [L99](../../AGENTS.md#L99), [L42](../../AGENTS.md#L42) |
| Dated replay and credit accounting foundations (0.29.6) | [L73](../../AGENTS.md#L73), [L69](../../AGENTS.md#L69) |
| Decision graph prototype (0.21.0) | [L31](../../AGENTS.md#L31), [L30](../../AGENTS.md#L30) |
| Decision session API (0.21.0) | [L31](../../AGENTS.md#L31), [L30](../../AGENTS.md#L30) |
| Deposit terminal treatment (0.29.7) | [L113](../../AGENTS.md#L113) |
| Explicit acquired card and tagged credit owners (local 0.29.7) | [L83](../../AGENTS.md#L83), [L85](../../AGENTS.md#L85) |
| Explicit acquired-mortgage analytical source (local 0.29.7) | [L87](../../AGENTS.md#L87) |
| Explicit card interest-basis ownership (local 0.29.7) | [L83](../../AGENTS.md#L83), [L85](../../AGENTS.md#L85) |
| Explicit product states and durable source routes (0.29.7) | [L75](../../AGENTS.md#L75), [L56](../../AGENTS.md#L56) |
| Formal ASC 815 hedge relationships (local 0.29.19) | [L125](../../AGENTS.md#L125), [L131](../../AGENTS.md#L131), [L133](../../AGENTS.md#L133), [L135](../../AGENTS.md#L135) |
| Incremental pricing (0.19.0) | [L11](../../AGENTS.md#L11), [L35](../../AGENTS.md#L35) |
| Incremental pricing (v0.19.0) | [L11](../../AGENTS.md#L11), [L35](../../AGENTS.md#L35) |
| Incremental spot pricing (v0.19.0) | [L11](../../AGENTS.md#L11), [L35](../../AGENTS.md#L35) |
| Indexed graph control, deposit revisions and closing capital (local 0.29.8) | [L93](../../AGENTS.md#L93), [L95](../../AGENTS.md#L95), [L97](../../AGENTS.md#L97) |
| Ledger and dynamic replay (0.23.0) | [L42](../../AGENTS.md#L42), [L52](../../AGENTS.md#L52) |
| Liquidity applicability, transfer approval and HTM accounting (0.29.8) | [L121](../../AGENTS.md#L121), [L123](../../AGENTS.md#L123) |
| Market indexed sources and acquired mortgage mixed replay (local 0.29.7) | [L93](../../AGENTS.md#L93), [L89](../../AGENTS.md#L89) |
| Native built-in product coverage (0.27.0) | [L6](../../AGENTS.md#L6), [L9](../../AGENTS.md#L9) |
| Native corporate and CD spot-risk lifecycles (0.27.7) | [L6](../../AGENTS.md#L6), [L27](../../AGENTS.md#L27) |
| Native daily state and partitioned journals (0.26.0) | [L42](../../AGENTS.md#L42), [L52](../../AGENTS.md#L52) |
| Native incremental dependency graph (0.27.9) | [L35](../../AGENTS.md#L35) |
| Native journal pilot (0.25.0) | [L6](../../AGENTS.md#L6), [L42](../../AGENTS.md#L42) |
| Native linear calibration ownership (0.27.3) | [L6](../../AGENTS.md#L6), [L28](../../AGENTS.md#L28) |
| Native loan and separate fee journal (local0.29.17) | [L107](../../AGENTS.md#L107) |
| Native market-path ownership (0.27.2) | [L6](../../AGENTS.md#L6), [L27](../../AGENTS.md#L27) |
| Native mortgage risk and forward-stress lifecycles (0.27.6) | [L6](../../AGENTS.md#L6), [L27](../../AGENTS.md#L27) |
| Native nonlinear calibration ownership (0.27.4) | [L6](../../AGENTS.md#L6), [L28](../../AGENTS.md#L28) |
| Native public strategy ownership (0.27.1) | [L31](../../AGENTS.md#L31), [L30](../../AGENTS.md#L30) |
| Native public-controller and allocation follow-through (0.28.0) | [L33](../../AGENTS.md#L33), [L28](../../AGENTS.md#L28) |
| Native seeded shared draws (0.27.5) | [L27](../../AGENTS.md#L27), [L37](../../AGENTS.md#L37) |
| Native workflow ownership (0.28.0) | [L31](../../AGENTS.md#L31), [L42](../../AGENTS.md#L42) |
| Per-pool MBS prepay speed (0.29.6) | [L111](../../AGENTS.md#L111) |
| Performing mixed dated acquisition stage (local 0.29.7) | [L79](../../AGENTS.md#L79) |
| Pricing cache reuse (0.21.1) | [L35](../../AGENTS.md#L35) |
| Published forecast replay (v0.20.1) | [L33](../../AGENTS.md#L33) |
| Raw mixed-book lifecycles (0.27.8) | [L6](../../AGENTS.md#L6), [L50](../../AGENTS.md#L50) |
| Research discount-curve projection (v0.19.1) | [L65](../../AGENTS.md#L65), [L33](../../AGENTS.md#L33) |
| Retained-Q commercial original OAS (local 0.29.12) | [L101](../../AGENTS.md#L101) |
| Reviewed earned-fee refunds (0.29.18) | [L109](../../AGENTS.md#L109) |
| Runtime contract update (0.18.0) | [L30](../../AGENTS.md#L30), [L145](../../AGENTS.md#L145) |
| Rust decision sessions (0.21.0) | [L31](../../AGENTS.md#L31), [L30](../../AGENTS.md#L30) |
| Security basis and ledger scaling (0.24.0) | [L54](../../AGENTS.md#L54), [L121](../../AGENTS.md#L121), [L123](../../AGENTS.md#L123) |
| Separate nonintegral fee ownership (local0.29.16) | [L107](../../AGENTS.md#L107) |
| Shared model foundations (0.29.5) | [L65](../../AGENTS.md#L65), [L67](../../AGENTS.md#L67), [L71](../../AGENTS.md#L71) |
| Source-linked capital and FTP (0.29.3) | [L119](../../AGENTS.md#L119) |
| Strict additive financial instructions (local 0.29.7) | [L91](../../AGENTS.md#L91) |
| Tape cohort construction (0.29.0) | [L137](../../AGENTS.md#L137) |
| Temporary comparisons and native execution (v0.20.0) | [L11](../../AGENTS.md#L11), [L29](../../AGENTS.md#L29) |
| Verified modeled commercial prior openings (local0.29.15) | [L105](../../AGENTS.md#L105) |
| v0.18.0 run and allocation contract | [L30](../../AGENTS.md#L30), [L145](../../AGENTS.md#L145) |
| v0.18.0: review correctness and run-scoped reuse | [L27](../../AGENTS.md#L27), [L35](../../AGENTS.md#L35) |
| v0.20.0 incremental comparisons and native boundary | [L11](../../AGENTS.md#L11), [L35](../../AGENTS.md#L35) |
| v0.20.0 temporary instrument assumptions | [L11](../../AGENTS.md#L11), [L29](../../AGENTS.md#L29) |

## Archive inventory

Paths in archived text are relative to the original source file. Archive banners
are new; the dated section bodies are unchanged from the base commit.

| Original source | Archive | Sections |
|---|---|---:|
| `AGENTS.md` | [agents-history.md](../../docs/changelog/agents-history.md) | 32 |
| `packages/portfolio-risk/AGENTS.md` | [packages--portfolio-risk--AGENTS.md](../../docs/changelog/agents-history/packages--portfolio-risk--AGENTS.md) | 38 |
| `packages/portfolio-risk/skills/AGENTS.md` | [packages--portfolio-risk--skills--AGENTS.md](../../docs/changelog/agents-history/packages--portfolio-risk--skills--AGENTS.md) | 16 |
| `packages/portfolio-risk/skills/SKILL.md` | [packages--portfolio-risk--skills--SKILL.md](../../docs/changelog/agents-history/packages--portfolio-risk--skills--SKILL.md) | 49 |
| `packages/portfolio-risk/skills/references/internals.md` | [packages--portfolio-risk--skills--references--internals.md](../../docs/changelog/agents-history/packages--portfolio-risk--skills--references--internals.md) | 44 |
| `packages/portfolio-risk/skills/references/schemas.md` | [packages--portfolio-risk--skills--references--schemas.md](../../docs/changelog/agents-history/packages--portfolio-risk--skills--references--schemas.md) | 38 |
| `packages/portfolio-risk/skills/portfolio-risk-engine/SKILL.md` | [packages--portfolio-risk--skills--portfolio-risk-engine--SKILL.md](../../docs/changelog/agents-history/packages--portfolio-risk--skills--portfolio-risk-engine--SKILL.md) | 49 |
| `packages/portfolio-risk/skills/portfolio-risk-engine/references/internals.md` | [packages--portfolio-risk--skills--portfolio-risk-engine--references--internals.md](../../docs/changelog/agents-history/packages--portfolio-risk--skills--portfolio-risk-engine--references--internals.md) | 44 |
| `packages/portfolio-risk/skills/portfolio-risk-engine/references/schemas.md` | [packages--portfolio-risk--skills--portfolio-risk-engine--references--schemas.md](../../docs/changelog/agents-history/packages--portfolio-risk--skills--portfolio-risk-engine--references--schemas.md) | 38 |

## Validation

- Root line guard and overflow/missing-file tests, including LF/CRLF and final
  newline cases; architecture/script tests; native, HTTP and MCP contract checks.
- Exact source-to-archive byte comparison, archive checksums, active Markdown
  link resolution, mirrored operator references and skill frontmatter validation.
- Full `make check-architecture` and web build run in the hosted architecture job.
  No financial executable changes or native deployment are part of this change.
