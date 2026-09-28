import { useState } from "react";
import { Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis, Legend } from "recharts";
import { api, awaitJob, fmt$, rowsOf, type ForecastPreview, type ForecastRequest, type ForecastResult, type ResearchSnapshot } from "../lib/api";
import { Button, Input } from "./ui";

const selectClass = "w-full rounded border border-surface-3 bg-surface-2 px-2 py-1.5 text-xs text-paper";

export default function ForecastResearch({ snapshot, revision }: { snapshot: ResearchSnapshot; revision: number }) {
  const forecast = snapshot.forecast!;
  const [scenario, setScenario] = useState(forecast.runnable.includes("baseline") ? "baseline" : "median");
  const [start, setStart] = useState(forecast.periods[0] || "");
  const [horizon, setHorizon] = useState(27);
  const [preview, setPreview] = useState<ForecastPreview | null>(null);
  const [result, setResult] = useState<ForecastResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const request: ForecastRequest = { snapshot_id: snapshot.id, scenario, start_period: start,
    horizon_months: horizon, alignment: "relative_replay", expected_revision: revision };
  const [previewKey, setPreviewKey] = useState("");
  const key = JSON.stringify(request);
  const validPreview = preview && previewKey === key;
  const perform = async (fn: () => Promise<void>) => {
    setBusy(true); setError("");
    try { await fn(); } catch (e) { setError(String(e)); setStatus(""); } finally { setBusy(false); }
  };
  const monthly = result ? rowsOf<Record<string, number>>(result.monthly) : [];
  const deltas = monthly.reduce((v, r) => v + r.delta_nii, 0);
  const download = () => {
    if (!result) return;
    const body = { ...result, monthly, summary: rowsOf(result.summary), base_summary: rowsOf(result.base_summary),
      runoff: rowsOf(result.runoff), drivers: rowsOf(result.drivers) };
    const url = URL.createObjectURL(new Blob([JSON.stringify(body, null, 2)], { type: "application/json" }));
    const a = document.createElement("a"); a.href = url; a.download = `forecast-${snapshot.id.slice(0, 8)}.json`; a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  return <section className="space-y-3 border-t border-surface-3 pt-3" aria-label="Published forecast simulation">
    <p className="text-sm font-medium">Run a published forecast</p>
    <p className="text-xs text-paper-dim">Replay a published path on the current book. The selected source month becomes projection month 1; security dates stay unchanged. Results show conditional income and runoff, with no default-loss or capital forecast.</p>
    <div className="grid grid-cols-[repeat(auto-fit,minmax(160px,1fr))] gap-3">
      <label className="text-xs text-paper-dim">Scenario
        <select aria-label="Forecast scenario" className={selectClass} value={scenario} disabled={busy} onChange={e => setScenario(e.target.value)}>
          {forecast.runnable.map(s => <option key={s} value={s}>{s === "adverse" ? "Severely adverse" : s === "median" ? "Published median" : "Supervisory baseline"}</option>)}
        </select>
      </label>
      <label className="text-xs text-paper-dim">First source month
        <select aria-label="Forecast first month" className={selectClass} value={start} disabled={busy} onChange={e => setStart(e.target.value)}>
          {forecast.periods.map(p => <option key={p} value={p}>{p.slice(0, 7)}</option>)}
        </select>
      </label>
      <label className="text-xs text-paper-dim">Projection months
        <Input aria-label="Forecast months" type="number" min={3} max={120} value={horizon} disabled={busy} onChange={e => setHorizon(Number(e.target.value))} />
      </label>
    </div>
    <div className="flex flex-wrap gap-2">
      <Button disabled={busy || !start || horizon < 3 || horizon > 120 || !Number.isInteger(horizon)} onClick={() => perform(async () => {
        setPreview(null); setResult(null); setStatus("Preparing source paths…");
        const next = await api.previewForecast(request); setPreview(next); setPreviewKey(key); setStatus("Preview ready.");
      })}>Preview forecast</Button>
      <Button disabled={busy || !validPreview} onClick={() => perform(async () => {
        setResult(null); setStatus("Queued…");
        const job = await api.runForecast(request);
        const done = await awaitJob(job.id, j => setStatus(j.progress?.stage || j.status), 700);
        if (done.status !== "done") throw new Error(done.detail || "Forecast failed");
        setResult(done.result as ForecastResult); setStatus("Forecast complete. Active inputs are unchanged.");
      })}>Run conditional forecast</Button>
    </div>
    {error && <p role="alert" className="text-xs text-red-400">{error}</p>}
    {status && <p role="status" className="text-xs text-brand">{status}</p>}
    {preview && !validPreview && <p className="text-xs text-paper-dim">Selection or market changed. Preview again before running.</p>}
    {validPreview && <>
      <p className="text-xs text-paper-dim">Book date {preview.book_as_of} · source cutoff {preview.source_as_of} · {preview.horizon_months} months</p>
      <table className="w-full text-xs text-left"><thead><tr><th>Input</th><th>Source coverage ends</th><th>Months held flat in report</th></tr></thead>
        <tbody>{Object.entries(preview.coverage).map(([name, c]) => <tr key={name} className="border-t border-surface-3"><td className="py-1">{name.replaceAll("_", " ")}</td><td>Month {c.last_month}</td><td>{c.tail_months_in_report}</td></tr>)}</tbody>
      </table>
      <div className="max-h-52 overflow-auto"><table className="w-full text-xs text-left">
        <thead><tr><th>Month</th><th>Short rate</th><th>10y rate</th><th>Mortgage</th><th>House prices*</th></tr></thead>
        <tbody>{preview.drivers.map(r => <tr key={r.month} className="border-t border-surface-3">
          <td>{r.month}</td>{[r.short_rate ?? r.policy_rate, r.rate_10y, r.mortgage_rate].map((v, i) => <td key={i}>{v === undefined ? "Model" : `${(v * 100).toFixed(2)}%`}</td>)}
          <td>{r.hpi === undefined ? "Model" : (r.hpi * 100).toFixed(2)}</td>
        </tr>)}</tbody></table></div>
      <p className="text-xs text-paper-faint">*House prices rebased to 100 at replay start. Rates are model targets derived from source values. Mortgage incentives apply after the model lag.</p>
      <details className="text-xs text-paper-dim"><summary className="cursor-pointer">Assumptions and unused source variables</summary>
        <ul className="list-disc pl-4 space-y-1 mt-2">{preview.warnings.map((w, i) => <li key={i}>{w}</li>)}</ul>
        <p className="mt-2">Reference only: {preview.unused_variables.join(", ") || "None"}. These variables do not affect this run.</p>
      </details>
    </>}
    {result && validPreview && <div className="space-y-2" aria-label="Forecast results">
      <p className="text-sm">Cumulative NII change: <strong>{fmt$(deltas)}</strong> over {monthly.length} months</p>
      <p className="text-xs text-paper-dim">Result for {result.provenance.dataset}, source start {result.provenance.start_period}, input revision {result.provenance.revision}.</p>
      <div className="h-56 w-full"><ResponsiveContainer><LineChart data={monthly}>
        <XAxis dataKey="month" stroke="#a1a1aa" /><YAxis tickFormatter={fmt$} width={80} stroke="#a1a1aa" />
        <Tooltip formatter={(v: number) => fmt$(v)} contentStyle={{ background: "#18181b", borderColor: "#3f3f46" }} /><Legend />
        <Line dataKey="base_nii" name="Base monthly NII" stroke="#a1a1aa" dot={false} isAnimationActive={false} />
        <Line dataKey="nii" name="Conditional monthly NII" stroke="#34d399" dot={false} isAnimationActive={false} />
      </LineChart></ResponsiveContainer></div>
      <p className="text-xs text-paper-dim">{result.warnings.slice(-2).join(" ")}</p>
      <Button variant="ghost" onClick={download}>Download forecast results</Button>
    </div>}
  </section>;
}
