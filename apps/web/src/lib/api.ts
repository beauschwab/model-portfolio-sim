/** Thin typed client over the FastAPI service (proxied at /api in dev).
 *  Computed polars frames arrive as Apache Arrow IPC (see decodeEnvelope);
 *  scalar/list payloads stay JSON. */
import { DataType, tableFromIPC, type Table } from "apache-arrow";

const BASE = "/api";

export type { Table };
export type BookName = "mbs" | "loans" | "debt" | "deposits" | "cds" | "mm";
export type Row = Record<string, unknown>;
export type PricingOptions = {
  assumption_overrides?: Partial<Record<BookName, Record<string, Record<string, number>>>>;
  spread_overrides_bp?: Partial<Record<BookName, Record<string, number>>>;
  calibration_mode?: 'hold' | 'recalibrate'; include_analytics?: boolean;
  backend?: 'rust'; expected_revision?: number;
};
export interface Market {
  swap_tenors: number[]; swap_rates: number[]; vol_pts: number[][]; source: string; revision: number;
  provenance?: { curve?: string; volatility?: string; curve_as_of?: string; snapshot_id?: string; warnings?: string[] };
}
export interface ResearchSource { id: string; label: string; provider: string; access: string; use: string; notes: string; configured: boolean }
export interface ResearchSnapshot {
  id: string; dataset: string; as_of: string; fetched_at: string; observation_count: number; warnings: string[];
  curve: { swap_rates: number[]; swap_tenors: number[]; as_of: string; max_zero_error_bp_30y: number; max_df_error_30y: number } | null;
  observations?: { date: string; maturity?: string; series: string; value: number; unit: string; classification: string }[];
  forecast?: { scenarios: string[]; runnable: string[]; periods: string[]; variables: string[]; alignment: string } | null;
}
export interface ForecastRequest { snapshot_id: string; scenario: string; start_period: string; horizon_months: number; alignment: "relative_replay"; expected_revision: number }
export interface ForecastPreview {
  snapshot_id: string; dataset: string; book_as_of: string; source_as_of: string; revision: number;
  start_period: string; horizon_months: number; warnings: string[]; unused_variables: string[];
  coverage: Record<string, { first_month: number; last_month: number; tail_months_in_report: number }>;
  drivers: { month: number; short_rate?: number; policy_rate?: number; rate_10y?: number; mortgage_rate?: number; hpi?: number }[];
}
export interface ForecastResult { monthly: Table; summary: Table; base_summary: Table; runoff: Table; drivers: Table; warnings: string[]; provenance: ForecastPreview }
export interface BalanceStressResult {
  revision: number; model_version: string; specification: Record<string, unknown>; warnings: string[];
  summary: Table; path: Table; ledger: Table; actions: Table; breaches: Table;
  exposures: Table; attribution: Table; reverse_grid: Table;
  journal: Table; trial_balance: Table; funding_claims: Table;
  validation: { dynamic_validated: boolean; validation_scope: string; ruleset: string; breach_count: number };
}
export interface StressCapabilities { durable: boolean; rust: boolean; large_book: boolean; scope_id: string | null }
export interface PartitionTable {
  format: 'partitioned-parquet'; schema: Record<string, string>; rows: number;
  parts: { rows: number; ref: { key: string; sha256: string; bytes: number; format: string } }[];
}
export interface StressManifest {
  id: string; revision: number; tables: Record<string, PartitionTable>;
  execution: null | { backend: 'python' | 'rust'; model_version: string; binary_sha256: string;
    validation: { journal_replayed: boolean; dynamic_validated: boolean; journal_rows: number; gl_keys: number } };
}
export interface ResultPage { columns: string[]; rows: unknown[][]; offset: number; limit: number; total: number }
export interface Scenario { name: string; ust10y_bp: number[]; twos_tens_bp: number[]; spread_bp: number[]; vol_bp: number[] }
export interface Settings { compute_backend: 'python' | 'rust'; n_paths: number; n_paths_base: number; n_threads: number; seed: number; horizon_months: number; shocks_bp: number[] }
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
  id: string; revision: number; kind: string; status: "queued" | "running" | "done" | "error";
  detail?: string; result?: unknown; progress?: RunProgress;
}

async function j<T>(path: string, init?: RequestInit): Promise<T> {
  const r = await fetch(BASE + path, { headers: { "Content-Type": "application/json" }, ...init });
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
  const value = await r.json();
  if ((init?.method === "PUT" || init?.method === "DELETE") && !path.startsWith('/decision/')) window.dispatchEvent(new Event("engine:inputs-changed"));
  return value;
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
  treasuryExample: () => j<{specification:Record<string,unknown>;revision:number;durable:boolean}>('/treasury/example'),
  treasurySourceTemplate: (id:string,kind:'ledger'|'cashflows') => j<{specification:Record<string,unknown>;revision:number;note:string}>(`/treasury/sources/${encodeURIComponent(id)}/template?kind=${kind}`),
  runTreasuryBridge: (source_job:string,specification:Record<string,unknown>,expected_revision:number) => j<Job>('/treasury/bridges',{method:'POST',body:JSON.stringify({source_job,specification,expected_revision})}),
  runTreasury: (specification:Record<string,unknown>,expected_revision:number) => j<Job>('/treasury/runs', {method:'POST',body:JSON.stringify({specification,expected_revision})}),
  cohortPresets: () => j<{config: Record<string, unknown>; revision: number; durable: boolean; tapes: Record<string,{sha256:string;rows:number;source:string;columns:string[]}>}>('/cohorts/presets'),
  cohortExample: () => j<{csv:string}>('/cohorts/example'),
  importTape: (payload: {name:string;format:string;uri?:string;content_base64?:string}) => j<Job>('/cohorts/imports',{method:'POST',body:JSON.stringify(payload)}),
  adoptTape: (id:string,job_id:string,expected_revision:number) => j<{revision:number}>(`/cohorts/tapes/${encodeURIComponent(id)}`,{method:'PUT',body:JSON.stringify({job_id,expected_revision})}),
  buildCohorts: (payload:{tape_id:string;config:Record<string,unknown>;baseline_job?:string;previous_job?:string;expected_revision:number}) => j<Job>('/cohorts/builds',{method:'POST',body:JSON.stringify(payload)}),
  cohortSummary: (id:string) => j<CohortSummary>(`/cohorts/builds/${id}/summary`),
  cohortLineage: (id:string,filter:{loan_id?:string;cohort_id?:string;offset?:string}) => j<{total:number;rows:Row[]}>(`/cohorts/builds/${id}/lineage?${new URLSearchParams(filter)}`),
  cohortAnalytics: (id:string,products:string[],loan_ids?:string[]) => j<Job>(`/cohorts/builds/${id}/analytics`,{method:'POST',body:JSON.stringify({products,loan_ids})}),
  cohortAttribution: (id:string,analytics_job:string) => j<Job>(`/cohorts/builds/${id}/attribution`,{method:'POST',body:JSON.stringify({analytics_job})}),
  cohortAudit: (id:string,products:string[],tolerances:Record<string,unknown>) => j<Job>(`/cohorts/builds/${id}/audit`,{method:'POST',body:JSON.stringify({products,tolerances})}),
  cohortAuditSummary: (id:string) => j<{summary:{passed:boolean;loans:number;cohorts:number;checks:number;failed_checks:number;failed_cohorts:number}}>(`/cohorts/audits/${id}/summary`),
  publishCohorts: (id:string,products:string[],expected_revision:number,mode='replace_books') => j<{revision:number;books:Record<string,number>}>(`/cohorts/builds/${id}/publish`,{method:'PUT',body:JSON.stringify({products,expected_revision,mode})}),
  cohortTable: (id:string,path:string,offset=0) => j<ResultPage>(`/jobs/${id}/table?${new URLSearchParams({path,offset:String(offset),limit:'100'})}`),
  buildDecision: (expected_revision: number, options: unknown) => j<Job>('/decision/sessions', { method: 'POST', body: JSON.stringify({ expected_revision, options }) }),
  updateDecision: (id: string, request: unknown) => j<Job>(`/decision/sessions/${id}/update`, { method: 'POST', body: JSON.stringify(request) }),
  evaluateDecision: (id: string, request: unknown) => j<DecisionEvaluation>(`/decision/sessions/${id}/eval`, { method: 'POST', body: JSON.stringify(request) }),
  closeDecision: (id: string) => j(`/decision/sessions/${id}`, { method: 'DELETE' }),
  /** `inputs`: content hash per input node at `inputs.revision` (absent on older servers). */
  state: () => j<{ revision: number; library_ready: boolean; library_horizon: number | null;
    inputs?: { revision: number; nodes: Record<string, string> } }>("/state"),
  optimize: (options: unknown) => j<Job>("/optimize", { method: "POST", body: JSON.stringify(options) }),
  books: () => j<Record<string, { positions: number; balance: number }>>("/books"),
  book: (n: BookName) => jArrow(`/books/${n}`) as Promise<Table>,
  putBook: (n: BookName, rows: Row[]) => j(`/books/${n}`, { method: "PUT", body: JSON.stringify(rows) }),
  market: () => j<Market>("/market"),
  putMarket: (m: Partial<Market>) => j("/market", { method: "PUT", body: JSON.stringify(m) }),
  researchSources: () => j<ResearchSource[]>("/market-data/sources"),
  researchSnapshots: () => j<ResearchSnapshot[]>("/market-data/snapshots"),
  researchSnapshot: (id: string) => j<ResearchSnapshot>(`/market-data/snapshots/${id}?limit=25`),
  fetchResearch: (request: { dataset: string; as_of: string; start: string; identifier: string; series: string[] }) =>
    j<Job>("/market-data/fetch", { method: "POST", body: JSON.stringify(request) }),
  importResearch: (request: unknown) => j<ResearchSnapshot>("/market-data/import", { method: "POST", body: JSON.stringify(request) }),
  applyResearchCurve: (snapshot_id: string, expected_revision: number) => j<Market>("/market-data/active-curve", {
    method: "PUT", body: JSON.stringify({ snapshot_id, expected_revision }),
  }),
  previewForecast: (request: ForecastRequest) => j<ForecastPreview>("/forecasts/preview", { method: "POST", body: JSON.stringify(request) }),
  runForecast: (request: ForecastRequest) => j<Job>("/forecasts/run", { method: "POST", body: JSON.stringify(request) }),
  balanceStressExample: () => j<{ specification: Record<string, unknown>; contract: Record<string, unknown>; revision: number }>("/balance-stress/example"),
  stressCapabilities: () => j<StressCapabilities>('/balance-stress/capabilities'),
  runStreamedStress: (specification: Record<string, unknown>, expected_revision: number, backend: 'rust', large_book: boolean) =>
    j<Job>('/balance-stress/stream', { method: 'POST', body: JSON.stringify({ specification, expected_revision, backend, large_book }) }),
  stressManifest: (id: string) => j<StressManifest>(`/jobs/${encodeURIComponent(id)}/manifest`),
  resultPage: (id: string, path: string, offset: number, signal?: AbortSignal) =>
    j<ResultPage>(`/jobs/${encodeURIComponent(id)}/table?${new URLSearchParams({ path, offset: String(offset), limit: '100' })}`, { signal }),
  cancelJob: (id: string) => j<{ cancelled: boolean }>(`/jobs/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  downloadPartition: async (id: string, path: string, partition: number) => {
    const response = await fetch(`${BASE}/jobs/${encodeURIComponent(id)}/parquet?${new URLSearchParams({ path, partition: String(partition) })}`);
    if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
    return response.blob();
  },
  runBalanceStress: (specification: Record<string, unknown>, expected_revision: number) => j<Job>("/balance-stress/run", { method: "POST", body: JSON.stringify({ specification, expected_revision }) }),
  balanceStressInventory: () => j<{ revision: number; instruments: { source_id: string }[]; required_mapping: string[] }>('/balance-stress/inventory'),
  runSavedBalanceStress: (request: Record<string, unknown>, expected_revision: number) => j<Job>('/balance-stress/saved-book', { method: 'POST', body: JSON.stringify({ ...request, expected_revision }) }),
  settings: () => j<Settings>("/settings"),
  putSettings: (s: Settings) => j("/settings", { method: "PUT", body: JSON.stringify(s) }),
  assumptions: () => j<Row>("/assumptions"),
  putAssumptions: (p: Row) => j("/assumptions", { method: "PUT", body: JSON.stringify(p) }),
  scenarios: () => j<Record<string, Scenario>>("/scenarios"),
  putScenario: (s: Scenario) => j(`/scenarios/${s.name}`, { method: "PUT", body: JSON.stringify(s) }),
  pricingAssumptions: () => j<{ fields: Record<string, Record<string, [number, number]>>; defaults: Record<string, number> }>("/pricing/assumptions"),
  run: (kind: string, scenario?: string, books?: BookName[], pricing?: PricingOptions) =>
    j<Job>("/run", { method: "POST", body: JSON.stringify({ kind, scenario, books, ...pricing }) }),
  job: (id: string) => j<Job>(`/jobs/${id}`),
  jobResult: (id: string) => jArrow(`/jobs/${id}/result`),
  strategyEval: (alloc: unknown, signal?: AbortSignal) => jArrow("/strategy/eval", {
    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(alloc), signal,
  }),
};

export interface CohortSummary {
  build_id:string;source_sha256:string;revision:number;tape_id:string;config:Record<string,unknown>;
  summary:{loans:number;cohorts:number;balance:number;compression:number;backend:string};warnings:string[];
  comparison?:{cohorts_before:number;cohorts_after:number;changed_members:number;balance_difference:number};
  refresh_summary?:{balance_before:number;balance_after:number;balance_change:number;counts:Record<string,number>;warning:string};
  pricing_support:Record<string,{supported:boolean;reason:string}>;
}

export interface DecisionAllocation { template: string; purchase_m: number; notional: number }
export interface DecisionReplay {
  'nii_total_$': number; nii_incremental: number[]; funding_gap: number[];
  kpis: { cet1_horizon_pct: number };
  kpi_path: { lcr_pct: number[]; nsfr_pct: number[]; 'd_eve_pct_eve_+200': number[] };
}
export interface DecisionEvaluation { version: number; revision: number; replay: DecisionReplay[] }
export interface DecisionResult extends DecisionEvaluation {
  session_id: string; feasible: boolean; validated: boolean; validation: string; message?: string;
  'worst_case_nii_$'?: number; allocation: DecisionAllocation[];
  binding_constraints: { constraint: string; shadow_price: number }[];
  changed_positions: string[]; changed_templates: string[]; scenarios: string[];
  units?: { template: string; h: number; side: number }[];
  horizon_months: number; initialization_ms: number;
  work: { positions_repriced: number; templates_rebuilt: number; total_positions: number; ffi_request_bytes: number };
  solver: { model_reused: boolean; simplex_iterations: number };
  timings_ms: { total: number; native_solve: number; pricing_and_coefficients: number; validation_and_publication: number };
}

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

/** Readable text for a failed API call: the server's `detail` when it sent one. */
export function errorText(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  const body = message.replace(/^\d+\s*/, "");
  try {
    const parsed = JSON.parse(body) as { detail?: unknown };
    if (typeof parsed.detail === "string") return parsed.detail;
  } catch { /* not JSON: fall through */ }
  return body;
}
