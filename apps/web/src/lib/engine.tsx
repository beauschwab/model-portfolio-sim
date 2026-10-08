/** EngineContext — the app's shared nervous system.
 *
 * Owns the inputs every surface reads (market, settings, scenarios, the active
 * scenario, saved books and assumptions) and the downstream results computed
 * from them: KPIs, risk, NII and the 9Q stress. Each result is kept with the
 * input revision it was computed at. Any input write bumps the revision, which
 * marks every result stale and — with auto-recalculation on — queues the stale
 * results to recompute after a short quiet period. Panels read results here,
 * so an edit made in one grid updates the headline on another screen without
 * navigation.
 *
 * Runs go through one single-flight queue (kernels saturate cores), and a
 * finished job is only kept if its inputs did not change while it ran. */
import {
  createContext, useCallback, useContext, useEffect, useMemo, useRef, useState,
  type ReactNode,
} from "react";
import { api, type BookName, type Job, type Market, type PipelineNode, type RunPlan, type Scenario, type Settings, type PricingOptions } from "./api";
import type { Sample } from "../components/Heartbeat";

export type RunLog = { t: number; msg: string };
export type ResultKind = "kpis" | "risk" | "nii" | "stress";
export type Result<T = unknown> = { value: T; revision: number; at: Date };

export type Kpis = {
  eve: {
    eve_$: number; duration_gap_y: number; dur_assets_y: number; dur_liab_y: number;
    dv01_net_$: number; irrbb_outlier: boolean; irrbb_worst_pct_eve: number;
    sensitivity: { shock_bp: number; d_eve_pct_eve: number; d_eve_$: number; method: string }[];
  };
  lcr: { lcr_pct: number; hqla_$: number; net_outflows_$: number };
  nsfr: { nsfr_pct: number; asf_$: number; rsf_$: number };
  capital: {
    rwa_total_$: number; rwa_density_pct: number;
    cet1_path: { quarter: number; cet1_ratio_pct: number; cet1_$: number; drivers: string }[]; note: string;
  };
};

type RunOpts = { optimize?: unknown; pricing?: PricingOptions; onTick?: (job: Job) => void; scenario?: string; books?: BookName[] };

/** How long inputs must be quiet before stale results recompute. Slider drags
 * and multi-field edits therefore cost one run, not one per keystroke. */
const RECALC_QUIET_MS = 700;
const AUTO_KEY = "engine.autoRecalc";

/** Results that feed the headline screens. Requested by default so the Home
 * sheet is populated on load; stress is requested only from the Stress panel. */
const DEFAULT_WANTED: ResultKind[] = ["kpis", "risk", "nii"];
const RESULT_KINDS: ResultKind[] = ["kpis", "risk", "nii", "stress"];

interface EngineState {
  market: Market | null;
  settings: Settings | null;
  scenarios: Record<string, Scenario>;
  active: string;
  revision: number;
  libraryReady: boolean;
  libraryHorizon: number;
  // downstream results, each stamped with the revision it was computed at
  results: Partial<Record<ResultKind, Result>>;
  kpis: Kpis | null;
  kpisAt: Date | null;
  isStale: (kind: ResultKind) => boolean;
  pending: ResultKind[];
  /** Last failure for each downstream result; cleared when it succeeds. */
  errors: Partial<Record<ResultKind, string>>;
  autoRecalc: boolean;
  setAutoRecalc: (on: boolean) => void;
  // live run telemetry
  running: boolean;
  activeKind: string | null;
  stage: string;
  pct: number;
  elapsed: number;
  samples: Sample[];
  nodes: PipelineNode[];
  stats: Record<string, number>;
  plan: Partial<RunPlan>;
  log: RunLog[];
  // actions
  setActive: (name: string) => void;
  setSettings: (s: Settings) => void;
  refreshMarket: () => void;
  refreshScenarios: () => void;
  run: (kind: string, opts?: RunOpts) => Promise<Job>;
  /** Ask for a downstream result and keep it fresh from now on. */
  request: (kind: ResultKind, books?: BookName[]) => void;
}

const Ctx = createContext<EngineState | null>(null);
type EngineData = Pick<EngineState, "market" | "settings" | "scenarios" | "active" | "revision" | "libraryReady" | "libraryHorizon" | "running" | "setActive" | "setSettings" | "refreshMarket" | "refreshScenarios" | "run" | "results" | "kpis" | "isStale" | "pending" | "errors" | "autoRecalc" | "setAutoRecalc" | "request">;
const DataCtx = createContext<EngineData | null>(null);

function readAutoRecalc(): boolean {
  try { return localStorage.getItem(AUTO_KEY) !== "off"; } catch { return true; }
}

export function EngineProvider({ children }: { children: ReactNode }) {
  const [market, setMarket] = useState<Market | null>(null);
  const [settings, setSettingsState] = useState<Settings | null>(null);
  const [scenarios, setScenarios] = useState<Record<string, Scenario>>({});
  const [active, setActive] = useState("base");
  const [results, setResults] = useState<Partial<Record<ResultKind, Result>>>({});
  const [pending, setPending] = useState<ResultKind[]>([]);
  const [errors, setErrors] = useState<Partial<Record<ResultKind, string>>>({});
  const [autoRecalc, setAutoRecalcState] = useState<boolean>(readAutoRecalc);

  const [running, setRunning] = useState(false);
  const [activeKind, setActiveKind] = useState<string | null>(null);
  const [stage, setStage] = useState("");
  const [pct, setPct] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const [samples, setSamples] = useState<Sample[]>([]);
  const [nodes, setNodes] = useState<PipelineNode[]>([]);
  const [stats, setStats] = useState<Record<string, number>>({});
  const [plan, setPlan] = useState<Partial<RunPlan>>({});
  const [log, setLog] = useState<RunLog[]>([]);
  const [revision, setRevision] = useState(0);
  const [libraryReady, setLibraryReady] = useState(false);
  const [libraryHorizon, setLibraryHorizon] = useState(27);

  const revisionRef = useRef(0);
  const autoRef = useRef(autoRecalc);
  const resultsRef = useRef(results);
  resultsRef.current = results;
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const pendingRuns = useRef(new Map<string, Promise<Job>>());
  /** Downstream results to keep fresh, with the books each was requested for. */
  const wanted = useRef(new Map<ResultKind, BookName[] | undefined>(DEFAULT_WANTED.map(k => [k, undefined])));
  /** Revision at which a result last failed; a failed result is not retried until inputs change. */
  const failedAt = useRef(new Map<ResultKind, number>());
  const inflightJob = useRef<string | null>(null);
  const quietTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refreshState = useCallback(async () => {
    const state = await api.state();
    const changed = state.revision !== revisionRef.current;
    revisionRef.current = state.revision;
    setRevision(state.revision);
    setLibraryReady(state.library_ready);
    setLibraryHorizon(state.library_horizon ?? 27);
    if (changed && inflightJob.current) {
      // the running job answers an older revision; stop it rather than wait
      void api.cancelJob(inflightJob.current).catch(() => {});
    }
    return state;
  }, []);

  const refreshMarket = useCallback(() => { api.market().then(setMarket).catch(() => {}); }, []);
  const refreshScenarios = useCallback(() => {
    api.scenarios().then(s => {
      setScenarios(s);
      setActive(a => (s[a] ? a : s.base ? "base" : Object.keys(s)[0] ?? a));
    }).catch(() => {});
  }, []);

  const execute = useCallback(async (kind: string, opts?: RunOpts): Promise<Job> => {
    setRunning(true); setActiveKind(kind); setStage("starting"); setPct(0);
    setElapsed(0); setSamples([]); setNodes([]); setStats({}); setPlan({}); setLog([]);
    const t0 = performance.now();
    const clock = setInterval(() => setElapsed((performance.now() - t0) / 1000), 100);
    try {
      const j = kind === "optimize" ? await api.optimize(opts?.optimize) : await api.run(kind, opts?.scenario, opts?.books, opts?.pricing);
      inflightJob.current = j.id;
      let done = await pollWithTelemetry(j.id, job => {
        opts?.onTick?.(job);
        const p = job.progress ?? {};
        if (p.stage) setStage(p.stage);
        if (typeof p.pct === "number") setPct(p.pct);
        if (typeof p.elapsed_s === "number") setElapsed(p.elapsed_s);
        if (p.nodes) setNodes(p.nodes);
        if (p.stats) setStats(p.stats);
        if (p.plan) setPlan(p.plan);
        if (p.log) setLog(p.log);
        const pe = p.stats?.path_evaluations;
        if (typeof pe === "number") {
          setSamples(s => {
            const t = typeof p.elapsed_s === "number" ? p.elapsed_s : (performance.now() - t0) / 1000;
            const next = [...s, { t, pe }];
            return next.length > 240 ? next.slice(next.length - 240) : next;
          });
        }
      }, kind === "whatif" ? 75 : 300);
      inflightJob.current = null;
      const fresh = await refreshState();
      if (done.status === "done" && done.revision !== fresh.revision) {
        // inputs changed while this ran: its answer is stale, and the debounced refresh will rerun it
        done = { ...done, status: "error", detail: "Inputs changed during this run." };
      } else if (done.status === "error" && done.revision === fresh.revision) {
        if (RESULT_KINDS.includes(kind as ResultKind)) failedAt.current.set(kind as ResultKind, fresh.revision);
      }
      const isDownstream = RESULT_KINDS.includes(kind as ResultKind) && !opts?.scenario && !opts?.pricing && !opts?.optimize;
      if (done.status === "done") {
        setStage("done"); setPct(100);
        if (isDownstream) {
          setResults(prev => ({ ...prev, [kind]: { value: done.result, revision: done.revision, at: new Date() } }));
          setErrors(prev => ({ ...prev, [kind]: undefined }));
        }
      } else {
        setStage("error");
        if (isDownstream && done.revision === fresh.revision) {
          setErrors(prev => ({ ...prev, [kind]: done.detail ?? "The engine returned no result." }));
        }
      }
      return done;
    } catch (error) {
      inflightJob.current = null;
      setStage("error");
      return { id: "", kind, revision: revisionRef.current, status: "error", detail: String(error) };
    } finally {
      clearInterval(clock);
    }
  }, [refreshState]);

  const run = useCallback((kind: string, opts?: RunOpts): Promise<Job> => {
    const isDownstream = RESULT_KINDS.includes(kind as ResultKind) && !opts?.scenario && !opts?.pricing && !opts?.optimize;
    if (isDownstream) wanted.current.set(kind as ResultKind, opts?.books);
    const key = JSON.stringify([revisionRef.current, kind, opts?.scenario, opts?.books, opts?.optimize, opts?.pricing]);
    const existing = pendingRuns.current.get(key);
    if (existing) return existing;
    if (isDownstream) setPending(p => (p.includes(kind as ResultKind) ? p : [...p, kind as ResultKind]));
    setRunning(true);
    const task = queue.current.then(() => execute(kind, opts)).finally(() => {
      pendingRuns.current.delete(key);
      if (isDownstream) setPending(p => p.filter(k => k !== kind));
      if (!pendingRuns.current.size) setRunning(false);
    });
    pendingRuns.current.set(key, task);
    queue.current = task.catch(() => {});
    return task;
  }, [execute]);

  /** Queue every wanted result that is missing or computed at an older revision. */
  const refreshStale = useCallback(() => {
    for (const [kind, books] of wanted.current) {
      const current = resultsRef.current[kind];
      if (current && current.revision === revisionRef.current) continue;
      if (failedAt.current.get(kind) === revisionRef.current) continue;
      void run(kind, { books });
    }
  }, [run]);

  const scheduleRefresh = useCallback((ms: number) => {
    if (quietTimer.current) clearTimeout(quietTimer.current);
    quietTimer.current = setTimeout(() => {
      quietTimer.current = null;
      if (autoRef.current) refreshStale();
    }, ms);
  }, [refreshStale]);

  const request = useCallback((kind: ResultKind, books?: BookName[]) => {
    wanted.current.set(kind, books);
    void run(kind, { books });
  }, [run]);

  const setAutoRecalc = useCallback((on: boolean) => {
    autoRef.current = on;
    setAutoRecalcState(on);
    try { localStorage.setItem(AUTO_KEY, on ? "on" : "off"); } catch { /* per-viewer convenience only */ }
    if (on) scheduleRefresh(0);
  }, [scheduleRefresh]);

  const setSettings = useCallback((s: Settings) => {
    void api.putSettings(s).then(() => setSettingsState(s)).catch(e => alert(String(e)));
  }, []);

  useEffect(() => {
    refreshMarket();
    refreshScenarios();
    const onInputs = () => {
      void refreshState().then(() => {
        api.settings().then(setSettingsState).catch(() => {});
        refreshMarket(); refreshScenarios();
        scheduleRefresh(RECALC_QUIET_MS);
      }).catch(() => {});
    };
    void refreshState().then(() => {
      api.settings().then(setSettingsState).catch(() => {});
      scheduleRefresh(0);
    }).catch(() => {});
    window.addEventListener("engine:inputs-changed", onInputs);
    return () => {
      window.removeEventListener("engine:inputs-changed", onInputs);
      if (quietTimer.current) clearTimeout(quietTimer.current);
    };
  }, [refreshMarket, refreshScenarios, refreshState, scheduleRefresh]);

  const isStale = useCallback((kind: ResultKind) => {
    const r = results[kind];
    return !r || r.revision !== revision;
  }, [results, revision]);

  const kpis = (results.kpis?.value as Kpis | undefined) ?? null;
  const kpisAt = results.kpis?.at ?? null;

  const data = useMemo<EngineData>(() => ({ market, settings, scenarios, active, revision,
    libraryReady, libraryHorizon, running, setActive, setSettings, refreshMarket, refreshScenarios, run,
    results, kpis, isStale, pending, errors, autoRecalc, setAutoRecalc, request }),
    [market, settings, scenarios, active, revision, libraryReady, libraryHorizon, running,
      setSettings, refreshMarket, refreshScenarios, run, results, kpis, isStale, pending, errors, autoRecalc, setAutoRecalc, request]);
  const value = useMemo<EngineState>(() => ({
    market, settings, scenarios, active, revision, libraryReady, libraryHorizon,
    results, kpis, kpisAt, isStale, pending, errors, autoRecalc, setAutoRecalc,
    running, activeKind, stage, pct, elapsed, samples, nodes, stats, plan, log,
    setActive, setSettings, refreshMarket, refreshScenarios, run, request,
  }), [market, settings, scenarios, active, revision, libraryReady, libraryHorizon, results, kpis, kpisAt, isStale, pending, errors, autoRecalc, setAutoRecalc,
       running, activeKind, stage, pct, elapsed, samples, nodes, stats, plan, log, setSettings, refreshMarket, refreshScenarios, run, request]);

  return <DataCtx.Provider value={data}><Ctx.Provider value={value}>{children}</Ctx.Provider></DataCtx.Provider>;
}

/** Poll a job, surfacing each progress snapshot. Mirrors api.awaitJob but
 * exposes the RunProgress directly so callers can stream telemetry. */
async function pollWithTelemetry(
  id: string,
  onTick: (job: Job) => void,
  ms = 300,
): Promise<Job> {
  for (;;) {
    const s = await api.job(id);
    onTick(s);
    if (s.status === "error") return s;
    if (s.status === "done") { s.result = await api.jobResult(id); return s; }
    await new Promise(r => setTimeout(r, ms));
  }
}

export function useEngine() {
  const c = useContext(Ctx);
  if (!c) throw new Error("useEngine must be used within EngineProvider");
  return c;
}


/** Stable inputs/results/actions: elapsed-time updates do not rerender these consumers. */
export function useEngineData() {
  const c = useContext(DataCtx);
  if (!c) throw new Error("useEngineData must be used within EngineProvider");
  return c;
}
