import { Trash2 } from "lucide-react";
import { useEngineData } from "../lib/engine";
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
    <div className={hero ? "rounded-lg border border-brand/30 bg-brand/5 px-4 py-3" : "px-1 py-1"}>
      <div className="eyebrow">{label}</div>
      <div className={`num leading-tight text-paper ${hero ? "text-3xl font-semibold text-brand" : "text-lg font-medium"}`}
        title={full(value)}>{compact(v)}</div>
      {sub && <div className="num text-2xs text-paper-faint">{sub}</div>}
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
    <div className="space-y-3 rounded-md border border-line-strong bg-surface-1 shadow-inset-top p-4">
      <div className="flex items-center gap-3">
        <span className={`inline-block h-2 w-2 rounded-full ${err ? "bg-danger" : job.status === "done" ? "bg-up" : "bg-brand"}`} />
        <div className="text-md font-medium text-paper">
          {err ? "Solve failed" : job.status === "done" ? "Solve complete" : "Solving"}
          <span className="ml-2 text-sm font-normal text-paper-faint">{p.stage ?? job.status}</span>
        </div>
        <div className="num ml-auto text-sm text-paper-faint">{elapsed.toFixed(1)}s</div>
      </div>

      <div className="relative h-1.5 overflow-hidden rounded-sm bg-surface-3">
        <div className="absolute inset-y-0 left-0 rounded-sm bg-brand transition-[width] duration-base" style={{ width: `${pct}%` }} />
      </div>

      {err && <div className="text-xs text-danger">{job.detail}</div>}

      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Counter hero reduced={reduced} label="Path-evaluations" value={stats.path_evaluations ?? 0} sub="Calculations executed" />
        <Counter reduced={reduced} label="Revaluations" value={stats.revaluations ?? 0} sub="Full repricings" />
        <Counter reduced={reduced} label="Reductions" value={stats.reductions ?? 0} sub="Path → mean collapses" />
        <Counter reduced={reduced} label="Unit columns" value={stats.unit_columns ?? 0} sub="Priced into the LP" />
      </div>

      <Heartbeat samples={samples} running={running} reduced={reduced} />

      <div className="grid grid-cols-2 gap-x-4 gap-y-1 border-t border-line pt-2 text-xs sm:grid-cols-4">
        <div className="text-paper-faint">Records in scope <span className="num text-paper">{full(plan.in_scope ?? plan.records ?? 0)}</span></div>
        <div className="text-paper-faint">Scenario markets <span className="num text-paper">{plan.scenario_markets ?? 1}</span></div>
        <div className="text-paper-faint">MC paths <span className="num text-paper">{plan.monte_carlo_paths ?? 0}</span></div>
        <div className="text-paper-faint">Scenario paths <span className="num text-paper">{full(stats.scenario_paths ?? 0)}</span></div>
      </div>

      <div ref={logRef} className="max-h-32 overflow-auto rounded-md border border-line bg-surface-base p-2 font-mono text-2xs leading-relaxed">
        {(p.log ?? []).map((l, i) => (
          <div key={i} className={`flex gap-2 ${!reduced ? "log-in" : ""}`}>
            <span className="num shrink-0 text-paper-faint">{l.t.toFixed(2)}s</span>
            <span className="text-paper-dim">{l.msg}</span>
          </div>
        ))}
        {!(p.log ?? []).length && <div className="text-paper-faint">Waiting for first telemetry frame…</div>}
      </div>
    </div>
  );
}

export default function OptimizerPage() {
  const engine = useEngineData();
  const reduced = useReducedMotion();
  const [floors, setFloors] = useState({ lcr_min: 1.2, nsfr_min: 1.05, cet1_min: 0.10, eve_limit: 0.15, max_total_assets: 3e10, cash_budget: 0 });
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
      const done = await engine.run("optimize", {
        optimize: { ...floors, scenarios: picked, commercial: comm },
        onTick: (s) => {
        setJob(s);
        const pe = s.progress?.stats?.path_evaluations ?? 0;
        const t = s.progress?.elapsed_s ?? 0;
        setSamples(prev => (prev.length && prev[prev.length - 1].t === t ? prev : [...prev, { t, pe }].slice(-240)));
      }});
      setJob(done);
      if (done.status === "done") setRes(done.result as Result);
    } catch (e) {
      setJob(j => (j ? { ...j, status: "error", detail: String(e) } : j));
    } finally { setBusy(false); }
  };

  const renderFloor = ({ k, label, step }: { k: keyof typeof floors; label: string; step?: number }) => (
    <div><div className="mb-1 flex items-center text-2xs text-paper-faint">{label}
        <InfoPop width="15rem">{k === "lcr_min" ? "Liquidity coverage floor, held in base AND every selected scenario. If it binds, its shadow price is the worst-case NII cost of one more unit of LCR." : k === "nsfr_min" ? "Stable funding floor — ASF/RSF with deck maturities driving the buckets." : k === "cet1_min" ? "CET1 ratio floor at the configured horizon, NII-retention linearization (no AOCI leg)." : k === "eve_limit" ? "Two-sided |ΔEVE @ +200bp| cap as a fraction of EVE. 0.15 is the IRRBB outlier line." : k === "cash_budget" ? "Additional committed funding outside the base book, available throughout the horizon. With zero cash budget, new assets require matching funding throughout the horizon." : "Cap on total new asset notional the optimizer may deploy."}</InfoPop>
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
        <CardHeader title="Robust optimization" sub="Maximin worst-case NII s.t. ratio floors holding in base + every selected scenario; commercial plan as linear rows"
          right={<Button disabled={busy} onClick={run}>{busy ? "Solving…" : "Optimize"}</Button>} />
        <CardBody className="space-y-3">
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
            {renderFloor({ k: "lcr_min", label: "LCR floor",  })}{renderFloor({ k: "nsfr_min", label: "NSFR floor",  })}
            {renderFloor({ k: "cet1_min", label: "CET1 @ horizon floor", step: 0.005,  })}{renderFloor({ k: "eve_limit", label: "|ΔEVE+200| limit (× EVE)", step: 0.01,  })}
            {renderFloor({ k: "max_total_assets", label: "Max new assets $", step: 1e9 })}
            {renderFloor({ k: "cash_budget", label: "Committed funding budget $", step: 1e6 })}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-2xs text-paper-faint">Robust across:</span>
            <Badge tone="neutral">Base</Badge>
            {scens.map(s => (
              <button key={s} onClick={() => setPicked(p => p.includes(s) ? p.filter(x => x !== s) : [...p, s])}
                className={`rounded-sm px-2 py-0.5 text-2xs font-semibold ${picked.includes(s) ? "bg-brand/15 text-brand" : "bg-surface-3 text-paper-dim"}`}>{s}</button>
            ))}
            {!scens.length && <span className="text-2xs text-paper-faint">Define scenarios in Market & Scenarios</span>}
          </div>
          <div className="space-y-2">
            <div className="flex items-center gap-2 text-2xs text-paper-faint">commercial plan
              <Button variant="secondary" onClick={() => setComm([...comm, { label: `row_${comm.length}`, template: "agency_mbs", sense: ">=", rhs: 1e9 }])}>+ row</Button></div>
            {comm.map((c, i) => (
              <div key={i} className="flex items-center gap-2">
                <Input className="w-36" value={c.label} onChange={e => setComm(cs => cs.map((x, j) => j === i ? { ...x, label: e.target.value } : x))} />
                <select className="h-control rounded-sm border border-line-strong bg-surface-base px-2 text-sm text-paper" value={c.template}
                  onChange={e => setComm(cs => cs.map((x, j) => j === i ? { ...x, template: e.target.value } : x))}>
                  {TPL.map(t => <option key={t}>{t}</option>)}</select>
                <select className="h-control rounded-sm border border-line-strong bg-surface-base px-2 text-sm" value={c.sense}
                  onChange={e => setComm(cs => cs.map((x, j) => j === i ? { ...x, sense: e.target.value as Comm["sense"] } : x))}>
                  <option>{">="}</option><option>{"<="}</option></select>
                <Input className="w-32" type="number" value={c.rhs} onChange={e => setComm(cs => cs.map((x, j) => j === i ? { ...x, rhs: Number(e.target.value) } : x))} />
                <Button variant="ghost" size="sm" aria-label="Remove row" onClick={() => setComm(comm.filter((_, j) => j !== i))}><Trash2 aria-hidden className="h-3.5 w-3.5" strokeWidth={1.5} /></Button>
              </div>
            ))}
          </div>
        </CardBody>
      </Card>

      {showConsole && <SolveConsole job={job!} elapsed={elapsed} reduced={reduced} samples={samples} />}

      {res && !res.feasible && (
        <Card><CardHeader title="Infeasible" sub="The answer, not an error: the plan cannot hold these ratios in every scenario" />
          <CardBody><Badge tone="danger">{res.message}</Badge></CardBody></Card>
      )}
      {res?.feasible && (
        <>
          <p className="text-sm text-paper-faint">Linear coefficient replay passed. Dynamic stress has not run. Copy this allocation into a saved-book Balance-sheet Stress request and supply explicit template mappings to check daily cash, accounting and limits.</p>
          <details><summary className="cursor-pointer text-sm">Candidate allocation for dynamic replay</summary><pre className="overflow-auto p-2 text-sm">{JSON.stringify(res.allocation, null, 2)}</pre></details>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-3">
            <div className="reveal-row rounded-md border border-line-strong bg-surface-1 shadow-inset-top p-4">
              <div className="eyebrow">Worst-case total NII</div>
              <div className="num mt-1 text-2xl font-semibold text-paper">{fmt$(res.worst_case_nii_$!)}</div>
            </div>
            <div className="reveal-row rounded-md border border-line-strong bg-surface-1 shadow-inset-top p-4">
              <div className="eyebrow">New assets deployed</div>
              <div className="num mt-1 text-2xl font-semibold text-paper">{fmt$(res.total_new_assets_$!)}</div>
            </div>
            <div className="reveal-row rounded-md border border-line-strong bg-surface-1 shadow-inset-top p-4">
              <div className="eyebrow">Binding constraints</div>
              <div className="num mt-1 text-2xl font-semibold text-paper">{bindings.length}</div>
            </div>
          </div>
          <div className="grid gap-3 xl:grid-cols-2">
            <Card><CardHeader title="Optimal allocation" sub="Template × purchase month × notional" />
              <CardBody className="p-0"><DataTable rows={res.allocation as never} /></CardBody></Card>
            <Card>
              <CardHeader title="Shadow prices" sub="Marginal worst-case NII per unit of constraint — the price of liquidity / the cost of the mandate" />
              <CardBody className="space-y-2">
                {bindings.map((b, i) => (
                  <div key={b.constraint} className="reveal-row">
                    <div className="mb-1 flex items-baseline justify-between gap-3 text-xs">
                      <span className="truncate text-paper-dim">{b.constraint}</span>
                      <span className={`num shrink-0 ${b.shadow_price < 0 ? "text-down" : "text-paper"}`}>{b.shadow_price.toFixed(4)}</span>
                    </div>
                    <div className="h-2 overflow-hidden rounded-sm bg-surface-3">
                      <div className={`h-full rounded-sm ${b.shadow_price < 0 ? "bg-down" : "bg-brand"} transition-[width] duration-base`}
                        style={{ width: `${b.frac * 100}%` }} />
                    </div>
                  </div>
                ))}
                {!bindings.length && <div className="text-sm text-paper-faint">No binding constraints — the plan has slack everywhere</div>}
              </CardBody>
            </Card>
          </div>
        </>
      )}
    </div>
  );
}
