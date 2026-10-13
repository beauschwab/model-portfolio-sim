import { Trash2 } from "lucide-react";
import { useEngineData } from "../lib/engine";
/** Interactive strategy builder: allocations -> /strategy/eval (sync,
 * sub-ms) with live top-level KPI recalc. Requires the unit library
 * (one-time ~20s build); every slider move re-runs full KPIs. */
import { useEffect, useMemo, useRef, useState } from "react";
import { Area, AreaChart, CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { chartAxis, chartGrid, chartTip, still } from "../components/charts";
import { LIMITS, limitVariance } from "../lib/limits";
import { api, awaitJob, fmt$ } from "../lib/api";
import { Badge, Button, Card, CardBody, CardHeader, Input, Stat, InfoPop } from "../components/ui";

type Alloc = { template: string; purchase_m: number; notional: number };
type Eval = {
  nii_incremental: number[]; balance: number[]; fwd_dv01: number[];
  nii_total_$: number; dv01_at_t0_$: number;
  reporting_ordinals?:number[]; dv01_method?:string;
  kpis?: { "d_eve_pct_eve_+200": number; duration_gap_y: number; lcr_pct: number; nsfr_pct: number; cet1_q9_pct: number };
};
const TEMPLATES = ["agency_mbs", "resi_whole_loan", "cml_fixed_5y", "cml_float_3y", "auto_annuity_5y", "cd_2y", "mmda_growth"];

async function evalStrategy(alloc: Alloc[], signal?: AbortSignal): Promise<Eval> {
  return api.strategyEval(alloc, signal) as Promise<Eval>;
}

export default function StrategyPage() {
  const engine = useEngineData();
  const libReady = engine.libraryReady;
  const dated=['aged-units-1','product-units-1'].includes(engine.libraryDetails.schema??'');
  const templates=dated?engine.libraryDetails.templates:TEMPLATES;
  const libraryKey=JSON.stringify(engine.libraryDetails);
  const ordinalDate=(day:number)=>new Date((day-719163)*86400000).toISOString().slice(0,10);
  const [building, setBuilding] = useState(false);
  const [rows, setRows] = useState<Alloc[]>([
    { template: "agency_mbs", purchase_m: 0, notional: 2e9 },
    { template: "cd_2y", purchase_m: 0, notional: 1e9 },
  ]);
  const [res, setRes] = useState<Eval | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const timer = useRef<number | null>(null);
  useEffect(()=>{setRes(null);setErr(null);if(dated&&templates.length){setRows([{template:templates[0],purchase_m:engine.libraryDetails.grid[templates[0]]?.[0]??0,notional:1e6}]);}},[libraryKey]);

  const buildLib = async () => {
    setBuilding(true);
    try {
      const done = await engine.run("unitlib");
      if (done.status === "error") alert(done.detail);
    } finally { setBuilding(false); }
  };

  useEffect(() => { setRes(null); setErr(null); }, [engine.revision]);
  // Discard obsolete responses even when cancellation arrives after completion.
  useEffect(() => {
    if (!libReady) return;
    const controller = new AbortController();
    let current = true;
    const id = window.setTimeout(() => {
      evalStrategy(rows, controller.signal).then(r => {
        if (current) { setRes(r); setErr(null); }
      }).catch(e => {
        if (current && !controller.signal.aborted) {
          setErr(String(e));
          window.dispatchEvent(new Event("engine:inputs-changed"));
        }
      });
    }, 150);
    return () => { current = false; clearTimeout(id); controller.abort(); };
  }, [rows, libReady, engine.revision,libraryKey]);

  const niiData = useMemo(() => res?.nii_incremental.map((v, i) => ({ month: res.reporting_ordinals?ordinalDate(res.reporting_ordinals[i+1]):i + 1, nii: v })) ?? [], [res]);
  const dvData = useMemo(() => res?.fwd_dv01.map((v, i) => ({ month: res.reporting_ordinals?ordinalDate(res.reporting_ordinals[i]):i + 1, dv01: v })) ?? [], [res]);
  const set = (i: number, k: keyof Alloc, v: string) =>
    setRows(rs => rs.map((r, j) => j === i ? { ...r, [k]: k === "template" ? v : Number(v) || 0 } : r));

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-3">
        {!libReady
          ? <Button disabled={building} onClick={buildLib}>{building ? "building unit library…" : "Build unit library (~20s, one-time)"}</Button>
          : <Badge tone="up">{dated?'dated coefficients ready — incremental allocation evaluation':'unit library ready — edits recalc all KPIs live (~sub-ms)'}</Badge>}
        {err && <Badge tone="danger">{err.slice(0, 80)}</Badge>}
      </div>

      <Card>
        <CardHeader title={"Allocations"} sub={dated?'Actual-date products at supplied purchase quotes; original yield and purchase OAS retained':'forward-starting at-market purchases; behavioral models live in the unit tensor — see ⓘ on each row for template terms'}
          right={<Button variant="secondary" onClick={() => setRows([...rows, { template: templates[0], purchase_m:dated?(engine.libraryDetails.grid[templates[0]]?.[0]??0):0, notional: 1e9 }])}>+ row</Button>} />
        <CardBody className="space-y-2">
          {rows.map((r, i) => (
            <div key={i} className="flex items-center gap-2">
              <select className="h-8 rounded-md border border-line bg-surface-2 px-2 text-xs text-paper"
                value={r.template} onChange={e => {const name=e.target.value;setRows(rs=>rs.map((row,j)=>j===i?{...row,template:name,purchase_m:dated?(engine.libraryDetails.grid[name]?.[0]??0):row.purchase_m}:row));}}>
                {templates.map(t => <option key={t}>{t}</option>)}
              </select>
              <span className="text-2xs text-paper-faint">{dated?'purchase date':'month'}</span>
              {dated?<select aria-label="Purchase date" className="rounded border border-line bg-surface-2 px-2 text-xs" value={r.purchase_m} onChange={e=>set(i,'purchase_m',e.target.value)}>
                {(engine.libraryDetails.grid[r.template]??[]).map(h=><option key={h} value={h}>{ordinalDate(engine.libraryDetails.ordinals[h])}</option>)}
              </select>:<><input type="range" min={0} max={Math.max(0, engine.libraryHorizon - 1)} step={1} value={r.purchase_m} className="w-32 accent-brand"
                onChange={e => set(i, "purchase_m", e.target.value)} /><span className="num w-6 text-xs">{r.purchase_m}</span></>}
              <span className="text-2xs text-paper-faint">notional $</span>
              <Input className="w-36" value={r.notional} onChange={e => set(i, "notional", e.target.value)} />
              <span className="num text-sm text-paper-dim">{fmt$(r.notional)}</span>
              <InfoPop width="15rem">{({ agency_mbs: "New-production agency pool at fwd 10y + 130bp, full prepay model live.", resi_whole_loan: "Whole-loan resi at fwd + 170bp; RSF 65%, RWA 50%.", cml_fixed_5y: "5y fixed commercial at fwd 5y + 190bp, bullet.", cml_float_3y: "3y SOFR + 180bp floater, quarterly resets.", auto_annuity_5y: "5y auto at fwd + 280bp, linear amortization.", cd_2y: "2y retail CD at fwd 2y + 15bp — funding; ASF 100% beyond 1y.", mmda_growth: "MMDA growth cohort at the modeled equilibrium rate; attrition model live." } as Record<string, string>)[r.template]}</InfoPop>
              <Button variant="secondary" size="sm" aria-label="Remove row" onClick={() => setRows(rows.filter((_, j) => j !== i))}><Trash2 aria-hidden className="h-3.5 w-3.5" strokeWidth={1.5} /></Button>
            </div>
          ))}
        </CardBody>
      </Card>

      {res?.kpis && (
        <div className="grid grid-cols-2 gap-3 lg:grid-cols-6">
          <Stat label="Incr. NII (horizon)" value={fmt$(res.nii_total_$)} />
          <Stat label="ΔEVE @ +200 (new)" value={`${res.kpis["d_eve_pct_eve_+200"].toFixed(1)}%`}
            variance={limitVariance(Math.abs(res.kpis["d_eve_pct_eve_+200"]), LIMITS.eve200)} />
          <Stat label="Duration gap" value={`${res.kpis.duration_gap_y.toFixed(2)}y`}
            variance={limitVariance(res.kpis.duration_gap_y, LIMITS.durationGap, 2)} />
          <Stat label="LCR" value={`${res.kpis.lcr_pct.toFixed(0)}%`} variance={limitVariance(res.kpis.lcr_pct, LIMITS.lcr, 0)} />
          <Stat label="NSFR" value={`${res.kpis.nsfr_pct.toFixed(0)}%`} variance={limitVariance(res.kpis.nsfr_pct, LIMITS.nsfr, 0)} />
          <Stat label="CET1 @ horizon" value={`${res.kpis.cet1_q9_pct.toFixed(2)}%`}
            variance={limitVariance(res.kpis.cet1_q9_pct, LIMITS.cet1, 2)} />
        </div>
      )}

      {res && (
        <div className="grid gap-3 xl:grid-cols-2">
          {dated&&<div className="xl:col-span-2"><Stat label="Incremental NII across reporting intervals" value={fmt$(res.nii_total_$)}/></div>}
          <Card>
            <CardHeader title="Incremental NII" sub={dated?'Actual reporting intervals, $ — retained original effective yield':'monthly, $ — at-market carry of the program set'} />
            <CardBody>
              <ResponsiveContainer width="100%" height={220}>
                <AreaChart data={niiData}>
                  <CartesianGrid {...chartGrid} />
                  <XAxis dataKey="month" {...chartAxis} />
                  <YAxis {...chartAxis} tickFormatter={v => `${(v / 1e6).toFixed(0)}M`} />
                  <Tooltip {...chartTip}
                    formatter={(v: number) => `$${(v / 1e6).toFixed(2)}M`} />
                  <Area dataKey="nii" name="NII" stroke="var(--viz-1)" fill="var(--yellow-softer)" strokeWidth={1.5} {...still} />
                </AreaChart>
              </ResponsiveContainer>
            </CardBody>
          </Card>
          <Card>
            <CardHeader title="Forward dv01 added" sub={res.dv01_method??'$/bp; base unit sensitivity scaled by outstanding balance'} />
            <CardBody>
              <ResponsiveContainer width="100%" height={220}>
                <LineChart data={dvData}>
                  <CartesianGrid {...chartGrid} />
                  <XAxis dataKey="month" {...chartAxis} />
                  <YAxis {...chartAxis} tickFormatter={v => `${(v / 1e3).toFixed(0)}k`} />
                  <Tooltip {...chartTip}
                    formatter={(v: number) => `$${(v / 1e3).toFixed(0)}k/bp`} />
                  <Line dataKey="dv01" name="DV01" stroke="var(--viz-2)" dot={false} strokeWidth={1.5} {...still} />
                </LineChart>
              </ResponsiveContainer>
            </CardBody>
          </Card>
        </div>
      )}
    </div>
  );
}
