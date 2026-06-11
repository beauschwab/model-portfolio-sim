/** Thin typed client over the FastAPI service (proxied at /api in dev). */
const BASE = "/api";

export type BookName = "mbs" | "loans" | "debt" | "deposits" | "cds" | "mm";
export type Row = Record<string, unknown>;
export interface Market { swap_tenors: number[]; swap_rates: number[]; vol_pts: number[][]; source: string }
export interface Scenario { name: string; ust10y_bp: number[]; twos_tens_bp: number[]; spread_bp: number[]; vol_bp: number[] }
export interface Settings { n_paths: number; seed: number; horizon_months: number; shocks_bp: number[] }
export interface Job { id: string; kind: string; status: "queued" | "running" | "done" | "error"; detail?: string; result?: unknown }

async function j<T>(path: string, init?: RequestInit): Promise<T> {
  const r = await fetch(BASE + path, { headers: { "Content-Type": "application/json" }, ...init });
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
  return r.json();
}

export const api = {
  books: () => j<Record<string, { positions: number; balance: number }>>("/books"),
  book: (n: BookName) => j<Row[]>(`/books/${n}`),
  putBook: (n: BookName, rows: Row[]) => j(`/books/${n}`, { method: "PUT", body: JSON.stringify(rows) }),
  market: () => j<Market>("/market"),
  putMarket: (m: Partial<Market>) => j("/market", { method: "PUT", body: JSON.stringify(m) }),
  settings: () => j<Settings>("/settings"),
  putSettings: (s: Settings) => j("/settings", { method: "PUT", body: JSON.stringify(s) }),
  assumptions: () => j<Row>("/assumptions"),
  putAssumptions: (p: Row) => j("/assumptions", { method: "PUT", body: JSON.stringify(p) }),
  scenarios: () => j<Record<string, Scenario>>("/scenarios"),
  putScenario: (s: Scenario) => j(`/scenarios/${s.name}`, { method: "PUT", body: JSON.stringify(s) }),
  run: (kind: string, scenario?: string, books?: BookName[]) =>
    j<Job>("/run", { method: "POST", body: JSON.stringify({ kind, scenario, books }) }),
  job: (id: string) => j<Job>(`/jobs/${id}`),
};

/** Poll a job to completion. */
export async function awaitJob(id: string, onTick?: (s: Job) => void, ms = 1500): Promise<Job> {
  for (;;) {
    const s = await api.job(id);
    onTick?.(s);
    if (s.status === "done" || s.status === "error") return s;
    await new Promise(r => setTimeout(r, ms));
  }
}

export const fmt$ = (v: number) =>
  Math.abs(v) >= 1e9 ? `$${(v / 1e9).toFixed(2)}B` :
  Math.abs(v) >= 1e6 ? `$${(v / 1e6).toFixed(1)}M` :
  Math.abs(v) >= 1e3 ? `$${(v / 1e3).toFixed(0)}k` : `$${v.toFixed(0)}`;
export const fmtBp = (v: number) => `${v.toFixed(1)}bp`;
