/** The Morning Sheet — the strategist's entry point, typeset as a
 * decision memo: masthead with the engraved curve, the position in one
 * paragraph of prose, then the constraint ledger where every KPI is
 * shown as HEADROOM TO ITS LIMIT (the decision-maker's real mental
 * model), and a queue of next actions. One orchestrated load reveal;
 * reduced motion respected. */
import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { api, awaitJob, fmt$, type Market } from "../lib/api";
import { Button, InfoPop } from "../components/ui";

type Kpis = {
  eve: { eve_$: number; duration_gap_y: number; dv01_net_$: number;
    irrbb_outlier: boolean; irrbb_worst_pct_eve: number;
    sensitivity: { shock_bp: number; d_eve_pct_eve: number }[] };
  lcr: { lcr_pct: number }; nsfr: { nsfr_pct: number };
  capital: { cet1_path: { cet1_ratio_pct: number }[] };
};

/** Headroom row: value, limit, and the distance between them as a bar.
 * Brass marker sits at the limit; the bar is the room you have. */
function Headroom({ label, value, limit, sense, unit, to }: {
  label: string; value: number; limit: number;
  sense: "floor" | "ceiling"; unit: string; to: string;
}) {
  const room = sense === "floor" ? value - limit : limit - value;
  const pct = Math.max(0, Math.min(1, room / Math.max(Math.abs(limit), 1e-9)));
  const tight = room < 0.08 * Math.abs(limit);
  return (
    <Link to={to} className="memo-rise group grid grid-cols-12 items-center gap-3 border-b border-line py-2.5 hover:bg-surface-1">
      <div className="col-span-3 text-sm text-paper-dim group-hover:text-paper">{label}</div>
      <div className="col-span-2 num text-right text-sm text-paper">{value.toFixed(unit === "y" ? 2 : 1)}{unit}</div>
      <div className="col-span-2 num text-right text-xs text-paper-faint">{sense === "floor" ? "≥" : "≤"} {limit}{unit}</div>
      <div className="col-span-4">
        <div className="relative h-1.5 rounded-full bg-surface-3">
          <div className={`absolute inset-y-0 left-0 rounded-full ${room < 0 ? "bg-down" : tight ? "bg-brand" : "bg-up"}`}
            style={{ width: `${pct * 100}%` }} />
          <div className="absolute inset-y-0 left-0 w-px bg-brand" />
        </div>
      </div>
      <div className={`col-span-1 num text-right text-xs ${room < 0 ? "text-down" : tight ? "text-brand" : "text-up"}`}>
        {room >= 0 ? "+" : ""}{room.toFixed(1)}
      </div>
    </Link>
  );
}

export default function MorningSheet() {
  const [mkt, setMkt] = useState<Market | null>(null);
  const [k, setK] = useState<Kpis | null>(null);
  const [busy, setBusy] = useState(false);
  const today = useMemo(() => new Date().toLocaleDateString("en-US",
    { weekday: "long", month: "long", day: "numeric", year: "numeric" }), []);

  useEffect(() => { api.market().then(setMkt); }, []);
  const run = async () => {
    setBusy(true);
    try {
      const j = await api.run("kpis");
      const done = await awaitJob(j.id);
      if (done.status === "done") setK(done.result as Kpis); else alert(done.detail);
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

  const d200 = k?.eve.sensitivity.find(s => s.shock_bp === 200)?.d_eve_pct_eve ?? 0;

  return (
    <div className="mx-auto max-w-3xl">
      {/* masthead */}
      <header className="memo-rise border-b-2 border-paper-faint pb-4 pt-2">
        <div className="flex items-end justify-between">
          <div>
            <div className="text-[10px] uppercase tracking-[0.25em] text-paper-faint">Treasury · balance sheet & rate risk</div>
            <h1 className="font-display text-3xl font-medium text-paper" style={{ fontVariationSettings: '"opsz" 40' }}>
              The Morning Sheet
            </h1>
            <div className="mt-1 text-xs text-paper-faint">{today} · 10y {mkt ? (mkt.swap_rates[6] * 100).toFixed(2) : "—"}% · 2s10s {mkt ? ((mkt.swap_rates[6] - mkt.swap_rates[1]) * 1e4).toFixed(0) : "—"}bp</div>
          </div>
          <svg width="320" height="48" className="text-paper-dim" aria-label="par curve">
            <path d={curvePath} fill="none" stroke="currentColor" strokeWidth="1.25" />
            <path d={curvePath} fill="none" stroke="#fcd535" strokeWidth="1.25" strokeDasharray="2 5" opacity="0.6" />
          </svg>
        </div>
      </header>

      {/* the position, in prose */}
      <section className="memo-rise py-6" style={{ animationDelay: "80ms" }}>
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
            <Button disabled={busy} onClick={run}>{busy ? "computing…" : "Run the sheet"}</Button>
          </div>
        )}
      </section>

      {/* the constraint ledger: headroom, not levels */}
      {k && (
        <section style={{ animationDelay: "160ms" }} className="memo-rise">
          <div className="flex items-baseline justify-between border-b border-paper-faint pb-1">
            <h2 className="font-display text-sm font-medium uppercase tracking-[0.18em] text-paper-dim">Constraint ledger</h2>
            <span className="flex items-center text-[10px] text-paper-faint">headroom to limit — brass mark is the line
              <InfoPop width="15rem">Each row shows distance to its binding limit, not the ratio's level. Oxblood = breached, brass = inside 8% of the line, verdigris = comfortable. Click a row to open the tool that moves it.</InfoPop></span>
          </div>
          <Headroom label="EVE sensitivity (+200bp)" value={Math.abs(d200)} limit={15} sense="ceiling" unit="%" to="/kpis" />
          <Headroom label="Liquidity coverage" value={k.lcr.lcr_pct} limit={110} sense="floor" unit="%" to="/kpis" />
          <Headroom label="Stable funding" value={k.nsfr.nsfr_pct} limit={100} sense="floor" unit="%" to="/kpis" />
          <Headroom label="CET1, end of plan" value={k.capital.cet1_path[k.capital.cet1_path.length - 1].cet1_ratio_pct} limit={10} sense="floor" unit="%" to="/kpis" />
          <Headroom label="Duration gap" value={k.eve.duration_gap_y} limit={2.0} sense="ceiling" unit="y" to="/risk" />
        </section>
      )}

      {/* decisions queue */}
      <section className="memo-rise grid gap-3 py-8 sm:grid-cols-3" style={{ animationDelay: "240ms" }}>
        {([
          ["/strategy", "Test a reinvestment", "Slide allocations against live constraints."],
          ["/optimizer", "Price the constraints", "Solve the plan; read the shadow prices."],
          ["/market", "Move the market", "Set a 9Q path and rerun the sheet."],
        ] as const).map(([to, t, s]) => (
          <Link key={to} to={to} className="group border-t-2 border-brand pt-3 hover:bg-surface-1">
            <div className="font-display text-base text-paper group-hover:text-brand">{t}</div>
            <div className="mt-1 text-xs text-paper-faint">{s}</div>
          </Link>
        ))}
      </section>
    </div>
  );
}
