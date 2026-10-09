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
 * Runs go through one single-flight queue (kernels saturate cores) in two
 * lanes: anything a person asks for starts before a waiting automatic refresh. */
import {
  createContext, useCallback, useContext, useEffect, useMemo, useRef, useState,
  type ReactNode,
} from "react";
import { api, type BookName, type Job, type Market, type PipelineNode, type RunPlan, type Scenario, type Settings, type PricingOptions } from "./api";
import type { Sample } from "../components/Heartbeat";
import { changedInputs, dependsOn, nodesFor, type InputNodes, type ResultKind } from "./graph";
export type { ResultKind } from "./graph";

export type RunLog = { t: number; msg: string };
export type Result<T = unknown> = { value: T; revision: number; at: Date };
/** One result node of the recalculation graph, for display. */
export type GraphNode = {
  node: string; inputs: string[]; at: Date | null;
  status: "current" | "stale" | "updating" | "failed" | "missing";
  /** inputs that changed since this node was computed; null when unknown */
  changed: string[] | null;
};

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

type RunOpts = { optimize?: unknown; pricing?: PricingOptions; onTick?: (job: Job) => void; scenario?: string; books?: BookName[];
  /** "background" for automatic refreshes: they wait behind anything a person asked for. */
  priority?: "interactive" | "background" };

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
  /** Result nodes the engine keeps fresh, with their inputs and status. */
  graph: GraphNode[];
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
type EngineData = Pick<EngineState, "market" | "settings" | "scenarios" | "active" | "revision" | "libraryReady" | "libraryHorizon" | "running" | "setActive" | "setSettings" | "refreshMarket" | "refreshScenarios" | "run" | "results" | "kpis" | "isStale" | "graph" | "pending" | "errors" | "autoRecalc" | "setAutoRecalc" | "request">;
const DataCtx = createContext<EngineData | null>(null);

function readAutoRecalc(): boolean {
  try { return localStorage.getItem(AUTO_KEY) !== "off"; } catch { return true; }
}

export function EngineProvider({ children }: { children: ReactNode }) {
  const [market, setMarket] = useState<Market | null>(null);
  const [settings, setSettingsState] = useState<Settings | null>(null);
  const [scenarios, setScenarios] = useState<Record<string, Scenario>>({});
  const [active, setActive] = useState("base");
  /** Results per graph node: kpis, nii, risk:<book>, risk:hedges, stress:<book>. */
  const [entries, setEntries] = useState<Record<string, Result>>({});
  /** Bumped when new input fingerprints arrive, so staleness re-evaluates. */
  const [fpTick, setFpTick] = useState(0);
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
  const entriesRef = useRef(entries);
  /** Input fingerprints by revision, from `/state`; bounded. */
  const fingerprints = useRef(new Map<number, InputNodes>());
  /** Runs waiting for the single-flight slot. Interactive runs start before any
   * waiting background refresh; the server ranks its queue the same way. */
  const lanes = useRef<{ interactive: (() => Promise<unknown>)[]; background: (() => Promise<unknown>)[] }>({ interactive: [], background: [] });
  const busy = useRef(false);
  const pendingRuns = useRef(new Map<string, Promise<Job>>());
  /** Downstream results to keep fresh, with the books each was requested for. */
  const wanted = useRef(new Map<ResultKind, BookName[] | undefined>(DEFAULT_WANTED.map(k => [k, undefined])));
  /** Revision at which a result last failed; a failed result is not retried until inputs change. */
  const failedAt = useRef(new Map<ResultKind, number>());
  const inflightJob = useRef<{ id: string; kind: ResultKind | null; books?: BookName[]; revision: number } | null>(null);
  const quietTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  /** True when a node computed at `rev` still answers the current inputs: either
   * nothing moved, or none of the inputs it depends on hash differently. */
  const isCurrent = useCallback((node: string, rev: number) => {
    if (rev === revisionRef.current) return true;
    const moved = changedInputs(node, fingerprints.current.get(rev), fingerprints.current.get(revisionRef.current));
    return moved !== null && moved.length === 0;
  }, []);

  const refreshState = useCallback(async () => {
    const state = await api.state();
    const changed = state.revision !== revisionRef.current;
    if (state.inputs) {
      fingerprints.current.set(state.inputs.revision, state.inputs.nodes);
      while (fingerprints.current.size > 64) fingerprints.current.delete(fingerprints.current.keys().next().value!);
      setFpTick(t => t + 1);
    }
    revisionRef.current = state.revision;
    setRevision(state.revision);
    setLibraryReady(state.library_ready);
    setLibraryHorizon(state.library_horizon ?? 27);
    const inflight = inflightJob.current;
    if (changed && inflight) {
      // stop the running job only if an input it depends on moved; otherwise its answer still holds
      const affected = !inflight.kind || nodesFor(inflight.kind, inflight.books).some(n => !isCurrent(n, inflight.revision));
      if (affected) void api.cancelJob(inflight.id).catch(() => {});
    }
    return state;
  }, [isCurrent]);

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
      const j = kind === "optimize" ? await api.optimize(opts?.optimize) : await api.run(kind, opts?.scenario, opts?.books, opts?.pricing, opts?.priority);
      const downstreamKind = RESULT_KINDS.includes(kind as ResultKind) && !opts?.scenario && !opts?.pricing && !opts?.optimize
        ? kind as ResultKind : null;
      inflightJob.current = { id: j.id, kind: downstreamKind, books: opts?.books, revision: j.revision };
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
      await refreshState();
      // A finished result is kept at the revision it ran on; the graph decides
      // whether later input changes reach it.
      const isDownstream = downstreamKind !== null;
      if (done.status === "done") {
        setStage("done"); setPct(100);
        if (isDownstream) {
          const at = new Date();
          const next = { ...entriesRef.current };
          if (kind === "risk" || kind === "stress") {
            for (const [part, value] of Object.entries((done.result ?? {}) as Record<string, unknown>)) {
              next[`${kind}:${part}`] = { value, revision: done.revision, at };
            }
          } else {
            next[kind] = { value: done.result, revision: done.revision, at };
          }
          entriesRef.current = next;
          setEntries(next);
          failedAt.current.delete(kind as ResultKind);
          setErrors(prev => ({ ...prev, [kind]: undefined }));
        }
      } else {
        setStage("error");
        const cancelledForNewInputs = isDownstream && done.revision !== revisionRef.current;
        if (isDownstream && !cancelledForNewInputs) {
          failedAt.current.set(kind as ResultKind, done.revision);
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

  const pump = useCallback(() => {
    if (busy.current) return;
    const next = lanes.current.interactive.shift() ?? lanes.current.background.shift();
    if (!next) return;
    busy.current = true;
    void next().catch(() => {}).finally(() => { busy.current = false; pump(); });
  }, []);

  const enqueue = useCallback((kind: string, opts?: RunOpts): Promise<Job> => {
    const isDownstream = RESULT_KINDS.includes(kind as ResultKind) && !opts?.scenario && !opts?.pricing && !opts?.optimize;
    const key = JSON.stringify([revisionRef.current, kind, opts?.scenario, opts?.books, opts?.optimize, opts?.pricing]);
    const existing = pendingRuns.current.get(key);
    if (existing) return existing;
    if (isDownstream) setPending(p => (p.includes(kind as ResultKind) ? p : [...p, kind as ResultKind]));
    setRunning(true);
    const task = new Promise<Job>((resolve, reject) => {
      lanes.current[opts?.priority ?? "interactive"].push(() => execute(kind, opts).then(resolve, reject));
      pump();
    }).finally(() => {
      pendingRuns.current.delete(key);
      if (isDownstream) setPending(p => p.filter(k => k !== kind));
      if (!pendingRuns.current.size) setRunning(false);
    });
    pendingRuns.current.set(key, task);
    return task;
  }, [execute, pump]);

  /** Run on request; a downstream result run this way is then kept fresh. */
  const run = useCallback((kind: string, opts?: RunOpts): Promise<Job> => {
    const isDownstream = RESULT_KINDS.includes(kind as ResultKind) && !opts?.scenario && !opts?.pricing && !opts?.optimize;
    if (isDownstream) wanted.current.set(kind as ResultKind, opts?.books);
    return enqueue(kind, opts);
  }, [enqueue]);

  /** Recompute only the result nodes whose inputs moved, in graph order: per-book
   * risk and stress for the books that changed, whole runs for KPIs and NII. A
   * result that failed is retried only once its inputs change again. */
  const refreshStale = useCallback(() => {
    for (const [kind, books] of wanted.current) {
      const stale = nodesFor(kind, books).filter(n => {
        const e = entriesRef.current[n];
        return !e || !isCurrent(n, e.revision);
      });
      if (!stale.length) continue;
      const failed = failedAt.current.get(kind);
      if (failed !== undefined && stale.every(n => isCurrent(n, failed))) continue;
      if (kind === "risk") {
        // the hedge book is valued only in a run over every book
        const scope = stale.includes("risk:hedges") ? books : stale.map(n => n.split(":")[1] as BookName);
        void enqueue("risk", { books: scope, priority: "background" });
      } else if (kind === "stress") {
        void enqueue("stress", { books: stale.map(n => n.split(":")[1] as BookName), priority: "background" });
      } else {
        void enqueue(kind, { priority: "background" });
      }
    }
  }, [enqueue, isCurrent]);

  const scheduleRefresh = useCallback((ms: number) => {
    if (quietTimer.current) clearTimeout(quietTimer.current);
    quietTimer.current = setTimeout(() => {
      quietTimer.current = null;
      if (autoRef.current) refreshStale();
    }, ms);
  }, [refreshStale]);

  const request = useCallback((kind: ResultKind, books?: BookName[]) => {
    wanted.current.set(kind, books);
    void enqueue(kind, { books });
  }, [enqueue]);

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

  /** Per-kind view for panels: risk and stress assemble their per-book nodes. */
  const results = useMemo(() => {
    const view: Partial<Record<ResultKind, Result>> = {};
    for (const kind of ["kpis", "nii"] as const) if (entries[kind]) view[kind] = entries[kind];
    for (const kind of ["risk", "stress"] as const) {
      const parts = Object.entries(entries).filter(([k]) => k.startsWith(`${kind}:`));
      if (!parts.length) continue;
      view[kind] = {
        value: Object.fromEntries(parts.map(([k, e]) => [k.slice(kind.length + 1), e.value])),
        revision: Math.min(...parts.map(([, e]) => e.revision)),
        at: new Date(Math.max(...parts.map(([, e]) => e.at.getTime()))),
      };
    }
    return view;
  }, [entries]);

  const nodesOf = useCallback((kind: ResultKind) => wanted.current.has(kind)
    ? nodesFor(kind, wanted.current.get(kind))
    : Object.keys(entries).filter(k => k === kind || k.startsWith(`${kind}:`)), [entries]);

  const isStale = useCallback((kind: ResultKind) => {
    const nodes = nodesOf(kind);
    return !nodes.length || nodes.some(n => !entries[n] || !isCurrent(n, entries[n].revision));
    // revision and fpTick change what isCurrent sees
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entries, nodesOf, isCurrent, revision, fpTick]);

  const graph = useMemo<GraphNode[]>(() => {
    const now = fingerprints.current.get(revision);
    return [...wanted.current].flatMap(([kind, books]) => nodesFor(kind, books).map(node => {
      const e = entries[node];
      const current = !!e && isCurrent(node, e.revision);
      const status: GraphNode["status"] = pending.includes(kind) && !current ? "updating"
        : errors[kind] && !current ? "failed" : !e ? "missing" : current ? "current" : "stale";
      return { node, inputs: dependsOn(node), at: e?.at ?? null, status,
        changed: e ? changedInputs(node, fingerprints.current.get(e.revision), now) : null };
    }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entries, pending, errors, revision, fpTick, isCurrent]);

  const kpis = (results.kpis?.value as Kpis | undefined) ?? null;
  const kpisAt = results.kpis?.at ?? null;

  const data = useMemo<EngineData>(() => ({ market, settings, scenarios, active, revision,
    libraryReady, libraryHorizon, running, setActive, setSettings, refreshMarket, refreshScenarios, run,
    results, kpis, isStale, graph, pending, errors, autoRecalc, setAutoRecalc, request }),
    [market, settings, scenarios, active, revision, libraryReady, libraryHorizon, running,
      setSettings, refreshMarket, refreshScenarios, run, results, kpis, isStale, graph, pending, errors, autoRecalc, setAutoRecalc, request]);
  const value = useMemo<EngineState>(() => ({
    market, settings, scenarios, active, revision, libraryReady, libraryHorizon,
    results, kpis, kpisAt, isStale, graph, pending, errors, autoRecalc, setAutoRecalc,
    running, activeKind, stage, pct, elapsed, samples, nodes, stats, plan, log,
    setActive, setSettings, refreshMarket, refreshScenarios, run, request,
  }), [market, settings, scenarios, active, revision, libraryReady, libraryHorizon, results, kpis, kpisAt, isStale, graph, pending, errors, autoRecalc, setAutoRecalc,
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
