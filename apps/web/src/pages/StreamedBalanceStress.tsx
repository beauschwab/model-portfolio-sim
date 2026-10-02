import { useEffect, useRef, useState } from 'react';
import { api, type Job, type ResultPage, type StressCapabilities, type StressManifest } from '../lib/api';
import { useEngineData } from '../lib/engine';
import { Badge, Button, Card, CardBody, CardHeader, Spinner } from '../components/ui';

const field = 'h-8 min-w-0 rounded border border-line bg-surface-2 px-2 text-xs text-paper';
const number = (value: number) => value.toLocaleString();
const display = (value: unknown) => value == null ? '—' : typeof value === 'number'
  ? value.toLocaleString(undefined, { maximumFractionDigits: 6 }) : String(value);
const storageKey = (scope: string) => `balance-stream:v1:${scope}`;
function remember(scope: string | null, id: string) {
  try { if (scope) sessionStorage.setItem(storageKey(scope), id); } catch { /* Storage is optional. */ }
}

export default function StreamedBalanceStress() {
  const { revision } = useEngineData();
  const [capabilities, setCapabilities] = useState<StressCapabilities | null>(null);
  const [text, setText] = useState('');
  const backend = 'rust' as const;
  const [large, setLarge] = useState(false);
  const [job, setJob] = useState<Job | null>(null);
  const [id, setId] = useState('');
  const [openId, setOpenId] = useState('');
  const [manifest, setManifest] = useState<StressManifest | null>(null);
  const [table, setTable] = useState('/summary');
  const [offset, setOffset] = useState(0);
  const [page, setPage] = useState<{ key: string; data: ResultPage } | null>(null);
  const [partition, setPartition] = useState(0);
  const [pending, setPending] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pageError, setPageError] = useState<string | null>(null);
  const [retry, setRetry] = useState(0);
  const [pageRetry, setPageRetry] = useState(0);
  const generation = useRef(0);
  const busy = pending || job?.status === 'queued' || job?.status === 'running';

  useEffect(() => {
    let active = true;
    Promise.all([api.stressCapabilities(), api.balanceStressExample()]).then(([caps, example]) => {
      if (!active) return;
      setCapabilities(caps); setText(JSON.stringify(example.specification, null, 2));
      try {
        const saved = caps.scope_id && sessionStorage.getItem(storageKey(caps.scope_id));
        if (saved) { setId(saved); setOpenId(saved); }
      } catch { /* Users can reopen a run explicitly when storage is unavailable. */ }
    }).catch(e => { if (active) setError(String(e)); });
    return () => { active = false; generation.current++; };
  }, []);

  useEffect(() => {
    if (!id) return;
    let active = true;
    const token = generation.current;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const status = await api.job(id);
        if (!active || token !== generation.current) return;
        if (status.kind !== 'streamed_balance_stress') throw new Error('Select a partitioned balance-sheet run.');
        setJob(status);
        if (status.status === 'done') {
          const result = await api.stressManifest(id);
          if (active && token === generation.current) {
            setManifest(result); setTable('/summary'); setOffset(0); setPartition(0);
          }
        } else if (status.status === 'error') setError(status.detail ?? 'Simulation failed.');
        else timer = setTimeout(poll, 700);
      } catch (e) {
        if (active && token === generation.current) { setError(String(e)); setJob(null); }
      }
    }
    void poll();
    return () => { active = false; clearTimeout(timer); };
  }, [id, retry]);

  const descriptor = manifest?.tables[table];
  const pageKey = `${manifest?.id}:${table}:${offset}`;
  const visible = page?.key === pageKey ? page.data : null;
  useEffect(() => {
    if (!manifest || !descriptor) return;
    const controller = new AbortController();
    let active = true;
    setPage(null); setPageError(null);
    api.resultPage(manifest.id, table, offset, controller.signal)
      .then(data => { if (active) setPage({ key: pageKey, data }); })
      .catch(e => { if (active) setPageError(String(e)); });
    return () => { active = false; controller.abort(); };
  }, [manifest, descriptor, table, offset, pageKey, pageRetry]);

  function openRun(runId: string) {
    generation.current++; setError(null); setManifest(null); setPage(null); setJob(null);
    setId(runId); setOpenId(runId); setRetry(r => r + 1);
    remember(capabilities?.scope_id ?? null, runId);
  }
  async function run() {
    const token = ++generation.current;
    setPending(true); setError(null); setManifest(null); setJob(null); setId(''); setPage(null);
    try {
      const specification = JSON.parse(text) as Record<string, unknown>;
      const submitted = await api.runStreamedStress(specification, revision, backend, large);
      remember(capabilities?.scope_id ?? null, submitted.id);
      if (token === generation.current) { setId(submitted.id); setOpenId(submitted.id); setJob(submitted); }
    } catch (e) { if (token === generation.current) setError(String(e)); }
    finally { if (token === generation.current) setPending(false); }
  }
  async function cancel() {
    const token = generation.current;
    setPending(true); setError(null);
    try { await api.cancelJob(id); }
    catch (e) { if (token === generation.current) setError(String(e)); }
    finally { if (token === generation.current) { setPending(false); setRetry(r => r + 1); } }
  }
  async function download() {
    const token = generation.current;
    setDownloading(true); setError(null);
    try {
      const blob = await api.downloadPartition(id, table, partition);
      if (token !== generation.current) return;
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a'); link.href = url;
      link.download = `balance-${id}-${table.slice(1)}-${partition}.parquet`; link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (e) { if (token === generation.current) setError(String(e)); }
    finally { if (token === generation.current) setDownloading(false); }
  }

  return <div className="space-y-3">
    <Card><CardHeader title="Partitioned balance-sheet simulation" sub="Durable runs with a complete saved journal" right={<Badge tone="amber">Research model</Badge>} />
      <CardBody className="space-y-3">
        <p className="text-xs text-paper-dim">Run explicit balance-sheet assumptions in Rust. Journal checks finish before results are published. Browse 100 rows at a time and download individual Parquet files.</p>
        {capabilities && !capabilities.durable && <p role="status" className="text-xs text-paper-dim">Partitioned runs require the durable API and a separate calculation worker.</p>}
        <div className="flex flex-wrap items-center gap-3">
          <label className="text-xs">Engine <select aria-label="Partitioned engine" className={field} value={backend} disabled>
            <option value="rust">Rust{capabilities && !capabilities.rust ? ' — unavailable' : ''}</option>
          </select></label>
          <label className="flex items-center gap-2 text-xs"><input type="checkbox" checked={large} disabled={busy || !capabilities?.large_book} onChange={e => setLarge(e.target.checked)} />Large book (up to 60,000 instruments)</label>
        </div>
        <p className="text-xs text-paper-faint">{capabilities?.large_book ? 'Large-book capacity is enabled for this workspace.' : 'Standard capacity: up to 2,000 instruments. Large-book admission is disabled on this deployment.'}</p>
        <details><summary className="cursor-pointer text-xs">Edit partitioned specification (JSON)</summary>
          <label htmlFor="partitioned-specification" className="mt-2 block text-xs">Explicit balance-sheet specification</label>
          <textarea id="partitioned-specification" className="mt-1 h-80 w-full rounded border border-line bg-surface-2 p-2 font-mono text-xs" disabled={busy} value={text} onChange={e => setText(e.target.value)} spellCheck={false} />
        </details>
        <div className="flex flex-wrap items-center gap-2">
          <Button disabled={busy || downloading || !capabilities?.durable || !capabilities?.rust || !text} onClick={run}>Run partitioned simulation</Button>
          {job && busy && <Button variant="ghost" disabled={pending} onClick={cancel}>Cancel run</Button>}
          {busy && <span role="status" className="flex items-center gap-2 text-xs"><Spinner />{job?.progress?.stage ?? job?.status ?? 'Submitting'}</span>}
        </div>
        <form className="flex flex-wrap items-center gap-2" onSubmit={e => { e.preventDefault(); if (!busy && !downloading && openId.trim()) openRun(openId.trim()); }}>
          <label htmlFor="partitioned-run-id" className="text-xs">Saved run ID</label>
          <input id="partitioned-run-id" className={`${field} flex-1`} value={openId} onChange={e => setOpenId(e.target.value)} disabled={busy} />
          <Button variant="ghost" disabled={busy || downloading || !openId.trim() || !capabilities?.durable}>Open run</Button>
        </form>
        {job && <p className="break-all text-xs text-paper-faint">Run {job.id} · {job.status} · input revision {job.revision}</p>}
        <p className="text-xs text-paper-faint">Runs continue when you leave this panel. The latest run is remembered for this workspace in this browser tab; you can also reopen it by ID.</p>
        {error && <p role="alert" className="break-words text-xs text-down">{error}</p>}
      </CardBody>
    </Card>
    {manifest && descriptor && <Card><CardHeader title="Persisted run results" sub={`Run ${manifest.id} · saved revision ${manifest.revision} · current revision ${revision}`} />
      <CardBody className="space-y-3">
        <p className="text-xs text-paper-faint">These results belong to the saved run, independent of edits to the draft above.</p>
        <div className="flex flex-wrap gap-2">
          <Badge>{manifest.execution?.backend ?? 'Unknown'} engine</Badge>
          <Badge tone={manifest.execution?.validation.journal_replayed ? 'green' : 'amber'}>{manifest.execution?.validation.journal_replayed ? 'Saved journal verified' : 'Verification unavailable'}</Badge>
          <Badge tone={manifest.execution?.validation.dynamic_validated ? 'green' : 'amber'}>{manifest.execution?.validation.dynamic_validated ? 'Daily research limits passed' : 'Review daily limit breaches'}</Badge>
        </div>
        <label className="flex flex-wrap items-center gap-2 text-xs">Result table
          <select aria-label="Partitioned result table" className={field} value={table} disabled={downloading} onChange={e => { setTable(e.target.value); setOffset(0); setPartition(0); }}>
            {Object.keys(manifest.tables).map(name => <option key={name} value={name}>{name.slice(1).replaceAll('_', ' ')}</option>)}
          </select>
        </label>
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <span>{number(descriptor.rows)} rows · {number(descriptor.parts.length)} partitions</span>
          <Button variant="ghost" disabled={!offset} onClick={() => setOffset(o => Math.max(0, o - 100))}>Previous page</Button>
          <span>Page {Math.floor(offset / 100) + 1} of {Math.max(1, Math.ceil(descriptor.rows / 100))}</span>
          <Button variant="ghost" disabled={offset + 100 >= descriptor.rows} onClick={() => setOffset(o => o + 100)}>Next page</Button>
        </div>
        {pageError ? <div><p role="alert" className="text-xs text-down">{pageError}</p><Button variant="ghost" onClick={() => setPageRetry(r => r + 1)}>Retry table</Button></div>
          : !visible ? <p role="status" className="text-xs">Loading table page…</p>
          : <div className="max-h-[32rem] overflow-auto"><table aria-label="Partitioned results" className="w-full text-left text-[11px] num">
            <thead><tr>{visible.columns.map(c => <th key={c} className="whitespace-nowrap border-b border-line p-2">{c.replaceAll('_', ' ')}</th>)}</tr></thead>
            <tbody>{visible.rows.map((row, i) => <tr key={offset + i}>{row.map((value, j) => <td key={visible.columns[j]} className="whitespace-nowrap border-b border-line/50 p-2">{display(value)}</td>)}</tr>)}</tbody>
          </table>{!visible.rows.length && <p className="p-3 text-xs text-paper-faint">No rows in this table.</p>}</div>}
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <label>Parquet partition <input aria-label="Parquet partition" className={`${field} w-24`} type="number" min={0} max={Math.max(0, descriptor.parts.length - 1)} step={1} value={partition} disabled={downloading || !descriptor.parts.length} onChange={e => setPartition(Number(e.target.value))} /></label>
          <Button variant="ghost" disabled={downloading || !Number.isInteger(partition) || partition < 0 || partition >= descriptor.parts.length} onClick={download}>{downloading ? 'Downloading…' : 'Download partition'}</Button>
        </div>
        <p className="text-xs text-paper-faint">Research assumptions remain uncalibrated. Journal reconciliation does not establish regulatory compliance. Reverse-stress journals are not included in persisted replay.</p>
      </CardBody>
    </Card>}
  </div>;
}
