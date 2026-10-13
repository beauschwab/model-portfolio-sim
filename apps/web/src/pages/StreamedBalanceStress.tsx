import { useEffect, useRef, useState } from 'react';
import { api, type Job, type ResultPage, type StressCapabilities, type StressManifest } from '../lib/api';
import { useEngineData } from '../lib/engine';
import { Badge, Button, Card, CardBody, CardHeader, Spinner } from '../components/ui';

const field = 'h-8 min-w-0 rounded border border-line bg-surface-2 px-2 text-sm text-paper';
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
  const [datedText, setDatedText] = useState('');
  const [creditText, setCreditText] = useState('');
  const [creditPathText, setCreditPathText] = useState('');
  const [graphText,setGraphText]=useState('');
  const [drivenText,setDrivenText]=useState('');
  const [depositGraphText,setDepositGraphText]=useState('');
  const [depositIncomeText,setDepositIncomeText]=useState('');
  const [depositRevisedText,setDepositRevisedText]=useState('');
  const [depositContinuationText,setDepositContinuationText]=useState('');
  const [depositPortfolioText,setDepositPortfolioText]=useState('');
  const [mortgageMixedText,setMortgageMixedText]=useState('');
  const [timing, setTiming] = useState<'monthly' | 'dated' | 'credit' | 'credit_path' | 'graph' | 'driven' | 'deposit_graph' | 'deposit_income' | 'deposit_revised' | 'deposit_continuation' | 'deposit_portfolio' | 'mortgage_mixed'>('monthly');
  const draft = timing === 'dated' ? datedText : timing === 'credit' ? creditText : timing === 'credit_path' ? creditPathText : timing === 'graph' ? graphText : timing === 'driven' ? drivenText : timing === 'deposit_graph' ? depositGraphText : timing === 'deposit_income' ? depositIncomeText : timing === 'deposit_revised' ? depositRevisedText : timing === 'deposit_continuation' ? depositContinuationText : timing === 'deposit_portfolio' ? depositPortfolioText : timing === 'mortgage_mixed' ? mortgageMixedText : text;
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
    Promise.all([api.stressCapabilities(), api.balanceStressExample(), api.datedStressExample(), api.creditStressExample(),api.creditPathStressExample(),api.modeledStressExample('graph-dated-ledger-1'),api.modeledStressExample('driven-credit-ledger-1'),api.modeledStressExample('deposit-graph-dated-ledger-1'),api.modeledStressExample('deposit-graph-income-ledger-1'),api.modeledStressExample('mortgage-mixed-accounting-1'),api.modeledStressExample('deposit-revised-income-ledger-1'),api.modeledStressExample('deposit-performing-income-ledger-1'),api.modeledStressExample('deposit-performing-portfolio-ledger-2')]).then(([caps, example, dated, credit,creditPath,graph,driven,depositGraph,depositIncome,mortgageMixed,depositRevised,depositContinuation,depositPortfolio]) => {
      if (!active) return;
      setCapabilities(caps); setText(JSON.stringify(example.specification, null, 2));
      setDatedText(JSON.stringify(dated.specification, null, 2));
      setCreditText(JSON.stringify(credit.specification, null, 2));
      setCreditPathText(JSON.stringify(creditPath.specification,null,2));
      setGraphText(JSON.stringify(graph.specification,null,2));
      setDrivenText(JSON.stringify(driven.specification,null,2));
      setDepositGraphText(JSON.stringify(depositGraph.specification,null,2));
      setDepositIncomeText(JSON.stringify(depositIncome.specification,null,2));
      setMortgageMixedText(JSON.stringify(mortgageMixed.specification,null,2));
      setDepositRevisedText(JSON.stringify(depositRevised.specification,null,2));
      setDepositContinuationText(JSON.stringify(depositContinuation.specification,null,2));
      setDepositPortfolioText(JSON.stringify(depositPortfolio.specification,null,2));
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
        if (!['streamed_balance_stress', 'dated_balance_stress', 'credit_balance_stress','credit_path_balance_stress','modeled_balance_stress'].includes(status.kind)) throw new Error('Select a partitioned balance-sheet run.');
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
      const specification = JSON.parse(draft) as Record<string, unknown>;
      const submitted = timing === 'dated'
        ? await api.runDatedStress(specification, revision, large)
        : timing === 'credit' ? await api.runCreditStress(specification, revision, large)
        : timing === 'credit_path' ? await api.runCreditPathStress(specification,revision,large)
        : timing === 'graph' || timing === 'driven' || timing === 'deposit_graph' || timing === 'deposit_income' || timing === 'deposit_revised' || timing === 'deposit_continuation' || timing === 'deposit_portfolio' || timing === 'mortgage_mixed' ? await api.runModeledStress(specification,revision,large)
        : await api.runStreamedStress(specification, revision, backend, large);
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

  const mortgageVerified = manifest?.execution?.validation.native_mortgage_source_recomputed === true
    && manifest.execution.validation.single_daily_gl_verified === true
    && manifest.execution.validation.ordered_mixed_journal_recomputed === true
    && manifest.execution.validation.pending_memo_phase_verified === true
    && manifest.execution.validation.complete_unweighted_tail_verified === true;
  const portfolioDepositVerified=manifest?.execution?.validation.physical_deposit_portfolio_sources_recomputed === true
    && manifest?.execution?.validation.original_yield_continuations_recomputed === true
    && manifest?.execution?.validation.full_unweighted_gross_origins_verified === true
    && manifest?.execution?.validation.ordered_pooled_same_sim_journal_replayed === true
    && manifest?.execution?.validation.persisted_partitions_replayed === true;
  const continuationDepositVerified = manifest?.execution?.validation.physical_deposit_source_principal_recomputed === true
    && manifest?.execution?.validation.original_yield_continuation_recomputed === true
    && manifest?.execution?.validation.gross_origin_rights_reconciled === true
    && manifest?.execution?.validation.ordered_same_sim_journal_replayed === true;
  const revisedDepositVerified = manifest?.execution?.validation.physical_deposit_source_recomputed === true
    && manifest.execution.validation.original_yield_revision_recomputed === true
    && manifest.execution.validation.ordered_deposit_journal_recomputed === true
    && manifest.execution.validation.complete_gross_cash_origin_rights_verified === true;
  return <div className="space-y-3">
    <Card><CardHeader title="Partitioned balance-sheet simulation" sub="Durable runs with a complete saved journal" right={<Badge tone="warning">Research model</Badge>} />
      <CardBody className="space-y-3">
        <p className="text-sm text-paper-dim">Run explicit balance-sheet assumptions in Rust. Journal checks finish before results are published. Browse 100 rows at a time and download individual Parquet files.</p>
        {capabilities && !capabilities.durable && <p role="status" className="text-sm text-paper-dim">Partitioned runs require the durable API and a separate calculation worker.</p>}
        <div className="flex flex-wrap items-center gap-3">
          <label className="text-sm">Engine <select aria-label="Partitioned engine" className={field} value={backend} disabled>
            <option value="rust">Rust{capabilities && !capabilities.rust ? ' — unavailable' : ''}</option>
          </select></label>
          <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={large} disabled={busy || !capabilities?.large_book} onChange={e => setLarge(e.target.checked)} />Large book (up to 60,000 instruments)</label>
        </div>
        <p className="text-xs text-paper-faint">{capabilities?.large_book ? 'Large-book capacity is enabled for this workspace.' : 'Standard capacity: up to 2,000 instruments. Large-book admission is disabled on this deployment.'}</p>
        <label className="flex items-center gap-2 text-xs">Cashflow timing
          <select aria-label="Cashflow timing" className={field} disabled={busy} value={timing} onChange={e => setTiming(e.target.value as typeof timing)}>
            <option value="monthly">Monthly approximation</option>
            <option value="dated">Supplied actual-date schedules</option>
            <option value="credit">Daily loan credit and reviewed accounting</option>
            <option value="credit_path">Realized credit paths and reviewed accounting</option>
            <option value="graph">Consumer graph cashflows and dated recognition</option>
            <option value="driven">Borrower and macro drivers with realized credit</option>
            <option value="deposit_graph">Deposit runoff and daily interest expense</option>
            <option value="deposit_income">Deposit original yield and carrying value</option>
            <option value="deposit_revised">Deposit revised expectations at original yield</option>
            <option value="deposit_portfolio">Multiple performing deposits and explicit named paths</option>
            <option value="deposit_continuation">Deposit observed prefix and later revision</option>
            <option value="mortgage_mixed">Mortgage research expected accounting</option>
          </select>
        </label>
        {timing === 'dated' && <p className="text-xs text-paper-faint">Use complete source schedules and explicit recognition policies. The example is synthetic contractual cashflow; credit and behavioral changes must be supplied by their source model.</p>}
        {timing === 'credit' && <p className="text-xs text-paper-faint">Use explicit physical credit transitions, loan boundaries and reviewed accounting policies. The synthetic example demonstrates expected daily state transitions and recovery claims. Combined mixed-book execution and empirical calibration remain separate acceptance gates.</p>}
        {timing === 'credit_path' && <p className="text-xs text-paper-faint">Simulate physical borrower paths with explicit draws, repayments, default and complete recovery tails. Reviewed accounting policies determine recognition. Every saved path and posting is independently recomputed before publication; the example uses synthetic assumptions.</p>}
        {timing === 'graph' && <p className="text-xs text-paper-faint">Generate physical auto/personal cashflows from explicit histories and reviewed par-note recognition. Raw source economics and saved postings are verified. Default, delinquency, nonaccrual and fees require their own accounting route and are rejected here.</p>}
        {timing === 'driven' && <p className="text-xs text-paper-faint">Project a frozen physical credit model onto supplied borrower and macro drivers, then recognize realized paths under reviewed accounting policies. Synthetic models require explicit research use; adoption requires model eligibility and reviewed inputs.</p>}
        {timing === 'deposit_graph' && <p className="text-xs text-paper-faint">Replay explicit deposit runoff, daily simple-interest expense and complete cash rights at par. Saved sources and postings are independently verified. Migration, new inflows and non-par effective-yield policies require separate adoption.</p>}
        {timing === 'mortgage_mixed' && <p className="text-xs text-paper-faint">Supplied Physical whole-loan histories share one dated journal with an explicit backbook. Reviewed pending-default/prepay recognition, original EIR and signed realization remain distinct. Limits: 64 days, 10,000 monetary units per declared gross lane, 8 million work units and 32 MiB. Complete unweighted tails and explicit source/run reuse are required. Research policy only; no accounting approval, saved-book adoption, capital or FTP eligibility.</p>}
        {timing === 'deposit_income' && <p className="text-xs text-paper-faint">Recognize a deposit liability from its explicit purchase quote and reviewed original effective yield. Contractual expense, cash and basis amortization stay separate. Complete expected cash must reconcile to the original carrying value; changed expectations, migration, new inflows and seasoned acquisitions require another reviewed policy.</p>}
        {timing === 'deposit_portfolio' && <p className="text-xs text-paper-faint">Explicit owners and complete baseline/named physical paths share one account journal. Original purchase cost, yield, observed prefix and separate revisions are retained. Reviewed 90-day single and two 40-day sources fit; 365 days is a ceiling, not guaranteed admission. All sources and serialization must fit joined 8 million work units and 32 MiB. No migration, inflows, fees, negative coupon/yield, unknown prefix, accounting approval, saved/cohort adoption, capital or FTP; full portfolio-scale and empirical gates remain open.</p>}
        {timing === 'deposit_continuation' && <p className="text-xs text-paper-faint">Complete supplied purchase, observed prefix and revised physical histories retain original cost and yield. Native principal atoms, cash, earned origins and separate reviewed revision share one journal. No migration, inflows, fees, unknown prefix, negative coupon/yield, accounting approval, saved-book adoption, capital or FTP. At most 64 source days; complete named runs must separately fit 8 million work units and 32 MiB. Runtime monetary components are limited to 10,000; this is not an aggregate scale acceptance.</p>}
        {timing === 'deposit_revised' && <p className="text-xs text-paper-faint">Purchase-asof physical deposit histories retain the original quote, cost and effective yield. An explicit review identifies the separate asof basis revision; actual earned interest and cash origins share one journal. Complete original/revised tails are required. Limits: 64 days, 10,000 per monetary component, 8 million work units and 32 MiB. No migration, inflows, fees, seasoned ownership, negative coupon/yield, accounting approval, saved-book adoption, capital or FTP eligibility.</p>}
        <details><summary className="cursor-pointer text-xs">Edit partitioned specification (JSON)</summary>
          <label htmlFor="partitioned-specification" className="mt-2 block text-xs">Explicit balance-sheet specification</label>
          <textarea id="partitioned-specification" className="mt-1 h-80 w-full rounded border border-line bg-surface-2 p-2 font-mono text-xs" disabled={busy} value={draft} onChange={e => timing === 'dated' ? setDatedText(e.target.value) : timing === 'credit' ? setCreditText(e.target.value) : timing === 'credit_path' ? setCreditPathText(e.target.value) : timing === 'graph' ? setGraphText(e.target.value) : timing === 'driven' ? setDrivenText(e.target.value) : timing === 'deposit_graph' ? setDepositGraphText(e.target.value) : timing === 'deposit_income' ? setDepositIncomeText(e.target.value) : timing === 'deposit_revised' ? setDepositRevisedText(e.target.value) : timing === 'deposit_continuation' ? setDepositContinuationText(e.target.value) : timing === 'deposit_portfolio' ? setDepositPortfolioText(e.target.value) : timing === 'mortgage_mixed' ? setMortgageMixedText(e.target.value) : setText(e.target.value)} spellCheck={false} />
        </details>
        <div className="flex flex-wrap items-center gap-2">
          <Button disabled={busy || downloading || !capabilities?.durable || !capabilities?.rust || !draft.trim()} onClick={run}>Run partitioned simulation</Button>
          {job && busy && <Button variant="secondary" disabled={pending} onClick={cancel}>Cancel run</Button>}
          {busy && <span role="status" className="flex items-center gap-2 text-xs"><Spinner />{job?.progress?.stage ?? job?.status ?? 'Submitting'}</span>}
        </div>
        <form className="flex flex-wrap items-center gap-2" onSubmit={e => { e.preventDefault(); if (!busy && !downloading && openId.trim()) openRun(openId.trim()); }}>
          <label htmlFor="partitioned-run-id" className="text-sm">Saved run ID</label>
          <input id="partitioned-run-id" className={`${field} flex-1`} value={openId} onChange={e => setOpenId(e.target.value)} disabled={busy} />
          <Button variant="secondary" disabled={busy || downloading || !openId.trim() || !capabilities?.durable}>Open run</Button>
        </form>
        {job && <p className="break-all text-sm text-paper-faint">Run {job.id} · {job.status} · input revision {job.revision}</p>}
        <p className="text-sm text-paper-faint">Runs continue when you leave this panel. The latest run is remembered for this workspace in this browser tab; you can also reopen it by ID.</p>
        {error && <p role="alert" className="break-words text-sm text-danger">{error}</p>}
      </CardBody>
    </Card>
    {manifest && descriptor && <Card><CardHeader title="Persisted run results" sub={`Run ${manifest.id} · saved revision ${manifest.revision} · current revision ${revision}`} />
      <CardBody className="space-y-3">
        <p className="text-sm text-paper-faint">These results belong to the saved run, independent of edits to the draft above.</p>
        <div className="flex flex-wrap gap-2">
          <Badge>{manifest.execution?.backend ?? 'Unknown'} engine</Badge>
          <Badge tone={manifest.execution?.validation.journal_replayed || mortgageVerified || revisedDepositVerified || continuationDepositVerified || portfolioDepositVerified ? 'up' : 'warning'}>{manifest.execution?.validation.journal_replayed || mortgageVerified || revisedDepositVerified || continuationDepositVerified || portfolioDepositVerified ? 'Saved journal verified' : 'Verification unavailable'}</Badge>
          {portfolioDepositVerified && <Badge tone="up">Complete owner sources and pooled gross-right journal verified</Badge>}
          {portfolioDepositVerified && <Badge>Reviewed research policy; capital and FTP unavailable</Badge>}
          {continuationDepositVerified && <Badge tone="up">Original anchors, native principal and gross cash origins verified</Badge>}
          {continuationDepositVerified && <Badge>Reviewed research policy; capital and FTP unavailable</Badge>}
          {revisedDepositVerified && <Badge tone="up">Original yield revision and gross cash origins verified</Badge>}
          {revisedDepositVerified && <Badge>Reviewed research policy; capital and FTP unavailable</Badge>}
          {mortgageVerified && <Badge tone="up">Mortgage source economics verified</Badge>}
          {mortgageVerified && <Badge tone="up">Pending rights and memo phase verified</Badge>}
          {mortgageVerified && <Badge>Research journal only; capital and FTP unavailable</Badge>}
          {manifest.execution?.validation.dated_source_reconciled && <Badge tone="up">Dated source reconciled</Badge>}
          {manifest.execution?.validation.credit_source_reconciled && <Badge tone="up">Credit source reconciled</Badge>}
          {manifest.execution?.validation.credit_path_source_reconciled && <Badge tone="up">Realized credit paths reconciled</Badge>}
          {manifest.execution?.validation.physical_consumer_source_recomputed && <Badge tone="up">Consumer source economics verified</Badge>}
          {manifest.execution?.validation.physical_deposit_source_recomputed && <Badge tone="up">Deposit source economics verified</Badge>}
          {manifest.execution?.validation.original_deposit_income_recomputed && <Badge tone="up">Original deposit yield and basis verified</Badge>}
          {manifest.execution?.validation.driven_credit_source_recomputed && <Badge tone="up">Credit model and drivers verified</Badge>}
          <Badge tone={manifest.execution?.validation.dynamic_validated ? 'up' : 'warning'}>{manifest.execution?.validation.dynamic_validated === undefined ? 'Daily regulatory limits not evaluated' : manifest.execution.validation.dynamic_validated ? 'Daily research limits passed' : 'Review daily limit breaches'}</Badge>
        </div>
        <label className="flex flex-wrap items-center gap-2 text-sm">Result table
          <select aria-label="Partitioned result table" className={field} value={table} disabled={downloading} onChange={e => { setTable(e.target.value); setOffset(0); setPartition(0); }}>
            {Object.keys(manifest.tables).map(name => <option key={name} value={name}>{name.slice(1).replaceAll('_', ' ')}</option>)}
          </select>
        </label>
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span>{number(descriptor.rows)} rows · {number(descriptor.parts.length)} partitions</span>
          <Button variant="secondary" disabled={!offset} onClick={() => setOffset(o => Math.max(0, o - 100))}>Previous page</Button>
          <span>Page {Math.floor(offset / 100) + 1} of {Math.max(1, Math.ceil(descriptor.rows / 100))}</span>
          <Button variant="secondary" disabled={offset + 100 >= descriptor.rows} onClick={() => setOffset(o => o + 100)}>Next page</Button>
        </div>
        {pageError ? <div><p role="alert" className="text-sm text-danger">{pageError}</p><Button variant="secondary" onClick={() => setPageRetry(r => r + 1)}>Retry table</Button></div>
          : !visible ? <p role="status" className="text-sm">Loading table page…</p>
          : <div className="max-h-[32rem] overflow-auto"><table aria-label="Partitioned results" className="w-full text-left text-xs num">
            <thead><tr>{visible.columns.map(c => <th key={c} className="whitespace-nowrap border-b border-line p-2">{c.replaceAll('_', ' ')}</th>)}</tr></thead>
            <tbody>{visible.rows.map((row, i) => <tr key={offset + i}>{row.map((value, j) => <td key={visible.columns[j]} className="whitespace-nowrap border-b border-line/50 p-2">{display(value)}</td>)}</tr>)}</tbody>
          </table>{!visible.rows.length && <p className="p-3 text-sm text-paper-faint">No rows in this table.</p>}</div>}
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <label>Parquet partition <input aria-label="Parquet partition" className={`${field} w-24`} type="number" min={0} max={Math.max(0, descriptor.parts.length - 1)} step={1} value={partition} disabled={downloading || !descriptor.parts.length} onChange={e => setPartition(Number(e.target.value))} /></label>
          <Button variant="secondary" disabled={downloading || !Number.isInteger(partition) || partition < 0 || partition >= descriptor.parts.length} onClick={download}>{downloading ? 'Downloading…' : 'Download partition'}</Button>
        </div>
        <p className="text-sm text-paper-faint">Research assumptions remain uncalibrated. Journal reconciliation does not establish regulatory compliance. Reverse-stress journals are not included in persisted replay.</p>
      </CardBody>
    </Card>}
  </div>;
}
