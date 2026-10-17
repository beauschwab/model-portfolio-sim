import { useEffect, useRef, useState } from 'react';
import { api, awaitJob, type ResultPage } from '../lib/api';
import { Button, Card, CardBody, CardHeader, DataTable } from '../components/ui';

const labels:Record<string,string>={
  'credit-path-accounting-1':'Physical credit paths and reviewed accounting',
  'derivative-accounting-1':'Physical derivative accounting',
  'hedge-accounting-1':'ASC 815 fair-value and cash-flow hedge relationships',
  'hedge-treasury-1':'Hedge journal closing capital · reviewed AOCI eligibility',
  'credit-covariate-1':'Borrower and macro hazard fitting',
  'advanced-product-graph-1':'Consumer pricing and physical cashflow graph',
  'card-product-graph-1':'Card pricing, statements and physical cashflow graph',
  'deposit-product-graph-1':'Deposit liability pricing, retention and migration graph',
  'mortgage-product-graph-1':'Mortgage borrower, investor and remittance graph',
  'mortgage-credit-accounting-1':'Acquired whole-loan original yield and credit carry',
  'market-indexed-adapter-1':'Supplied curves · conditional indexed cash and swap pricing',
  'term-product-graph-2':'CD election sequences · locked original OAS / supplied P and Q',
  'term-product-reprice-2':'CD sequence repricing · immutable original quote and sources',
  'market-indexed-books-1':'Fully-bound indexed loans · supplied Pricing source',
  'funding-product-graph-1':'Funding, repo collateral and replacement cashflow graph',
  'graph-cohort-audit-1':'Product representative and account error audit',
  'term-product-graph-1':'Term-loan and CD pricing and physical cashflow graph',
  'product-units-1':'Purchase-date product risk, income and funding coefficients',
  'strategy-product-1':'Product allocation with reviewed category and funding limits',
  'product-candidate-journal-1':'Candidate purchases and original-yield journal replay',
  'product-mixed-dated-1':'Dated backbook and candidate purchases in one journal',
  'product-mixed-consumer-credit-1':'Dated mixed book with acquired installment credit owners',
  'product-mixed-product-credit-1':'Dated mixed book with acquired installment and card credit owners',
  'consumer-installment-1':'Auto and personal installment loans','consumer-card-1':'Revolving credit cards',
  'consumer-credit-accounting-1':'Acquired installment original-yield and reviewed credit accounting',
  'card-credit-accounting-1':'Acquired card original-yield and reviewed credit accounting',
  'card-credit-accounting-2':'Card interest rights and carrying value accounting',
  'deposit-state-1':'Deposit retention and repricing','mortgage-state-1':'Mortgage and agency investor cashflows',
  'funding-contract-1':'Funding, repo and collateral','indexed-contract-1':'Indexed corporate contracts',
  'conditional-exercise-1':'Corporate call and put exercise','cd-election-1':'CD withdrawal and renewal',
  'dated-income-1':'Actual-date effective-yield income','model-market-1':'Curves, volatility smile and joint drivers',
  'aged-units-1':'Aged forward coefficients','credit-path-1':'Stochastic credit paths',
  'credit-snapshot-calibration-1':'Observed credit snapshot fitting','derivative-contract-1':'Derivatives and collateral margin',
};

export default function ModelLab(){
  const [schemas,setSchemas]=useState<string[]>([]),[schema,setSchema]=useState('consumer-installment-1');
  const [draft,setDraft]=useState(''),[revision,setRevision]=useState<number>(),[durable,setDurable]=useState(false);
  const [busy,setBusy]=useState(false),[error,setError]=useState(''),[job,setJob]=useState(''),[reopen,setReopen]=useState('');
  const [paths,setPaths]=useState<string[]>([]),[path,setPath]=useState(''),[table,setTable]=useState<ResultPage>();
  const [summary,setSummary]=useState<Record<string,unknown>>({});
  const [verification,setVerification]=useState<Record<string,boolean>>({});
  const [adopted,setAdopted]=useState('');
  const generation=useRef(0),pageSequence=useRef(0);
  const clear=()=>{setPaths([]);setPath('');setTable(undefined);setSummary({});setVerification({});setJob('');setAdopted('');};
  const load=async(selected:string)=>{
    const token=++generation.current;setBusy(true);setError('');clear();
    try{const value=await api.modelExample(selected);if(token===generation.current){setDraft(JSON.stringify(value.request,null,2));setRevision(value.revision);}}
    catch(e){if(token===generation.current)setError(String(e));}finally{if(token===generation.current)setBusy(false);}
  };
  useEffect(()=>{let active=true;api.modelContracts().then(value=>{if(active){setSchemas(value.schemas);setDurable(value.durable);void load(value.schemas[0]);}}).catch(e=>{if(active)setError(String(e));});
    return()=>{active=false;generation.current++;};},[]);
  const show=async(id:string,token:number)=>{
    const manifest=await api.modelManifest(id);
    if(!manifest.execution?.schema || !labels[manifest.execution.schema])throw new Error('Select a completed product model analysis job.');
    const next=Object.keys(manifest.tables);const first=next[0];
    const frame=first?await api.cohortTable(id,first):undefined;
    if(token===generation.current){setJob(id);setReopen(id);setPaths(next);setPath(first??'');setTable(frame);setSummary(manifest.execution.summary);setVerification(manifest.execution.source_verification??{});}
  };
  const run=async()=>{const token=++generation.current;setBusy(true);setError('');clear();
    try{const submitted=await api.runModel({schema_name:schema,request:JSON.parse(draft),expected_revision:revision!});
      const done=await awaitJob(submitted.id);if(done.status!=='done')throw new Error(done.detail||'Analysis failed');await show(done.id,token);}
    catch(e){if(token===generation.current)setError(String(e));}finally{if(token===generation.current)setBusy(false);}
  };
  const page=async(selected:string,offset:number)=>{const token=generation.current,sequence=++pageSequence.current;
    try{const frame=await api.cohortTable(job,selected,offset);if(token===generation.current&&sequence===pageSequence.current){setPath(selected);setTable(frame);}}
    catch(e){if(token===generation.current&&sequence===pageSequence.current)setError(String(e));}
  };
  return <div className="space-y-3"><Card><CardHeader title="Product model analysis" sub="Explicit assumptions · dated cashflows · retained source identities"/><CardBody>
    <p className="mb-3 text-xs text-paper-faint">Examples use synthetic assumptions. Each run stores its complete inputs and model identity. Analysis does not change saved books or establish observed predictive accuracy; adoption into pricing, strategy and accounting requires the corresponding model checks.</p>
    <div className="mb-3 flex flex-wrap items-end gap-2"><label className="text-xs">Product model <select aria-label="Product model" value={schema} disabled={busy} className="ml-2 rounded border border-line bg-surface p-1" onChange={e=>{setSchema(e.target.value);void load(e.target.value);}}>
      {schemas.map(s=><option key={s} value={s}>{labels[s]??s}</option>)}</select></label><Button disabled={busy} onClick={()=>void load(schema)}>Load synthetic example</Button></div>
    <label htmlFor="model-input" className="text-xs">Product terms, path assumptions and source policies</label>
    <textarea id="model-input" disabled={busy} className="mt-1 h-72 w-full rounded border border-line bg-surface p-2 font-mono text-xs" value={draft} onChange={e=>{generation.current++;setDraft(e.target.value);clear();}}/>
    <div className="mt-2 flex flex-wrap gap-2"><Button disabled={busy||!durable||revision===undefined||!draft.trim()} onClick={()=>void run()}>{busy?'Calculating…':'Run product analysis'}</Button>
      {['aged-units-1','product-units-1'].includes(schema)&&<Button disabled={busy||!durable||revision===undefined||!draft.trim()} onClick={async()=>{
        const token=++generation.current;setBusy(true);setError('');clear();
        try{const submitted=await api.buildAgedLibrary({schema_name:schema,request:JSON.parse(draft),expected_revision:revision!});
          const done=await awaitJob(submitted.id);if(done.status!=='done')throw new Error(done.detail||'Library build failed');
          if(token===generation.current){setAdopted(done.id);window.dispatchEvent(new Event('engine:inputs-changed'));}}
        catch(e){if(token===generation.current)setError(String(e));}finally{if(token===generation.current)setBusy(false);}
      }}>Build dated strategy library</Button>}
      <Button disabled={busy} onClick={async()=>{try{setRevision((await api.state()).revision);setError('');}catch(e){setError(String(e));}}}>Refresh workspace revision</Button>
      <a className="self-center text-xs underline" href={`/api/models/examples/${encodeURIComponent(schema)}`}>Download example JSON</a></div>
    <div className="mt-3 flex flex-wrap items-end gap-2"><label className="text-xs">Stored analysis job <input aria-label="Stored analysis job" disabled={busy} className="ml-2 rounded border border-line bg-surface p-1" value={reopen} onChange={e=>setReopen(e.target.value)}/></label>
      <Button disabled={busy||!reopen.trim()} onClick={async()=>{const token=++generation.current;setBusy(true);setError('');clear();try{await show(reopen.trim(),token);}catch(e){if(token===generation.current)setError(String(e));}finally{if(token===generation.current)setBusy(false);}}}>Open stored analysis</Button></div>
    {!durable&&<p className="mt-2 text-xs">Durable storage is required.</p>}{error&&<p role="alert" className="mt-2 text-xs text-red-400">{error}</p>}
    {job&&<p role="status" className="mt-2 text-xs">Analysis saved: {job}</p>}
    {adopted&&<p role="status" className="mt-2 text-xs">Dated coefficients saved: {adopted}. Strategy Lab now evaluates these incremental allocations; saved-book KPI baselines require separate adoption.</p>}
  </CardBody></Card>
  {job&&<Card><CardHeader title="Model results and source identity"/><CardBody><pre className="overflow-auto text-xs">{JSON.stringify(summary,null,2)}</pre></CardBody></Card>}
  {job&&Object.keys(verification).length>0&&<Card><CardHeader title="Independent source verification"/><CardBody>
    <pre className="overflow-auto text-xs">{JSON.stringify(verification,null,2)}</pre>
    {verification.standalone_candidate_only&&<p className="mt-2 text-xs text-paper-faint">Candidate purchases, original-yield recognition and complete closing journals were independently replayed from the stored sources. Opening provenance is supplied; mixed-book and empirical acceptance remain open.</p>}
    {verification.mixed_performing_only&&<p className="mt-2 text-xs text-paper-faint">The dated backbook and candidate purchases were replayed in one chronological journal with complete closing rights. Opening provenance and backbook recognition policies are supplied. Credit, joint stochastic and empirical acceptance remain open.</p>}
    {verification.supplied_consumer_source_recomputed&&<p className="mt-2 text-xs text-paper-faint">Supplied installment histories, frozen original-yield income and reviewed credit intents were independently reconstructed. Acquisition, allowance and recovery policies remain explicit inputs; observed predictive accuracy requires separate validation.</p>}
    {verification.mixed_consumer_credit_only&&<p className="mt-2 text-xs text-paper-faint">Acquired installment owners and the dated backbook share one ordered daily cash and general ledger. Purchase cash, reviewed allowance and recovery costs are recognized once; complete path claims were checked through closing. Joint stochastic, saved-book and empirical gates remain open.</p>}
    {verification.supplied_card_source_recomputed&&<p className="mt-2 text-xs text-paper-faint">Supplied card payments, statements, original-yield income and reviewed credit intents were independently reconstructed. Complete interest and commitment tails are retained; fees, grace and new draws require separate policies.</p>}
    {verification.atomic_gross_basis_rights_recomputed&&<p className="mt-2 text-xs text-paper-faint">Original-yield income and reviewed basis realization were checked separately. Gross interest and its carrying adjustment remain visible through their retained tail. Fees, grace, new draws and broader credit policies require separate models.</p>}
    {verification.card_v2_rights_owned&&<p className="mt-2 text-xs text-paper-faint">Gross card interest and its carrying adjustment share the dated book's journal. Original-yield income and basis realization are reported separately. Closing capital requires supplied reviewed policies and the complete-tail date; modeled FTP requires separate adoption.</p>}
    {verification.independent_raw_cash_price_risk_verified&&<p className="mt-2 text-xs text-paper-faint">Every loan was explicitly bound to supplied named curves. Contractual cash, prices, paired risk and original OAS were independently checked before and after storage. This Pricing source does not supply Physical income, accounting journals or capital/FTP.</p>}
    {verification.analytical_market_indexed_only&&<p className="mt-2 text-xs text-paper-faint">Supplied collateral and named index curves were checked against conditional projected coupons and clean swap cashflows. Original reviewed OAS stays separate. Rates are pricing assumptions; observed fixings, calendar validation, joint dynamics, saved-book and accounting adoption require separate checks.</p>}
    {verification.analytical_mortgage_source_only&&<p className="mt-2 text-xs text-paper-faint">Original purchase cost and yield were checked against the complete original mortgage cashflows. Changed credit and payment histories retain that anchor. Default and prepayment settlement occurs at the supplied source boundaries; daily journal and capital adoption require separate checks.</p>}
    {verification.formal_hedge_relationships_replayed&&<p className="mt-2 text-xs text-paper-faint">Formal relationships, assessments, hedged-item basis and OCI mechanics were independently replayed from the stored derivative source under the explicit rule matrix and reviewed assessment band. Accounting-standard approval, observed effectiveness, portfolio-layer and saved-book adoption remain open.</p>}
    {verification.closing_capital_recomputed&&<p className="mt-2 text-xs text-paper-faint">The formal hedge journal was replayed, its closing balances mapped to reviewed bridge accounts and the capital ratios recomputed independently. Cash-flow-hedge AOCI enters eligible capital only under the supplied policy; an explicit interim report selects a reporting node before the complete tail. Regulatory eligibility, risk weights and FTP remain supplied or open.</p>}
    {verification.mixed_product_credit_only&&<p className="mt-2 text-xs text-paper-faint">Acquired installment and card owners share the dated book's ordered daily cash and general ledger. Purchase, allowance, interest and recovery rights were checked through complete closing. Joint stochastic, saved-book and empirical gates remain open.</p>}
  </CardBody></Card>}
  {paths.length>0&&<Card><CardHeader title="Dated results and state" sub={`${table?.total??0} rows in selected table`}/><CardBody>
    <label className="text-xs">Result table <select aria-label="Model result table" value={path} className="ml-2 rounded border border-line bg-surface p-1" onChange={e=>void page(e.target.value,0)}>{paths.map(p=><option key={p} value={p}>{p.replace('/model_result/','').replaceAll('_',' ')}</option>)}</select></label>
    {table&&<><DataTable rows={table.rows.map(row=>Object.fromEntries(table.columns.map((column,i)=>[column,typeof row[i]==='object'&&row[i]!==null?JSON.stringify(row[i]):row[i]])))}/>
      <div className="mt-2 flex gap-2"><Button disabled={!table.offset} onClick={()=>void page(path,Math.max(0,table.offset-100))}>Previous</Button><Button disabled={table.offset+table.rows.length>=table.total} onClick={()=>void page(path,table.offset+100)}>Next</Button>
        <a className="self-center text-xs underline" href={`/api/jobs/${encodeURIComponent(job)}/parquet?path=${encodeURIComponent(path)}`}>Download Parquet</a></div></>}
  </CardBody></Card>}</div>;
}
