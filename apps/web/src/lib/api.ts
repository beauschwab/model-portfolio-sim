/** Thin typed client over the FastAPI service (proxied at /api in dev).
 *  Computed polars frames arrive as Apache Arrow IPC (see decodeEnvelope);
 *  scalar/list payloads stay JSON. */
import { DataType, tableFromIPC, type Table } from "apache-arrow";

const BASE = "/api";

export type { Table };
export type BookName = "mbs" | "loans" | "debt" | "deposits" | "cds" | "mm";
export type Row = Record<string, unknown>;
export interface Market { swap_tenors: number[]; swap_rates: number[]; vol_pts: number[][]; source: string }
export interface Scenario { name: string; ust10y_bp: number[]; twos_tens_bp: number[]; spread_bp: number[]; vol_bp: number[] }
export interface Settings { n_paths: number; seed: number; horizon_months: number; shocks_bp: number[] }
export interface RunPlan {
  kind: string; records: number; records_by_book: Record<string, number>;
  in_scope: number; monte_carlo_paths: number; horizon_months: number;
  rate_shocks_bp: number[]; scenario_path_steps: number; revaluations: number;
  path_evaluations: number; reductions: number; crn_seed: number; note?: string;
  scenario_markets?: number;
}
export type NodeKind = "build" | "branch" | "paths" | "cashflow" | "oas" | "reduce" | "solve";
export type NodeStatus = "pending" | "running" | "done" | "error";
export interface PipelineNode {
  id: string;
  parent: string | null;
  label: string;
  kind: NodeKind;
  status: NodeStatus;
  detail?: string | null;
  stat?: Record<string, number>;
  t0?: number | null;
  t1?: number | null;
}
export interface RunProgress {
  stage?: string; pct?: number; elapsed_s?: number;
  plan?: Partial<RunPlan>;
  stats?: Record<string, number>;
  log?: { t: number; msg: string }[];
  nodes?: PipelineNode[];
}
export interface Job {
  id: string; kind: string; status: "queued" | "running" | "done" | "error";
  detail?: string; result?: unknown; progress?: RunProgress;
}

async function j<T>(path: string, init?: RequestInit): Promise<T> {
  const r = await fetch(BASE + path, { headers: { "Content-Type": "application/json" }, ...init });
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
  return r.json();
}

// ---- Arrow envelope decoding ------------------------------------------------
/** Decode the ARW1 binary envelope: a JSON skeleton plus N Arrow IPC blobs,
 *  with each {"__arrow__": i} marker rehydrated into the i-th Arrow Table.
 *  Zero-blob payloads (no frames) round-trip as a plain JSON tree. */
export function decodeEnvelope(buf: ArrayBuffer): unknown {
  const u8 = new Uint8Array(buf);
  if (u8[0] !== 0x41 || u8[1] !== 0x52 || u8[2] !== 0x57 || u8[3] !== 0x31)
    throw new Error("bad arrow envelope magic");
  const dv = new DataView(buf);
  let off = 4;
  const skelLen = dv.getUint32(off, true); off += 4;
  const skeleton = JSON.parse(new TextDecoder().decode(new Uint8Array(buf, off, skelLen)));
  off += skelLen;
  const n = dv.getUint32(off, true); off += 4;
  const lens: number[] = [];
  for (let i = 0; i < n; i++) { lens.push(dv.getUint32(off, true)); off += 4; }
  const tables: Table[] = [];
  for (let i = 0; i < n; i++) { tables.push(tableFromIPC(new Uint8Array(buf, off, lens[i]))); off += lens[i]; }
  return rehydrate(skeleton, tables);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function rehydrate(node: any, tables: Table[]): any {
  if (node && typeof node === "object") {
    if (!Array.isArray(node) && typeof node.__arrow__ === "number") return tables[node.__arrow__];
    if (Array.isArray(node)) return node.map(v => rehydrate(v, tables));
    const out: Record<string, unknown> = {};
    for (const k in node) out[k] = rehydrate(node[k], tables);
    return out;
  }
  return node;
}

/** Arrow date/timestamp cells decode to epoch-ms numbers (or Dates); render
 *  them as ISO yyyy-mm-dd strings to match the prior JSON wire. */
function isoDate(v: unknown): string | null {
  if (v == null) return null;
  const d = v instanceof Date ? v : new Date(Number(v));
  return Number.isNaN(d.getTime()) ? null : d.toISOString().slice(0, 10);
}

/** Materialize an Arrow Table (or pass an existing row array straight through)
 *  into plain row objects for Recharts / DataTable / row-wise consumers.
 *  i64 columns surface as BigInt -> coerced to number; temporal columns ->
 *  ISO date strings. */
export function rowsOf<T = Row>(t: Table | T[]): T[] {
  if (Array.isArray(t)) return t;
  const fields = t.schema.fields;
  const names = fields.map(f => f.name);
  const temporal = new Set(
    fields.filter(f => DataType.isDate(f.type) || DataType.isTimestamp(f.type)).map(f => f.name));
  const rows: T[] = new Array(t.numRows);
  for (let i = 0; i < t.numRows; i++) {
    const src = t.get(i)! as Record<string, unknown>;
    const obj: Record<string, unknown> = {};
    for (const name of names) {
      const v = src[name];
      obj[name] = temporal.has(name) ? isoDate(v) : typeof v === "bigint" ? Number(v) : v;
    }
    rows[i] = obj as T;
  }
  return rows;
}

/** A single numeric column as a JS number[] (BigInt-safe) for aggregation. */
export function colOf(t: Table, name: string): number[] {
  const col = t.getChild(name);
  if (!col) return [];
  const out: number[] = new Array(col.length);
  for (let i = 0; i < col.length; i++) {
    const v = col.get(i);
    out[i] = typeof v === "bigint" ? Number(v) : (v as number);
  }
  return out;
}

async function jArrow(path: string, init?: RequestInit): Promise<unknown> {
  const r = await fetch(BASE + path, init);
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
  return decodeEnvelope(await r.arrayBuffer());
}

export const api = {
  books: () => j<Record<string, { positions: number; balance: number }>>("/books"),
  book: (n: BookName) => jArrow(`/books/${n}`) as Promise<Table>,
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
  jobResult: (id: string) => jArrow(`/jobs/${id}/result`),
  strategyEval: (alloc: unknown) => jArrow("/strategy/eval", {
    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(alloc),
  }),
};

/** Poll a job to completion. Status/progress arrive as JSON; the computed
 *  result is fetched once (as an Arrow envelope) when the job is done and
 *  attached to `s.result`, preserving the consumer contract. */
export async function awaitJob(id: string, onTick?: (s: Job) => void, ms = 1500): Promise<Job> {
  for (;;) {
    const s = await api.job(id);
    onTick?.(s);
    if (s.status === "error") return s;
    if (s.status === "done") { s.result = await api.jobResult(id); return s; }
    await new Promise(r => setTimeout(r, ms));
  }
}

export const fmt$ = (v: number) =>
  Math.abs(v) >= 1e9 ? `$${(v / 1e9).toFixed(2)}B` :
  Math.abs(v) >= 1e6 ? `$${(v / 1e6).toFixed(1)}M` :
  Math.abs(v) >= 1e3 ? `$${(v / 1e3).toFixed(0)}k` : `$${v.toFixed(0)}`;
export const fmtBp = (v: number) => `${v.toFixed(1)}bp`;
