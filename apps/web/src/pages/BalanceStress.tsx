import { useEffect, useMemo, useRef, useState } from 'react';
import { Legend, Line, LineChart, CartesianGrid, XAxis, YAxis, Tooltip, ResponsiveContainer, ReferenceLine } from 'recharts';
import { chartAxis, chartGrid, chartLegend, chartTip } from '../components/charts';
import { api, rowsOf, type BalanceStressResult, type Job, type Row } from '../lib/api';
import { useEngineData } from '../lib/engine';
import { Badge, Button, Card, CardBody, CardHeader, Spinner } from '../components/ui';
import StreamedBalanceStress from './StreamedBalanceStress';

const selectClass = 'h-8 rounded border border-line bg-surface-2 px-2 text-sm text-paper';
const display = (v: unknown) => v == null ? '—' : typeof v === 'number' ? v.toLocaleString(undefined, { maximumFractionDigits: 4 }) : String(v);

function ResultsTable({ rows, columns, name }: { rows: Row[]; columns: string[]; name: string }) {
  const [page, setPage] = useState(0);
  useEffect(() => setPage(0), [rows]);
  function download() {
    const csv = [columns, ...rows.map(r => columns.map(c => r[c] ?? ''))]
      .map(row => row.map(v => {
        const cell = typeof v === 'string' && /^[=+@-]/.test(v) ? `'${v}` : String(v);
        return `"${cell.replaceAll('"', '""')}"`;
      }).join(',')).join('\r\n');
    const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
    const link = document.createElement('a'); link.href = url; link.download = `balance-stress-${name}.csv`; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 0);
  }
  return <div className="space-y-2">
    <div className="flex flex-wrap items-center gap-2 text-sm text-paper-faint"><span>{rows.length} rows</span>
      <Button variant="secondary" onClick={download} disabled={!rows.length}>Export {name}</Button>
      <Button variant="secondary" disabled={page === 0} onClick={() => setPage(p => p - 1)}>Previous</Button>
      <span>Page {page + 1} of {Math.max(1, Math.ceil(rows.length / 50))}</span>
      <Button variant="secondary" disabled={(page + 1) * 50 >= rows.length} onClick={() => setPage(p => p + 1)}>Next</Button></div>
    <div className="overflow-x-auto"><table aria-label={name} className="w-full text-left text-xs num">
      <thead><tr>{columns.map(c => <th key={c} className="whitespace-nowrap border-b border-line p-2 font-medium text-paper-faint">{c.replaceAll('_', ' ')}</th>)}</tr></thead>
      <tbody>{rows.slice(page * 50, (page + 1) * 50).map((r, i) => <tr key={page * 50 + i}>{columns.map(c => <td key={c} className="whitespace-nowrap border-b border-line/50 p-2">{display(r[c])}</td>)}</tr>)}</tbody>
    </table>{!rows.length && <p className="p-3 text-sm text-paper-faint">No events for this selection.</p>}</div>
  </div>;
}

export default function BalanceStress() {
  const [mode, setMode] = useState('standard');
  return <div className="space-y-3">
    <label className="flex flex-wrap items-center gap-2 text-sm">Simulation workflow
      <select aria-label="Simulation workflow" className={selectClass} value={mode} onChange={e => setMode(e.target.value)}>
        <option value="standard">Standard / saved-book analysis</option>
        <option value="partitioned">Partitioned simulation</option>
      </select>
    </label>
    {mode === 'standard' ? <StandardBalanceStress /> : <StreamedBalanceStress />}
  </div>;
}

function StandardBalanceStress() {
  const { revision } = useEngineData();
  const [text, setText] = useState('');
  const [savedBook, setSavedBook] = useState(false);
  const [job, setJob] = useState<Job | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<{ data: BalanceStressResult; input: string } | null>(null);
  const [scenario, setScenario] = useState('baseline');
  const [account, setAccount] = useState('');
  const [view, setView] = useState<'attribution' | 'ledger' | 'journal' | 'trial_balance' | 'funding_claims' | 'actions' | 'breaches' | 'exposures' | 'reverse_grid'>('attribution');
  const generation = useRef(0);
  const submittedInput = useRef('');
  useEffect(() => {
    let active = true;
    api.balanceStressExample().then(r => { if (active) setText(JSON.stringify(r.specification, null, 2)); })
      .catch(e => { if (active) setError(String(e)); });
    return () => { active = false; generation.current++; };
  }, []);
  const busy = submitting || job?.status === 'queued' || job?.status === 'running';
  const jid = job?.id;
  useEffect(() => {
    if (!jid) return;
    let active = true;
    const token = generation.current;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const status = await api.job(jid!);
        if (!active || token !== generation.current) return;
        setJob(status);
        if (status.status === 'done') {
          const data = await api.jobResult(jid!) as BalanceStressResult;
          if (active && token === generation.current) { setResult({ data, input: submittedInput.current }); setAccount(String(rowsOf(data.summary)[0]?.account ?? '')); }
        } else if (status.status === 'error') setError(status.detail ?? 'Stress run failed.');
        else timer = setTimeout(poll, 700);
      } catch (e) { if (active && token === generation.current) { setError(String(e)); setJob(null); } }
    }
    void poll();
    return () => { active = false; clearTimeout(timer); };
  }, [jid]);

  async function run() {
    const token = ++generation.current;
    setError(null); setResult(null); setJob(null); setSubmitting(true);
    try {
      const specification = JSON.parse(text) as Record<string, unknown>;
      const submitted = await (savedBook ? api.runSavedBalanceStress(specification, revision) : api.runBalanceStress(specification, revision));
      if (token === generation.current) { submittedInput.current = text; setJob(submitted); }
    } catch (e) { if (token === generation.current) setError(String(e)); }
    finally { if (token === generation.current) setSubmitting(false); }
  }
  async function loadMapping() {
    const token = ++generation.current;
    setError(null); setSubmitting(true); setResult(null); setJob(null);
    try {
      const [inventory, example] = await Promise.all([api.balanceStressInventory(), api.balanceStressExample()]);
      if (token !== generation.current) return;
      if (inventory.revision !== revision) throw new Error('Saved inputs changed. Reload before mapping.');
      setSavedBook(true);
      setText(JSON.stringify({ specification: { ...example.specification, accounts: [], positions: [], policies: [], netting_sets: [], reverse_severities: [] },
        amount_scale: 1, position_mapping: Object.fromEntries(inventory.instruments.map(r => [r.source_id,
          Object.fromEntries(inventory.required_mapping.map(k => [k, null]))])), allocation: [], template_mapping: {}, observations: [] }, null, 2));
    } catch (e) { if (token === generation.current) setError(String(e)); }
    finally { if (token === generation.current) setSubmitting(false); }
  }
  const shown = result && result.input === text && result.data.revision === revision ? result.data : null;
  const summary = useMemo(() => shown ? rowsOf(shown.summary) : [], [shown]);
  const scenarios = [...new Set(summary.map(r => String(r.scenario)))];
  const accounts = [...new Set(summary.map(r => String(r.account)))];
  const selectedScenario = scenarios.includes(scenario) ? scenario : scenarios[0] ?? 'baseline';
  const selectedAccount = accounts.includes(account) ? account : accounts[0] ?? '';
  const matches = (r: Row) => r.scenario === selectedScenario && r.account === selectedAccount;
  const path = useMemo(() => shown ? rowsOf(shown.path).filter(r => r.scenario === selectedScenario && r.account === selectedAccount) : [], [shown, selectedScenario, selectedAccount]);
  const table = useMemo(() => shown ? rowsOf(shown[view]).filter(r => r.scenario === selectedScenario && (r.account === selectedAccount || (view === 'actions' && r.destination === selectedAccount))) : [], [shown, view, selectedScenario, selectedAccount]);
  const selected = summary.find(matches);
  const columns: Record<typeof view, string[]> = {
    attribution: ['event', 'cash_delta', 'earnings_delta', 'aoci_delta'],
    ledger: ['day', 'event', 'cash', 'earnings', 'aoci', 'memo_amount'],
    journal: ['transaction_id', 'day', 'event', 'gl_account', 'instrument_id', 'debit', 'credit'],
    trial_balance: ['gl_account', 'instrument_id', 'balance'],
    funding_claims: ['claim_id', 'position', 'face', 'balance', 'rate', 'due'],
    actions: ['day', 'policy', 'account', 'destination', 'kind', 'status', 'amount', 'reason'],
    breaches: ['day', 'metric'],
    exposures: ['day', 'position', 'kind', 'balance', 'allowance', 'watch_fraction', 'commitment', 'encumbered_fraction'],
    reverse_grid: ['severity', 'breached', 'first_cash_breach_day', 'first_capital_breach_day', 'first_leverage_breach_day', 'first_lcr_breach_day', 'first_nsfr_breach_day', 'first_htm_breach_day', 'peak_cash_shortfall'],
  };
  return <div className="space-y-3">
    <Card><CardHeader title="Balance-sheet Stress" sub="Daily liquidity, credit and capital with explicit management policies" right={<Badge tone="warning">Research prototype</Badge>} />
      <CardBody className="space-y-3">
        <p className="text-sm text-paper-dim">Start with a balanced synthetic bank and dealer in USD millions, or map the saved books explicitly. Saved-book runs use the product engines’ monthly expected cashflows and can replay candidate allocations through daily accounting and limits.</p>
        <Button variant="secondary" disabled={busy} onClick={loadMapping}>Load saved-book mapping</Button>
        {savedBook && <p className="text-sm text-paper-faint">Complete every null mapping, supply balanced opening accounts and set the amount conversion. Paste a solver allocation into allocation and supply template_mapping to replay it. Unmapped hedge trades are rejected. This replaces the synthetic example input.</p>}
        <details><summary className="cursor-pointer text-sm text-paper-dim">Edit stress specification (JSON)</summary>
          <label className="mt-2 block text-sm text-paper-faint" htmlFor="balance-stress-specification">Accounts, cohorts, policies, scenarios and reverse-stress severities</label>
          <textarea id="balance-stress-specification" value={text} onChange={e => setText(e.target.value)} disabled={busy} spellCheck={false} className="mt-1 h-80 w-full rounded border border-line bg-surface-2 p-2 font-mono text-sm text-paper" />
          <p className="text-sm text-paper-faint">Rates and weights are decimals; amounts use one consistent unit per currency. Opening assets must equal liabilities plus supplied equity. Unsupported fields are rejected.</p>
        </details>
        <div className="flex flex-wrap items-center gap-2"><Button onClick={run} disabled={busy || !text}>Run balance-sheet stress</Button>
          {busy && <span role="status" className="flex items-center gap-2 text-sm"><Spinner />{job?.progress?.stage ?? job?.status ?? 'Submitting'} {job?.progress?.pct ?? 0}%</span>}
          {job && <span className="text-2xs text-paper-faint">Run {job.id} · revision {job.revision}</span>}</div>
        {error && <p role="alert" className="break-words text-sm text-danger">{error}</p>}
        {result && !shown && <p role="status" className="text-sm text-paper-faint">Inputs changed; run again to see matching results.</p>}
      </CardBody></Card>
    {shown && <>
      <Card><CardHeader title="Scenario comparison" sub="Cash is local to each entity and currency; no automatic transfer of group surplus." /><CardBody>
        <ResultsTable rows={summary} name="comparison" columns={['scenario', 'account', 'currency', 'minimum_cash', 'peak_cash_shortfall', 'final_equity', 'first_cash_breach_day', 'first_capital_breach_day', 'max_reconciliation_error']} />
      </CardBody></Card>
      <div className="flex flex-wrap items-center gap-3">
        <Badge tone={shown.validation.dynamic_validated ? 'up' : 'danger'}>{shown.validation.dynamic_validated ? 'Daily research limits passed' : `${shown.validation.breach_count} daily limit breaches`}</Badge>
        <label className="text-sm">Scenario <select aria-label="Stress scenario" className={selectClass} value={selectedScenario} onChange={e => setScenario(e.target.value)}>{scenarios.map(s => <option key={s}>{s}</option>)}</select></label>
        <label className="text-sm">Account <select aria-label="Stress account" className={selectClass} value={selectedAccount} onChange={e => setAccount(e.target.value)}>{accounts.map(a => <option key={a}>{a}</option>)}</select></label>
        <Badge tone={selected?.first_cash_breach_day == null ? 'up' : 'danger'}>{selected?.first_cash_breach_day == null ? 'No cash breach in horizon' : `First cash breach: day ${selected.first_cash_breach_day}`}</Badge>
      </div>
      <Card><CardHeader title="Cash timeline" sub="Daily through day 30, then every 30 days. Negative cash represents unfunded obligations." /><CardBody>
        <div className="h-64 min-w-0" role="img" aria-label="Cash balance and required cash floor by simulation day"><ResponsiveContainer width="100%" height="100%">
          <LineChart data={path}><CartesianGrid {...chartGrid} /><XAxis dataKey="day" type="number" domain={['dataMin', 'dataMax']} {...chartAxis} /><YAxis width={65} {...chartAxis} /><Tooltip {...chartTip} /><Legend {...chartLegend} /><ReferenceLine y={0} stroke="var(--border-strong)" />
            <Line name="Cash" dataKey="cash" stroke="var(--viz-1)" dot={false} isAnimationActive={false} /><Line name="Cash floor" dataKey="cash_floor" stroke="var(--warning-500)" dot={false} strokeDasharray="4 4" isAnimationActive={false} />
          </LineChart></ResponsiveContainer></div>
        <details><summary className="cursor-pointer text-sm text-paper-dim">Balance-sheet and capital timeline</summary>
          <ResultsTable rows={path} name="timeline" columns={['day', 'cash', 'restricted_cash', 'assets', 'liabilities', 'equity', 'cet1', 'rwa', 'cet1_ratio', 'leverage_ratio', 'usable_collateral', 'lcr_proxy', 'nsfr_proxy', 'lcr_headroom', 'nsfr_headroom', 'htm_headroom']} /></details>
      </CardBody></Card>
      <Card><CardHeader title="Explain the result" sub="Attribution is the additive event difference from baseline; it is not a causal factor decomposition." /><CardBody className="space-y-3">
        <label className="text-sm">View <select aria-label="Stress detail view" className={selectClass} value={view} onChange={e => setView(e.target.value as typeof view)}>{Object.keys(columns).map(v => <option key={v} value={v}>{v.replaceAll('_', ' ')}</option>)}</select></label>
        <ResultsTable rows={table} columns={columns[view]} name={view} />
      </CardBody></Card>
      <Card><CardHeader title="Model boundaries" sub={`${shown.model_version} · explicit assumptions, not calibrated bank risk`} /><CardBody><ul className="list-disc space-y-1 pl-4 text-sm text-paper-faint">{shown.warnings.map(w => <li key={w}>{w}</li>)}</ul></CardBody></Card>
    </>}
  </div>;
}
