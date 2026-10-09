# DESIGN.md — Rates Workbench UI system

The web app (`apps/web`) follows the **Aperture Risk Design System**, maintained as a
Claude Design design-system project ("Aperture Risk Design System"). That project's
`readme.md`, `tokens/*.css` and component sources are the reference; this file records
how the workbench implements them. When the two disagree, update the code to match the
design system (or change the design system first and then port the change).

Aperture in one line: a dark, dense, numeric risk terminal. Layered near-black greys,
one deep golden-yellow accent used sparingly, green/red only for direction, Inter
with tabular figures, hairline borders, near-square corners, fast restrained motion.

## Where things live

| Concern | File |
|---|---|
| Tokens (colors, type, spacing, radii, shadows, motion) | `apps/web/src/styles/aperture.css` |
| Tailwind names mapped onto the tokens | `apps/web/tailwind.config.js` |
| Base element styles, `.num`, `.eyebrow`, focus ring, motion | `apps/web/src/index.css` |
| Primitives (Panel/Card, Button, Input, Badge, Stat, DataTable, Tabs, Popover, Accordion) | `apps/web/src/components/ui.tsx` |
| Chart theme + series palette | `apps/web/src/components/charts.tsx` |
| Canvas token reads | `apps/web/src/lib/tokens.ts` |
| Management limits shown as KPI references | `apps/web/src/lib/limits.ts` |
| Dock (dockview) chrome | `apps/web/src/workspace/dockview-theme.css` |
| Brand mark | `apps/web/public/mark.svg` (Aperture `assets/mark.svg`) |

Never hard-code hex in components. Use a Tailwind token class, or `var(--token)` where
a class cannot reach (SVG attributes, Recharts props, inline styles). Canvas code reads
tokens through `token()`.

## Color

Tailwind names resolve to Aperture semantic aliases (RGB channels, so `bg-brand/15` works):

| Tailwind | Aperture token | Use |
|---|---|---|
| `surface` | `--bg-canvas` | App canvas |
| `surface-base` | `--bg-base` | Inputs, table headers, segmented controls, logs |
| `surface-1` | `--surface-1` | Panels / cards |
| `surface-2` | `--surface-2` | Hover, secondary buttons |
| `surface-3` | `--surface-3` | Viz plot areas, tracks |
| `surface-overlay` | `--surface-overlay` | Popovers, command palette |
| `line` / `line-strong` / `line-heavy` | `--border-subtle` / `--border-default` / `--border-strong` | Internal dividers / panel + input borders / emphasis |
| `paper-heading` / `paper` / `paper-dim` / `paper-faint` | `--text-heading` / `--text-primary` / `--text-secondary` / `--text-tertiary` | Text hierarchy |
| `brand` (`-hover`, `-press`) | `--accent` (yellow 500/400/600) | Primary action, focus, live, selection, target overlays |
| `ink` | `--text-on-accent` | Text on yellow |
| `up` / `down` | `--up-500` / `--down-500` | Direction only: gains, increases, favorable variance / losses, decreases, unfavorable variance |
| `warning` | `--warning-500` | Caution, "tight to limit", research-model labels |
| `danger` | `--danger-500` | Errors, breaches, failed validation |
| `info`, `ai`, `sky` | `--info-500`, `--ai-500`, `--sky-500` | Info, AI surfaces, chart series |

Rules:
- **Yellow is precious.** One primary button per surface; active rail item; focus; live
  dot; chart target overlays. Not for borders on popovers, info icons, status text or tree carets.
- **Green/red are never categorical.** A value is green/red only when it is a direction or a
  variance to a stated reference. Errors use `danger`, caution uses `warning`.
- Status badges: `up` for passed/validated, `danger` for failed/breach, `warning` for
  caution/research, `accent` for live, `neutral` otherwise.

## Type

- Inter only, bundled locally as **Inter Variable** (`@fontsource-variable/inter`, opsz axis),
  so the UI never depends on a font CDN. No monospace face: numerics use Inter tabular
  figures via `.num` (`font-variant-numeric: tabular-nums` + `tnum cv01 cv02`).
- Body is 13px with `-0.006em` tracking and `cv11` (single-story a).
- Tailwind's `fontSize` scale is **replaced** by Aperture's: `2xs` 10, `xs` 11, `sm` 12,
  `base` 13, `md` 14, `lg` 16, `xl` 20, `2xl` 26, `3xl` 34, `4xl` 46 (px). Avoid
  arbitrary `text-[Npx]`.
- Eyebrows/micro-labels: `.eyebrow` (10px, semibold, uppercase, `0.09em`). That is the only ALL-CAPS.
- Copy is sentence case: buttons, headers, menus, empty states. No emoji. `▲ ▼` and `+ / −`
  carry direction in numeric contexts.

## Shape, space, elevation

- **Radii:** Tailwind's `borderRadius` scale is replaced. `sm`/`DEFAULT`/`md` = 1px
  (controls, badges, panels), `lg`/`xl`/`2xl` = 2px (modals, large overlays), `full` only for
  true circles (status dots). Bars, pills and chips are squared.
  *Note:* the Aperture readme prose mentions 8px/3px/2px radii in places; its
  `tokens/radii.css` sets 1–2px. The tokens are treated as authoritative here, and the
  readme should be corrected in the design-system project.
- **Controls:** `h-control` 30px (default), `h-control-sm` 24px; rows `h-row` 28px.
- **Panels:** `surface-1`, 1px `line-strong` border, `shadow-inset-top`, no drop shadow at rest.
  Header: 14px semibold title + 11px subtitle over a `line` divider. Raised overlays use `shadow-md`/`shadow-xl`.
- **Focus:** 1px yellow outline + 3px soft yellow ring (`--ring-focus`).

## Data visualization

- Shared Recharts theme in `charts.tsx`: `chartAxis` (muted ticks, no tick lines),
  `chartGrid` (horizontal, faint dotted), `chartTip` (overlay surface, 1px radius),
  `chartLegend`, and `still` (no entrance animation).
- Series order is `VIZ`: yellow → sky → violet → deep sky → grey → deep yellow. Reference
  or baseline series use `REF` (grey, dashed).
- Rate-shock families encode direction and magnitude (sky ramp for falling rates, yellow ramp
  for rising). They do not encode good or bad.
- No gradients. Area fills use flat `--yellow-softer`.
- KPI tiles (`Stat`) pair the actual with an explicit `variance` against a stated reference
  (`limitVariance()` against `lib/limits.ts`). Neutral context goes in `detail`. Bare
  green/red deltas without a reference are not allowed.

## Motion

- 90–150ms on `--ease-out` for hover, press and transitions; entrances are a 150ms fade with
  no travel, spring or stagger.
- Value-change flash: a 240ms up/down wash.
- No decorative looping animation: no shimmer sweeps, pings or pulsing glows. A spinner is the
  only looping indicator, and the engine Heartbeat repaints only when samples arrive.
- `prefers-reduced-motion` disables all animation and transitions.

## Icons

Lucide (`lucide-react`) at 1.5px stroke: 18px in the activity rail, 14–16px in chrome and
tables. Icons inherit `currentColor` and sit at `paper-faint` until hovered or active.

## Shell

Aperture AppShell pattern: a 56px icon rail on `--gray-1000` in four labelled sections (Home,
Monitor, Decide, Data; active item: yellow-soft fill plus a 2px yellow edge rail). The pipeline
opens from the run-status readout in the command bar, not the rail. A 48px command bar (identity mark, market read-out, engine
status, scenario/settings popovers, primary "Run sheet", ⌘K), and dock panels on the canvas.
