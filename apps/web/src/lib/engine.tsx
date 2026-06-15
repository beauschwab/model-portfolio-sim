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
import { api, type Job, type Market, type Scenario, type Settings } from "./api";
import type { Sample } from "../components/Heartbeat";

export type Kpis = {
  eve: {
    eve_$: number; duration_gap_y: number; dur_assets_y: number; dur_liab_y: number;
    dv01_net_$: number; irrbb_outlier: boolean; irrbb_worst_pct_eve: number;
    sensitivity: Record<string, number | string>[];
  };
  lcr: { lcr_pct: number; hqla_$: number };
  nsfr: { nsfr_pct: number; asf_$: number };
  capital: {
    rwa_total_$: number; rwa_density_pct: number;
    cet1_path: { quarter: number; cet1_ratio_pct: number }[]; note: string;
  };
};

type RunOpts = { scenario?: string; books?: ("mbs" | "loans" | "debt" | "deposits" | "cds" | "mm")[] };

interface EngineState {
  market: Market | null;
  settings: Settings | null;
  scenarios: Record<string, Scenario>;
  active: string;
  kpis: Kpis | null;
  // live run telemetry
  running: boolean;
  activeKind: string | null;
  stage: string;
  pct: number;
  elapsed: number;
  samples: Sample[];
  // actions
  setActive: (name: string) => void;
  setSettings: (s: Settings) => void;
  refreshMarket: () => void;
  refreshScenarios: () => void;
  run: (kind: string, opts?: RunOpts) => Promise<Job | null>;
}

const Ctx = createContext<EngineState | null>(null);

export function EngineProvider({ children }: { children: ReactNode }) {
  const [market, setMarket] = useState<Market | null>(null);
  const [settings, setSettingsState] = useState<Settings | null>(null);
  const [scenarios, setScenarios] = useState<Record<string, Scenario>>({});
  const [active, setActive] = useState("base");
  const [kpis, setKpis] = useState<Kpis | null>(null);

  const [running, setRunning] = useState(false);
  const [activeKind, setActiveKind] = useState<string | null>(null);
  const [stage, setStage] = useState("");
  const [pct, setPct] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const [samples, setSamples] = useState<Sample[]>([]);
  const inFlight = useRef(false);

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
    api.settings().then(setSettingsState).catch(() => {});
  }, [refreshMarket, refreshScenarios]);

  const setSettings = useCallback((s: Settings) => {
    setSettingsState(s);
    void api.putSettings(s).catch(() => {});
  }, []);

  const run = useCallback(async (kind: string, opts?: RunOpts): Promise<Job | null> => {
    if (inFlight.current) return null; // single-flight: kernels saturate cores
    inFlight.current = true;
    setRunning(true); setActiveKind(kind); setStage("starting"); setPct(0);
    setElapsed(0); setSamples([]);
    const t0 = performance.now();
    const clock = setInterval(() => setElapsed((performance.now() - t0) / 1000), 100);
    try {
      const j = await api.run(kind, opts?.scenario, opts?.books);
      const done = await pollWithTelemetry(j.id, p => {
        if (p.stage) setStage(p.stage);
        if (typeof p.pct === "number") setPct(p.pct);
        if (typeof p.elapsed_s === "number") setElapsed(p.elapsed_s);
        const pe = p.stats?.path_evaluations;
        if (typeof pe === "number") {
          setSamples(s => {
            const t = typeof p.elapsed_s === "number" ? p.elapsed_s : (performance.now() - t0) / 1000;
            const next = [...s, { t, pe }];
            return next.length > 240 ? next.slice(next.length - 240) : next;
          });
        }
      });
      if (done.status === "done") {
        setStage("done"); setPct(100);
        if (kind === "kpis") setKpis(done.result as Kpis);
      } else {
        setStage("error");
      }
      return done;
    } catch {
      setStage("error");
      return null;
    } finally {
      clearInterval(clock);
      setRunning(false);
      inFlight.current = false;
    }
  }, []);

  const value = useMemo<EngineState>(() => ({
    market, settings, scenarios, active, kpis,
    running, activeKind, stage, pct, elapsed, samples,
    setActive, setSettings, refreshMarket, refreshScenarios, run,
  }), [market, settings, scenarios, active, kpis, running, activeKind, stage, pct, elapsed, samples,
       setSettings, refreshMarket, refreshScenarios, run]);

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

/** Poll a job, surfacing each progress snapshot. Mirrors api.awaitJob but
 * exposes the RunProgress directly so callers can stream telemetry. */
async function pollWithTelemetry(
  id: string,
  onTick: (p: NonNullable<Job["progress"]>) => void,
  ms = 300,
): Promise<Job> {
  for (;;) {
    const s = await api.job(id);
    if (s.progress) onTick(s.progress);
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
