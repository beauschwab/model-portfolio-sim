---
target: market page
total_score: 24
p0_count: 0
p1_count: 3
timestamp: 2026-06-13T13-11-57Z
slug: apps-web-src-pages-market-tsx
---
# Critique — Market & Scenarios page (`apps/web/src/pages/Market.tsx`)

## Design Health Score

| # | Heuristic | Score | Key Issue |
|---|-----------|-------|-----------|
| 1 | Visibility of System Status | 2 | Curve panel renders a silent flat 0% line when `/market` fails; scenario panel is blank during a multi-second run |
| 2 | Match System / Real World | 4 | Excellent trader-space language (10y, 2s10s, bp, OAS); method disclosure in card subs |
| 3 | User Control and Freedom | 2 | No undo on drag-sculpted legs, no cancel on a running NII job, no reset-to-base curve |
| 4 | Consistency and Standards | 2 | Drifts from the Dashboard `ChartState` run-state pattern; alert() vs inline feedback |
| 5 | Error Prevention | 2 | `mkt!` can throw on early Save; runs fire with empty legs; range accepts lo>hi |
| 6 | Recognition Rather Than Recall | 3 | InfoPop notes are strong; active preset not indicated; range control hidden behind a faint link |
| 7 | Flexibility and Efficiency | 2 | Drag is fast, but no keyboard path for legs (blocks power users AND a11y) |
| 8 | Aesthetic and Minimalist Design | 3 | Clean, dense, purposeful — on-brand |
| 9 | Error Recovery | 1 | `alert(done.detail)` dumps raw engine detail; no retry affordance; failed market load shows nothing |
| 10 | Help and Documentation | 3 | InfoPop + descriptive card subs are genuinely good contextual help |
| **Total** | | **24/40** | **Acceptable — significant improvements needed** |

## Anti-Patterns Verdict

**Not AI slop.** This is a characterful, dense trading-desk surface that follows the Binance-dark PRODUCT.md faithfully: numbers are monospaced and semantic, color carries meaning (yellow accent, turquoise/red legs), no eyebrows, no hero-metric template, no identical card grid. The DragSeries leg editor is a genuinely original affordance.

**Deterministic scan**: `detect.mjs` over `Market.tsx` + `DragSeries.tsx` returned `[]` — zero slop findings. Clean.

## Overall Impression
The design language is right; the engineering underneath the polish is where it falls down. The single biggest opportunity: make the page honest about system state (loading, failure, empty) the way the Dashboard already is, and close the accessibility gap that PRODUCT.md explicitly requires for drag controls.

## What's Working
1. **Domain fluency.** Every label and card-sub speaks the strategist's language and discloses method ("2s10s twist around the 5y pivot", "base OAS held fixed"). This is the product's voice executed well.
2. **The DragSeries leg editor.** Sculpting a 9Q path by dragging points is a real, differentiated interaction — far better than nine number inputs.
3. **Contextual help via InfoPop.** Method notes live at the point of output, matching the "disclose, don't smooth" principle.

## Priority Issues

- **[P1] `LegEditor` is defined inside the render body.** Every keystroke in the name field or a range input creates a brand-new component identity, remounting all four `DragSeries` SVGs and the range popover inputs — causing focus loss and dropped interaction state mid-edit.
  - **Why it matters**: The page's core promise is fluid assumption-sliding; a remount on every render fights exactly that.
  - **Fix**: Hoist `LegEditor` to module scope, passing `rng`/`setRng` as props.
  - **Suggested command**: `/impeccable polish`

- **[P1] DragSeries is pointer-only — no keyboard, no focus ring, no ARIA.** PRODUCT.md's Accessibility section explicitly states "All slider/drag controls (assumption editors, DragSeries) need keyboard equivalents and visible focus rings." This is a documented requirement being violated.
  - **Why it matters**: Sam (keyboard/screen-reader) cannot build a scenario at all; the page fails WCAG 2.1 AA, the stated target.
  - **Fix**: Make each point a `role="slider"` with `tabIndex={0}`, arrow/PageUp/Home/End handling, `aria-valuenow/min/max/text`, and a visible yellow focus halo. Give the SVG an accessible group name.
  - **Suggested command**: `/impeccable harden`

- **[P1] The par-curve panel fails silently.** When `/market` errors (I hit a live 500 with the API down), the chart renders a flat 0.000% line auto-scaled to 0–400% with no indication anything is wrong, and the 10 tenor inputs show `0.000`. The shared `ChartState` loading/error/empty component exists and is used on the Dashboard — but not here.
  - **Why it matters**: A strategist could mistake a failed fetch for a real zero curve and act on it — the exact "never silently extrapolates" violation PRODUCT.md warns against.
  - **Fix**: Track fetch status; render `ChartState` loading/error (with retry) instead of a misleading chart.
  - **Suggested command**: `/impeccable harden`

- **[P2] Scenario-run feedback drifts from the Dashboard and dumps raw errors.** The run panel is a bare empty `<div>` during the multi-second job (only the button says "running…"), and failures surface via `alert(done.detail)` with raw engine text. The Dashboard uses `ChartState` with an elapsed timer, an empty hint, and an `onRetry`.
  - **Why it matters**: Inconsistent run-state vocabulary across two adjacent surfaces; raw `alert` is a poor recovery path.
  - **Fix**: Adopt the Dashboard's status machine + `ChartState` (loading/elapsed, error+retry, empty) for the 9Q NII result.
  - **Suggested command**: `/impeccable polish`

- **[P2] `paper-faint` (#707a8a) small text fails AA contrast.** On `surface-1` it computes to ≈3.62:1 (AA needs 4.5:1 for normal text). It's used for the card sub, the empty-state hint, the Q1–Q9 axis labels, and the "range …bp" link.
  - **Why it matters**: Directly contradicts PRODUCT.md ("body text must clear 4.5:1 … the paper ramp, not muted gray").
  - **Fix**: Promote small body/help text from `paper-faint` to `paper-dim` (#929aa5 ≈ 5.57:1); reserve faint for large/decorative only.
  - **Suggested command**: `/impeccable colorize`

## Persona Red Flags

**Treasury/ALM strategist (project persona — model running live while sliding assumptions)**: The remount bug interrupts the live-sliding loop the whole product is built around. A failed market fetch shows a plausible-looking zero curve with no warning — the strategist could trust a broken number.

**Alex (Power User)**: No keyboard path to set legs — must mouse-drag every point. No "Run" hotkey, no copy-a-leg, no cancel on a running job. Drag-sculpting nine points by mouse is the only path.

**Sam (Accessibility)**: Cannot operate DragSeries at all (no focus, no keys, no ARIA). Tenor inputs have visual `1y` labels with no programmatic association — a screen reader announces "edit text, 4.250" with no context. Recharts panels have no text alternative.

**Riley (Stress Tester)**: Clicking "Save market" before data loads throws on `mkt!`. "Run 9Q NII" fires with all legs empty (no guard). The range popover accepts lo > hi and a zero/negative step.

## Minor Observations
- Active scenario preset has no selected state — you can't tell which of `bear_steepener` / … is loaded; the preset buttons are tiny `text-[10px]` and low-contrast.
- The empty-state line mashes a run hint and the `base OAS held fixed` badge mid-sentence, producing awkward wrapping.
- The "range −150…300bp" control is a dotted-underline faint link — very low discoverability for something that changes drag bounds.
- `saveMarket`/`saveScenario` give crude `alert()` confirmation; an inline transient "Saved" would respect flow.

## Questions to Consider
- What if a failed or stale market fetch were as loud as a binding constraint — could the page refuse to show a curve it doesn't trust?
- Should the scenario builder, not the curve, be the visually dominant card, given it's the primary working surface?
- What would a keyboard-first version of leg-sculpting feel like — could arrow-stepping a point be faster than dragging for fine bp moves?
