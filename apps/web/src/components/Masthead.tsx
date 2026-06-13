/** Masthead — the desk's status bar and run console in one strip.
 *
 * Left: identity + the engraved par-swap curve (the market, inked). Center:
 * the engine heartbeat rail + a live stage/elapsed read-out that pulses
 * whenever ANY job runs. Right: the active scenario and run settings as rich
 * popovers (their collapsed triggers already tell you the state), the global
 * Run, and the ⌘K trigger. Sticky, so the engine's pulse is always in view. */
import { useEffect, useMemo, useState } from "react";
import { useEngine } from "../lib/engine";
import { useReducedMotion, TweenNumber, compact } from "./motion";
import { Heartbeat } from "./Heartbeat";
import { Button, Input, InfoPop, Popover, Spinner } from "./ui";
import type { Scenario, Settings } from "../lib/api";

export function Masthead() {
  const engine = useEngine();
  const reduced = useReducedMotion();
  const { market, running, stage, pct, elapsed, samples, activeKind } = engine;

  const today = useMemo(() => new Date().toLocaleDateString("en-US",
    { weekday: "long", month: "long", day: "numeric", year: "numeric" }), []);

  // engraved curve: market pillars as one inked stroke + a dashed brass ghost
  const curvePath = useMemo(() => {
    if (!market) return "";
    const t = [1, 2, 3, 4, 5, 7, 10, 15, 20, 30];
    const xs = t.map(x => 6 + (Math.log(x) / Math.log(30)) * 200);
    const r = market.swap_rates;
    const lo = Math.min(...r), hi = Math.max(...r);
    const ys = r.map(v => 30 - ((v - lo) / Math.max(hi - lo, 1e-9)) * 22);
    return xs.map((x, i) => `${i ? "L" : "M"}${x.toFixed(1)},${ys[i].toFixed(1)}`).join(" ");
  }, [market]);

  const peSamples = samples.length ? samples[samples.length - 1].pe : 0;

  return (
    <header className="sticky top-0 z-40 border-b border-line bg-surface/85 backdrop-blur supports-[backdrop-filter]:bg-surface/70">
      <div className="mx-auto flex max-w-[1600px] flex-wrap items-center gap-x-5 gap-y-2.5 px-5 py-2.5">
        {/* identity */}
        <div className="flex items-center gap-2.5">
          <div className="grid h-7 w-7 place-items-center rounded-sm border border-brand font-display text-base leading-none text-brand">R</div>
          <div className="leading-tight">
            <div className="font-display text-sm font-medium tracking-tight text-paper">Rates Workbench</div>
            <div className="text-[10px] text-paper-faint">{today}</div>
          </div>
        </div>

        {/* engraved curve + key rates */}
        <svg width="212" height="34" className="hidden text-paper-dim lg:block" aria-label="par swap curve" role="img">
          <path d={curvePath} fill="none" stroke="currentColor" strokeWidth="1.25" />
          <path d={curvePath} fill="none" stroke="#fcd535" strokeWidth="1.25" strokeDasharray="2 5" opacity="0.55" />
        </svg>
        <div className="num hidden text-xs text-paper-faint sm:block">
          10y <span className="text-paper">{market ? (market.swap_rates[6] * 100).toFixed(2) : "—"}%</span>
          <span className="mx-1.5 text-paper-faint/50">·</span>
          2s10s <span className="text-paper">{market ? ((market.swap_rates[6] - market.swap_rates[1]) * 1e4).toFixed(0) : "—"}bp</span>
        </div>

        {/* engine heartbeat rail — the nervous system, always in view */}
        <div className="order-last flex min-w-[220px] flex-1 items-center gap-3 lg:order-none">
          <div className="flex shrink-0 items-center gap-1.5">
            <span className={`h-1.5 w-1.5 rounded-full ${running ? "bg-brand" : stage === "error" ? "bg-down" : "bg-up/70"} ${running && !reduced ? "animate-pulse" : ""}`} />
            <span className="text-[10px] uppercase tracking-wider text-paper-faint">
              {running ? (activeKind ?? "engine") : stage === "error" ? "error" : "idle"}
            </span>
          </div>
          <div className="relative h-9 flex-1">
            <Heartbeat samples={samples} running={running} reduced={reduced} variant="rail" />
          </div>
          <div className="shrink-0 text-right leading-tight">
            <div className="num text-xs text-paper">
              {running ? <TweenNumber value={peSamples} format={compact} /> : peSamples ? compact(peSamples) : "—"}
              <span className="text-paper-faint"> pe</span>
            </div>
            <div className="num text-[10px] text-paper-faint">
              {running ? `${stage} · ${pct.toFixed(0)}%` : elapsed > 0 ? `${elapsed.toFixed(1)}s` : "ready"}
            </div>
          </div>
        </div>

        {/* controls */}
        <div className="ml-auto flex items-center gap-2">
          <ScenarioPicker
            scenarios={engine.scenarios}
            active={engine.active}
            onPick={engine.setActive}
          />
          {engine.settings && <SettingsEditor settings={engine.settings} onSave={engine.setSettings} />}
          <Button disabled={running} onClick={() => engine.run("kpis")} title="Run the KPI sheet (⌘K for more)">
            {running ? <><Spinner className="h-3.5 w-3.5" />running… {elapsed > 0 ? `${elapsed.toFixed(0)}s` : ""}</> : "Run sheet"}
          </Button>
          <button
            type="button"
            onClick={() => window.dispatchEvent(new Event("palette:open"))}
            title="Command palette"
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-line bg-surface-2 px-2.5 text-[11px] font-medium text-paper-dim hover:bg-surface-3"
          >
            <span className="num">⌘K</span>
          </button>
        </div>
      </div>
    </header>
  );
}

/** Scenario popover — collapsed trigger shows the active path's 10y move. */
function ScenarioPicker({ scenarios, active, onPick }: {
  scenarios: Record<string, Scenario>; active: string; onPick: (name: string) => void;
}) {
  const sc = scenarios[active];
  const d10 = sc ? sc.ust10y_bp[sc.ust10y_bp.length - 1] : 0;
  return (
    <Popover width="18rem" trigger={
      <span className="inline-flex h-8 items-center gap-1.5 rounded-md border border-line bg-surface-2 px-3 text-xs text-paper-dim hover:bg-surface-3">
        <span className="text-[10px] uppercase tracking-wide text-paper-faint">scenario</span>
        <span className="font-medium text-paper">{active}</span>
        {sc && <span className={`num ${d10 >= 0 ? "text-up" : "text-down"}`}>{d10 >= 0 ? "+" : ""}{d10}bp</span>}
      </span>
    }>
      <div className="mb-2 flex items-center justify-between">
        <div className="text-[11px] uppercase tracking-wide text-paper-faint">saved scenarios</div>
        <InfoPop width="15rem">The active scenario drives scenario NII runs (legs map onto the LMM market: 10y level, 2s10s, spread, vol). Base OAS stays fixed — scenario runs never re-solve it.</InfoPop>
      </div>
      <div className="flex max-h-52 flex-col gap-1 overflow-auto">
        {Object.values(scenarios).map(s => {
          const last = s.ust10y_bp[s.ust10y_bp.length - 1];
          return (
            <button key={s.name} onClick={() => onPick(s.name)}
              className={`flex items-center justify-between rounded-md px-2 py-1.5 text-left text-xs ${s.name === active ? "bg-surface-3 text-brand" : "text-paper-dim hover:bg-surface-2"}`}>
              <span className="font-medium">{s.name}</span>
              <span className={`num ${last >= 0 ? "text-paper-faint" : "text-down"}`}>10y {last >= 0 ? "+" : ""}{last}bp</span>
            </button>
          );
        })}
      </div>
    </Popover>
  );
}

/** Settings popover — collapsed trigger summarizes the run config. */
function SettingsEditor({ settings, onSave }: { settings: Settings; onSave: (s: Settings) => void }) {
  const [s, setS] = useState<Settings>(settings);
  useEffect(() => setS(settings), [settings]);
  return (
    <Popover width="20rem" trigger={
      <span className="inline-flex h-8 items-center gap-1.5 rounded-md border border-line bg-surface-2 px-3 text-xs text-paper-dim hover:bg-surface-3">
        <span className="text-[10px] uppercase tracking-wide text-paper-faint">settings</span>
        <span className="num text-paper">{settings.n_paths}p</span>
        <span className="num text-paper-faint">· seed {settings.seed}</span>
        <span className="num text-paper-faint">· {settings.horizon_months}m</span>
      </span>
    }>
      <div className="mb-2 flex items-center text-[11px] uppercase tracking-wide text-paper-faint">run settings
        <InfoPop width="16rem">One CRN draw set per run — every revaluation shares it, so risk numbers are differences of means under common randoms. Changing the seed changes every number coherently. Saved values persist server-side and apply to the next run.</InfoPop>
      </div>
      <div className="space-y-2">
        {([["n_paths", "Monte Carlo paths"], ["seed", "CRN seed"], ["horizon_months", "Horizon (months)"]] as const).map(([k, label]) => (
          <div key={k} className="flex items-center gap-2">
            <div className="w-40 text-xs text-paper-dim">{label}</div>
            <Input type="number" value={s[k]} onChange={e => setS({ ...s, [k]: parseInt(e.target.value) || 0 })} />
          </div>
        ))}
        <div className="flex items-center gap-2">
          <div className="w-40 text-xs text-paper-dim">stress shocks (bp)</div>
          <Input value={s.shocks_bp.join(", ")}
            onChange={e => setS({ ...s, shocks_bp: e.target.value.split(",").map(x => parseFloat(x)).filter(n => !isNaN(n)) })} />
        </div>
      </div>
      <Button className="mt-3 w-full justify-center" onClick={() => onSave(s)}>Save settings</Button>
    </Popover>
  );
}
