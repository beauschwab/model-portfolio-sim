/** Interactive strategy builder: allocations -> /strategy/eval (sync,
 * sub-ms) with live top-level KPI recalc. Requires the unit library
 * (one-time ~20s build); every slider move re-runs full KPIs. */
import { useEffect, useMemo, useRef, useState } from "react";
import { Area, AreaChart, CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { api, awaitJob, fmt$ } from "../lib/api";
import { Badge, Button, Card, CardBody, CardHeader, Input, Stat, InfoPop } from "../components/ui";

type Alloc = { template: string; purchase_m: number; notional: number };
type Eval = {
  nii_incremental: number[]; balance: number[]; fwd_dv01: number[];
  nii_total_$: number; dv01_at_t0_$: number;
  kpis?: { "d_eve_pct_eve_+200": number; duration_gap_y: number; lcr_pct: number; nsfr_pct: number; cet1_q9_pct: number };
};
const TEMPLATES = ["agency_mbs", "resi_whole_loan", "cml_fixed_5y", "cml_float_3y", "auto_annuity_5y", "cd_2y", "mmda_growth"];

async function evalStrategy(alloc: Alloc[]): Promise<Eval> {
  const r = await fetch("/api/strategy/eval", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(alloc) });
  if (!r.ok) throw new Error(await r.text());
  return r.json();
}

export default function StrategyPage() {
  const [libReady, setLibReady] = useState(false);
  const [building, setBuilding] = useState(false);
  const [rows, setRows] = useState<Alloc[]>([
    { template: "agency_mbs", purchase_m: 0, notional: 2e9 },
    { template: "cd_2y", purchase_m: 0, notional: 1e9 },
  ]);
  const [res, setRes] = useState<Eval | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const timer = useRef<number | null>(null);

  const buildLib = async () => {
    setBuilding(true);
    try {
      const j = await api.run("unitlib");
      const done = await awaitJob(j.id);
      if (done.status === "done") setLibReady(true); else alert(done.detail);
    } finally { setBuilding(false); }
  };

  // debounced live eval on every edit — the interactive loop
  useEffect(() => {
    if (!libReady) return;
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      evalStrategy(rows).then(r => { setRes(r); setErr(null); }).catch(e => setErr(String(e)));
    }, 150);
  }, [rows, libReady]);

  const niiData = useMemo(() => res?.nii_incremental.map((v, i) => ({ month: i + 1, nii: v })) ?? [], [res]);
  const dvData = useMemo(() => res?.fwd_dv01.map((v, i) => ({ month: i + 1, dv01: v })) ?? [], [res]);
  const set = (i: number, k: keyof Alloc, v: string) =>
    setRows(rs => rs.map((r, j) => j === i ? { ...r, [k]: k === "template" ? v : Number(v) || 0 } : r));

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-3">
        {!libReady
          ? <Button disabled={building} onClick={buildLib}>{building ? "building unit library…" : "Build unit library (~20s, one-time)"}</Button>
          : <Badge tone="green">unit library ready — edits recalc all KPIs live (~sub-ms)</Badge>}
        {err && <Badge tone="red">{err.slice(0, 80)}</Badge>}
      </div>

      <Card>
        <CardHeader title={"Allocations"} sub="forward-starting at-market purchases; behavioral models live in the unit tensor — see ⓘ on each row for template terms"
          right={<Button variant="ghost" onClick={() => setRows([...rows, { template: "agency_mbs", purchase_m: 0, notional: 1e9 }])}>+ row</Button>} />
        <CardBody className="space-y-2">
          {rows.map((r, i) => (
            <div key={i} className="flex items-center gap-2">
              <select className="h-8 rounded-md border border-line bg-surface-2 px-2 text-xs text-paper"
                value={r.template} onChange={e => set(i, "template", e.target.value)}>
                {TEMPLATES.map(t => <option key={t}>{t}</option>)}
              </select>
              <span className="text-[10px] text-paper-faint">month</span>
              <input type="range" min={0} max={24} step={1} value={r.purchase_m} className="w-32 accent-[#fcd535]"
                onChange={e => set(i, "purchase_m", e.target.value)} />
              <span className="num w-6 text-xs">{r.purchase_m}</span>
              <span className="text-[10px] text-paper-faint">notional $</span>
              <Input className="w-36" value={r.notional} onChange={e => set(i, "notional", e.target.value)} />
              <span className="num text-xs text-paper-dim">{fmt$(r.notional)}</span>
              <InfoPop width="15rem">{({ agency_mbs: "New-production agency pool at fwd 10y + 130bp, full prepay model live.", resi_whole_loan: "Whole-loan resi at fwd + 170bp; RSF 65%, RWA 50%.", cml_fixed_5y: "5y fixed commercial at fwd 5y + 190bp, bullet.", cml_float_3y: "3y SOFR + 180bp floater, quarterly resets.", auto_annuity_5y: "5y auto at fwd + 280bp, linear amortization.", cd_2y: "2y retail CD at fwd 2y + 15bp — funding; ASF 100% beyond 1y.", mmda_growth: "MMDA growth cohort at the modeled equilibrium rate; attrition model live." } as Record<string, string>)[r.template]}</InfoPop>
              <Button variant="danger" onClick={() => setRows(rows.filter((_, j) => j !== i))}>×</Button>
            </div>
          ))}
        </CardBody>
      </Card>

      {res?.kpis && (
        <div className="grid grid-cols-2 gap-3 lg:grid-cols-6">
          <Stat label="Incr. NII (27m)" value={fmt$(res.nii_total_$)} />
          <Stat label="ΔEVE @ +200 (new)" value={`${res.kpis["d_eve_pct_eve_+200"].toFixed(1)}%`}
            delta={Math.abs(res.kpis["d_eve_pct_eve_+200"]) > 15 ? "-IRRBB outlier" : "inside 15%"} />
          <Stat label="Duration gap" value={`${res.kpis.duration_gap_y.toFixed(2)}y`} />
          <Stat label="LCR" value={`${res.kpis.lcr_pct.toFixed(0)}%`} />
          <Stat label="NSFR" value={`${res.kpis.nsfr_pct.toFixed(0)}%`} />
          <Stat label="CET1 @ Q9" value={`${res.kpis.cet1_q9_pct.toFixed(2)}%`} />
        </div>
      )}

      {res && (
        <div className="grid gap-3 xl:grid-cols-2">
          <Card>
            <CardHeader title="Incremental NII" sub="monthly, $ — at-market carry of the program set" />
            <CardBody>
              <ResponsiveContainer width="100%" height={220}>
                <AreaChart data={niiData}>
                  <CartesianGrid stroke="#2b3139" strokeDasharray="3 3" />
                  <XAxis dataKey="month" stroke="#707a8a" fontSize={10} />
                  <YAxis stroke="#707a8a" fontSize={10} tickFormatter={v => `${(v / 1e6).toFixed(0)}M`} />
                  <Tooltip contentStyle={{ background: "#1e2329", border: "1px solid #2b3139", borderRadius: 8, fontSize: 11 }}
                    formatter={(v: number) => `$${(v / 1e6).toFixed(2)}M`} />
                  <Area dataKey="nii" stroke="#fcd535" fill="#fcd53522" strokeWidth={1.5} />
                </AreaChart>
              </ResponsiveContainer>
            </CardBody>
          </Card>
          <Card>
            <CardHeader title="Forward dv01 added" sub="$/bp by month as cohorts stack and amortize" />
            <CardBody>
              <ResponsiveContainer width="100%" height={220}>
                <LineChart data={dvData}>
                  <CartesianGrid stroke="#2b3139" strokeDasharray="3 3" />
                  <XAxis dataKey="month" stroke="#707a8a" fontSize={10} />
                  <YAxis stroke="#707a8a" fontSize={10} tickFormatter={v => `${(v / 1e3).toFixed(0)}k`} />
                  <Tooltip contentStyle={{ background: "#1e2329", border: "1px solid #2b3139", borderRadius: 8, fontSize: 11 }}
                    formatter={(v: number) => `$${(v / 1e3).toFixed(0)}k/bp`} />
                  <Line dataKey="dv01" stroke="#2dbdb6" dot={false} strokeWidth={1.5} />
                </LineChart>
              </ResponsiveContainer>
            </CardBody>
          </Card>
        </div>
      )}
    </div>
  );
}
