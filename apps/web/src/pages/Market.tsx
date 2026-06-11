/** Market data + 9Q scenario builder: curve editor, vol grid, scenario
 * legs (10y level / 2s10s / spread / vol) with the projected curve. */
import { useEffect, useMemo, useState } from "react";
import { api, awaitJob, type Market, type Scenario } from "../lib/api";
import { CurveChart, ScenarioPath } from "../components/charts";
import { Button, Card, CardBody, CardHeader, Input, Badge } from "../components/ui";

const TENORS = [1, 2, 3, 4, 5, 7, 10, 15, 20, 30];
const emptySc = (name: string): Scenario => ({ name, ust10y_bp: [], twos_tens_bp: [], spread_bp: [], vol_bp: [] });

export default function MarketPage() {
  const [mkt, setMkt] = useState<Market | null>(null);
  const [rates, setRates] = useState<number[]>([]);
  const [scs, setScs] = useState<Record<string, Scenario>>({});
  const [sc, setSc] = useState<Scenario>(emptySc("bear_steepener"));
  const [path, setPath] = useState<Record<string, number>[] | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.market().then(m => { setMkt(m); setRates(m.swap_rates); });
    api.scenarios().then(setScs);
  }, []);

  const legAtQ = (xs: number[], q: number) => (xs.length ? xs[Math.min(q, xs.length - 1)] : 0);
  const curveData = useMemo(() => TENORS.map((t, i) => {
    const lvl = legAtQ(sc.ust10y_bp, 0) * 1e-4;
    const tw = legAtQ(sc.twos_tens_bp, 0) * 1e-4;
    const twist = tw * (Math.min(Math.max(t, 2), 10) - 5) / 8;
    return { tenor: t, base: rates[i] ?? 0, scenario: (rates[i] ?? 0) + lvl + twist };
  }), [rates, sc]);

  const saveMarket = async () => { await api.putMarket({ swap_rates: rates, vol_pts: mkt!.vol_pts }); alert("market saved"); };
  const saveScenario = async () => { await api.putScenario(sc); setScs(await api.scenarios()); };
  const runScenarioNii = async () => {
    setBusy(true);
    try {
      await api.putScenario(sc);
      const j = await api.run("nii", sc.name);
      const done = await awaitJob(j.id);
      if (done.status === "done") setPath((done.result as { path: Record<string, number>[] }).path);
      else alert(done.detail);
    } finally { setBusy(false); }
  };

  const LegEditor = ({ label, value, onChange }: { label: string; value: number[]; onChange: (v: number[]) => void }) => (
    <div className="flex items-center gap-2">
      <div className="w-28 text-xs text-zinc-400">{label}</div>
      <Input placeholder="e.g. 25, 50, 75, 100  (bp per quarter, last extends)"
        value={value.join(", ")}
        onChange={e => onChange(e.target.value.split(",").map(s => parseFloat(s.trim())).filter(n => !isNaN(n)))} />
    </div>
  );

  return (
    <div className="grid gap-4 xl:grid-cols-2">
      <Card>
        <CardHeader title="Par swap curve" sub={mkt?.source ?? ""} right={<Button variant="ghost" onClick={saveMarket}>Save market</Button>} />
        <CardBody>
          <CurveChart data={curveData} />
          <div className="mt-3 grid grid-cols-5 gap-2">
            {TENORS.map((t, i) => (
              <div key={t}>
                <div className="mb-1 text-[10px] text-zinc-500">{t}y</div>
                <Input value={((rates[i] ?? 0) * 100).toFixed(3)}
                  onChange={e => { const r = [...rates]; r[i] = parseFloat(e.target.value) / 100 || 0; setRates(r); }} />
              </div>
            ))}
          </div>
        </CardBody>
      </Card>

      <Card>
        <CardHeader title="9Q scenario builder" sub="trader-space legs mapped onto the LMM market: level via 10y, 2s10s twist around the 5y pivot, spread first-order on dv01, vol parallel on the ATM surface"
          right={<div className="flex gap-2">
            <Button variant="ghost" onClick={saveScenario}>Save</Button>
            <Button disabled={busy} onClick={runScenarioNii}>{busy ? "running…" : "Run 9Q NII"}</Button>
          </div>} />
        <CardBody className="space-y-3">
          <div className="flex items-center gap-2">
            <div className="w-28 text-xs text-zinc-400">name</div>
            <Input value={sc.name} onChange={e => setSc({ ...sc, name: e.target.value })} />
            <div className="flex gap-1">{Object.keys(scs).map(n => (
              <button key={n} className="rounded bg-surface-3 px-2 py-1 text-[10px] text-zinc-400 hover:text-brand"
                onClick={() => setSc(scs[n])}>{n}</button>))}
            </div>
          </div>
          <LegEditor label="10y UST (bp)" value={sc.ust10y_bp} onChange={v => setSc({ ...sc, ust10y_bp: v })} />
          <LegEditor label="2s10s (bp)" value={sc.twos_tens_bp} onChange={v => setSc({ ...sc, twos_tens_bp: v })} />
          <LegEditor label="spread (bp)" value={sc.spread_bp} onChange={v => setSc({ ...sc, spread_bp: v })} />
          <LegEditor label="vol (bp)" value={sc.vol_bp} onChange={v => setSc({ ...sc, vol_bp: v })} />
          {path
            ? <ScenarioPath data={path} />
            : <div className="flex h-40 items-center justify-center text-xs text-zinc-600">
                define legs and run — each quarter revalues the full balance sheet on the shifted market <Badge tone="zinc">base OAS held fixed</Badge>
              </div>}
        </CardBody>
      </Card>
    </div>
  );
}
