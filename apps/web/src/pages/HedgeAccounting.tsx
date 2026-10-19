import { useEffect, useMemo, useRef, useState } from 'react';
import { api, awaitJob, type ResultPage } from '../lib/api';
import { Accordion, Badge, Button, Card, CardBody, CardHeader, DataTable, Input, Tabs, tabPanelProps } from '../components/ui';

/* Dedicated ASC 815 workflow over the durable hedge-accounting-1 model run: reviewed policy and
   adoption setup, source linking, designation, events, then the independently replayed
   history, assessments, basis and OCI roll-forwards and journal, with reopen and download. */

type Json = Record<string, any>;
const SCHEMA = 'hedge-accounting-1';
const RESULT_TABS = [
  { id: 'relationships', label: 'Relationships' },
  { id: 'lifecycle', label: 'History' },
  { id: 'assessments', label: 'Assessments' },
  { id: 'closing', label: 'Basis and OCI' },
  { id: 'intents', label: 'Journal' },
];
const EVENT_KINDS = ['assess', 'discontinue', 'forecast_status', 'rate_election', 'debt_issued', 'debt_replacement'];
const REASONS = ['dedesignation', 'hedged_item_sold', 'instrument_terminated', 'hedged_item_matured'];
const STATUSES = ['no_longer_probable', 'probable_not_to_occur', 'delayed', 'period_probable_not_to_occur'];

// Proleptic Gregorian day ordinals as in the engine (2000-01-01 is 730120).
const EPOCH = Date.UTC(2000, 0, 1), EPOCH_ORDINAL = 730120;
const iso = (ordinal: number) => new Date(EPOCH + (ordinal - EPOCH_ORDINAL) * 86400000).toISOString().slice(0, 10);
const ordinalOf = (value: string) => Math.round((Date.parse(value + 'T00:00:00Z') - EPOCH) / 86400000) + EPOCH_ORDINAL;
// Enum identifiers read as words; identities, dates and amounts are shown as stored.
const fmt = (v: unknown) => typeof v === 'number' ? (Number.isInteger(v) ? String(v) : v.toFixed(6))
  : v === null || v === undefined ? '' : typeof v === 'object' ? JSON.stringify(v)
  : /^[a-z0-9]+(_[a-z0-9]+)+$/.test(String(v)) ? String(v).replaceAll('_', ' ') : String(v);

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <label className="flex flex-col gap-1 text-xs text-paper-dim">{label}{children}</label>;
}
function Select({ label, value, options, onChange, disabled }: { label: string; value: string; options: string[]; onChange: (v: string) => void; disabled?: boolean }) {
  return <select aria-label={label} value={value} disabled={disabled} onChange={e => onChange(e.target.value)}
    className="h-control rounded border border-line-strong bg-surface-base px-2 text-sm text-paper">{options.map(o => <option key={o} value={o}>{o ? o.replaceAll('_', ' ') : '—'}</option>)}</select>;
}

export default function HedgeAccounting() {
  const [request, setRequest] = useState<Json>();
  const [revision, setRevision] = useState<number>();
  const [durable, setDurable] = useState(false);
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  const [job, setJob] = useState(''), [reopen, setReopen] = useState('');
  const [tab, setTab] = useState('lifecycle');
  const [tables, setTables] = useState<Record<string, ResultPage>>({});
  const [verification, setVerification] = useState<Record<string, unknown>>({});
  const [summary, setSummary] = useState<Record<string, unknown>>({});
  const [raw, setRaw] = useState<Record<string, string>>({});
  const generation = useRef(0);

  const loadExample = async () => {
    const token = ++generation.current; setBusy(true); setError('');
    try {
      const [contracts, example] = await Promise.all([api.modelContracts(), api.modelExample(SCHEMA)]);
      if (token !== generation.current) return;
      setDurable(contracts.durable); setRevision(example.revision); setRequest(example.request as Json); setRaw({});
    } catch (e) { if (token === generation.current) setError(String(e)); }
    finally { if (token === generation.current) setBusy(false); }
  };
  useEffect(() => { void loadExample(); return () => { generation.current++; }; }, []);

  const update = (change: (draft: Json) => void) => setRequest(previous => {
    if (!previous) return previous;
    const draft = structuredClone(previous); change(draft); return draft;
  });
  const derivative = request?.derivative as Json | undefined;
  const nodes: Json[] = derivative?.derivative?.operation?.request?.paths?.[0]?.nodes ?? [];
  const positions: string[] = (derivative?.opening?.trades ?? []).map((t: Json) => t.position_id);
  const items: string[] = (request?.hedged_items ?? []).map((i: Json) => i.item_id);
  const exposures: string[] = (request?.forecast_exposures ?? []).map((x: Json) => x.exposure_id);
  const relationshipIds: string[] = (request?.relationships ?? []).map((r: Json) => r.relationship_id);
  const policy = request?.policy as Json | undefined;
  const asu = policy?.adoption?.asu_2025_09 as Json | undefined;

  const show = async (id: string, token: number) => {
    const manifest = await api.modelManifest(id);
    if (manifest.execution?.schema !== SCHEMA) throw new Error('Select a completed hedge accounting job.');
    const loaded: Record<string, ResultPage> = {};
    for (const t of RESULT_TABS) {
      const path = `/model_result/${t.id}`;
      if (path in manifest.tables) loaded[t.id] = await api.cohortTable(id, path);
    }
    if (token !== generation.current) return;
    setJob(id); setReopen(id); setTables(loaded); setSummary(manifest.execution.summary);
    setVerification(manifest.execution.source_verification ?? {});
  };
  const run = async () => {
    if (!request) return;
    const token = ++generation.current; setBusy(true); setError(''); setJob(''); setTables({});
    try {
      const submitted = await api.runModel({ schema_name: SCHEMA, request, expected_revision: revision! });
      const done = await awaitJob(submitted.id);
      if (done.status !== 'done') throw new Error(done.detail || 'Hedge accounting run failed');
      await show(done.id, token);
    } catch (e) { if (token === generation.current) setError(String(e)); }
    finally { if (token === generation.current) setBusy(false); }
  };
  const open = async () => {
    const token = ++generation.current; setBusy(true); setError('');
    try { await show(reopen.trim(), token); } catch (e) { if (token === generation.current) setError(String(e)); }
    finally { if (token === generation.current) setBusy(false); }
  };
  const editRaw = (key: string, text: string) => {
    setRaw(r => ({ ...r, [key]: text }));
    try { const value = JSON.parse(text); update(d => { d[key] = value; }); setError(''); }
    catch { setError(`The ${key.replaceAll('_', ' ')} JSON is not valid yet.`); }
  };
  const download = () => {
    if (!request) return;
    const url = URL.createObjectURL(new Blob([JSON.stringify(request, null, 2)], { type: 'application/json' }));
    const a = document.createElement('a'); a.href = url; a.download = 'hedge-accounting-request.json'; a.click(); URL.revokeObjectURL(url);
  };
  const rows = useMemo(() => {
    const page = tables[tab];
    if (!page) return [];
    return page.rows.map(row => {
      const record = Object.fromEntries(page.columns.map((c, i) => [c, row[i]]));
      const parsed: Json = 'source_json' in record ? JSON.parse(String(record.source_json)) : record;
      return Object.fromEntries(Object.entries(parsed).map(([k, v]) => [k.replaceAll('_', ' '), k.endsWith('ordinal') && typeof v === 'number' ? iso(v) : fmt(v)]));
    });
  }, [tables, tab]);
  const replayed = verification.formal_hedge_relationships_replayed === true;
  const activeLabel = RESULT_TABS.find(t => t.id === tab)?.label ?? '';

  return <div className="space-y-3">
    <Card><CardHeader title="Hedge accounting" sub="ASC 815 relationships · assessments · basis and OCI · independent replay" /><CardBody>
      <p className="mb-3 text-xs text-paper-faint">Runs explicit fair-value, cash-flow and portfolio-layer relationships on one reviewed Physical derivative source. Every result is independently replayed before it is stored. Results are not accounting-standard approval or observed effectiveness.</p>
      {!request ? <p className="text-xs">Loading the reviewed example…</p> : <div className="space-y-4">
        <section aria-label="Source" className="space-y-2">
          <div className="eyebrow">Source</div>
          <div className="flex flex-wrap items-center gap-2 text-xs text-paper-dim">
            <span>{String(derivative?.entity)} · {String(derivative?.currency)}</span>
            <span>Path {nodes.length ? `${iso(nodes[0].ordinal)} to ${iso(nodes[nodes.length - 1].ordinal)}` : '—'}</span>
            <span>Positions {positions.join(', ')}</span>
            <Button variant="secondary" size="sm" disabled={busy} onClick={() => void loadExample()}>Load reviewed example</Button>
          </div>
          <Accordion id="hedge-derivative" title="Derivative source" summary="Complete derivative-accounting-1 request">
            <textarea aria-label="Derivative source JSON" className="h-48 w-full rounded border border-line bg-surface-base p-2 text-xs"
              value={raw.derivative ?? JSON.stringify(request.derivative, null, 2)} onChange={e => editRaw('derivative', e.target.value)} />
          </Accordion>
        </section>
        <section aria-label="Policy and adoption" className="space-y-2">
          <div className="eyebrow">Policy and adoption</div>
          <div className="flex flex-wrap items-end gap-3">
            <Field label="Assessment lower bound"><Input aria-label="Assessment lower bound" type="number" step="0.01" value={policy?.assessment?.lower_bound}
              onChange={e => update(d => { d.policy.assessment.lower_bound = Number(e.target.value); })} /></Field>
            <Field label="Assessment upper bound"><Input aria-label="Assessment upper bound" type="number" step="0.01" value={policy?.assessment?.upper_bound}
              onChange={e => update(d => { d.policy.assessment.upper_bound = Number(e.target.value); })} /></Field>
            <label className="flex items-center gap-2 text-xs"><input type="checkbox" aria-label="ASU 2025-09 adopted" checked={!!asu?.adopted}
              onChange={e => update(d => { const a = d.policy.adoption.asu_2025_09; a.adopted = e.target.checked; a.adoption_ordinal = e.target.checked ? (a.adoption_ordinal ?? nodes[0]?.ordinal) : null; })} />ASU 2025-09 adopted</label>
            {asu?.adopted && <Field label="Adoption date"><Input aria-label="Adoption date" type="date" value={iso(asu.adoption_ordinal)}
              onChange={e => update(d => { d.policy.adoption.asu_2025_09.adoption_ordinal = ordinalOf(e.target.value); })} /></Field>}
            {asu?.adopted && ['similar_risk_grouping', 'selectable_rate_debt', 'swap_written_option_exception'].map(k =>
              <label key={k} className="flex items-center gap-2 text-xs"><input type="checkbox" aria-label={k.replaceAll('_', ' ')} checked={!!asu.elections[k]}
                onChange={e => update(d => { d.policy.adoption.asu_2025_09.elections[k] = e.target.checked; })} />{k.replaceAll('_', ' ')}</label>)}
            <label className="flex items-center gap-2 text-xs"><input type="checkbox" aria-label="ASU 2022-01 portfolio layers" checked={!!policy?.adoption?.asu_2022_01_portfolio_layer}
              onChange={e => update(d => { d.policy.adoption.asu_2022_01_portfolio_layer = e.target.checked;
                if (e.target.checked) d.policy.portfolio_layer ??= { identity: { id: 'layer-policy', revision: 'r1' }, breach_order: 'last_designated_first' };
                else delete d.policy.portfolio_layer; })} />ASU 2022-01 portfolio layers</label>
            {policy?.portfolio_layer && <Field label="Breach order"><Select label="Breach order" value={policy.portfolio_layer.breach_order} options={['last_designated_first', 'pro_rata']}
              onChange={v => update(d => { d.policy.portfolio_layer.breach_order = v; })} /></Field>}
          </div>
        </section>
        <section aria-label="Hedged items and exposures" className="space-y-2">
          <div className="eyebrow">Hedged items and forecast exposures</div>
          <p className="text-xs text-paper-dim">Items {items.join(', ') || 'none'} · exposures {exposures.join(', ') || 'none'}</p>
          <Accordion id="hedge-items" title="Hedged items" summary={`${items.length} reviewed`}>
            <textarea aria-label="Hedged items JSON" className="h-40 w-full rounded border border-line bg-surface-base p-2 text-xs"
              value={raw.hedged_items ?? JSON.stringify(request.hedged_items, null, 2)} onChange={e => editRaw('hedged_items', e.target.value)} />
          </Accordion>
          <Accordion id="hedge-exposures" title="Forecast exposures" summary={`${exposures.length} reviewed`}>
            <textarea aria-label="Forecast exposures JSON" className="h-40 w-full rounded border border-line bg-surface-base p-2 text-xs"
              value={raw.forecast_exposures ?? JSON.stringify(request.forecast_exposures, null, 2)} onChange={e => editRaw('forecast_exposures', e.target.value)} />
          </Accordion>
        </section>
        <section aria-label="Designations" className="space-y-2">
          <div className="eyebrow">Designations</div>
          <table className="w-full text-xs"><thead><tr className="text-left text-paper-faint">
            <th>Relationship</th><th>Designation</th><th>Instrument</th><th>Proportion</th><th>Hedged item or exposure</th><th>Inception</th><th>Benchmark</th><th /></tr></thead>
            <tbody>{(request.relationships as Json[]).map((r, i) => <tr key={i} className="border-t border-line">
              <td>{r.relationship_id}</td>
              <td><Select label={`Designation ${r.relationship_id}`} value={r.designation} options={['fair_value_hedge', 'cash_flow_hedge']}
                onChange={v => update(d => { d.relationships[i].designation = v; })} /></td>
              <td><Select label={`Instrument ${r.relationship_id}`} value={r.hedging_instrument_position_id} options={positions}
                onChange={v => update(d => { d.relationships[i].hedging_instrument_position_id = v; })} /></td>
              <td><Input aria-label={`Proportion ${r.relationship_id}`} type="number" step="0.05" value={r.designated_proportion}
                onChange={e => update(d => { d.relationships[i].designated_proportion = Number(e.target.value); })} /></td>
              <td>{r.designation === 'fair_value_hedge'
                ? <Select label={`Hedged item ${r.relationship_id}`} value={r.hedged_item_id ?? ''} options={items} onChange={v => update(d => { d.relationships[i].hedged_item_id = v; })} />
                : r.group ? <span>group of {r.group.member_exposure_ids.length}</span>
                : <Select label={`Exposure ${r.relationship_id}`} value={r.forecast_exposure_id ?? ''} options={exposures} onChange={v => update(d => { d.relationships[i].forecast_exposure_id = v; })} />}</td>
              <td><Input aria-label={`Inception ${r.relationship_id}`} type="date" value={iso(r.inception_ordinal)}
                onChange={e => update(d => { d.relationships[i].inception_ordinal = ordinalOf(e.target.value); })} /></td>
              <td>{r.designation === 'fair_value_hedge' && <Input aria-label={`Benchmark ${r.relationship_id}`} type="number" step="0.0025" value={r.benchmark_rate ?? 0}
                onChange={e => update(d => { d.relationships[i].benchmark_rate = Number(e.target.value); })} />}</td>
              <td><Button variant="secondary" size="sm" onClick={() => update(d => { d.relationships.splice(i, 1); d.events = d.events.filter((ev: Json) => ev.relationship_id !== r.relationship_id); })}>Remove</Button></td>
            </tr>)}</tbody></table>
          <Accordion id="hedge-relationships" title="Relationship documentation" summary="Hypotheticals, groups, openings and layers">
            <textarea aria-label="Relationships JSON" className="h-48 w-full rounded border border-line bg-surface-base p-2 text-xs"
              value={raw.relationships ?? JSON.stringify(request.relationships, null, 2)} onChange={e => editRaw('relationships', e.target.value)} />
          </Accordion>
        </section>
        <section aria-label="Reviewed events" className="space-y-2">
          <div className="eyebrow">Reviewed events</div>
          <table className="w-full text-xs"><thead><tr className="text-left text-paper-faint">
            <th>Event</th><th>Relationship</th><th>Date</th><th>Sequence</th><th>Kind</th><th>Reason, status or index</th><th /></tr></thead>
            <tbody>{(request.events as Json[]).map((ev, i) => <tr key={i} className="border-t border-line">
              <td>{ev.event_id}</td>
              <td><Select label={`Event relationship ${ev.event_id}`} value={ev.relationship_id} options={relationshipIds} onChange={v => update(d => { d.events[i].relationship_id = v; })} /></td>
              <td><Input aria-label={`Event date ${ev.event_id}`} type="date" value={iso(ev.ordinal)} onChange={e => update(d => { d.events[i].ordinal = ordinalOf(e.target.value); })} /></td>
              <td><Input aria-label={`Event sequence ${ev.event_id}`} type="number" value={ev.sequence} onChange={e => update(d => { d.events[i].sequence = Number(e.target.value); })} /></td>
              <td><Select label={`Event kind ${ev.event_id}`} value={ev.kind} options={EVENT_KINDS} onChange={v => update(d => {
                const e = d.events[i]; e.kind = v; delete e.reason; delete e.status; delete e.index_id;
                if (v === 'discontinue') e.reason = 'dedesignation'; if (v === 'forecast_status') e.status = 'no_longer_probable'; })} /></td>
              <td>{ev.kind === 'discontinue' ? <Select label={`Reason ${ev.event_id}`} value={ev.reason} options={REASONS} onChange={v => update(d => { d.events[i].reason = v; })} />
                : ev.kind === 'forecast_status' ? <Select label={`Status ${ev.event_id}`} value={ev.status} options={STATUSES} onChange={v => update(d => { d.events[i].status = v; })} />
                : ['rate_election', 'debt_issued', 'debt_replacement'].includes(ev.kind) ? <Input aria-label={`Index ${ev.event_id}`} value={ev.index_id ?? ''} onChange={e => update(d => { d.events[i].index_id = e.target.value; })} />
                : null}</td>
              <td><Button variant="secondary" size="sm" onClick={() => update(d => { d.events.splice(i, 1); })}>Remove</Button></td>
            </tr>)}</tbody></table>
          <Button variant="secondary" size="sm" disabled={!relationshipIds.length} onClick={() => update(d => {
            const last = d.events.at(-1); const ordinal = last?.ordinal ?? nodes[0]?.ordinal;
            d.events.push({ event_id: `review-${d.events.length + 1}`, relationship_id: relationshipIds[0], ordinal, sequence: (last?.sequence ?? -1) + 1,
              kind: 'assess', review: { id: `review-${d.events.length + 1}`, revision: 'r1' } });
          })}>Add reviewed event</Button>
        </section>
        <div className="flex flex-wrap items-center gap-2">
          <Button disabled={busy || !durable || revision === undefined} onClick={() => void run()}>{busy ? 'Replaying…' : 'Run hedge accounting'}</Button>
          <Button variant="secondary" disabled={busy} onClick={download}>Download request JSON</Button>
          <Button variant="secondary" disabled={busy} onClick={async () => { try { setRevision((await api.state()).revision); setError(''); } catch (e) { setError(String(e)); } }}>Refresh workspace revision</Button>
        </div>
      </div>}
      <div className="mt-3 flex flex-wrap items-end gap-2">
        <Field label="Stored hedge job"><Input aria-label="Stored hedge job" value={reopen} disabled={busy} onChange={e => setReopen(e.target.value)} /></Field>
        <Button variant="secondary" disabled={busy || !reopen.trim()} onClick={() => void open()}>Open stored hedge job</Button>
      </div>
      {!durable && request && <p className="mt-2 text-xs">Durable storage is required.</p>}
      {error && <p role="alert" className="mt-2 text-xs text-danger">{error}</p>}
      {job && <p role="status" className="mt-2 text-xs">Hedge accounting saved: {job}</p>}
    </CardBody></Card>
    {job && <Card><CardHeader title="Replayed results" sub={String(summary.rule_version ?? '')} right={<div className="flex gap-1">
      {replayed ? <Badge tone="up" dot>Independently replayed</Badge> : <Badge tone="danger">Not replayed</Badge>}
      <Badge tone="warning">Not accounting-standard approval</Badge></div>} /><CardBody>
      <Tabs id="hedge-results" tabs={RESULT_TABS.filter(t => t.id in tables).map(t => t.label)} active={activeLabel}
        onChange={label => setTab(RESULT_TABS.find(t => t.label === label)?.id ?? 'lifecycle')} />
      <div {...tabPanelProps('hedge-results', activeLabel, activeLabel)}>
        <DataTable rows={rows} />
        <div className="mt-2 flex gap-3 text-xs">
          <a className="underline" href={`/api/jobs/${encodeURIComponent(job)}/parquet?path=${encodeURIComponent(`/model_result/${tab}`)}`}>Download Parquet</a>
          <span className="text-paper-faint">{tables[tab]?.total ?? 0} rows</span>
        </div>
      </div>
    </CardBody></Card>}
  </div>;
}
