import { LIMITS, headroom, isTight } from "../lib/limits";
import { useEngineData } from "../lib/engine";
/** The Morning Sheet — the strategist's entry point, typeset as a
 * decision memo: masthead with the engraved curve, the position in one
 * paragraph of prose, then the constraint ledger where every KPI is
 * shown as HEADROOM TO ITS LIMIT (the decision-maker's real mental
 * model), and a queue of next actions. One orchestrated load reveal;
 * reduced motion respected. */
import { useEffect, useMemo, useState } from "react";
import { api, fmt$, type Market } from "../lib/api";
import { Button, InfoPop } from "../components/ui";
import { useWorkspace } from "../workspace/Workspace";
import type { PanelId } from "../workspace/panels";

type Kpis = {
  eve: { eve_$: number; duration_gap_y: number; dv01_net_$: number;
    irrbb_outlier: boolean; irrbb_worst_pct_eve: number;
    sensitivity: { shock_bp: number; d_eve_pct_eve: number }[] };
  lcr: { lcr_pct: number }; nsfr: { nsfr_pct: number };
  capital: { cet1_path: { cet1_ratio_pct: number }[] };
};

/** Headroom row: value, limit, and the distance between them as a bar.
 * Brass marker sits at the limit; the bar is the room you have. */
function Headroom({ label, value, limit, sense, unit, panel }: {
  label: string; value: number; limit: number;
  sense: "floor" | "ceiling"; unit: "%" | "y"; panel: PanelId;
}) {
  const { openPanel } = useWorkspace();
  const room = headroom(value, { label, limit, sense, unit });
  const pct = Math.max(0, Math.min(1, room / Math.max(Math.abs(limit), 1e-9)));
  const tight = isTight(value, { label, limit, sense, unit });
  return (
    <button type="button" onClick={() => openPanel(panel)} className="memo-rise group grid w-full grid-cols-12 items-center gap-3 border-b border-line py-2.5 text-left hover:bg-surface-1">
      <div className="col-span-3 text-md text-paper-dim group-hover:text-paper">{label}</div>
      <div className="col-span-2 num text-right text-md text-paper">{value.toFixed(unit === "y" ? 2 : 1)}{unit}</div>
      <div className="col-span-2 num text-right text-sm text-paper-faint">{sense === "floor" ? "≥" : "≤"} {limit}{unit}</div>
      <div className="col-span-4">
        <div className="relative h-1.5 rounded-sm bg-surface-3">
          <div className={`absolute inset-y-0 left-0 rounded-sm ${room < 0 ? "bg-down" : tight ? "bg-warning" : "bg-up"}`}
            style={{ width: `${pct * 100}%` }} />
          <div className="absolute inset-y-0 left-0 w-px bg-brand" />
        </div>
      </div>
      <div className={`col-span-1 num text-right text-sm ${room < 0 ? "text-down" : tight ? "text-warning" : "text-up"}`}>
        {room >= 0 ? "+" : ""}{room.toFixed(1)}
      </div>
    </button>
  );
}

export default function MorningSheet() {
  const engine = useEngineData();
  const [mkt, setMkt] = useState<Market | null>(null);
  const k = engine.kpis;
  const [busy, setBusy] = useState(false);
  const today = useMemo(() => new Date().toLocaleDateString("en-US",
    { weekday: "long", month: "long", day: "numeric", year: "numeric" }), []);

  useEffect(() => { api.market().then(setMkt); }, []);
  const run = async () => {
    setBusy(true);
    try {
      const done = await engine.run("kpis");
      if (done.status === "error") alert(done.detail);
    } finally { setBusy(false); }
  };

  // the engraved curve: market pillars as a single inked stroke
  const curvePath = useMemo(() => {
    if (!mkt) return "";
    const t = [1, 2, 3, 4, 5, 7, 10, 15, 20, 30];
    const xs = t.map(x => 12 + (Math.log(x) / Math.log(30)) * 296);
    const r = mkt.swap_rates;
    const [lo, hi] = [Math.min(...r), Math.max(...r)];
    const ys = r.map(v => 44 - ((v - lo) / Math.max(hi - lo, 1e-9)) * 34);
    return xs.map((x, i) => `${i ? "L" : "M"}${x.toFixed(1)},${ys[i].toFixed(1)}`).join(" ");
  }, [mkt]);

  const { openPanel } = useWorkspace();
  const d200 = k?.eve.sensitivity.find(s => s.shock_bp === 200)?.d_eve_pct_eve ?? 0;

  return (
    <div className="mx-auto max-w-3xl">
      {/* masthead */}
      <header className="memo-rise border-b-2 border-paper-faint pb-4 pt-2">
        <div className="flex items-end justify-between">
          <div>
            <div className="eyebrow">Treasury · balance sheet & rate risk</div>
            <div className="mt-1 text-sm text-paper-faint">{today} · 10y {mkt ? (mkt.swap_rates[6] * 100).toFixed(2) : "—"}% · 2s10s {mkt ? ((mkt.swap_rates[6] - mkt.swap_rates[1]) * 1e4).toFixed(0) : "—"}bp</div>
          </div>
          <svg width="320" height="48" className="text-paper-dim" aria-label="Par curve">
            <path d={curvePath} fill="none" stroke="currentColor" strokeWidth="1.25" />
            <path d={curvePath} fill="none" stroke="var(--accent)" strokeWidth="1.25" strokeDasharray="2 5" opacity="0.6" />
          </svg>
        </div>
      </header>

      {/* the position, in prose */}
      <section className="memo-rise py-6">
        {k ? (
          <p className="font-display text-lg leading-relaxed text-paper" style={{ fontVariationSettings: '"opsz" 18' }}>
            The book holds <span className="num text-brand">{fmt$(k.eve.eve_$)}</span> of economic value of equity,
            running <span className="num">{k.eve.duration_gap_y.toFixed(2)}y</span> long with net dv01 of{" "}
            <span className="num">{fmt$(k.eve.dv01_net_$)}/bp</span>. A +200bp shock moves EVE{" "}
            <span className={`num ${Math.abs(d200) > 15 ? "text-down" : "text-up"}`}>{d200.toFixed(1)}%</span>
            {k.eve.irrbb_outlier
              ? " — outside the 15% line. The overlay needs work before this clears review."
              : " — inside the 15% line; the hedge overlay is doing its job."}
          </p>
        ) : (
          <div className="flex items-center gap-4">
            <p className="font-display text-lg text-paper-dim">Pull this morning's position to begin.</p>
            <Button disabled={busy} onClick={run}>{busy ? "Computing…" : "Run the sheet"}</Button>
          </div>
        )}
      </section>

      {/* the constraint ledger: headroom, not levels */}
      {k && (
        <section className="memo-rise">
          <div className="flex items-baseline justify-between border-b border-paper-faint pb-1">
            <h2 className="eyebrow">Constraint ledger</h2>
            <span className="flex items-center text-2xs text-paper-faint">headroom to limit — brass mark is the line
              <InfoPop width="15rem">Each row shows distance to its binding limit, not the ratio's level. Oxblood = breached, brass = inside 8% of the line, verdigris = comfortable. Click a row to open the tool that moves it.</InfoPop></span>
          </div>
          <Headroom {...LIMITS.eve200} value={Math.abs(d200)} panel="kpis" />
          <Headroom {...LIMITS.lcr} value={k.lcr.lcr_pct} panel="kpis" />
          <Headroom {...LIMITS.nsfr} value={k.nsfr.nsfr_pct} panel="kpis" />
          <Headroom {...LIMITS.cet1} value={k.capital.cet1_path[k.capital.cet1_path.length - 1].cet1_ratio_pct} panel="kpis" />
          <Headroom {...LIMITS.durationGap} value={k.eve.duration_gap_y} panel="risk" />
        </section>
      )}

      {/* decisions queue */}
      <section className="memo-rise grid gap-3 py-8 sm:grid-cols-3">
        {([
          ["strategy", "Test a reinvestment", "Slide allocations against live constraints."],
          ["optimizer", "Price the constraints", "Solve the plan; read the shadow prices."],
          ["market", "Move the market", "Set a 9Q path and rerun the sheet."],
        ] as const).map(([panel, t, s]) => (
          <button key={panel} type="button" onClick={() => openPanel(panel)} className="group border-t-2 border-brand pt-3 text-left hover:bg-surface-1">
            <div className="font-display text-lg text-paper group-hover:text-brand">{t}</div>
            <div className="mt-1 text-sm text-paper-faint">{s}</div>
          </button>
        ))}
      </section>
    </div>
  );
}
