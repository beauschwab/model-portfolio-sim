import { useEffect, useRef, useState } from 'react';
import { api, fmt$, type CohortSummary, type Job, type Row, type ResultPage } from '../lib/api';
import { Button, Card, CardBody, CardHeader, DataTable, Input, Spinner } from '../components/ui';

const area = 'w-full rounded border border-line bg-surface-2 p-2 font-mono text-xs text-paper';
type Dimension = {field:string;edges?:number[];separate_missing?:boolean};
type RuleConfig = {rules:Record<string,{dimensions:Dimension[];averages:string[]}>};
const pageRows = (p:ResultPage):Row[] => p.rows.map(row => Object.fromEntries(p.columns.map((key,i)=>[key,row[i]])));
function fileBase64(file:File):Promise<string> {
  return new Promise((resolve,reject)=>{
    const reader=new FileReader();
    reader.onerror=()=>reject(new Error('Could not read file'));
    reader.onload=()=>resolve(String(reader.result).split(',')[1]);
    reader.readAsDataURL(file);
  });
}

export default function Cohorts() {
  const [catalog,setCatalog]=useState<Awaited<ReturnType<typeof api.cohortPresets>>|null>(null);
  const [rules,setRules]=useState('');
  const [tape,setTape]=useState('loan-tape');
  const [uri,setUri]=useState('');
  const [format,setFormat]=useState('csv');
  const [file,setFile]=useState<File|null>(null);
  const [busy,setBusy]=useState('');
  const [error,setError]=useState('');
  const [notice,setNotice]=useState('');
  const [buildId,setBuildId]=useState('');
  const [baseline,setBaseline]=useState('');
  const [previousBuild,setPreviousBuild]=useState('');
  const [summary,setSummary]=useState<CohortSummary|null>(null);
  const [table,setTable]=useState<Row[]>([]);
  const [tablePath,setTablePath]=useState('/cohorts');
  const [offset,setOffset]=useState(0);
  const [total,setTotal]=useState(0);
  const [loan,setLoan]=useState('');
  const [lineage,setLineage]=useState<Row[]>([]);
  const [products,setProducts]=useState<string[]>([]);
  const [analyticsJob,setAnalyticsJob]=useState('');
  const [analyticsRows,setAnalyticsRows]=useState<Row[]>([]);
  const [riskRows,setRiskRows]=useState<Row[]>([]);
  const [outputJob,setOutputJob]=useState('');
  const [analyticsLabel,setAnalyticsLabel]=useState('');
  const [ruleProduct,setRuleProduct]=useState('mortgage');
  const [auditJob,setAuditJob]=useState('');
  const [auditSummary,setAuditSummary]=useState<Awaited<ReturnType<typeof api.cohortAuditSummary>>['summary']|null>(null);
  const [auditRows,setAuditRows]=useState<Row[]>([]);
  const [relativeTolerance,setRelativeTolerance]=useState('1');
  const [absoluteTolerance,setAbsoluteTolerance]=useState('0.01');
  let parsedRules:RuleConfig|null=null;
  try {
    const parsed=JSON.parse(rules) as RuleConfig;
    if(parsed?.rules && typeof parsed.rules==='object' && Object.values(parsed.rules).every(r=>r && Array.isArray(r.dimensions) && r.dimensions.every(d=>d && typeof d.field==='string' && (d.edges===undefined || Array.isArray(d.edges)))))parsedRules=parsed;
  } catch { /* Keep invalid advanced edits visible. */ }
  function editDimension(index:number,edit:Partial<Dimension>) {
    try {
      const parsed=JSON.parse(rules) as RuleConfig;
      Object.assign(parsed.rules[ruleProduct].dimensions[index],edit);
      setRules(JSON.stringify(parsed,null,2));setError('');
    } catch {setError('Correct the rules JSON before using the guided editor.');}
  }
  const mounted=useRef(true);
  const pageRequest=useRef(0);
  useEffect(()=>{
    mounted.current=true;
    api.cohortPresets().then(value=>{
      if(mounted.current){setCatalog(value);setRules(JSON.stringify(value.config,null,2));}
    }).catch(e=>{if(mounted.current)setError(String(e));});
    return()=>{mounted.current=false;};
  },[]);
  async function refresh() { const c=await api.cohortPresets(); if(mounted.current)setCatalog(c); return c; }
  async function wait(job:Job) {
    for(;;){
      if(!mounted.current)throw new Error('Panel closed; job continues in Pipeline');
      const current=await api.job(job.id);
      if(current.status==='error')throw new Error(current.detail || 'Job failed');
      if(current.status==='done')return job.id;
      setBusy(`${job.kind}: ${current.progress?.stage || current.status}`);
      await new Promise(resolve=>setTimeout(resolve,400));
    }
  }
  async function action(label:string,fn:()=>Promise<void>) {
    setError('');setNotice('');setBusy(label);
    try {await fn();}catch(e){if(mounted.current)setError(String(e));}
    finally {if(mounted.current)setBusy('');}
  }
  async function showPage(id:string,path:string,start:number) {
    const sequence=++pageRequest.current;
    const page=await api.cohortTable(id,path,start);
    if(mounted.current && sequence===pageRequest.current){setTable(pageRows(page));setTotal(page.total);setOffset(start);setTablePath(path);}
  }
  async function importFile(example=false) {
    const c=await refresh();
    let payload:{name:string;format:string;uri?:string;content_base64?:string};
    if(example){const demo=await api.cohortExample();payload={name:'synthetic-example.csv',format:'csv',content_base64:btoa(demo.csv)};}
    else if(uri.trim())payload={name:tape,format,uri:uri.trim()};
    else {
      if(!file)throw new Error('Select a CSV/Parquet file or enter a configured source URI');
      if(file.size>32*1024*1024)throw new Error('The import limit is 32 MiB');
      payload={name:file.name,format:file.name.toLowerCase().endsWith('.parquet')?'parquet':'csv',content_base64:await fileBase64(file)};
    }
    const id=await wait(await api.importTape(payload));
    await api.adoptTape(tape,id,c.revision);
    await refresh();setNotice('Immutable tape saved. Review the rules, then build cohorts.');
    setPreviousBuild(summary?.tape_id===tape?buildId:'');
    setBuildId('');setSummary(null);setBaseline('');setAnalyticsJob('');setAnalyticsRows([]);setRiskRows([]);setLineage([]);
    setAuditJob('');setAuditSummary(null);setAuditRows([]);
  }
  async function build() {
    if(!catalog) return;
    const config=JSON.parse(rules) as Record<string,unknown>;
    const id=await wait(await api.buildCohorts({tape_id:tape,config,baseline_job:baseline||undefined,previous_job:previousBuild||undefined,expected_revision:catalog.revision}));
    const s=await api.cohortSummary(id);
    setBuildId(id);setSummary(s);setProducts(Object.entries(s.pricing_support).filter(([,v])=>v.supported).map(([k])=>k));
    setAnalyticsJob('');setAnalyticsRows([]);setRiskRows([]);setLineage([]);setAnalyticsLabel('');
    setAuditJob('');setAuditSummary(null);setAuditRows([]);
    await showPage(id,'/cohorts',0);
  }
  async function analyze(individual:boolean) {
    const id=await wait(await api.cohortAnalytics(buildId,products,individual?[loan]:undefined));
    if(!individual)setAnalyticsJob(id);
    const [page,risk]=await Promise.all([api.cohortTable(id,'/cashflows'),api.cohortTable(id,'/risk')]);
    setAnalyticsRows(pageRows(page));setRiskRows(pageRows(risk));setOutputJob(id);
    setAnalyticsLabel(individual?'Individual model repricing — original loan terms':'Representative cohort repricing — weighted terms');
  }
  return <div className="space-y-3">
    <Card><CardHeader title="Tape & Cohorts" sub="Versioned rules · product-specific behavior · loan-to-position lineage" />
      <CardBody className="space-y-3">
        <p className="text-xs text-paper-faint">Import CSV or Parquet, adjust behavioral groups, and compare builds on the same tape. Amounts use currency units; rates and ratios use decimals. Loan IDs remain strings.</p>
        {catalog && !catalog.durable && <p role="alert" className="text-xs text-down">Start the SQLite/PostgreSQL API and worker to use durable tape workflows.</p>}
        <div className="grid gap-3 md:grid-cols-3">
          <label className="text-xs">Tape name<Input aria-label="Tape name" value={tape} onChange={e=>setTape(e.target.value)} list="saved-tapes"/><datalist id="saved-tapes">{Object.keys(catalog?.tapes||{}).map(k=><option key={k} value={k}/>)}</datalist></label>
          <label className="text-xs">Upload a file<input aria-label="Tape file" type="file" accept=".csv,.parquet" onChange={e=>{setFile(e.target.files?.[0]||null);setUri('');}} className="block w-full text-xs"/></label>
          <label className="text-xs">Or configured local / S3 URI<Input aria-label="Tape URI" value={uri} onChange={e=>setUri(e.target.value)}/></label>
        </div>
        {uri && <label className="text-xs">Source format <select aria-label="Source format" value={format} onChange={e=>setFormat(e.target.value)} className="bg-surface-2"><option>csv</option><option>parquet</option></select></label>}
        <div className="flex flex-wrap gap-2">
          <Button disabled={!!busy||!catalog?.durable||!tape} onClick={()=>void action('Importing tape',()=>importFile())}>Import tape</Button>
          <Button variant="ghost" disabled={!!busy||!catalog?.durable} onClick={()=>void action('Importing synthetic example',()=>importFile(true))}>Load synthetic example</Button>
          <Button variant="ghost" disabled={!!busy} onClick={()=>void action('Refreshing tapes',async()=>{await refresh();})}>Refresh tapes</Button>
        </div>
        {catalog?.tapes[tape] && <p className="text-xs text-paper-faint">{catalog.tapes[tape].rows.toLocaleString()} records · source: {catalog.tapes[tape].source} · SHA-256 {catalog.tapes[tape].sha256.slice(0,16)}…</p>}
        <details open><summary className="cursor-pointer text-sm">Cohort rules and column mapping</summary>
          <div className="my-3 space-y-2">
            <label className="text-xs">Product rules <select aria-label="Rule product" value={ruleProduct} onChange={e=>setRuleProduct(e.target.value)} className="rounded bg-surface-2 p-1">{Object.keys(parsedRules?.rules||{}).map(p=><option key={p}>{p}</option>)}</select></label>
            {Array.isArray(parsedRules?.rules[ruleProduct]?.dimensions) && parsedRules.rules[ruleProduct].dimensions.map((d,i)=><div key={`${ruleProduct}-${d.field}-${JSON.stringify(d.edges)}`} className="flex flex-wrap items-center gap-2 text-xs">
              <span className="w-36">{d.field}</span>
              {Array.isArray(d.edges)?<label>Bucket edges <Input aria-label={`${ruleProduct} ${d.field} bucket edges`} defaultValue={d.edges.join(', ')} disabled={!!busy} onBlur={e=>{
                const parts=e.target.value.split(',').map(v=>v.trim());const edges=parts.map(Number);
                if(parts.some(v=>!v)||edges.some((v,j)=>!Number.isFinite(v)||(j>0&&v<=edges[j-1])))setError('Bucket edges must be finite numbers in increasing order.');
                else editDimension(i,{edges});
              }}/></label>:<span className="text-paper-faint">Separate each category</span>}
              <label><input type="checkbox" checked={!!d.separate_missing} disabled={!!busy} onChange={e=>editDimension(i,{separate_missing:e.target.checked})}/> Separate missing values</label>
            </div>)}
          </div>
          <p className="my-2 text-xs text-paper-faint">column_map maps canonical names to your source columns. defaults are explicit missing-value assumptions. Bucket edges are left-closed. Product, currency, entity, accounting category and assumption set always remain separate. averages require complete numeric data.</p>
          <textarea aria-label="Cohort rules JSON" rows={15} className={area} value={rules} onChange={e=>setRules(e.target.value)}/>
        </details>
        <div className="flex flex-wrap gap-2">
          <Button disabled={!!busy||!catalog?.tapes[tape]} onClick={()=>void action('Building cohorts',build)}>Build cohorts</Button>
          <Button variant="ghost" disabled={!buildId||!!busy} onClick={()=>{setBaseline(buildId);setNotice('Current build pinned. Edit the rules and rebuild to compare.');}}>Pin as comparison baseline</Button>
          {baseline && <Button variant="ghost" disabled={!!busy} onClick={()=>setBaseline('')}>Clear baseline</Button>}
          {previousBuild && <Button variant="ghost" disabled={!!busy} onClick={()=>setPreviousBuild('')}>Clear prior-tape comparison</Button>}
        </div>
        {busy && <p role="status" className="flex items-center gap-2 text-xs"><Spinner/>{busy}</p>}
        {error && <p role="alert" className="whitespace-pre-wrap text-xs text-down">{error}</p>}
        {notice && <p role="status" className="text-xs text-up">{notice}</p>}
      </CardBody>
    </Card>
    {summary && <>
      <Card><CardHeader title="Build impact" sub={`Build ${summary.build_id.slice(0,16)} · native ${summary.summary.backend} grouping`}/><CardBody className="space-y-3">
        <p className="text-sm">{summary.summary.loans.toLocaleString()} loans → {summary.summary.cohorts.toLocaleString()} cohorts · {summary.summary.compression.toFixed(1)}× compression · {fmt$(summary.summary.balance)}</p>
        {summary.comparison && <p className="text-xs">Cohorts: {summary.comparison.cohorts_before} → {summary.comparison.cohorts_after}. Changed cohort identities: {summary.comparison.changed_members} loans. Balance difference: {fmt$(summary.comparison.balance_difference)}.</p>}
        {summary.refresh_summary && <p className="text-xs">Tape refresh: {Object.entries(summary.refresh_summary.counts).map(([s,n])=>`${n} ${s}`).join(' · ')}. Balance change: {fmt$(summary.refresh_summary.balance_change)}. {summary.refresh_summary.warning}</p>}
        {summary.warnings.map(w=><p key={w} className="text-xs text-paper-faint">{w}</p>)}
        <div className="flex flex-wrap gap-2">{['/cohorts','/dispersion',...(summary.comparison?['/migration']:[]),...(summary.refresh_summary?['/refresh']:[])].map(path=><Button key={path} variant="ghost" disabled={!!busy} onClick={()=>void action('Loading table',()=>showPage(buildId,path,0))}>{path.slice(1)}</Button>)}</div>
        <DataTable rows={table}/><div className="flex items-center gap-2 text-xs"><Button variant="ghost" disabled={!!busy||offset===0} onClick={()=>void action('Loading page',()=>showPage(buildId,tablePath,Math.max(0,offset-100)))}>Previous</Button>{offset+1}–{Math.min(offset+100,total)} of {total}<Button variant="ghost" disabled={!!busy||offset+100>=total} onClick={()=>void action('Loading page',()=>showPage(buildId,tablePath,offset+100))}>Next</Button></div>
      </CardBody></Card>
      <Card><CardHeader title="Simulation and loan drilldown" sub="Publish replaces the selected simulation books. Other books remain in the saved balance sheet."/><CardBody className="space-y-3">
        {Object.entries(summary.pricing_support).map(([p,v])=><label key={p} className="flex items-center gap-2 text-xs"><input type="checkbox" disabled={!v.supported||!!busy} checked={products.includes(p)} onChange={e=>setProducts(old=>e.target.checked?[...old,p]:old.filter(x=>x!==p))}/>{p}: {v.reason}</label>)}
        <div className="flex flex-wrap gap-2"><Button disabled={!!busy||!products.length} onClick={()=>void action('Pricing representative positions',()=>analyze(false))}>Price cohorts</Button>
          <Button variant="ghost" disabled={!!busy||!products.length} onClick={()=>void action('Publishing selected books',async()=>{const p=await api.publishCohorts(buildId,products,summary.revision);await refresh();setNotice(`Published ${Object.keys(p.books).join(', ')} at revision ${p.revision}.`);})}>Replace selected books with cohorts</Button>
          <Button variant="ghost" disabled={!!busy||!products.length} onClick={()=>void action('Updating tape positions',async()=>{const p=await api.publishCohorts(buildId,products,summary.revision,'replace_tape');await refresh();setNotice(`Updated this tape’s positions at revision ${p.revision}; other sources retained.`);})}>Update only this tape’s positions</Button></div>
        <label className="block text-xs">Loan/account ID<Input aria-label="Loan ID" value={loan} onChange={e=>setLoan(e.target.value)}/></label>
        <div className="flex flex-wrap gap-2"><Button variant="ghost" disabled={!!busy||!loan} onClick={()=>void action('Looking up lineage',async()=>{const p=await api.cohortLineage(buildId,{loan_id:loan});setLineage(p.rows);if(!p.total)setNotice('Loan not found in this build.');})}>Find cohort</Button>
          <Button variant="ghost" disabled={!!busy||!loan||!products.length} onClick={()=>void action('Repricing original loan',()=>analyze(true))}>Reprice original loan</Button>
          <Button variant="ghost" disabled={!!busy||!analyticsJob} onClick={()=>void action('Allocating cohort results',async()=>{const id=await wait(await api.cohortAttribution(buildId,analyticsJob));const [flows,risk]=await Promise.all([api.cohortTable(id,'/cashflows'),api.cohortTable(id,'/positions')]);setAnalyticsRows(pageRows(flows));setRiskRows(pageRows(risk));setOutputJob(id);setAnalyticsLabel('Balance-weighted allocation — not individual repricing');})}>Allocate cohort cashflows to loans</Button></div>
        {lineage.length>0 && <DataTable rows={lineage}/>}
        {analyticsLabel && <><p className="text-xs">{analyticsLabel} · job {outputJob} · first 100 rows per table; complete tables are available from the job’s Parquet export.</p><p className="text-xs">Risk and income</p><DataTable rows={riskRows}/><p className="text-xs">Monthly cashflows</p><DataTable rows={analyticsRows}/></>}
      </CardBody></Card>
      <Card><CardHeader title="Cohort accuracy" sub="Compare every selected loan with its cohort under base and ±200 bp rate scenarios."/><CardBody className="space-y-3">
        <p className="text-xs text-paper-faint">Checks PV, DV01 and NII in all three scenarios, plus monthly base cashflows. Each instrument keeps its own base OAS. Passing checks measures cohort approximation; it does not validate behavior against observed outcomes.</p>
        <div className="flex flex-wrap gap-3"><label className="text-xs">Relative tolerance (%)<Input aria-label="Audit relative tolerance" type="number" min="0" step="0.1" value={relativeTolerance} onChange={e=>setRelativeTolerance(e.target.value)}/></label><label className="text-xs">Absolute tolerance ($)<Input aria-label="Audit absolute tolerance" type="number" min="0" step="0.01" value={absoluteTolerance} onChange={e=>setAbsoluteTolerance(e.target.value)}/></label></div>
        <Button disabled={!!busy||!products.length} onClick={()=>void action('Auditing cohort accuracy',async()=>{
          const absolute=Number(absoluteTolerance),relative=Number(relativeTolerance)/100;
          if(!absoluteTolerance.trim()||!relativeTolerance.trim()||![absolute,relative].every(x=>Number.isFinite(x)&&x>=0))throw new Error('Tolerances must be finite and nonnegative.');
          const metrics=['market_value','dv01','nii','principal','cash_interest','accrual_interest','book_amortization'];
          const tolerances=Object.fromEntries(products.map(p=>[p,Object.fromEntries(metrics.map(m=>[m,{absolute,relative}]))]));
          const id=await wait(await api.cohortAudit(buildId,products,tolerances));
          const [report,errors]=await Promise.all([api.cohortAuditSummary(id),api.cohortTable(id,'/errors')]);
          setAuditJob(id);setAuditSummary(report.summary);setAuditRows(pageRows(errors));
        })}>Audit all selected loans</Button>
        {auditSummary && <><p role="status" className={`text-xs ${auditSummary.passed?'text-up':'text-down'}`}>Accuracy audit: {auditSummary.passed?'passed':'outside tolerance'} · {auditSummary.loans} loans · {auditSummary.failed_checks} of {auditSummary.checks} checks outside tolerance across {auditSummary.failed_cohorts} cohorts.</p>
          <div className="flex flex-wrap gap-2">{['errors','suggestions','loan_risk','loan_cashflows'].map(path=><Button key={path} variant="ghost" disabled={!!busy} onClick={()=>void action('Loading audit output',async()=>{
            const rows=pageRows(await api.cohortTable(auditJob,`/${path}`));setAuditRows(rows);
            if(!rows.length)setNotice('No rows were produced for this audit table.');
          })}>{path.replaceAll('_',' ')}</Button>)}</div><DataTable rows={auditRows}/><p className="text-xs text-paper-faint">First 100 rows shown. Full partitioned outputs: job {auditJob}.</p></>}
      </CardBody></Card>
    </>}
  </div>;
}
