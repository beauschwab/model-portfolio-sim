import { useEffect, useRef, useState } from 'react';
import { api, awaitJob, type ResultPage } from '../lib/api';
import { Button, Card, CardBody, CardHeader, DataTable } from '../components/ui';

const tables = [
  ['capital_metrics', 'Capital ratios and headroom'],
  ['ftp_positions', 'Product profitability after FTP'],
  ['ftp_reconciliation', 'Treasury offset and consolidated profit'],
  ['capital_bridge', 'Ledger-to-capital reconciliation'],
  ['rwa_contributions', 'Mapped credit exposures'],
  ['ftp_preparation', 'Cashflow balances and funding life'],
] as const;

export default function Treasury() {
  const [input, setInput] = useState('');
  const [revision, setRevision] = useState<number>();
  const [durable, setDurable] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [job, setJob] = useState('');
  const [mode, setMode] = useState<'manual'|'ledger'|'cashflows'>('manual');
  const [source, setSource] = useState('');
  const [note, setNote] = useState('');
  const [result, setResult] = useState<Record<string, ResultPage>>({});
  const generation = useRef(0);
  const pageRequests = useRef<Record<string, number>>({});
  useEffect(() => {
    let active = true;
    api.treasuryExample().then(data => {
      if (active) { setInput(JSON.stringify(data.specification, null, 2)); setRevision(data.revision); setDurable(data.durable); }
    }).catch(e => { if (active) setError(String(e)); });
    return () => { active = false; generation.current++; };
  }, []);
  const run = async () => {
    const token = ++generation.current;
    setBusy(true); setError(''); setResult({}); setJob('');
    try {
      const spec = JSON.parse(input);
      const submitted = mode === 'manual' ? await api.runTreasury(spec, revision!) : await api.runTreasuryBridge(source, spec, revision!);
      const done = await awaitJob(submitted.id);
      if (done.status !== 'done') throw new Error(done.detail || 'Report failed');
      const selected = tables.filter(([key],i) => i<3 || (mode==='ledger' ? key!=='ftp_preparation' : mode==='cashflows' && key==='ftp_preparation'));
      const frames = await Promise.all(selected.map(async ([key]) => [key, await api.cohortTable(done.id, `/${key}`)] as const));
      if (token === generation.current) { setResult(Object.fromEntries(frames)); setJob(done.id); }
    } catch (e) { if (token === generation.current) setError(String(e)); }
    finally { if (token === generation.current) setBusy(false); }
  };
  const page = async (key: string, offset: number) => {
    const token = generation.current;
    const sequence = (pageRequests.current[key] ?? 0) + 1;
    pageRequests.current[key] = sequence;
    try {
      const frame = await api.cohortTable(job, `/${key}`, offset);
      if (token === generation.current && sequence === pageRequests.current[key]) setResult(old => ({...old, [key]: frame}));
    } catch (e) { if (token === generation.current && sequence === pageRequests.current[key]) setError(String(e)); }
  };
  return <div className="space-y-3">
    <Card><CardHeader title="Capital & funds transfer pricing" sub="Explicit capital policy, eligible balances, and internal funding costs" />
      <CardBody>
        <p className="mb-3 text-xs text-paper-faint">Supply eligible capital, scenario exposures and your bank’s effective requirements, or connect a completed simulation below. Missing requirements are labeled unconfigured. FTP is an internal management allocation; treasury offsets preserve consolidated profit. Rates are decimal fractions; amounts use the specified currency.</p>
        <div className="mb-3 flex flex-wrap gap-2">
          <label className="text-xs">Input source <select aria-label="Treasury input source" disabled={busy} value={mode}
            className="rounded border border-line bg-surface p-1" onChange={e=>{generation.current++;setMode(e.target.value as typeof mode);setResult({});setJob('');setNote('Load the source template before running.');}}>
            <option value="manual">Explicit inputs</option><option value="ledger">Completed ledger simulation</option><option value="cashflows">Captured cohort or loan cashflows</option></select></label>
          {mode!=='manual' && <><label className="text-xs">Source job ID <input aria-label="Treasury source job ID" className="rounded border border-line bg-surface p-1" value={source} disabled={busy}
            onChange={e=>{generation.current++;setSource(e.target.value);setResult({});setJob('');}} /></label>
            <Button disabled={busy||!source} onClick={async()=>{
              const token=++generation.current;setBusy(true);setError('');setResult({});setJob('');
              try{const data=await api.treasurySourceTemplate(source,mode);if(token===generation.current){setInput(JSON.stringify(data.specification,null,2));setRevision(data.revision);setNote(data.note);}}
              catch(e){if(token===generation.current)setError(String(e));}finally{if(token===generation.current)setBusy(false);}
            }}>Load source template</Button></>}
        </div>
        {mode==='manual' && <Button disabled={busy} onClick={async()=>{const token=++generation.current;setBusy(true);try{
          const data=await api.treasuryExample();if(token===generation.current){setInput(JSON.stringify(data.specification,null,2));setResult({});setJob('');setRevision(data.revision);setNote('Synthetic explicit-input example loaded.');}}
          catch(e){if(token===generation.current)setError(String(e));}finally{if(token===generation.current)setBusy(false);}}}>Load explicit example</Button>}
        {note && <p className="mb-2 text-xs text-paper-faint">{note}</p>}
        <label htmlFor="treasury-input" className="text-xs">Capital policy, scenario snapshots and FTP inputs</label>
        <textarea id="treasury-input" className="mt-1 h-72 w-full rounded border border-line bg-surface p-2 font-mono text-xs" value={input} disabled={busy}
          onChange={e => { generation.current++; setInput(e.target.value); setResult({}); setJob(''); }} />
        <div className="mt-2 flex gap-2"><Button disabled={busy || revision === undefined || !durable} onClick={run}>{busy ? 'Calculating…' : 'Run capital & FTP report'}</Button>
          <Button disabled={busy} onClick={async () => { try { setRevision((await api.state()).revision); setError(''); } catch (e) { setError(String(e)); } }}>Refresh workspace revision</Button></div>
        {!durable && <p className="text-xs text-paper-faint">Durable storage is required.</p>}
        {error && <p role="alert" className="mt-2 text-xs text-red-400">{error}</p>}
        {job && <p role="status" className="mt-2 text-xs">Report saved: {job}</p>}
      </CardBody></Card>
    {tables.map(([key, title]) => {
      const frame = result[key];
      return frame && <Card key={key}><CardHeader title={title} sub={`${frame.total} rows · ratios expressed as decimals`} />
        <CardBody><DataTable rows={frame.rows.map(row => Object.fromEntries(frame.columns.map((col, i) => [col, row[i]])))} />
          <div className="mt-2 flex gap-2"><Button disabled={!frame.offset} onClick={() => page(key, Math.max(0, frame.offset-100))}>Previous</Button>
            <Button disabled={frame.offset+frame.rows.length>=frame.total} onClick={() => page(key, frame.offset+100)}>Next</Button>
            <a className="text-xs underline" href={`/api/jobs/${job}/parquet?path=${encodeURIComponent(`/${key}`)}`}>Download Parquet</a></div>
        </CardBody></Card>;
    })}
  </div>;
}
