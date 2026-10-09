/** EngineContext — the app's shared nervous system.
 *
 * Owns the things every surface reaches for (market, settings, scenarios,
 * the active scenario) and the single global run channel. Any run started
 * through `run()` streams its telemetry here, so the masthead heartbeat and
 * the global status read-out reflect the engine working regardless of which
 * tile (or the command palette) kicked it off.
 *
 * Engine invariants surface as behavior, not decoration: one CRN draw set
 * per run (seed is shown), scenario runs keep base OAS fixed, and the run
 * channel is single-flight (kernels saturate cores; a second run waits). */
import {
  createContext, useCallback, useContext, useEffect, useMemo, useRef, useState,
  type ReactNode,
} from "react";
import { api, type Job, type Market, type PipelineNode, type RunPlan, type Scenario, type Settings, type PricingOptions } from "./api";
import type { Sample } from "../components/Heartbeat";

export type RunLog = { t: number; msg: string };

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

type RunOpts = { optimize?: unknown; pricing?: PricingOptions; onTick?: (job: Job) => void; scenario?: string; books?: ("mbs" | "loans" | "debt" | "deposits" | "cds" | "mm")[] };

interface EngineState {
  market: Market | null;
  settings: Settings | null;
  scenarios: Record<string, Scenario>;
  active: string;
  kpis: Kpis | null;
  /** When `kpis` was computed; cleared with it when inputs change. */
  kpisAt: Date | null;
  revision: number;
  libraryReady: boolean;
  libraryHorizon: number;
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
}

const Ctx = createContext<EngineState | null>(null);
type EngineData = Pick<EngineState, "market" | "settings" | "scenarios" | "active" | "kpis" | "revision" | "libraryReady" | "libraryHorizon" | "running" | "setActive" | "setSettings" | "refreshMarket" | "refreshScenarios" | "run">;
const DataCtx = createContext<EngineData | null>(null);

export function EngineProvider({ children }: { children: ReactNode }) {
  const [market, setMarket] = useState<Market | null>(null);
  const [settings, setSettingsState] = useState<Settings | null>(null);
  const [scenarios, setScenarios] = useState<Record<string, Scenario>>({});
  const [active, setActive] = useState("base");
  const [kpis, setKpis] = useState<Kpis | null>(null);
  const [kpisAt, setKpisAt] = useState<Date | null>(null);

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
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const pending = useRef(new Map<string, Promise<Job>>());
  const refreshState = useCallback(async () => {
    const state = await api.state();
    if (state.revision !== revisionRef.current) { setKpis(null); setKpisAt(null); }
    revisionRef.current = state.revision;
    setRevision(state.revision);
    setLibraryReady(state.library_ready);
    setLibraryHorizon(state.library_horizon ?? 27);
    return state;
  }, []);

  const refreshMarket = useCallback(() => { api.market().then(setMarket).catch(() => {}); }, []);
  const refreshScenarios = useCallback(() => {
    api.scenarios().then(s => {
      setScenarios(s);
      setActive(a => (s[a] ? a : s.base ? "base" : Object.keys(s)[0] ?? a));
    }).catch(() => {});
  }, []);

  useEffect(() => {
    refreshMarket();
    refreshScenarios();
    const refresh = () => {
      void refreshState().catch(() => {});
      api.settings().then(setSettingsState).catch(() => {});
      refreshMarket(); refreshScenarios();
    };
    refresh();
    window.addEventListener("engine:inputs-changed", refresh);
    return () => window.removeEventListener("engine:inputs-changed", refresh);
  }, [refreshMarket, refreshScenarios, refreshState]);

  const setSettings = useCallback((s: Settings) => {
    void api.putSettings(s).then(() => setSettingsState(s)).catch(e => alert(String(e)));
  }, []);

  const execute = useCallback(async (kind: string, opts?: RunOpts): Promise<Job> => {
    setRunning(true); setActiveKind(kind); setStage("starting"); setPct(0);
    setElapsed(0); setSamples([]); setNodes([]); setStats({}); setPlan({}); setLog([]);
    const t0 = performance.now();
    const clock = setInterval(() => setElapsed((performance.now() - t0) / 1000), 100);
    try {
      const j = kind === "optimize" ? await api.optimize(opts?.optimize) : await api.run(kind, opts?.scenario, opts?.books, opts?.pricing);
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
      const fresh = await refreshState();
      if (done.status === "done" && done.revision !== fresh.revision) {
        done = { ...done, status: "error", detail: "Inputs changed during this run. Run again for current results." };
      }
      if (done.status === "done") {
        setStage("done"); setPct(100);
        if (kind === "kpis") { setKpis(done.result as Kpis); setKpisAt(new Date()); }
      } else {
        setStage("error");
      }
      return done;
    } catch (error) {
      setStage("error");
      return { id: "", kind, revision: revisionRef.current, status: "error", detail: String(error) };
    } finally {
      clearInterval(clock);

    }
  }, [refreshState]);

  const run = useCallback((kind: string, opts?: RunOpts): Promise<Job> => {
    const key = JSON.stringify([revisionRef.current, kind, opts?.scenario, opts?.books, opts?.optimize, opts?.pricing]);
    const existing = pending.current.get(key);
    if (existing) return existing;
    setRunning(true);
    const task = queue.current.then(() => execute(kind, opts)).finally(() => {
      pending.current.delete(key);
      if (!pending.current.size) setRunning(false);
    });
    pending.current.set(key, task);
    queue.current = task.catch(() => {});
    return task;
  }, [execute]);

  const data = useMemo<EngineData>(() => ({ market, settings, scenarios, active, kpis, revision,
    libraryReady, libraryHorizon, running, setActive, setSettings, refreshMarket, refreshScenarios, run }),
    [market, settings, scenarios, active, kpis, revision, libraryReady, libraryHorizon, running,
      setSettings, refreshMarket, refreshScenarios, run]);
  const value = useMemo<EngineState>(() => ({
    market, settings, scenarios, active, kpis, kpisAt, revision, libraryReady, libraryHorizon,
    running, activeKind, stage, pct, elapsed, samples, nodes, stats, plan, log,
    setActive, setSettings, refreshMarket, refreshScenarios, run,
  }), [market, settings, scenarios, active, kpis, kpisAt, revision, libraryReady, libraryHorizon, running, activeKind, stage, pct, elapsed, samples,
       nodes, stats, plan, log, setSettings, refreshMarket, refreshScenarios, run]);

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


/** Stable inputs/actions: elapsed-time updates do not rerender these consumers. */
export function useEngineData() {
  const c = useContext(DataCtx);
  if (!c) throw new Error("useEngineData must be used within EngineProvider");
  return c;
}
