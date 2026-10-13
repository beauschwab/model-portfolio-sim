import { useEffect, useRef, useState } from 'react';
import { api, awaitJob, awaitJobStatus, type ResultPage, type ModeledFTPSourceKind, type ModeledFTPSourceMetadata } from '../lib/api';
import { Button, Card, CardBody, CardHeader, DataTable } from '../components/ui';

const tables = [
  ['capital_metrics', 'Capital ratios and headroom'],
  ['ftp_positions', 'Product profitability after FTP'],
  ['ftp_reconciliation', 'Treasury offset and consolidated profit'],
  ['capital_bridge', 'Ledger-to-capital reconciliation'],
  ['rwa_contributions', 'Mapped credit exposures'],
  ['ftp_preparation', 'Cashflow balances and funding life'],
  ['source_journal', 'Verified modeled-source journal'],
  ['source_completion', 'Retained native source completion'],
] as const;

export default function Treasury() {
  const [input, setInput] = useState('');
  const [revision, setRevision] = useState<number>();
  const [durable, setDurable] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [job, setJob] = useState('');
  const [reopen, setReopen] = useState('');
  const [mode, setMode] = useState<'manual'|'ledger'|'credit_ledger'|'cashflows'|'modeled_products'|'completed_products'>('manual');
  const [source, setSource] = useState('');
  const [note, setNote] = useState('');
  const [result, setResult] = useState<Record<string, ResultPage>>({});
  type Selection = {job:string;kind:ModeledFTPSourceKind;alias:string;review:string;reviewRevision:string;metadata?:ModeledFTPSourceMetadata};
  const [sources,setSources] = useState<Selection[]>([{job:'',kind:'card_v2',alias:'',review:'',reviewRevision:''}]);
  const modeled = mode==='modeled_products'||mode==='completed_products';
  const generation = useRef(0);
  const changeSource = (index:number,patch:Partial<Selection>) => {
    generation.current++;setSources(old=>old.map((s,i)=>i===index?{...s,...patch,metadata:('job' in patch||'kind' in patch)?undefined:s.metadata}:s));setResult({});setJob('');setError('');
  };
  const checkSource = async(index:number) => {
    const token=++generation.current;const selected=sources[index];setBusy(true);setError('');
    try{
      const metadata=await api.modeledFTPSourceMetadata(selected.job,selected.kind);
      if(token===generation.current)setSources(old=>old.map((s,i)=>i===index?{...s,metadata}:s));
    }catch(e){if(token===generation.current)setError(String(e));}
    finally{if(token===generation.current)setBusy(false);}
  };
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
      if(mode==='completed_products'){
        if(sources.some(s=>!s.metadata||s.metadata.job_id!==s.job||s.metadata.kind!==s.kind||!s.alias.trim()||!s.review.trim()||!s.reviewRevision.trim()))throw new Error('Check each completed source and supply its source alias and scope review');
        if(spec.source?.sources!==undefined)throw new Error('Completed-source policy must omit raw source histories');
      }
      const selection={schema:'modeled-product-ftp-completed-selection-1',sources:sources.map(s=>({source_id:s.alias,source_job:s.job,kind:s.kind,scope:s.metadata?.scope,scope_review:{id:s.review,revision:s.reviewRevision}})),specification:spec};
      const submitted = mode === 'completed_products' ? await api.runCompletedModeledProductFTP(selection,revision!) : mode === 'modeled_products' ? await api.runModeledProductFTP(spec, revision!) : mode === 'manual' ? await api.runTreasury(spec, revision!) : await api.runTreasuryBridge(source, spec, revision!);
      const done = await (modeled ? awaitJobStatus(submitted.id) : awaitJob(submitted.id));
      if (done.status !== 'done') throw new Error(done.detail || 'Report failed');
      const selected = tables.filter(([key],i) => modeled ? ['ftp_positions','ftp_reconciliation','source_journal','source_completion'].includes(key) : i<3 || (i<6 && (mode==='ledger'||mode==='credit_ledger' ? key!=='ftp_preparation' : mode==='cashflows' && key==='ftp_preparation')));
      const frames = await Promise.all(selected.map(async ([key]) => [key, await api.cohortTable(done.id, modeled ? `/tables/${key}` : `/${key}`)] as const));
      if (token === generation.current) { setResult(Object.fromEntries(frames)); setJob(done.id); if(modeled)setReopen(done.id); }
    } catch (e) { if (token === generation.current) setError(String(e)); }
    finally { if (token === generation.current) setBusy(false); }
  };
  const page = async (key: string, offset: number) => {
    const token = generation.current;
    const sequence = (pageRequests.current[key] ?? 0) + 1;
    pageRequests.current[key] = sequence;
    try {
      const frame = await api.cohortTable(job, modeled ? `/tables/${key}` : `/${key}`, offset);
      if (token === generation.current && sequence === pageRequests.current[key]) setResult(old => ({...old, [key]: frame}));
    } catch (e) { if (token === generation.current && sequence === pageRequests.current[key]) setError(String(e)); }
  };
  return <div className="space-y-3">
    <Card><CardHeader title="Capital & funds transfer pricing" sub="Explicit capital policy, eligible balances, and internal funding costs" />
      <CardBody>
        <p className="mb-3 text-sm text-paper-faint">Supply eligible capital, scenario exposures and your bank’s effective requirements, or connect a completed simulation below. Missing requirements are labeled unconfigured. FTP is an internal management allocation; treasury offsets preserve consolidated profit. Rates are decimal fractions; amounts use the specified currency.</p>
        <div className="mb-3 flex flex-wrap gap-2">
          <label className="text-sm">Input source <select aria-label="Treasury input source" disabled={busy} value={mode}
            className="rounded border border-line bg-surface p-1" onChange={e=>{generation.current++;setMode(e.target.value as typeof mode);setResult({});setJob('');setNote('Load the source template before running.');}}>
            <option value="manual">Explicit inputs</option><option value="modeled_products">Raw modeled-product funding and FTP</option><option value="completed_products">Completed modeled-product sources for FTP</option><option value="ledger">Completed ledger simulation</option><option value="credit_ledger">Verified daily credit journal</option><option value="cashflows">Captured cohort or loan cashflows</option></select></label>
          {mode!=='manual' && !modeled && <><label className="text-xs">Source job ID <input aria-label="Treasury source job ID" className="rounded border border-line bg-surface p-1" value={source} disabled={busy}
            onChange={e=>{generation.current++;setSource(e.target.value);setResult({});setJob('');}} /></label>
            <Button disabled={busy||!source} onClick={async()=>{
              const token=++generation.current;setBusy(true);setError('');setResult({});setJob('');
              try{const data=await api.treasurySourceTemplate(source,mode as 'ledger'|'credit_ledger'|'cashflows');if(token===generation.current){setInput(JSON.stringify(data.specification,null,2));setRevision(data.revision);setNote(data.note);}}
              catch(e){if(token===generation.current)setError(String(e));}finally{if(token===generation.current)setBusy(false);}
            }}>Load source template</Button></>}
        </div>
        {mode==='manual' && <Button disabled={busy} onClick={async()=>{const token=++generation.current;setBusy(true);try{
          const data=await api.treasuryExample();if(token===generation.current){setInput(JSON.stringify(data.specification,null,2));setResult({});setJob('');setRevision(data.revision);setNote('Synthetic explicit-input example loaded.');}}
          catch(e){if(token===generation.current)setError(String(e));}finally{if(token===generation.current)setBusy(false);}}}>Load explicit example</Button>}
        {mode==='modeled_products' && <><p className="mb-2 text-xs text-paper-faint">Rebuild reviewed physical raw sources and their complete journals. Original effective interest, credit expense, basis realization and cash remain separate. Curves and contingent costs require explicit reviewed policies; this research preparation does not grant accounting, regulatory or FTP source eligibility.</p>
          <Button disabled={busy} onClick={async()=>{const token=++generation.current;setBusy(true);setError('');try{
            const data=await api.modeledProductFTPExample();if(token===generation.current){setInput(JSON.stringify(data.specification,null,2));setRevision(data.revision);setDurable(data.durable);setResult({});setJob('');setNote('Synthetic raw deposit source and reviewed funding-cost policies loaded.');}}
            catch(e){if(token===generation.current)setError(String(e));}finally{if(token===generation.current)setBusy(false);}}}>Load modeled-product example</Button></>}
        {mode==='completed_products' && <div className="my-3 space-y-3">
          <p className="text-xs text-paper-faint">Select completed Card2 mixed or performing-deposit jobs. Checking reads bounded compatibility metadata; it does not grant eligibility. The new worker independently verifies the original source and journal. Supply a complete reviewed funding policy below, including indexed path reviews. Funding curves, cash assignment and charges are never inferred.</p>
          {sources.map((s,i)=><fieldset key={i} className="rounded border border-line p-2 space-y-2" disabled={busy}>
            <legend className="text-xs">Completed source {i+1}</legend>
            <div className="flex flex-wrap gap-2">
              <label className="text-xs">Product <select aria-label={`FTP completed source kind ${i+1}`} className="rounded border border-line bg-surface p-1" value={s.kind} onChange={e=>changeSource(i,{kind:e.target.value as ModeledFTPSourceKind})}><option value="card_v2">Card2 mixed journal</option><option value="deposit_performing">Performing deposit journal</option><option value="deposit_portfolio">Performing deposit portfolio</option></select></label>
              <label className="text-xs">Completed job ID <input aria-label={`FTP completed source job ${i+1}`} className="rounded border border-line bg-surface p-1" value={s.job} maxLength={100} onChange={e=>changeSource(i,{job:e.target.value})}/></label>
              <Button disabled={busy||!s.job} onClick={()=>checkSource(i)}>Check completed source {i+1}</Button>
            </div>
            <div className="flex flex-wrap gap-2">
              <label className="text-xs">Source alias in funding reviews <input aria-label={`FTP source alias ${i+1}`} className="rounded border border-line bg-surface p-1" value={s.alias} maxLength={128} onChange={e=>changeSource(i,{alias:e.target.value})}/></label>
              <label className="text-xs">Scope review ID <input aria-label={`FTP scope review ID ${i+1}`} className="rounded border border-line bg-surface p-1" value={s.review} onChange={e=>changeSource(i,{review:e.target.value})}/></label>
              <label className="text-xs">Scope review revision <input aria-label={`FTP scope review revision ${i+1}`} className="rounded border border-line bg-surface p-1" value={s.reviewRevision} onChange={e=>changeSource(i,{reviewRevision:e.target.value})}/></label>
            </div>
            {s.metadata&&<div className="text-xs" data-testid={`ftp-source-metadata-${i+1}`}><p>Completed source metadata checked · revision {s.metadata.revision}. Journal verification occurs in the new job.</p><p>Funding scope: {s.kind==='card_v2'?'Only exclusive product owners; inherited dated sources remain retained and are not funded.':s.kind==='deposit_portfolio'?'Every explicit performing deposit owner and supplied run.':'The whole isolated performing deposit owner.'}</p><ul>{s.metadata.paths.map(p=><li key={`${p.owner_index}/${p.run_index}/${p.path_index}`}>Owner {p.owner_index} · run {p.run_index} · path {p.path_index} — {p.position_id} / {p.run_id}</li>)}</ul></div>}
          </fieldset>)}
          <Button disabled={busy||sources.length>=2} onClick={()=>{generation.current++;setSources(old=>[...old,{job:'',kind:'deposit_performing',alias:'',review:'',reviewRevision:''}]);}}>Add second completed source</Button>
          {sources.length>1&&<Button disabled={busy} onClick={()=>{generation.current++;setSources(old=>old.slice(0,1));}}>Remove second source</Button>}
          <p className="text-xs text-paper-faint">Source aliases must match the supplied policy’s funding reviews. For this mode, omit source.sources from the policy. These research reports do not approve accounting or regulatory policy, or adopt saved-book models.</p>
        </div>}
        {modeled && <div className="mt-2 flex gap-2"><label className="text-xs">Saved modeled FTP report ID <input aria-label="Saved modeled FTP report ID" className="rounded border border-line bg-surface p-1" value={reopen} disabled={busy} onChange={e=>setReopen(e.target.value)} /></label>
          <Button disabled={busy||!reopen} onClick={async()=>{const token=++generation.current;setBusy(true);setError('');setResult({});setJob('');try{
            const done=await awaitJobStatus(reopen);if(done.status!=='done'||done.kind!=='modeled_product_ftp')throw new Error('A completed modeled-product FTP report is required');
            const frames=await Promise.all(['ftp_positions','ftp_reconciliation','source_journal','source_completion'].map(async key=>[key,await api.cohortTable(done.id,`/tables/${key}`)]as const));
            if(token===generation.current){setResult(Object.fromEntries(frames));setJob(done.id);setNote('Verified persisted source and Treasury partitions reopened.');}}
            catch(e){if(token===generation.current)setError(String(e));}finally{if(token===generation.current)setBusy(false);}}}>Reopen modeled FTP report</Button></div>}
        {note && <p className="mb-2 text-xs text-paper-faint">{note}</p>}
        <label htmlFor="treasury-input" className="text-xs">{mode==='completed_products'?'Reviewed funding policy and indexed source reviews':'Capital policy, scenario snapshots and FTP inputs'}</label>
        <textarea id="treasury-input" className="mt-1 h-72 w-full rounded border border-line bg-surface p-2 font-mono text-xs" value={input} disabled={busy}
          onChange={e => { generation.current++; setInput(e.target.value); setResult({}); setJob(''); }} />
        <div className="mt-2 flex gap-2"><Button disabled={busy || revision === undefined || !durable || (mode==='completed_products' && sources.some(s=>!s.metadata||!s.alias.trim()||!s.review.trim()||!s.reviewRevision.trim()))} onClick={run}>{busy ? 'Calculating…' : 'Run capital & FTP report'}</Button>
          <Button disabled={busy} onClick={async () => { try { setRevision((await api.state()).revision); setError(''); } catch (e) { setError(String(e)); } }}>Refresh workspace revision</Button></div>
        {!durable && <p className="text-sm text-paper-faint">Durable storage is required.</p>}
        {error && <p role="alert" className="mt-2 text-sm text-red-400">{error}</p>}
        {job && <p role="status" className="mt-2 text-sm">Report saved: {job}</p>}
      </CardBody></Card>
    {tables.map(([key, title]) => {
      const frame = result[key];
      return frame && <Card key={key}><CardHeader title={title} sub={`${frame.total} rows · ratios expressed as decimals`} />
        <CardBody><DataTable rows={frame.rows.map(row => Object.fromEntries(frame.columns.map((col, i) => [col, row[i]])))} />
          <div className="mt-2 flex gap-2"><Button disabled={!frame.offset} onClick={() => page(key, Math.max(0, frame.offset-100))}>Previous</Button>
            <Button disabled={frame.offset+frame.rows.length>=frame.total} onClick={() => page(key, frame.offset+100)}>Next</Button>
            <a className="text-xs underline" href={`/api/jobs/${job}/parquet?path=${encodeURIComponent(modeled ? `/tables/${key}` : `/${key}`)}`}>Download Parquet</a></div>
        </CardBody></Card>;
    })}
  </div>;
}
