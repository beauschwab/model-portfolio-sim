/** Robust balance-sheet optimizer — OVERDRIVE: the solve is the spectacle.
 * Hitting Optimize opens a live solve console driven by the engine's run
 * telemetry: compute counters tween upward, a canvas "compute heartbeat"
 * traces path-evaluation throughput, and the solve log streams stage by
 * stage. When the LP lands, binding constraints snap in sorted by shadow
 * price and the allocation streams below. Everything degrades to a static,
 * fully-populated result under prefers-reduced-motion. */
import { useEffect, useMemo, useRef, useState } from "react";
import { api, awaitJob, fmt$, type Job, type RunProgress } from "../lib/api";
import { Badge, Button, Card, CardBody, CardHeader, DataTable, Input, InfoPop } from "../components/ui";
import { useReducedMotion, useTween, compact, full } from "../components/motion";
import { Heartbeat } from "../components/Heartbeat";

type Comm = { label: string; template: string; sense: ">=" | "<="; rhs: number };
type Result = {
  feasible: boolean; message?: string; worst_case_nii_$?: number; total_new_assets_$?: number;
  allocation?: { template: string; purchase_m: number; notional: number }[];
  binding_constraints?: { constraint: string; shadow_price: number }[];
};
const TPL = ["agency_mbs", "resi_whole_loan", "cml_fixed_5y", "cml_float_3y", "auto_annuity_5y", "cd_2y", "mmda_growth", "ALL_ASSET", "ALL_LIAB"];

/** A single tweened compute counter. Hero variant carries the headline number. */
function Counter({ label, value, sub, hero, reduced }: {
  label: string; value: number; sub?: string; hero?: boolean; reduced: boolean;
}) {
  const v = useTween(value, reduced);
  return (
    <div className={hero ? "rounded-lg border border-brand/30 bg-brand-deep/30 px-4 py-3" : "px-1 py-1"}>
      <div className="text-[10px] uppercase tracking-wide text-paper-faint">{label}</div>
      <div className={`num leading-tight text-paper ${hero ? "text-3xl font-semibold text-brand" : "text-lg font-medium"}`}
        title={full(value)}>{compact(v)}</div>
      {sub && <div className="num text-[10px] text-paper-faint">{sub}</div>}
    </div>
  );
}

/** The live solve console — header, progress, compute counters, heartbeat, log. */
function SolveConsole({ job, elapsed, reduced, samples }: {
  job: Job; elapsed: number; reduced: boolean; samples: { t: number; pe: number }[];
}) {
  const p: RunProgress = job.progress ?? {};
  const stats = p.stats ?? {};
  const plan = p.plan ?? {};
  const pct = Math.max(0, Math.min(100, p.pct ?? 0));
  const logRef = useRef<HTMLDivElement>(null);
  useEffect(() => { logRef.current?.scrollTo({ top: 1e6 }); }, [p.log?.length]);
  const err = job.status === "error";
  const running = job.status === "running" || job.status === "queued";

  return (
    <div className="space-y-3 rounded-xl border border-line bg-surface-1 p-4">
      <div className="flex items-center gap-3">
        <span className={`inline-block h-2 w-2 rounded-full ${err ? "bg-down" : job.status === "done" ? "bg-up" : "bg-brand"} ${running && !reduced ? "animate-pulse" : ""}`} />
        <div className="text-sm font-medium text-paper">
          {err ? "Solve failed" : job.status === "done" ? "Solve complete" : "Solving"}
          <span className="ml-2 text-xs font-normal text-paper-faint">{p.stage ?? job.status}</span>
        </div>
        <div className="num ml-auto text-xs text-paper-faint">{elapsed.toFixed(1)}s</div>
      </div>

      <div className="relative h-1.5 overflow-hidden rounded-full bg-surface-3">
        <div className="absolute inset-y-0 left-0 rounded-full bg-brand transition-[width] duration-300 ease-out" style={{ width: `${pct}%` }} />
        {running && !reduced && (
          <div className="absolute inset-y-0 left-0 w-1/4 bg-gradient-to-r from-transparent via-white/25 to-transparent solve-sweep" />
        )}
      </div>

      {err && <div className="text-[11px] text-down">{job.detail}</div>}

      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Counter hero reduced={reduced} label="path-evaluations" value={stats.path_evaluations ?? 0} sub="calculations executed" />
        <Counter reduced={reduced} label="revaluations" value={stats.revaluations ?? 0} sub="full repricings" />
        <Counter reduced={reduced} label="reductions" value={stats.reductions ?? 0} sub="path → mean collapses" />
        <Counter reduced={reduced} label="unit columns" value={stats.unit_columns ?? 0} sub="priced into the LP" />
      </div>

      <Heartbeat samples={samples} running={running} reduced={reduced} />

      <div className="grid grid-cols-2 gap-x-4 gap-y-1 border-t border-line pt-2 text-[11px] sm:grid-cols-4">
        <div className="text-paper-faint">records in scope <span className="num text-paper">{full(plan.in_scope ?? plan.records ?? 0)}</span></div>
        <div className="text-paper-faint">scenario markets <span className="num text-paper">{plan.scenario_markets ?? 1}</span></div>
        <div className="text-paper-faint">MC paths <span className="num text-paper">{plan.monte_carlo_paths ?? 0}</span></div>
        <div className="text-paper-faint">scenario paths <span className="num text-paper">{full(stats.scenario_paths ?? 0)}</span></div>
      </div>

      <div ref={logRef} className="max-h-32 overflow-auto rounded-lg border border-line bg-ink/40 p-2 font-mono text-[10.5px] leading-relaxed">
        {(p.log ?? []).map((l, i) => (
          <div key={i} className={`flex gap-2 ${!reduced ? "log-in" : ""}`}>
            <span className="num shrink-0 text-paper-faint">{l.t.toFixed(2)}s</span>
            <span className="text-paper-dim">{l.msg}</span>
          </div>
        ))}
        {!(p.log ?? []).length && <div className="text-paper-faint">waiting for first telemetry frame…</div>}
      </div>
    </div>
  );
}

export default function OptimizerPage() {
  const reduced = useReducedMotion();
  const [floors, setFloors] = useState({ lcr_min: 1.2, nsfr_min: 1.05, cet1_min: 0.10, eve_limit: 0.15, max_total_assets: 3e10 });
  const [scens, setScens] = useState<string[]>([]);
  const [picked, setPicked] = useState<string[]>([]);
  const [comm, setComm] = useState<Comm[]>([{ label: "min_cml", template: "cml_float_3y", sense: ">=", rhs: 5e9 }]);
  const [res, setRes] = useState<Result | null>(null);
  const [job, setJob] = useState<Job | null>(null);
  const [busy, setBusy] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [samples, setSamples] = useState<{ t: number; pe: number }[]>([]);

  useEffect(() => { api.scenarios().then(s => setScens(Object.keys(s))); }, []);

  // smooth local elapsed clock while solving (backend elapsed snaps per poll)
  useEffect(() => {
    if (!busy) return;
    const t0 = Date.now();
    const id = setInterval(() => setElapsed((Date.now() - t0) / 1000), 100);
    return () => clearInterval(id);
  }, [busy]);

  const run = async () => {
    setBusy(true); setRes(null); setElapsed(0); setSamples([]);
    try {
      const r = await fetch("/api/optimize", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...floors, scenarios: picked, commercial: comm }),
      });
      const first = (await r.json()) as Job;
      setJob(first);
      const done = await awaitJob(first.id, (s) => {
        setJob(s);
        const pe = s.progress?.stats?.path_evaluations ?? 0;
        const t = s.progress?.elapsed_s ?? 0;
        setSamples(prev => (prev.length && prev[prev.length - 1].t === t ? prev : [...prev, { t, pe }].slice(-240)));
      }, 300);
      setJob(done);
      if (done.status === "done") setRes(done.result as Result);
    } catch (e) {
      setJob(j => (j ? { ...j, status: "error", detail: String(e) } : j));
    } finally { setBusy(false); }
  };

  const F = ({ k, label, step }: { k: keyof typeof floors; label: string; step?: number }) => (
    <div><div className="mb-1 flex items-center text-[10px] text-paper-faint">{label}
        <InfoPop width="15rem">{k === "lcr_min" ? "Liquidity coverage floor, held in base AND every selected scenario. If it binds, its shadow price is the worst-case NII cost of one more unit of LCR." : k === "nsfr_min" ? "Stable funding floor — ASF/RSF with deck maturities driving the buckets." : k === "cet1_min" ? "CET1 ratio floor at quarter 9, NII-retention linearization (no AOCI leg)." : k === "eve_limit" ? "Two-sided |ΔEVE @ +200bp| cap as a fraction of EVE. 0.15 is the IRRBB outlier line." : "Cap on total new asset notional the optimizer may deploy."}</InfoPop>
      </div>
      <Input type="number" step={step ?? 0.01} value={floors[k]} onChange={e => setFloors({ ...floors, [k]: Number(e.target.value) })} /></div>
  );

  // shadow prices sorted by magnitude for the reveal
  const bindings = useMemo(() => {
    const b = res?.binding_constraints ?? [];
    const mx = Math.max(1e-9, ...b.map(x => Math.abs(x.shadow_price)));
    return [...b].sort((a, c) => Math.abs(c.shadow_price) - Math.abs(a.shadow_price)).map(x => ({ ...x, frac: Math.abs(x.shadow_price) / mx }));
  }, [res]);

  const showConsole = job && (busy || job.status === "error" || (job.status === "done" && !res));

  return (
    <div className="space-y-3">
      <Card>
        <CardHeader title="Robust optimization" sub="maximin worst-case NII s.t. ratio floors holding in base + every selected scenario; commercial plan as linear rows"
          right={<Button disabled={busy} onClick={run}>{busy ? "solving…" : "Optimize"}</Button>} />
        <CardBody className="space-y-3">
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
            <F k="lcr_min" label="LCR floor" /><F k="nsfr_min" label="NSFR floor" />
            <F k="cet1_min" label="CET1 @ Q9 floor" step={0.005} /><F k="eve_limit" label="|ΔEVE+200| limit (× EVE)" step={0.01} />
            <F k="max_total_assets" label="Max new assets $" step={1e9} />
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-[10px] text-paper-faint">robust across:</span>
            <Badge tone="zinc">base</Badge>
            {scens.map(s => (
              <button key={s} onClick={() => setPicked(p => p.includes(s) ? p.filter(x => x !== s) : [...p, s])}
                className={`rounded-full px-2 py-0.5 text-[10px] ${picked.includes(s) ? "bg-emerald-950 text-brand" : "bg-surface-3 text-paper-dim"}`}>{s}</button>
            ))}
            {!scens.length && <span className="text-[10px] text-paper-faint">define scenarios in Market & Scenarios</span>}
          </div>
          <div className="space-y-2">
            <div className="flex items-center gap-2 text-[10px] text-paper-faint">commercial plan
              <Button variant="ghost" onClick={() => setComm([...comm, { label: `row_${comm.length}`, template: "agency_mbs", sense: ">=", rhs: 1e9 }])}>+ row</Button></div>
            {comm.map((c, i) => (
              <div key={i} className="flex items-center gap-2">
                <Input className="w-36" value={c.label} onChange={e => setComm(cs => cs.map((x, j) => j === i ? { ...x, label: e.target.value } : x))} />
                <select className="h-8 rounded-md border border-line bg-surface-2 px-2 text-xs text-paper" value={c.template}
                  onChange={e => setComm(cs => cs.map((x, j) => j === i ? { ...x, template: e.target.value } : x))}>
                  {TPL.map(t => <option key={t}>{t}</option>)}</select>
                <select className="h-8 rounded-md border border-line bg-surface-2 px-2 text-xs" value={c.sense}
                  onChange={e => setComm(cs => cs.map((x, j) => j === i ? { ...x, sense: e.target.value as Comm["sense"] } : x))}>
                  <option>{">="}</option><option>{"<="}</option></select>
                <Input className="w-32" type="number" value={c.rhs} onChange={e => setComm(cs => cs.map((x, j) => j === i ? { ...x, rhs: Number(e.target.value) } : x))} />
                <Button variant="danger" onClick={() => setComm(comm.filter((_, j) => j !== i))}>×</Button>
              </div>
            ))}
          </div>
        </CardBody>
      </Card>

      {showConsole && <SolveConsole job={job!} elapsed={elapsed} reduced={reduced} samples={samples} />}

      {res && !res.feasible && (
        <Card><CardHeader title="Infeasible" sub="the answer, not an error: the plan cannot hold these ratios in every scenario" />
          <CardBody><Badge tone="red">{res.message}</Badge></CardBody></Card>
      )}
      {res?.feasible && (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-3">
            <div className="reveal-row rounded-xl border border-line bg-surface-1 p-4" style={{ animationDelay: "40ms" }}>
              <div className="text-[11px] uppercase tracking-wide text-paper-faint">Worst-case 27m NII</div>
              <div className="num mt-1 text-2xl font-semibold text-paper">{fmt$(res.worst_case_nii_$!)}</div>
            </div>
            <div className="reveal-row rounded-xl border border-line bg-surface-1 p-4" style={{ animationDelay: "100ms" }}>
              <div className="text-[11px] uppercase tracking-wide text-paper-faint">New assets deployed</div>
              <div className="num mt-1 text-2xl font-semibold text-paper">{fmt$(res.total_new_assets_$!)}</div>
            </div>
            <div className="reveal-row rounded-xl border border-line bg-surface-1 p-4" style={{ animationDelay: "160ms" }}>
              <div className="text-[11px] uppercase tracking-wide text-paper-faint">Binding constraints</div>
              <div className="num mt-1 text-2xl font-semibold text-paper">{bindings.length}</div>
            </div>
          </div>
          <div className="grid gap-3 xl:grid-cols-2">
            <Card><CardHeader title="Optimal allocation" sub="template × purchase month × notional" />
              <CardBody className="p-0"><DataTable rows={res.allocation as never} /></CardBody></Card>
            <Card>
              <CardHeader title="Shadow prices" sub="marginal worst-case NII per unit of constraint — the price of liquidity / the cost of the mandate" />
              <CardBody className="space-y-2">
                {bindings.map((b, i) => (
                  <div key={b.constraint} className="reveal-row" style={{ animationDelay: `${i * 70}ms` }}>
                    <div className="mb-1 flex items-baseline justify-between gap-3 text-[11px]">
                      <span className="truncate text-paper-dim">{b.constraint}</span>
                      <span className={`num shrink-0 ${b.shadow_price < 0 ? "text-down" : "text-paper"}`}>{b.shadow_price.toFixed(4)}</span>
                    </div>
                    <div className="h-2 overflow-hidden rounded-full bg-surface-3">
                      <div className={`h-full rounded-full ${b.shadow_price < 0 ? "bg-down" : "bg-brand"} transition-[width] duration-700 ease-out`}
                        style={{ width: `${b.frac * 100}%` }} />
                    </div>
                  </div>
                ))}
                {!bindings.length && <div className="text-xs text-paper-faint">no binding constraints — the plan has slack everywhere</div>}
              </CardBody>
            </Card>
          </div>
        </>
      )}
    </div>
  );
}
