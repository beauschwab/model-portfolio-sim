/** Workbench — the whole desk on one composable surface.
 *
 * A sticky masthead carries the engine heartbeat; an always-on constraint
 * ledger states the position in prose and headroom; below sits a grid of
 * composable tiles (drag to reorder, cycle width, expand one to fill the
 * surface, add/remove from ⌘K). Each tile reuses a battle-tested page body,
 * so the desk is a composition of real tools, not a reskin. Nothing
 * navigates by default — you compose what you need in place. */
import { useMemo } from "react";
import { Line, LineChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { fmt$ } from "../lib/api";
import { EngineProvider, useEngine, type Kpis } from "../lib/engine";
import { TilesProvider, TileGrid, useTiles, type TileDef } from "../components/Tiles";
import { CommandPalette } from "../components/CommandPalette";
import { Masthead } from "../components/Masthead";
import { useReducedMotion, useTween } from "../components/motion";
import { Badge, Button, DataTable } from "../components/ui";
import Dashboard from "./Dashboard";
import Positions from "./Positions";
import StrategyPage from "./Strategy";
import OptimizerPage from "./Optimizer";
import MarketPage from "./Market";
import BalanceSheet from "./BalanceSheet";
import AssumptionsPanel from "./Settings";

/* ── KPI detail (shared ledger run; no second button) ─────────────────── */
function KpiDetail({ k }: { k: Kpis }) {
  return (
    <div className="grid gap-3 xl:grid-cols-2">
      <div>
        <div className="mb-2 text-xs text-paper-faint">ΔEVE by parallel shock — first-order (parallel dv01); convexity lives in the 9Q stress pack</div>
        <div className="rounded-lg border border-line"><DataTable rows={k.eve.sensitivity} maxH="16rem" /></div>
      </div>
      <div>
        <div className="mb-2 text-xs text-paper-faint">CET1 projection (9Q) — {k.capital.note}</div>
        <ResponsiveContainer width="100%" height={220}>
          <LineChart data={k.capital.cet1_path}>
            <CartesianGrid stroke="#2b3139" strokeDasharray="3 3" />
            <XAxis dataKey="quarter" stroke="#707a8a" fontSize={10} />
            <YAxis stroke="#707a8a" fontSize={10} domain={["auto", "auto"]} tickFormatter={v => `${v.toFixed(1)}%`} />
            <Tooltip contentStyle={{ background: "#1e2329", border: "1px solid #2b3139", borderRadius: 8, fontSize: 11 }}
              formatter={(v: number) => `${v.toFixed(2)}%`} />
            <Line dataKey="cet1_ratio_pct" stroke="#fcd535" dot strokeWidth={1.5} isAnimationActive />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}

function KpiTile() {
  const { kpis, running, run } = useEngine();
  if (!kpis)
    return (
      <div className="flex h-full min-h-[12rem] flex-col items-center justify-center gap-3 text-center">
        <p className="text-xs text-paper-faint">Run the sheet to compute EVE · LCR · NSFR · CET1.</p>
        <Button disabled={running} onClick={() => run("kpis")}>{running ? "computing…" : "Run KPI sheet"}</Button>
      </div>
    );
  return <KpiDetail k={kpis} />;
}

/* ── constraint ledger: always-on, headroom not levels ────────────────── */
function Headroom({ label, value, limit, sense, unit, onOpen }: {
  label: string; value: number; limit: number;
  sense: "floor" | "ceiling"; unit: string; onOpen: () => void;
}) {
  const reduced = useReducedMotion();
  const v = useTween(value, reduced);
  const room = sense === "floor" ? value - limit : limit - value;
  const pct = Math.max(0, Math.min(1, room / Math.max(Math.abs(limit), 1e-9)));
  const tight = room < 0.08 * Math.abs(limit);
  return (
    <button onClick={onOpen}
      className="group grid w-full grid-cols-12 items-center gap-3 border-b border-line py-1.5 text-left last:border-0 hover:bg-surface-2">
      <div className="col-span-6 text-[13px] text-paper-dim group-hover:text-paper md:col-span-3">{label}</div>
      <div className="col-span-3 num text-right text-[13px] text-paper md:col-span-2">{v.toFixed(unit === "y" ? 2 : 1)}{unit}</div>
      <div className="col-span-3 num text-right text-xs text-paper-faint md:col-span-2">{sense === "floor" ? "≥" : "≤"} {limit}{unit}</div>
      <div className="col-span-9 md:col-span-4">
        <div className="relative h-1.5 rounded-full bg-surface-3">
          <div className={`absolute inset-y-0 left-0 rounded-full transition-[width] duration-500 ease-out motion-reduce:transition-none ${room < 0 ? "bg-down" : tight ? "bg-brand" : "bg-up"}`}
            style={{ width: `${pct * 100}%` }} />
          <div className="absolute inset-y-0 left-0 w-px bg-brand" />
        </div>
      </div>
      <div className={`col-span-3 num text-right text-xs md:col-span-1 ${room < 0 ? "text-down" : tight ? "text-brand" : "text-up"}`}>
        {room >= 0 ? "+" : ""}{room.toFixed(1)}
      </div>
    </button>
  );
}

function Ledger() {
  const { kpis: k, running, run } = useEngine();
  const tiles = useTiles();
  const open = (id: string) => { tiles.add(id); tiles.toggleExpand(id); };
  const reduced = useReducedMotion();

  const d200 = k?.eve.sensitivity.find(s => Number(s.shock_bp) === 200);
  const d200v = d200 ? Number((d200 as Record<string, unknown>).d_eve_pct_eve) : 0;
  const eve = useTween(k?.eve.eve_$ ?? 0, reduced);

  if (!k)
    return (
      <section className="flex flex-wrap items-center gap-3 rounded-lg border border-line bg-surface-1 p-3.5">
        <p className="font-display text-sm text-paper-dim">Run the sheet to populate the constraint ledger.</p>
        <Button disabled={running} onClick={() => run("kpis")}>{running ? "computing…" : "Run sheet"}</Button>
        <span className="text-xs text-paper-faint">or press <kbd className="rounded border border-line px-1">⌘K</kbd></span>
      </section>
    );

  return (
    <section className="rounded-lg border border-line bg-surface-1 p-3.5">
      <p className="font-display text-[13px] leading-relaxed text-paper">
        The book holds <span className="num text-brand">{fmt$(eve)}</span> of economic value of equity,
        running <span className="num">{k.eve.duration_gap_y.toFixed(2)}y</span> long with net dv01 of{" "}
        <span className="num">{fmt$(k.eve.dv01_net_$)}/bp</span>. A +200bp shock moves EVE{" "}
        <span className={`num ${Math.abs(d200v) > 15 ? "text-down" : "text-up"}`}>{d200v.toFixed(1)}%</span>
        {k.eve.irrbb_outlier
          ? " — outside the 15% line; the overlay needs work before this clears review."
          : " — inside the 15% line; the hedge overlay is doing its job."}
      </p>
      <div className="mt-3 flex items-baseline justify-between border-b border-line pb-1">
        <h2 className="text-[11px] font-medium uppercase tracking-[0.18em] text-paper-dim">Constraint ledger</h2>
        <span className="hidden text-[10px] text-paper-faint sm:block">headroom to limit — brass mark is the line · click a row to open its tool</span>
      </div>
      <Headroom label="EVE sensitivity (+200bp)" value={Math.abs(d200v)} limit={15} sense="ceiling" unit="%" onOpen={() => open("kpis")} />
      <Headroom label="Liquidity coverage" value={k.lcr.lcr_pct} limit={110} sense="floor" unit="%" onOpen={() => open("kpis")} />
      <Headroom label="Stable funding" value={k.nsfr.nsfr_pct} limit={100} sense="floor" unit="%" onOpen={() => open("kpis")} />
      <Headroom label="CET1, end of plan" value={k.capital.cet1_path[k.capital.cet1_path.length - 1].cet1_ratio_pct} limit={10} sense="floor" unit="%" onOpen={() => open("kpis")} />
      <Headroom label="Duration gap" value={k.eve.duration_gap_y} limit={2.0} sense="ceiling" unit="y" onOpen={() => open("risk")} />
    </section>
  );
}

/* ── tile catalog ─────────────────────────────────────────────────────── */
const TILE_DEFS: TileDef[] = [
  { id: "risk", title: "Risk Desk", subtitle: "KRD profile · NII forecast · 9Q stress P&L", defaultSize: "full", defaultShown: true, render: () => <Dashboard /> },
  { id: "kpis", title: "KPI detail", subtitle: "ΔEVE sensitivity · CET1 (9Q) projection", defaultSize: "lg", defaultShown: true, render: () => <KpiTile /> },
  { id: "positions", title: "Positions", subtitle: "side → book → position · indicative client-side derivations", defaultSize: "lg", defaultShown: true, render: () => <Positions /> },
  { id: "optimizer", title: "Optimizer", subtitle: "robust balance-sheet LP · shadow prices", defaultSize: "full", defaultShown: true, render: () => <OptimizerPage /> },
  { id: "strategy", title: "Strategy Lab", subtitle: "live allocation sandbox · sub-ms KPI recalc", defaultSize: "full", defaultShown: false, render: () => <StrategyPage /> },
  { id: "market", title: "Market & Scenarios", subtitle: "par curve · 9Q scenario builder", defaultSize: "full", defaultShown: false, render: () => <MarketPage /> },
  { id: "books", title: "Book Editor", subtitle: "6 books · table view + JSON edit", defaultSize: "full", defaultShown: false, render: () => <BalanceSheet /> },
  { id: "assumptions", title: "Assumptions & Settings", subtitle: "deposit attrition · prepay vector · run config", defaultSize: "lg", defaultShown: false, badge: <Badge tone="amber">prepay restart</Badge>, render: () => <AssumptionsPanel /> },
];

function WorkbenchInner() {
  return (
    <div className="min-h-screen bg-surface">
      <Masthead />
      <main className="mx-auto max-w-[1600px] space-y-3 px-4 py-3 pb-16">
        <Ledger />
        <TileGrid />
        <div className="pt-2 text-center text-[10px] leading-relaxed text-paper-faint">
          portfolio-risk v0.17.3 · LMM Monte Carlo · fixed-OAS / CRN · synthetic WFC-1Q26-proportional book · <kbd className="rounded border border-line px-1">⌘K</kbd> for commands
        </div>
      </main>
      <CommandPalette />
    </div>
  );
}

export default function Workbench() {
  const tiles = useMemo(() => TILE_DEFS, []);
  return (
    <EngineProvider>
      <TilesProvider tiles={tiles}>
        <WorkbenchInner />
      </TilesProvider>
    </EngineProvider>
  );
}
