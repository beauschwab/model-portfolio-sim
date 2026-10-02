import { useEffect, useRef, useState } from 'react';
import { api, awaitJob, fmt$, type BookName, type Row, type DecisionResult, type DecisionEvaluation } from '../lib/api';
import { Badge, Button, Card, CardBody, CardHeader, Input, Stat } from '../components/ui';

const defaults = { lcr_min: 1.10, nsfr_min: 1.05, cet1_min: .10, eve_limit: .15, max_total_assets: 3e10, cash_budget: 0 };
const labels = { lcr_min: 'Minimum LCR (%)', nsfr_min: 'Minimum NSFR (%)', cet1_min: 'Minimum CET1 (%)', eve_limit: 'Maximum EVE change (%)', max_total_assets: 'New asset cap ($m)', cash_budget: 'Outside-book cash budget ($m)' };
const selectClass = 'h-8 w-full rounded-md border border-line bg-surface-2 px-2 text-xs';
const books: BookName[] = ['mbs', 'loans', 'debt', 'deposits', 'cds'];

export default function DecisionLab() {
  const [result, setResult] = useState<DecisionResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState('Build a session to initialize pricing and strategy coefficients.');
  const [error, setError] = useState('');
  const [stale, setStale] = useState(false);
  const [limits, setLimits] = useState(defaults);
  const [scenarios, setScenarios] = useState<string[]>([]);
  const [selectedScenarios, setSelectedScenarios] = useState<string[]>([]);
  const [fields, setFields] = useState<Record<string, Record<string, [number, number]>>>({});
  const [book, setBook] = useState<BookName>('loans');
  const [rows, setRows] = useState<Row[]>([]);
  const [instrument, setInstrument] = useState('');
  const [field, setField] = useState('coupon_or_spread');
  const [value, setValue] = useState('0.06');
  const [editInstrument, setEditInstrument] = useState(false);
  const [resetInstrument, setResetInstrument] = useState(false);
  const [editTemplate, setEditTemplate] = useState(false);
  const [template, setTemplate] = useState('cml_fixed_5y');
  const [spread, setSpread] = useState('250');
  const [scale, setScale] = useState('100');
  const [manual, setManual] = useState<DecisionEvaluation | null>(null);
  const generation = useRef(0);
  const session = useRef<string | null>(null);

  useEffect(() => {
    let alive = true;
    Promise.all([api.scenarios(), api.pricingAssumptions()]).then(([s, f]) => {
      if (alive) { setScenarios(Object.keys(s)); setFields(f.fields); }
    }).catch(e => { if (alive) setError(String(e)); });
    const invalidated = () => { generation.current++; setStale(true); setBusy(false); setManual(null); setStatus('Saved inputs changed. Build a new session.'); };
    window.addEventListener('engine:inputs-changed', invalidated);
    return () => { alive = false; generation.current++; window.removeEventListener('engine:inputs-changed', invalidated); if (session.current) void api.closeDecision(session.current).catch(() => {}); };
  }, []);
  useEffect(() => {
    let alive = true;
    setRows([]); setInstrument('');
    api.book(book).then(table => {
      if (!alive) return;
      const loaded = table.toArray().map(row => row.toJSON() as Row);
      setRows(loaded); setInstrument(String(loaded[0]?.[book === 'mbs' ? 'cusip' : 'id'] ?? ''));
    }).catch(e => { if (alive) setError(String(e)); });
    return () => { alive = false; };
  }, [book]);

  async function run(build: boolean) {
    const id = ++generation.current;
    setBusy(true); setError(''); setManual(null);
    try {
      const saved = await api.state();
      if (id !== generation.current) return;
      if (!build && result && saved.revision !== result.revision) { setStale(true); throw new Error('Saved inputs changed. Build a new session.'); }
      if (build && session.current) { await api.closeDecision(session.current); session.current = null; }
      const job = build ? await api.buildDecision(saved.revision, { ...limits, scenarios: selectedScenarios }) : await api.updateDecision(result!.session_id, {
        expected_revision: result!.revision, version: result!.version, constraints: limits,
        edits: editInstrument ? { [`${book}:${instrument}`]: { [field]: resetInstrument ? null : Number(value) } } : {},
        templates: editTemplate ? { [template]: { spread_bp: Number(spread) } } : {},
      });
      const done = await awaitJob(job.id, j => { if (id === generation.current) setStatus(j.progress?.stage ?? j.status); }, 75);
      if (done.status === 'error') throw new Error(done.detail);
      const next = done.result as DecisionResult;
      if (id !== generation.current) { if (build) void api.closeDecision(next.session_id); return; }
      const current = await api.state();
      if (id !== generation.current || next.revision !== current.revision) {
        if (build) void api.closeDecision(next.session_id);
        if (id === generation.current) { setStale(true); throw new Error('Saved inputs changed during the run. Build a new session.'); }
        return;
      }
      session.current = next.session_id;
      setResult(previous => ({ ...next, units: next.units ?? previous?.units }));
      setStale(false); setStatus(next.validation);
    } catch (e) { if (id === generation.current) setError(String(e)); }
    finally { if (id === generation.current) setBusy(false); }
  }
  async function replay() {
    if (!result) return;
    const id = ++generation.current;
    setBusy(true); setError('');
    try {
      const next = await api.evaluateDecision(result.session_id, { version: result.version, expected_revision: result.revision,
        allocation: result.allocation.map(a => ({ ...a, notional: a.notional * Number(scale) / 100 })) });
      if (id === generation.current) setManual(next);
    } catch (e) { if (id === generation.current) setError(String(e)); }
    finally { if (id === generation.current) setBusy(false); }
  }
  const names = [...new Set(result?.units?.map(u => u.template) ?? ['cml_fixed_5y'])];
  const replays = manual?.replay ?? result?.replay;
  return <div className="space-y-3">
    <Card><CardHeader title="Decision Lab" sub="Change assumptions, optimize the balance sheet, and inspect the independently checked allocation." right={<Badge tone="amber">Prototype</Badge>} />
      <CardBody className="space-y-3">
        <p className="text-xs text-paper-dim">Temporary instrument edits hold original calibration. New-business spreads affect future strategy units. Saved books remain unchanged.</p>
        <div className="flex flex-wrap items-center gap-2">
          <Button disabled={busy} onClick={() => void run(true)}>{result ? 'Rebuild session' : 'Build decision session'}</Button>
          <Button disabled={!result || stale || busy} onClick={() => void run(false)}>Apply and optimize</Button>
          {result && <Badge tone={stale ? 'red' : 'green'}>{stale ? 'Rebuild required' : `Version ${result.version} · ${result.work.total_positions} instruments`}</Badge>}
        </div>
        <div role="status" aria-live="polite" className="text-xs text-paper-dim">{busy ? `Working: ${status}` : status}</div>
        {error && <div role="alert" className="text-xs text-down">{error}</div>}
        <fieldset disabled={busy} className="flex flex-wrap gap-3 text-xs"><legend className="mb-1 text-paper-faint">Scenarios for the next session (base always included)</legend>
          {scenarios.map(s => <label key={s} className="flex items-center gap-1"><input type="checkbox" checked={selectedScenarios.includes(s)} onChange={e => setSelectedScenarios(old => e.target.checked ? [...old, s] : old.filter(x => x !== s))} />{s}</label>)}
        </fieldset>
      </CardBody></Card>
    <Card><CardHeader title="Targets and funding" sub="Constraint changes reuse pricing and unit coefficients. Cash budget represents additional committed funding; its cost is not priced." />
      <CardBody><fieldset disabled={busy} className="grid grid-cols-2 gap-3 xl:grid-cols-3">
        {(Object.keys(labels) as (keyof typeof defaults)[]).map(key => {
          const multiplier = key === 'cash_budget' || key === 'max_total_assets' ? 1e-6 : 100;
          return <label key={key} className="space-y-1 text-xs text-paper-dim">{labels[key]}<Input aria-label={labels[key]} type="number" step="any" value={Number((limits[key] * multiplier).toPrecision(12))} onChange={e => setLimits(old => ({ ...old, [key]: Number(e.target.value) / multiplier }))} /></label>;
        })}
      </fieldset></CardBody></Card>
    <div className="grid gap-3 xl:grid-cols-2">
      <Card><CardHeader title="Instrument assumption" sub="One selected field per apply. Values use engine units: 0.06 means a 6% rate." /><CardBody>
        <fieldset disabled={busy} className="space-y-2 text-xs">
          <label className="flex gap-2"><input type="checkbox" checked={editInstrument} onChange={e => setEditInstrument(e.target.checked)} />Include instrument edit</label>
          <label className="block">Book<select aria-label="Decision book" className={selectClass} value={book} onChange={e => { const b = e.target.value as BookName; setBook(b); setField(Object.keys(fields[b] ?? {})[0] ?? ''); }}>{books.map(b => <option key={b}>{b}</option>)}</select></label>
          <label className="block">Instrument<select aria-label="Decision instrument" className={selectClass} value={instrument} onChange={e => setInstrument(e.target.value)}>{rows.map(r => { const id = String(r[book === 'mbs' ? 'cusip' : 'id']); return <option key={id}>{id}</option>; })}</select></label>
          <label className="block">Assumption<select aria-label="Decision assumption" className={selectClass} value={field} onChange={e => setField(e.target.value)}>{Object.keys(fields[book] ?? {}).map(f => <option key={f}>{f}</option>)}</select></label>
          <label className="block">Value<Input aria-label="Decision assumption value" type="number" step="any" disabled={resetInstrument} value={value} onChange={e => setValue(e.target.value)} /></label>
          <label className="flex gap-2"><input type="checkbox" checked={resetInstrument} onChange={e => setResetInstrument(e.target.checked)} />Restore this field to its saved value</label>
        </fieldset>
      </CardBody></Card>
      <Card><CardHeader title="New-business assumption" sub="Rebuilds only this template’s unit columns across the session scenarios." /><CardBody>
        <fieldset disabled={busy} className="space-y-2 text-xs">
          <label className="flex gap-2"><input type="checkbox" checked={editTemplate} onChange={e => setEditTemplate(e.target.checked)} />Include template edit</label>
          <label className="block">Template<select aria-label="Decision template" className={selectClass} value={template} onChange={e => setTemplate(e.target.value)}>{names.map(n => <option key={n}>{n}</option>)}</select></label>
          <label className="block">Spread (basis points)<Input aria-label="Template spread (bp)" type="number" value={spread} onChange={e => setSpread(e.target.value)} /></label>
        </fieldset>
        <p className="mt-4 text-xs text-paper-faint">Rust owns dependency updates, portfolio aggregates, coefficients and solver state. Product cashflows use the existing engine; HiGHS remains the native solver. Synthetic demo inputs and existing model approximations still apply.</p>
      </CardBody></Card>
    </div>
    {result && <>
      <div className="grid grid-cols-2 gap-2 xl:grid-cols-4">
        <Stat label="Worst-case NII" value={result.feasible ? fmt$(result['worst_case_nii_$']!) : 'Infeasible'} />
        <Stat label="Latest update" value={`${result.timings_ms.total.toFixed(1)} ms`} />
        <Stat label="Positions repriced" value={String(result.work.positions_repriced)} />
        <Stat label="Templates rebuilt" value={String(result.work.templates_rebuilt)} />
      </div>
      <Card><CardHeader title="Published allocation" sub={`${result.horizon_months} months · ${result.scenarios.join(', ')} · ${result.solver.model_reused ? 'existing solver model updated' : 'solver model initialized'}`} /><CardBody className="space-y-3">
        {!result.feasible ? <p role="alert" className="text-sm text-brand">{result.message} Adjust targets or funding, then apply again.</p> : <>
          <div className="overflow-x-auto"><table className="w-full text-left text-xs"><thead><tr><th className="p-2">Template</th><th>Purchase month</th><th>Notional</th></tr></thead><tbody>{result.allocation.map(a => <tr key={`${a.template}:${a.purchase_m}`} className="border-t border-line"><td className="p-2">{a.template}</td><td>{a.purchase_m}</td><td>{fmt$(a.notional)}</td></tr>)}</tbody></table></div>
          <div className="flex flex-wrap items-end gap-2"><label className="text-xs">Allocation scale (%)<Input aria-label="Allocation scale (%)" type="number" min="0" value={scale} onChange={e => setScale(e.target.value)} /></label><Button variant="ghost" disabled={busy || stale || Number(scale) < 0 || !Number.isFinite(Number(scale))} onClick={() => void replay()}>Replay allocation</Button></div>
          <p className="text-xs text-paper-faint">Manual replay shows outcomes without pricing or solving. A scaled allocation is exploratory and may violate targets.</p>
        </>}
        {replays && <div className="overflow-x-auto"><table className="w-full text-left text-xs"><caption className="mb-2 text-left text-paper-dim">{manual ? 'Manual allocation replay' : 'Validated allocation outcomes'}</caption><thead><tr><th>Scenario</th><th>Min LCR</th><th>Min NSFR</th><th>Final CET1</th><th>Max |EVE|</th></tr></thead><tbody>{replays.map((r, i) => <tr key={i} className="border-t border-line"><td className="py-2">{result.scenarios[i]}</td><td>{Math.min(...r.kpi_path.lcr_pct).toFixed(2)}%</td><td>{Math.min(...r.kpi_path.nsfr_pct).toFixed(2)}%</td><td>{r.kpis.cet1_horizon_pct.toFixed(2)}%</td><td>{Math.max(...r.kpi_path['d_eve_pct_eve_+200'].map(Math.abs)).toFixed(2)}%</td></tr>)}</tbody></table></div>}
        {!!result.binding_constraints?.length && <details><summary className="cursor-pointer text-xs">Binding constraints and marginal NII values</summary><ul className="mt-2 space-y-1 text-xs text-paper-dim">{result.binding_constraints.map(c => <li key={c.constraint}>{c.constraint}: {c.shadow_price.toPrecision(5)}</li>)}</ul></details>}
        <p className="text-xs text-paper-faint">Changed instruments: {result.changed_positions.join(', ') || 'none'} · Templates: {result.changed_templates.join(', ') || 'none'} · Initial build: {(result.initialization_ms / 1000).toFixed(2)}s</p>
      </CardBody></Card>
    </>}
  </div>;
}
