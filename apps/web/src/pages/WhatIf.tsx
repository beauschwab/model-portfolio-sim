import { useEffect, useMemo, useState } from 'react';
import { api, fmt$, rowsOf, type BookName, type Row, type Table, type PricingOptions } from '../lib/api';
import { useEngineData } from '../lib/engine';
import { Badge, Button, Card, CardBody, CardHeader, Input, Spinner } from '../components/ui';

type PricingBook = Exclude<BookName, 'mm'>;
type Patches = Partial<Record<BookName, Record<string, Record<string, number>>>>;
type Catalog = Awaited<ReturnType<typeof api.pricingAssumptions>>;
type Valuation = { positions: Record<string, Table>; scope_net_value: number;
  graph: Record<string, { computed: number; reused: number }>; nii?: { total: number };
  kpis?: { eve: { dv01_net_$: number }; lcr: { lcr_pct: number }; nsfr: { nsfr_pct: number }; capital: { cet1_path: { cet1_ratio_pct: number }[] } } };
type Comparison = { baseline: Valuation; revised: Valuation; comparison: Record<string, Table>;
  calibration_mode: 'hold' | 'recalibrate'; revision: number; net_value_change: number };
const BOOKS: PricingBook[] = ['mbs', 'loans', 'debt', 'deposits', 'cds'];
const LABELS: Record<PricingBook, string> = { mbs: 'Mortgages', loans: 'Loans', debt: 'Debt', deposits: 'Deposits', cds: 'CDs' };
const FIRST: Record<PricingBook, string> = { mbs: 'wac', loans: 'coupon_or_spread', debt: 'coupon_or_spread', deposits: 'rate_paid', cds: 'rate' };
const FIELDS: Record<string, [string, number]> = {
  wac: ['Borrower coupon (%)', 100], net_coupon: ['Investor coupon (%)', 100],
  coupon_or_spread: ['Coupon / floating spread (%)', 100], rate: ['Contract rate (%)', 100],
  rate_paid: ['Rate paid (%)', 100], cap: ['Coupon cap (%)', 100], floor: ['Coupon floor (%)', 100],
  call_threshold: ['Call threshold (%)', 100], penalty_months: ['Withdrawal penalty (months)', 1],
  ew_mult: ['Withdrawal multiplier', 1], attrition_base: ['Monthly base attrition (%)', 100],
  attrition_amp: ['Attrition response (%)', 100], attrition_slope: ['Attrition slope', 1],
  attrition_gap: ['Attrition rate gap (%)', 100], svc_cost: ['Annual servicing cost (%)', 100],
  age_months: ['Account age (months)', 1], avg_account_size: ['Average account size ($)', 1],
  wam: ['Remaining term (months)', 1], age: ['Pool age (months)', 1], oltv: ['Original loan-to-value (%)', 100],
  factor: ['Pool factor (%)', 100], fico: ['FICO score', 1], avg_loan_size: ['Average loan size ($)', 1],
  hpi_orig_ratio: ['Home price ratio since origination', 1], prepay_mult: ['Prepay speed ×', 1],
};
const selectClass = 'h-8 min-w-0 w-full rounded-md border border-line bg-surface-2 px-2 text-sm text-paper focus:outline-brand';
const money = (value: unknown) => typeof value === 'number' && Number.isFinite(value) ? fmt$(Math.abs(value) < .5 ? 0 : value) : '—';
const percent = (value: unknown) => typeof value === 'number' && Number.isFinite(value) ? (Math.abs(value) < .005 ? 0 : value).toFixed(2) : '—';

export default function WhatIf() {
  const { run, revision, scenarios, settings } = useEngineData();
  const [book, setBook] = useState<PricingBook>('loans');
  const [frames, setFrames] = useState<Partial<Record<PricingBook, Row[]>>>({});
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [loadedRevision, setLoadedRevision] = useState<number | null>(null);
  const [instrument, setInstrument] = useState('');
  const [field, setField] = useState('coupon_or_spread');
  const [value, setValue] = useState('');
  const [spread, setSpread] = useState('0');
  const [patches, setPatches] = useState<Patches>({});
  const [spreads, setSpreads] = useState<PricingOptions['spread_overrides_bp']>({});
  const [fullScope, setFullScope] = useState(false);
  const [analytics, setAnalytics] = useState(true);
  const [scenario, setScenario] = useState('');
  const [mode, setMode] = useState<'hold' | 'recalibrate'>('hold');
  const [enabled, setEnabled] = useState(false);
  const [nonce, setNonce] = useState(0);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<{ key: string; data: Comparison } | null>(null);
  const [page, setPage] = useState(0);

  useEffect(() => {
    let active = true;
    setEnabled(false); setLoadedRevision(null); setResult(null); setPatches({}); setSpreads({}); setError(null);
    Promise.all([api.pricingAssumptions(), ...BOOKS.map(b => api.book(b))]).then(([spec, ...tables]) => {
      if (!active) return;
      setCatalog(spec);
      setFrames(Object.fromEntries(BOOKS.map((b, i) => [b, rowsOf(tables[i])])));
      setLoadedRevision(revision);
    }).catch(e => { if (active) setError(String(e)); });
    return () => { active = false; };
  }, [revision]);

  const positions = frames[book] ?? [];
  const idOf = (r: Row) => String(r[book === 'mbs' ? 'cusip' : 'id']);
  const selectedId = positions.some(r => idOf(r) === instrument) ? instrument : positions.length ? idOf(positions[0]) : '';
  const selected = positions.find(r => idOf(r) === selectedId);
  const [label, scale] = FIELDS[field] ?? [field, 1];
  const current = selected?.[field] ?? catalog?.defaults[field];
  const bounds = catalog?.fields[book]?.[field];
  const invalid = value !== '' && (!Number.isFinite(Number(value)) || !bounds || Number(value)/scale < bounds[0] || Number(value)/scale > bounds[1]);
  const invalidSpread = spread.trim() === '' || !Number.isFinite(Number(spread)) || Math.abs(Number(spread)) > 2000;
  const scope = fullScope ? BOOKS : [book];
  const scopedPatches = Object.fromEntries(scope.filter(b => patches[b]).map(b => [b, patches[b]]));
  const scopedSpreads = Object.fromEntries(scope.filter(b => spreads?.[b]).map(b => [b, spreads?.[b]]));
  const key = JSON.stringify([scope, scopedPatches, scopedSpreads, analytics, scenario, mode, loadedRevision, nonce]);

  useEffect(() => {
    if (!enabled || loadedRevision !== revision || loadedRevision === null) return;
    let active = true;
    setPending(true); setError(null);
    const timer = setTimeout(async () => {
      const job = await run('whatif', { books: scope, scenario: scenario || undefined, pricing: {
        assumption_overrides: scopedPatches, spread_overrides_bp: scopedSpreads, include_analytics: analytics,
        calibration_mode: mode, expected_revision: loadedRevision,
      } });
      if (!active) return;
      setPending(false);
      if (job.status === 'done') setResult({ key, data: job.result as Comparison });
      else setError(job.detail ?? 'Comparison failed.');
    }, 150);
    return () => { active = false; clearTimeout(timer); };
    // The serialized key includes every request input; editor-only focus changes
    // must not enqueue duplicate jobs. Cleanup rejects obsolete completions.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, enabled, revision, loadedRevision, run]);

  const shown = result?.key === key && result.data.revision === revision ? result.data : null;
  const rows = useMemo(() => shown?.comparison[book] ? rowsOf(shown.comparison[book]) : [], [shown, book]);
  const changeCount = Object.values(scopedPatches).reduce((n, ids) => n + Object.values(ids ?? {}).reduce((m, fields) => m + Object.keys(fields).length, 0), 0)
    + Object.values(scopedSpreads).reduce((n, ids) => n + Object.keys(ids ?? {}).length, 0);

  function addChange() {
    if (!selectedId || invalid || invalidSpread) return;
    if (value.trim() !== '') setPatches(old => ({ ...old, [book]: { ...old[book], [selectedId]: { ...old[book]?.[selectedId], [field]: Number(value)/scale } } }));
    setSpreads(old => {
      const next = { ...old?.[book] };
      if (Number(spread) === 0) delete next[selectedId]; else next[selectedId] = Number(spread);
      return { ...old, [book]: next };
    });
    setMode('hold'); setEnabled(true);
  }

  return <div className="space-y-3">
    <Card><CardHeader title="Instrument What-if" sub="Explore temporary assumptions. Saved instruments stay unchanged." right={<Badge tone={mode === 'hold' ? 'up' : 'warning'}>{mode === 'hold' ? 'Baseline OAS held' : 'Recalibrated comparison'}</Badge>} />
      <CardBody className="space-y-3">
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <label className="text-sm text-paper-dim">Book<select aria-label="What-if book" className={selectClass} value={book} onChange={e => { const b = e.target.value as PricingBook; setBook(b); setField(FIRST[b]); setInstrument(''); setValue(''); setSpread('0'); setPage(0); }}>
            {BOOKS.map(b => <option key={b} value={b}>{LABELS[b]}</option>)}</select></label>
          <label className="text-sm text-paper-dim">Instrument<select aria-label="What-if instrument" className={selectClass} value={selectedId} onChange={e => { setInstrument(e.target.value); setValue(''); setSpread('0'); }}>
            {positions.map(r => <option key={idOf(r)} value={idOf(r)}>{idOf(r)}</option>)}</select></label>
          <label className="text-sm text-paper-dim">Assumption<select aria-label="What-if assumption" className={selectClass} value={field} onChange={e => { setField(e.target.value); setValue(''); }}>
            {Object.keys(catalog?.fields[book] ?? {}).map(f => <option key={f} value={f}>{FIELDS[f]?.[0] ?? f}</option>)}</select></label>
          <label className="text-sm text-paper-dim">New value<Input aria-label="Assumption value" type="number" step="any" value={value} aria-invalid={invalid} placeholder={current == null ? 'Model default' : String(Number(current) * scale)} onChange={e => setValue(e.target.value)} />
            <span className="text-2xs text-paper-faint">{label} · current {current == null ? 'model default' : (Number(current)*scale).toLocaleString(undefined, { maximumFractionDigits: 5 })}</span></label>
          <label className="text-sm text-paper-dim">Spread shift (bp)<Input aria-label="Instrument spread shift" type="number" min={-2000} max={2000} value={spread} onChange={e => setSpread(e.target.value)} aria-invalid={invalidSpread} /></label>
          <label className="text-sm text-paper-dim">Market scenario<select aria-label="What-if scenario" className={selectClass} value={scenario} onChange={e => setScenario(e.target.value)}><option value="">Unchanged market</option>{Object.keys(scenarios).map(s => <option key={s}>{s}</option>)}</select></label>
          <label className="flex items-center gap-2 text-sm text-paper-dim"><input type="checkbox" checked={fullScope} onChange={e => setFullScope(e.target.checked)} />Full balance-sheet totals</label>
          <label className="flex items-center gap-2 text-sm text-paper-dim"><input type="checkbox" checked={analytics} onChange={e => setAnalytics(e.target.checked)} />Include risk and earnings</label>
        </div>
        {scenario && <p className="text-xs text-paper-faint">Uses the selected scenario’s first-quarter shock as the starting market.</p>}
        {invalid && <p role="alert" className="text-sm text-danger">Enter a value within the supported assumption range.</p>}
        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={addChange} disabled={!selectedId || loadedRevision !== revision || invalid || invalidSpread}>Apply temporary change</Button>
          <Button variant="secondary" onClick={() => { setEnabled(true); setNonce(n => n+1); }} disabled={loadedRevision !== revision}>Compare</Button>
          <Button variant="secondary" onClick={() => { setPatches({}); setSpreads({}); setValue(''); setSpread('0'); setMode('hold'); setEnabled(true); }}>Reset changes</Button>
          <span className="text-sm text-paper-faint">{changeCount} temporary settings · changes refresh the comparison automatically</span>
        </div>
        <details className="text-sm text-paper-dim"><summary className="cursor-pointer">Calibration and saved changes</summary><p className="my-2">Comparisons hold original OAS and accounting yields. Recalibrating fits the modified contracts to their saved target prices for this comparison only. To save contract changes, use the Book Editor.</p>
          <Button variant="secondary" onClick={() => { setMode('recalibrate'); setEnabled(true); setNonce(n => n+1); }} disabled={!changeCount}>Recalibrate comparison</Button>
          <Button variant="secondary" onClick={() => { setMode('hold'); setEnabled(true); }}>Restore held baseline</Button>
        </details>
      </CardBody></Card>
    {error && <p role="alert" className="rounded border border-danger/40 p-3 text-sm text-danger">{error}</p>}
    {!shown && !error && <div role="status" className="flex items-center gap-2 p-3 text-sm text-paper-dim">{pending && enabled ? <><Spinner />Updating comparison; previous results are hidden.</> : loadedRevision === null ? 'Loading instruments…' : 'Choose an assumption or compare the unchanged baseline.'}</div>}
    {shown && <>
      <div className="grid gap-3 sm:grid-cols-3">
        {[['Original net value', shown.baseline.scope_net_value], ['Revised net value', shown.revised.scope_net_value], ['Net value change', shown.net_value_change]].map(([name, amount]) => <Card key={String(name)}><CardBody><div className="text-sm text-paper-faint">{name}</div><div className="num text-lg" data-testid={name === 'Net value change' ? 'value-change' : undefined}>{money(amount)}</div></CardBody></Card>)}
      </div>
      <p className="text-xs text-paper-faint">Net values above cover the five supported pricing books in scope; money markets and hedges enter full-scope risk/earnings KPIs below. Synthetic demo positions and stylized model assumptions.</p>
      {shown.revised.nii && <Card><CardHeader title="Risk and earnings impact" sub={`${settings?.horizon_months ?? 27}-month earnings forecast · ${fullScope ? 'full balance sheet, including money markets and hedges' : 'selected book only'}`} /><CardBody>
        <table className="w-full text-right text-sm"><thead><tr><th className="text-left">Measure</th><th>Original</th><th>Revised</th><th>Change</th></tr></thead><tbody>
          {([['Net interest income', shown.baseline.nii?.total, shown.revised.nii.total], ['Net DV01 ($/bp)', shown.baseline.kpis?.eve.dv01_net_$, shown.revised.kpis?.eve.dv01_net_$]] as const).map(([name,a,b]) => <tr key={name}><td className="py-2 text-left">{name}</td><td>{money(a)}</td><td>{money(b)}</td><td>{money(typeof a === 'number' && typeof b === 'number' ? b-a : null)}</td></tr>)}
          {fullScope && ([['LCR (%)', shown.baseline.kpis?.lcr.lcr_pct, shown.revised.kpis?.lcr.lcr_pct],
            ['NSFR (%)', shown.baseline.kpis?.nsfr.nsfr_pct, shown.revised.kpis?.nsfr.nsfr_pct],
            ['Horizon CET1 (%)', shown.baseline.kpis?.capital.cet1_path.at(-1)?.cet1_ratio_pct, shown.revised.kpis?.capital.cet1_path.at(-1)?.cet1_ratio_pct]] as const).map(([name,a,b]) => <tr key={name}><td className="py-2 text-left">{name}</td><td>{percent(a)}</td><td>{percent(b)}</td><td>{percent(typeof a === 'number' && typeof b === 'number' ? b-a : null)}</td></tr>)}
        </tbody></table>
      </CardBody></Card>}
      <Card><CardHeader title={`${LABELS[book]} comparison`} sub="Prices are percent of par. Positive DV01 means value rises as rates fall." /><CardBody className="overflow-auto">
        <table className="w-full text-right text-sm"><thead><tr>{['Instrument','Original price','Revised price','Value change','Base OAS (bp)',...(analytics ? ['DV01 ($/bp)','5y KRD ($/bp)'] : [])].map(c => <th key={c} className="whitespace-nowrap px-2 py-2">{c}</th>)}</tr></thead><tbody>
          {rows.slice(page*50, page*50+50).map(r => <tr key={String(r.id)} className="border-t border-line"><td className="px-2 py-2 text-left">{String(r.id)}</td><td>{Number(r.original_price).toFixed(5)}</td><td>{Number(r.model_price).toFixed(5)}</td><td>{money(r.value_change)}</td><td>{Number(r.base_oas_bp).toFixed(3)}</td>{analytics && <><td>{money(r.dv01)}</td><td>{money(r.krd01_5y)}</td></>}</tr>)}
        </tbody></table>
        <div className="mt-3 flex gap-2"><Button variant="secondary" disabled={page === 0} onClick={() => setPage(p => p-1)}>Previous</Button><Button variant="secondary" disabled={(page+1)*50 >= rows.length} onClick={() => setPage(p => p+1)}>Next</Button><span className="text-sm text-paper-faint">{rows.length} instruments · input revision {shown.revision}</span></div>
      </CardBody></Card>
    </>}
  </div>;
}
