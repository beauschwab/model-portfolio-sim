import { useEffect, useState } from "react";
import { api, awaitJob, type Market, type ResearchSnapshot, type ResearchSource } from "../lib/api";
import { Button, Card, CardBody, CardHeader, Input, Badge } from "./ui";
import ForecastResearch from "./ForecastResearch";

const selectClass = "rounded border border-surface-3 bg-surface-2 px-2 py-1.5 text-sm text-paper w-full";
const today = () => new Date().toLocaleDateString("en-CA");

export default function ResearchData({ market, onMarket }: { market: Market | null; onMarket: (m: Market) => void }) {
  const [sources, setSources] = useState<ResearchSource[]>([]);
  const [snapshots, setSnapshots] = useState<ResearchSnapshot[]>([]);
  const [dataset, setDataset] = useState("eris_sofr");
  const [asOf, setAsOf] = useState(today);
  const [start, setStart] = useState(() => `${new Date().getFullYear() - 5}-01-01`);
  const [identifier, setIdentifier] = useState("");
  const [series, setSeries] = useState("");
  const [selected, setSelected] = useState<ResearchSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const source = sources.find(s => s.id === dataset);

  useEffect(() => {
    let active = true;
    Promise.all([api.researchSources(), api.researchSnapshots()]).then(([s, h]) => {
      if (active) { setSources(s); setSnapshots(h); }
    }).catch(e => { if (active) setError(String(e)); });
    return () => { active = false; };
  }, []);

  const perform = async (fn: () => Promise<void>) => {
    setBusy(true); setError(""); setMessage("");
    try { await fn(); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const preview = async (id: string) => { setSelected(await api.researchSnapshot(id)); };
  const fetchData = () => perform(async () => {
    const job = await api.fetchResearch({ dataset, as_of: asOf, start, identifier,
      series: series.split(",").map(s => s.trim()).filter(Boolean) });
    setMessage("Downloading and validating source data…");
    const done = await awaitJob(job.id, undefined, 500);
    if (done.status !== "done") throw new Error(done.detail || "Download failed");
    await preview((done.result as ResearchSnapshot).id);
    setSnapshots(await api.researchSnapshots());
    setMessage("Snapshot saved. Review it below; your active market has not changed.");
  });
  const apply = () => perform(async () => {
    if (!market || !selected?.curve) return;
    const updated = await api.applyResearchCurve(selected.id, market.revision);
    onMarket(updated);
    setMessage("Research curve applied. Volatility and behavioral assumptions are retained; rebuild existing strategy libraries.");
  });

  return <Card>
    <CardHeader title="Research market data" sub="Dated public datasets with reproducible snapshots"
      right={<Badge tone="neutral">Research</Badge>} />
    <CardBody className="space-y-3">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-3">
        <label className="space-y-1 text-sm text-paper-dim">Source
          <select aria-label="Research source" className={selectClass} value={dataset} disabled={busy}
            onChange={e => { setDataset(e.target.value); setIdentifier(""); setSeries(""); }}>
            {sources.map(s => <option key={s.id} value={s.id}>{s.label} · {s.provider}</option>)}
          </select>
        </label>
        <label className="space-y-1 text-sm text-paper-dim">As of
          <Input aria-label="Research as of" type="date" value={asOf} max={today()} disabled={busy} onChange={e => setAsOf(e.target.value)} />
        </label>
        <label className="space-y-1 text-sm text-paper-dim">History starts
          <Input aria-label="History starts" type="date" value={start} max={asOf} disabled={busy || dataset.startsWith("eris") || source?.use === "forecast" || dataset === "fed_stress"} onChange={e => setStart(e.target.value)} />
        </label>
        <div className="flex items-end gap-2">
          <Button disabled={busy || !source?.configured || !asOf || !start} onClick={fetchData}>{busy ? "Working…" : "Fetch snapshot"}</Button>
          <Button variant="secondary" disabled={busy} onClick={() => perform(async () => {
            onMarket(await api.market()); setSnapshots(await api.researchSnapshots());
          })}>Reload</Button>
        </div>
      </div>
      <p className="text-sm text-paper-dim">{source?.notes}</p>
      {(dataset === "fdic" || dataset === "sec" || dataset === "fhfa") && <label className="block max-w-md text-sm text-paper-dim">
        {dataset === "fdic" ? "FDIC certificate number" : dataset === "sec" ? "SEC CIK" : "FHFA place ID (blank = USA)"}
        <Input aria-label="Source identifier" value={identifier} disabled={busy} onChange={e => setIdentifier(e.target.value)} />
      </label>}
      {["fred", "nyfed", "sec"].includes(dataset) && <label className="block max-w-xl text-sm text-paper-dim">
        Series, separated by commas (blank = default selection)
        <Input aria-label="Research series" value={series} disabled={busy} onChange={e => setSeries(e.target.value)} />
      </label>}
      {source && !source.configured && source.access !== "registered_download" &&
        <p className="text-sm text-paper-dim">This source needs server configuration before downloading. {source.access === "api_key" ? "FRED API key required." : "SEC contact identity required."}</p>}
      {source?.access === "registered_download" && <label className="block text-sm text-paper-dim">
        Import an authorized normalized JSON extract
        <input aria-label="Import research extract" type="file" accept=".json" disabled={busy} className="ml-3"
          onChange={e => {
            const file = e.target.files?.[0];
            if (file) void perform(async () => {
              if (file.size > 20 * 1024 * 1024) throw new Error("Extract must be under 20 MiB");
              const body = JSON.parse(await file.text());
              if (body.dataset !== dataset) throw new Error("Extract dataset must match the selected source");
              const saved = await api.importResearch(body);
              await preview(saved.id); setSnapshots(await api.researchSnapshots()); setMessage("Authorized extract saved for research.");
            });
          }} />
      </label>}
      <div className="flex flex-wrap items-center gap-2 text-sm text-paper-dim">
        <span>Active curve: {market?.provenance?.curve || "assumed"}</span>
        <span>· Volatility: {market?.provenance?.volatility || "assumed"}</span>
        {market?.provenance?.curve_as_of && <span>· {market.provenance.curve_as_of}</span>}
      </div>
      {market?.provenance?.snapshot_id && <p className="text-sm text-paper-dim">The curve uses a saved research snapshot. Book valuation dates, security prices and behavioral histories remain unchanged.</p>}
      {error && <p role="alert" className="text-sm text-red-400">{error}</p>}
      {message && <p role="status" className="text-sm text-paper-dim">{message}</p>}
      <label className="block text-sm text-paper-dim">Saved snapshots
        <select aria-label="Saved research snapshots" className={selectClass} disabled={busy} value={selected?.id || ""}
          onChange={e => { if (e.target.value) void perform(() => preview(e.target.value)); else setSelected(null); }}>
          <option value="">Select a snapshot to inspect</option>
          {snapshots.map(s => <option key={s.id} value={s.id}>{s.dataset} · {s.as_of} · {s.observation_count.toLocaleString()} observations · {s.id.slice(0, 8)}</option>)}
        </select>
      </label>
      {selected && <div className="space-y-3 rounded border border-surface-3 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="text-sm text-paper-dim">{selected.observation_count.toLocaleString()} observations · retrieved {new Date(selected.fetched_at).toLocaleString()}</span>
          <div className="flex items-center gap-3">
            <a className="text-sm text-brand-hover underline underline-offset-2" href={`/api/market-data/snapshots/${selected.id}/export`}>Download snapshot</a>
            {selected.curve && <Button disabled={busy || !market} onClick={apply}>Apply research curve</Button>}
          </div>
        </div>
        {selected.warnings.map((w, i) => <p key={i} className="text-sm text-paper-dim">{w}</p>)}
        {selected.curve && <>
          <p className="text-sm text-paper-dim">Source curve date: {selected.curve.as_of}. Maximum zero-rate difference after projection: {selected.curve.max_zero_error_bp_30y.toFixed(3)} bp through 30 years.</p>
          <div className="grid grid-cols-[repeat(auto-fit,minmax(95px,1fr))] gap-2">
            {selected.curve.swap_tenors.map((t, i) => <div key={t} className="text-sm">
              <span className="text-paper-faint">{t}y</span>
              <div>{(selected.curve!.swap_rates[i] * 100).toFixed(3)}%</div>
              {market && <span className="text-paper-dim">{((selected.curve!.swap_rates[i] - market.swap_rates[i]) * 10000).toFixed(1)} bp change</span>}
            </div>)}
          </div>
        </>}
        <div className="max-h-48 overflow-auto">
          <table className="w-full text-left text-sm">
            <thead className="text-paper-faint"><tr>{[selected.curve ? "Maturity" : "Date", "Series", "Value", "Unit", "Type"].map(s => <th key={s} className="p-1">{s}</th>)}</tr></thead>
            <tbody>{selected.observations?.map((r, i) => <tr key={i} className="border-t border-surface-3">
              <td className="p-1">{r.maturity || r.date}</td><td className="p-1">{r.series}</td><td className="p-1 tabular-nums">{r.value.toLocaleString(undefined, { maximumSignificantDigits: 8 })}</td>
              <td className="p-1">{r.unit}</td><td className="p-1">{r.classification}</td>
            </tr>)}</tbody>
          </table>
        </div>
        <p className="text-sm text-paper-faint">Preview shows the first 25 observations. Download includes all observations and source checksums.</p>
        {!!selected.forecast?.runnable.length && market && <ForecastResearch key={selected.id} snapshot={selected} revision={market.revision} />}
      </div>}
    </CardBody>
  </Card>;
}
