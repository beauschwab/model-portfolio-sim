# PRODUCT.md — Rates Workbench

## Register

product

## What it is
A balance-sheet analytics workbench for bank Treasury: one Monte Carlo
model prices the full bank book (MBS, loans, deposits, CDs, money
markets, ASC 815 hedges), forecasts NII, computes regulatory KPIs
(EVE/duration gap, LCR, NSFR, CET1), and turns strategy/optimization into
interactive tools rather than batch jobs.

## Who it's for
- **Treasury/ALM strategists**: EVE & NII under scenarios, hedge sizing,
  IRRBB outlier monitoring, reinvestment strategy.
- **Liquidity & capital teams**: LCR/NSFR/CET1 projections driven by
  modeled behavior (real runoff WALs, contractual maturities), not static
  buckets; weight tables are explicit calibration seams.
- **ALCO**: the Optimizer's shadow prices answer "what does the LCR floor
  cost in NII" and "what does the loan mandate cost" — with infeasibility
  reported as the finding, not an error.

These are quantitative finance professionals working in a focused, dense,
keyboard-and-numbers context — often with the model running live while
they slide assumptions and read deltas. The screen is a working surface,
not a presentation.

## Product Purpose
Collapse the gap between the batch risk run and the trader's intuition.
Treasury teams traditionally wait overnight for EVE/NII/regulatory
numbers; Rates Workbench makes the same institutional-grade Monte Carlo
engine answer at slider speed, so strategy and constraint-pricing become
interactive exploration rather than ticketed jobs. Success is a strategist
testing a reinvestment plan against live LCR/NSFR/CET1 floors and reading
the shadow price of each constraint in seconds — and trusting the number
because every simplification is disclosed at the point of output.

## Core user journeys
1. **Risk & forecast**: Dashboard → run risk / NII / 9Q stress on the
   seeded WFC-1Q26-proportional book (or your own via book editors).
2. **Scenario analysis**: Market & Scenarios → define 9Q paths in trader
   terms (10y, 2s10s, spread, vol) → full-sheet revaluation per quarter.
3. **Strategy Lab**: build the unit library once (~20s; new origination
   through the live behavioral engines), then slider-speed KPI recalc.
4. **Optimizer**: floors + commercial plan + scenario set → robust LP
   allocation + binding constraints with shadow prices.

## Brand Personality
Institutional, precise, candid. Three words: **trustworthy, dense,
unflinching.** The voice is a senior quant explaining their work to a
peer — no marketing gloss, no reassurance theatre. Numbers are the hero;
the chrome stays quiet so the data carries the signal. The interface
should feel like a professional trading-desk tool: confident in dark, fast
to read, color used as semantic signal (price up/down, binding constraint)
rather than decoration.

## Anti-references
- **Not flashy crypto-retail.** The palette borrows Binance's
  near-black canvas, yellow accent, and trading green/red, but the product
  is an institutional ALM tool — not a consumer exchange. No hype copy, no
  coin illustrations, no "316M USERS TRUST US" stat-callout heroes, no
  pill CTAs shouting for sign-ups. Yellow is a scarce accent for the
  primary action and a single value-claim, never decorative voltage.
- **Not generic SaaS-cream dashboards.** No warm near-white canvas, no
  hero-metric template (big gradient number + supporting stats), no
  identical icon-heading-text card grids.
- **Never silently extrapolates.** The product discloses model
  simplifications at the point of output; the UI must surface method
  columns, notes, and fitter warnings rather than hide them for polish.

## Design Principles
1. **Numbers are the hero.** Tabular, monospaced, right-aligned, color
   only as semantic signal. Chrome recedes so the data reads.
2. **Disclose, don't smooth.** Every simplification, warning, and synthetic
   seam is visible at the point of output. Honesty is a feature, not a
   footnote — never trade candor for visual tidiness.
3. **Slider speed is the promise.** Interactive recalc paths (Strategy
   Lab, Positions) must stay instant; never put a slow engine call behind
   a control that feels live.
4. **Density with rhythm.** This is a dense working surface, but spacing
   carries hierarchy — vary it deliberately so dense ≠ cramped.
5. **One accent, semantic color.** Yellow is the scarce primary signal;
   green/red are reserved for price-direction and threshold breaches.
   Color always means something here.

## Honesty principles (product-level)
Demo data is synthetic and says so; every regulatory weight table is
stylized and labeled as the seam for internal mappings; every model
simplification is disclosed at the point of output (method columns,
notes fields, fitter warnings like the deposit-beta plateau warning).
The product never silently extrapolates: it tells you when it is.

## Accessibility & Inclusion
Target WCAG 2.1 AA. On the near-black canvas, body text must clear 4.5:1
(the `paper` ramp, not muted gray, for primary reading); large/secondary
text clears 3:1. Price-direction is never carried by color alone — pair
green/red with arrows, signs, or labels so red/green color-vision
deficiency doesn't lose the signal. All slider/drag controls (assumption
editors, DragSeries) need keyboard equivalents and visible focus rings
(yellow `#fcd535`). Honor `prefers-reduced-motion` for every transition
and the memo-rise reveal.

## Roadmap candidates
Effectiveness testing for ASC 815; CVaR objective and MILP lots in the
Optimizer; AOCI leg wired from 9Q stress into CET1; persistence
(Postgres) behind the repository-shaped store; GPU kernels for 512-path
interactive accuracy; per-path coupon fixing in the unit library.
