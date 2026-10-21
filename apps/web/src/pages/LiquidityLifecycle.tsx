import { useEffect, useMemo, useRef, useState } from 'react';
import { awaitNativeJob, NativeError, nativeApi } from '../lib/api';
import type { DocumentEntry, Job, LifecyclePlan, LiquidityStep, ProducerReceipt, Requirement, SavedBook, TablePage } from '../lib/native-api.generated';
import { Accordion, Badge, Button, Card, CardBody, CardHeader, DataTable, Input, Tabs, tabPanelProps, type Tone } from '../components/ui';

/* Liquidity lifecycle workbench over the Rust service only (stage F, issue #48): a saved book, an
   acquisition or unwind plan with explicit hedge and financing treatment, the mapping check, the
   durable native run, and the independently verified and published results, reopened from storage.
   Every amount shown comes from the native receipt or the published tables; nothing is computed here. */

type Json = Record<string, any>;
const BOOK = 'liquidity-book-1', PLAN = 'liquidity-plan-1';
const HEDGE_TREATMENTS = ['', 'terminate', 'retain', 'offset', 'rebalance'];
const FUNDING_TREATMENTS = ['', 'repay', 'retain', 'replace'];
const ACTION_KINDS = ['acquisition', 'unwind', 'offset', 'rebalance', 'refinance'];
const TABLES = [
  { id: 'lifecycle_journal_lineage', label: 'Lifecycle postings' },
  { id: 'lifecycle_custody_journal', label: 'Custody memo' },
  { id: 'trial_balance', label: 'Trial balance' },
  { id: 'journal', label: 'Ordered journal' },
];
const RESULT_TABS = [
  { id: 'liquidity', label: 'Liquidity timeline' },
  { id: 'obligations', label: 'Obligations' },
  { id: 'collateral', label: 'Collateral and facilities' },
  { id: 'facts', label: 'Accounting facts' },
  { id: 'decisions', label: 'Funding decisions' },
  { id: 'monthly', label: 'Monthly' },
  ...TABLES,
];
const labelOf = (id: string) => RESULT_TABS.find(t => t.id === id)?.label ?? id;
const INSTRUCTION_LABELS: Record<string, string> = {
  purchases: 'Purchase', financings: 'Financing advance', facility_draws: 'Facility draw', repayments: 'Funding repayment',
  sales: 'Security sale', hedge_entries: 'Hedge entry', terminations: 'Derivative termination',
  facility_repayments: 'Facility repayment', initial_margin: 'Initial margin',
};

// Proleptic Gregorian day ordinals as in the engine (2000-01-01 is 730120).
const EPOCH = Date.UTC(2000, 0, 1), EPOCH_ORDINAL = 730120;
const ordinalOf = (value: string) => Math.round((Date.parse(value + 'T00:00:00Z') - EPOCH) / 86400000) + EPOCH_ORDINAL;
const iso = (ordinal: unknown) => typeof ordinal === 'number' ? new Date(EPOCH + (ordinal - EPOCH_ORDINAL) * 86400000).toISOString().slice(0, 10) : '';
const money = (v: unknown) => typeof v === 'number' ? v.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 }) : '';
const words = (v: unknown) => String(v ?? '').replaceAll('_', ' ');
const statusTone: Record<string, Tone> = { settled: 'up', failed: 'danger', blocked: 'warning', native_done: 'up', native_error: 'danger', native_cancelled: 'warning', native_running: 'info', native_queued: 'neutral' };

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <label className="flex flex-col gap-1 text-xs text-paper-dim">{label}{children}</label>;
}
function Select({ label, value, options, onChange, disabled }: { label: string; value: string; options: string[]; onChange: (v: string) => void; disabled?: boolean }) {
  return <select aria-label={label} value={value} disabled={disabled} onChange={e => onChange(e.target.value)}
    className="h-control rounded border border-line-strong bg-surface-base px-2 text-sm text-paper">{options.map(o => <option key={o} value={o}>{o ? words(o) : '— choose —'}</option>)}</select>;
}
function Requirements({ list }: { list: Requirement[] }) {
  return <section aria-label="Mapping requirements" className="space-y-1 rounded border border-warning/40 p-2">
    <div className="eyebrow">Mapping required before this plan can run</div>
    <ul className="space-y-1 text-xs">{list.map((r, i) => <li key={i}>
      <span className="font-medium text-paper">{r.record}</span>: {r.detail}. <span className="text-paper-dim">Fix: {r.fix}.</span>
    </li>)}</ul>
  </section>;
}

export default function LiquidityLifecycle() {
  const [revision, setRevision] = useState<number>();
  const [documents, setDocuments] = useState<DocumentEntry[]>([]);
  const [bookName, setBookName] = useState(''), [book, setBook] = useState<SavedBook>();
  const [planName, setPlanName] = useState(''), [plan, setPlan] = useState<LifecyclePlan>();
  const [requirements, setRequirements] = useState<Requirement[]>([]);
  const [ready, setReady] = useState('');
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  const [job, setJob] = useState<Job>(), [reopen, setReopen] = useState('');
  const [receipt, setReceipt] = useState<ProducerReceipt>();
  const [scenario, setScenario] = useState('');
  const [tab, setTab] = useState('liquidity');
  const [pages, setPages] = useState<Record<string, TablePage>>({});
  const [instructionText, setInstructionText] = useState('');
  const generation = useRef(0);

  const guarded = async (work: (token: number) => Promise<void>) => {
    const token = ++generation.current; setBusy(true); setError('');
    try { await work(token); }
    catch (e) {
      if (token !== generation.current) return;
      if (e instanceof NativeError && e.requirements.length) { setRequirements(e.requirements); setError(''); }
      else setError(String(e instanceof Error ? e.message : e));
    } finally { if (token === generation.current) setBusy(false); }
  };
  const refresh = async (keepBook?: string, keepPlan?: string) => {
    const head = await nativeApi.head();
    const list = await nativeApi.documents(head.revision);
    setRevision(head.revision); setDocuments(list.documents);
    const books = list.documents.filter(d => d.schema === BOOK), plans = list.documents.filter(d => d.schema === PLAN);
    const b = keepBook && books.some(d => d.name === keepBook) ? keepBook : books[0]?.name ?? '';
    const p = keepPlan && plans.some(d => d.name === keepPlan) ? keepPlan : plans[0]?.name ?? '';
    setBookName(b); setPlanName(p);
    setBook(b ? await nativeApi.book(b, head.revision) : undefined);
    const loaded = p ? await nativeApi.plan(p, head.revision) : undefined;
    setPlan(loaded); setInstructionText(loaded ? JSON.stringify(loaded.instructions, null, 2) : '');
  };
  useEffect(() => { void guarded(() => refresh()); return () => { generation.current++; }; }, []);

  const saveExample = () => guarded(async () => {
    const example = await nativeApi.example();
    const head = await nativeApi.head();
    const afterBook = await nativeApi.save('worked-unwind-book', example.book, head.revision);
    const afterPlan = await nativeApi.save('worked-unwind-plan', example.plan, afterBook.revision);
    await nativeApi.save('worked-unwind-policy-plan', example.policy_plan, afterPlan.revision);
    await refresh('worked-unwind-book', 'worked-unwind-plan');
  });
  const selectBook = (name: string) => guarded(async () => { setBookName(name); setBook(await nativeApi.book(name, revision)); setRequirements([]); setReady(''); });
  const selectPlan = (name: string) => guarded(async () => {
    setPlanName(name); const p = await nativeApi.plan(name, revision);
    setPlan(p); setInstructionText(JSON.stringify(p.instructions, null, 2)); setRequirements([]); setReady('');
  });
  const edit = (change: (draft: LifecyclePlan) => void) => {
    setPlan(previous => { if (!previous) return previous; const draft = structuredClone(previous); change(draft); return draft; });
    setRequirements([]); setReady('');
  };
  const editInstructions = (text: string) => {
    setInstructionText(text);
    try { const value = JSON.parse(text); edit(d => { d.instructions = value; }); setError(''); }
    catch { setError('The instructions JSON is not valid yet.'); }
  };

  const records = book?.plan;
  const lots = new Map((records?.lots ?? []).map(l => [l.lot_id, l]));
  const touched = new Set((plan?.actions ?? []).flatMap(a => a.lots.map(l => l.lot_id)));
  const affectedHedges = (records?.hedge_allocations ?? []).filter(h => touched.has(h.exposure_lot_id) || touched.has(h.hedge_lot_id));
  const affectedFunding = (records?.funding_allocations ?? []).filter(f => touched.has(f.attributed_lot_id));
  const formalSelected = (plan?.actions ?? []).some(a => a.accounting_treatment === 'formal');
  const mode = plan?.business_plan?.mode ?? 'schedule';
  const policy = plan?.business_plan?.funding_policy;
  const setMode = (next: 'schedule' | 'runoff' | 'policy') => edit(d => {
    if (next === 'schedule') { delete d.business_plan; return; }
    if (next === 'runoff') { d.business_plan = { mode: 'runoff' }; return; }
    const horizon = Math.max(...(book?.scenarios ?? []).map(sc => sc.horizon_end_ordinal), records?.as_of_ordinal ?? 0);
    d.business_plan = { mode: 'policy', funding_policy: d.business_plan?.funding_policy ?? {
      policy: { id: 'treasury-funding', revision: 'r1' }, minimum_usable_cash: 0, repay_surplus: true,
      facilities: (records?.facilities ?? []).map(f => ({ facility_id: f.facility_id, access: [{ from_ordinal: records!.as_of_ordinal, to_ordinal: horizon }] })),
    } };
  });
  const planned = useMemo(() => Object.entries(plan?.instructions ?? {}).flatMap(([kind, list]) => (list as Json[] ?? []).map(i => ({
    status: 'planned', instruction: i.instruction_id ?? (kind === 'initial_margin' ? `im:${i.agreement_id}` : kind), kind: INSTRUCTION_LABELS[kind] ?? kind,
    lot: i.lot_id ?? i.facility_id ?? i.agreement_id ?? '',
    date: iso(i.settlement_ordinal ?? i.repayment_ordinal ?? i.ordinal ?? i.effective_ordinal),
    amount: money(i.amount ?? i.principal ?? i.quantity ?? i.posted_requirement),
    'depends on': (i.depends_on ?? []).join(', '),
  }))), [plan]);

  const check = () => guarded(async () => {
    if (!plan || revision === undefined) return;
    setRequirements([]); setReady('');
    const c = await nativeApi.compose(bookName, plan, revision);
    setReady(c.request_sha256);
  });
  const savePlan = () => guarded(async () => {
    if (!plan || revision === undefined) return;
    const name = planName || 'lifecycle-plan';
    await nativeApi.save(name, plan, revision);
    await refresh(bookName, name);
  });
  const show = async (id: string, token: number) => {
    const view = await nativeApi.job(id);
    if (token !== generation.current) return;
    setJob(view); setReopen(id);
    if (view.status !== 'native_done') { setReceipt(undefined); setPages({}); return; }
    const r = await nativeApi.receipt(id);
    const loaded: Record<string, TablePage> = {};
    for (const t of TABLES) {
      try { loaded[t.id] = await nativeApi.table(id, t.id, 0, 100); } catch { /* table absent from this run */ }
    }
    if (token !== generation.current) return;
    setReceipt(r); setPages(loaded);
    setScenario(s => r.workflow.source_receipt.journals.some(j => j.scenario_id === s) ? s : r.workflow.source_receipt.journals[0]?.scenario_id ?? '');
  };
  const run = () => guarded(async token => {
    if (!plan || revision === undefined) return;
    setRequirements([]); setReceipt(undefined); setPages({});
    const admitted = await nativeApi.submit(bookName, plan, revision, `${bookName}:${planName}:${revision}:${Date.now()}`);
    const done = await awaitNativeJob(admitted.job_id, j => { if (token === generation.current) setJob(j); });
    await show(done.id, token);
  });
  const open = () => guarded(token => show(reopen.trim(), token));
  const cancel = () => guarded(async () => { if (job) { await nativeApi.cancel(job.id); setJob(await nativeApi.job(job.id)); } });
  const page = (id: string, offset: number) => guarded(async () => {
    if (!job) return; const p = await nativeApi.table(job.id, id, offset, 100); setPages(ps => ({ ...ps, [id]: p }));
  });
  const download = (id: string) => guarded(async () => {
    if (!job) return;
    // Page by page: the browser never asks the service to hydrate the whole stream.
    const rows: unknown[] = []; let offset = 0, total = 1;
    while (offset < total) { const p = await nativeApi.table(job.id, id, offset, 1000); rows.push(...p.rows); total = p.total; offset += 1000; }
    const url = URL.createObjectURL(new Blob([JSON.stringify({ job: job.id, table: id, rows }, null, 1)], { type: 'application/json' }));
    const a = document.createElement('a'); a.href = url; a.download = `${id}-${job.id}.json`; a.click(); URL.revokeObjectURL(url);
  });

  const journal = receipt?.workflow.source_receipt.journals.find(j => j.scenario_id === scenario);
  const steps: LiquidityStep[] = journal?.receipt.steps ?? [];
  const last = steps[steps.length - 1];
  const stepRows = steps.map(s => ({
    date: iso(s.ordinal), event: s.event_id, phase: String(s.phase), status: s.status, 'cash movement': money(s.cash_movement),
    'usable cash after': money(s.usable_cash_after), 'restricted cash': money(s.restricted_cash_after),
    'facility drawn': money(s.facility_drawn_after), 'undrawn capacity (not cash)': money(s.facility_undrawn_after),
    'pledged quantity': money(s.pledged_quantity_after), 'posted VM': money(s.posted_vm_after), 'posted IM': money(s.posted_im_after),
    'depends on': s.depends_on.join(', '),
  }));
  const obligationRows = steps.map(s => ({
    date: iso(s.ordinal), event: s.event_id, status: s.status,
    'pending receipt after': money(s.pending_receivable_after), 'pending payment after': money(s.pending_payable_after),
    'depends on': s.depends_on.join(', '),
  }));
  const decisionRows = (journal?.receipt.policy_decisions ?? []).map(d => ({
    date: iso(d.ordinal), decision: words(d.kind), facility: d.facility_id, generation: String(d.generation),
    amount: money(d.amount), trigger: d.trigger, 'usable cash before': money(d.usable_cash_before),
    'usable cash after': money(d.usable_cash_after), 'need covered': d.kind === 'draw' ? money(d.need) : '',
  }));
  const monthlyRows = (journal?.receipt.monthly ?? []).map(m => ({
    month: m.month, 'opening cash': money(m.opening_cash), 'cash in': money(m.cash_in), 'cash out': money(m.cash_out),
    'net cash': money(m.net_cash), 'closing cash': money(m.closing_cash),
    // The lowest usable cash after any event in the month: a positive close cannot hide a dip.
    'minimum usable cash (any event)': money(m.minimum_usable_cash), 'closing usable cash': money(m.closing_usable_cash),
    'facility drawn': money(m.closing_facility_drawn), 'interest and fees': money(m.interest_expense + m.fees),
    'margin posted / returned': `${money(m.margin_posted)} / ${money(m.margin_returned)}`,
    'funding decisions': String(m.policy_decisions), 'failed / blocked': `${m.failed_events} / ${m.blocked_events}`,
  }));
  const factRows = (journal?.receipt.accounting_facts ?? []).map(f => ({
    kind: f.kind, lot: f.lot_id, quantity: money(f.quantity), 'residual quantity': money(f.retained_quantity),
    effective: iso(f.effective_ordinal), settles: iso(f.settlement_ordinal), consideration: money(f.consideration),
    'removed carrying': money(f.removed_carrying), realized: money(f.realized),
    'economic links': f.economic_allocation_ids.join(', '),
    // Formal relationship identities appear only when an accepted formal owner accounted for them.
    'formal relationships': receipt?.workflow.source_receipt.formal_hedge_accounting ? f.formal_relationship_ids.join(', ') : 'none (economic links only)',
  }));
  // The funding-need explanation names native steps and their native amounts only.
  const bridgeIndex = steps.findIndex(s => s.cash_movement > 0 && s.facility_drawn_after > 0);
  const bridge = bridgeIndex >= 0 ? steps[bridgeIndex] : undefined;
  const beforeBridge = bridgeIndex > 0 ? steps[bridgeIndex - 1].usable_cash_after : journal?.receipt.opening_cash;
  const bridgeUsers = bridge ? steps.filter(s => s.depends_on.includes(bridge.event_id) && s.cash_movement < 0) : [];
  const saleAfter = steps.find(s => s.cash_movement > 0 && s.depends_on.length > 0 && s !== bridge);
  const vmUsable = steps.find(s => s.event_id.startsWith('vm_return'));
  const imUsable = steps.find(s => s.event_id.startsWith('im_return'));

  return <div className="space-y-3">
    <Card><CardHeader title="Liquidity lifecycle" sub="Saved book · acquisition and unwind plans · settlement dependencies · verified native publication" /><CardBody>
      <p className="mb-3 text-xs text-paper-faint">Runs through the Rust workbench service: composition from a saved book revision, the native lifecycle process, independent Rust verification and checksummed publication. Amounts come from the verified native receipt and published tables. Economic hedge links never confer formal hedge accounting.</p>
      <div className="flex flex-wrap items-end gap-2">
        <Field label="Saved book"><Select label="Saved book" value={bookName} options={['', ...documents.filter(d => d.schema === BOOK).map(d => d.name)]} onChange={v => void selectBook(v)} disabled={busy} /></Field>
        <Field label="Plan"><Select label="Plan" value={planName} options={['', ...documents.filter(d => d.schema === PLAN).map(d => d.name)]} onChange={v => void selectPlan(v)} disabled={busy} /></Field>
        <span className="text-xs text-paper-dim">Revision {revision ?? '—'}</span>
        <Button variant="secondary" size="sm" disabled={busy} onClick={() => void saveExample()}>Save worked example as a saved book</Button>
      </div>
      {error && <p role="alert" className="mt-2 text-xs text-danger">{error}</p>}
    </CardBody></Card>

    {records && <Card><CardHeader title="Saved book" sub={`${bookName} · ${records.entity} · ${records.currency} · as of ${iso(records.as_of_ordinal)}`} /><CardBody className="space-y-3">
      <section aria-label="Opening accounts"><div className="eyebrow">Opening accounts</div>
        <DataTable rows={records.accounts.map(a => ({ account: a.account_id, cash: money(a.cash), equity: money(a.equity) }))} /></section>
      <section aria-label="Lots"><div className="eyebrow">Lots</div>
        <DataTable rows={records.lots.map(l => ({ lot: l.lot_id, position: l.position_id, kind: l.kind, category: words(l.accounting_category), quantity: money(l.quantity), 'original cost': money(l.original_cost), account: l.account_id }))} /></section>
      <section aria-label="Funding and pledges" className="grid gap-3 md:grid-cols-2">
        <div><div className="eyebrow">Funding allocations</div>
          <DataTable rows={(records.funding_allocations ?? []).map(f => ({ allocation: f.allocation_id, funding: f.funding_lot_id, finances: f.attributed_lot_id, amount: money(f.amount), 'book instruction': f.instruction }))} /></div>
        <div><div className="eyebrow">Pledges</div>
          <DataTable rows={(records.pledges ?? []).map(p => ({ pledge: p.pledge_id, lot: p.lot_id, to: p.agreement_id, holder: p.holder, quantity: money(p.quantity), haircut: p.haircut }))} /></div>
      </section>
      <section aria-label="Margin and facilities" className="grid gap-3 md:grid-cols-2">
        <div><div className="eyebrow">Margin agreements</div>
          <DataTable rows={(records.margin_agreements ?? []).map(a => ({ agreement: a.agreement_id, counterparty: a.counterparty, segregation: a.segregation, 'VM at termination': words(a.vm_at_termination), 'IM': words(a.initial_margin), 'lag days': String(a.settlement_lag_days) }))} /></div>
        <div><div className="eyebrow">Facilities (capacity, not cash)</div>
          <DataTable rows={(records.facilities ?? []).map(f => ({ facility: f.facility_id, provider: f.provider, commitment: money(f.commitment), drawn: money(f.drawn), undrawn: money(f.commitment - f.drawn), expires: iso(f.expiry_ordinal) }))} /></div>
      </section>
      <section aria-label="Hedge allocations"><div className="eyebrow">Hedge allocations</div>
        <DataTable rows={(records.hedge_allocations ?? []).map(h => ({ allocation: h.allocation_id, exposure: h.exposure_lot_id, hedge: h.hedge_lot_id, amount: money(h.amount), status: h.formal_relationship_id ? `formal relationship ${h.formal_relationship_id} (needs an accepted accounting owner)` : 'economic link (undesignated)' }))} /></section>
    </CardBody></Card>}

    {plan && records && <Card><CardHeader title="Plan" sub={`${planName || 'unsaved plan'} · actions, treatments and instructions`} /><CardBody className="space-y-3">
      {plan.actions.map((a, ai) => <section key={a.action_id} aria-label={`Action ${a.action_id}`} className="space-y-2 rounded border border-line p-2">
        <div className="flex flex-wrap items-end gap-2">
          <span className="text-sm font-medium text-paper">{a.action_id}</span>
          <Field label="Kind"><Select label={`Kind of ${a.action_id}`} value={a.kind} options={ACTION_KINDS} onChange={v => edit(d => { d.actions[ai].kind = v as typeof a.kind; })} /></Field>
          <Field label="Accounting treatment"><Select label={`Accounting treatment of ${a.action_id}`} value={a.accounting_treatment} options={['economic', 'formal']} onChange={v => edit(d => { d.actions[ai].accounting_treatment = v as 'economic' | 'formal'; })} /></Field>
          <span className="text-xs text-paper-dim">Trade {iso(a.trade_ordinal)} · effective {iso(a.effective_ordinal)} · settles {iso(a.settlement_ordinal)}</span>
        </div>
        <table className="text-xs"><tbody>{a.lots.map((l, li) => {
          const held = lots.get(l.lot_id)?.quantity ?? 0;
          return <tr key={l.lot_id}><td className="pr-3">{l.lot_id}</td>
            <td className="pr-3"><Input aria-label={`Quantity of ${l.lot_id}`} type="number" value={l.quantity} onChange={e => edit(d => { d.actions[ai].lots[li].quantity = Number(e.target.value); })} className="w-28" /></td>
            <td className="pr-3">of {money(held)}</td>
            <td>{a.kind === 'acquisition' ? <Badge>acquisition</Badge> : l.quantity === held ? <Badge tone="info">full exit</Badge> : <Badge tone="warning">partial exit</Badge>}</td></tr>;
        })}</tbody></table>
      </section>)}
      {formalSelected && <p className="text-xs text-warning">Formal treatment needs an implemented ASC 815 owner for this lane. The run is refused (FORMAL_OWNER_REQUIRED) rather than treated as economic, and no qualified status is shown.</p>}
      <section aria-label="Hedge and financing treatment" className="grid gap-3 md:grid-cols-2">
        <div className="space-y-1"><div className="eyebrow">Hedge treatment</div>
          {affectedHedges.length ? affectedHedges.map(h => <Field key={h.allocation_id} label={`${h.allocation_id}: ${h.exposure_lot_id} hedged by ${h.hedge_lot_id}`}>
            <Select label={`Hedge treatment for ${h.allocation_id}`} value={plan.hedge_treatments?.[h.allocation_id] ?? ''} options={HEDGE_TREATMENTS}
              onChange={v => edit(d => { d.hedge_treatments = { ...(d.hedge_treatments ?? {}) }; if (v) d.hedge_treatments[h.allocation_id] = v as 'terminate'; else delete d.hedge_treatments[h.allocation_id]; })} /></Field>)
            : <p className="text-xs text-paper-faint">No hedge allocation touches these lots.</p>}</div>
        <div className="space-y-1"><div className="eyebrow">Financing treatment</div>
          {affectedFunding.length ? affectedFunding.map(f => <Field key={f.allocation_id} label={`${f.allocation_id}: ${f.funding_lot_id} finances ${f.attributed_lot_id}`}>
            <Select label={`Financing treatment for ${f.allocation_id}`} value={plan.funding_instructions?.[f.allocation_id] ?? ''} options={FUNDING_TREATMENTS}
              onChange={v => edit(d => { d.funding_instructions = { ...(d.funding_instructions ?? {}) }; if (v) d.funding_instructions[f.allocation_id] = v as 'repay'; else delete d.funding_instructions[f.allocation_id]; })} /></Field>)
            : <p className="text-xs text-paper-faint">No funding allocation finances these lots.</p>}</div>
      </section>
      <section aria-label="Business plan" className="space-y-2 rounded border border-line p-2">
        <div className="eyebrow">Business plan</div>
        <div className="flex flex-wrap items-end gap-2">
          <Field label="Mode"><Select label="Business plan mode" value={mode} options={['schedule', 'runoff', 'policy']} onChange={v => setMode(v as 'schedule' | 'runoff' | 'policy')} /></Field>
          {policy && <>
            <Field label="Minimum usable cash"><Input aria-label="Minimum usable cash" type="number" value={policy.minimum_usable_cash}
              onChange={e => edit(d => { d.business_plan!.funding_policy!.minimum_usable_cash = Number(e.target.value); })} className="w-32" /></Field>
            <label className="flex items-center gap-1 text-xs text-paper-dim"><input type="checkbox" aria-label="Repay surplus" checked={policy.repay_surplus ?? false}
              onChange={e => edit(d => { d.business_plan!.funding_policy!.repay_surplus = e.target.checked; })} />Repay drawn facilities from surplus at the end of each day</label>
          </>}
        </div>
        <p className="text-xs text-paper-faint">{mode === 'runoff' ? 'Runoff executes no actions: only contractual events move.'
          : mode === 'policy' ? 'Each payment is funded from the cash, facilities and obligations known at that moment. Draws need a declared access window and an unexpired facility; renewal is bounded and explicit. No perpetual funding or horizon payoff is invented.'
          : 'The supplied instructions run exactly as scheduled.'}</p>
        {policy && <table className="text-xs"><tbody>{policy.facilities.map((f, fi) => <tr key={f.facility_id}>
          <td className="pr-3">{f.facility_id}</td>
          <td className="pr-3">access</td>
          <td className="pr-1"><Input aria-label={`Access from for ${f.facility_id}`} type="date" value={iso(f.access[0]?.from_ordinal)}
            onChange={e => edit(d => { d.business_plan!.funding_policy!.facilities[fi].access = [{ from_ordinal: ordinalOf(e.target.value), to_ordinal: f.access[0]?.to_ordinal ?? ordinalOf(e.target.value) }]; })} /></td>
          <td className="pr-3"><Input aria-label={`Access to for ${f.facility_id}`} type="date" value={iso(f.access[0]?.to_ordinal)}
            onChange={e => edit(d => { d.business_plan!.funding_policy!.facilities[fi].access = [{ from_ordinal: f.access[0]?.from_ordinal ?? ordinalOf(e.target.value), to_ordinal: ordinalOf(e.target.value) }]; })} /></td>
          <td>{f.renewal ? `renews up to ${f.renewal.max_generations} generations of ${f.renewal.tenor_days} days` : 'no renewal'}</td>
        </tr>)}</tbody></table>}
      </section>
      <section aria-label="Planned obligations"><div className="eyebrow">Planned obligations</div><DataTable rows={planned} /></section>
      <Accordion id="lifecycle-instructions" title="Edit instructions (JSON)" summary="Sales, repayments with pledge releases, terminations, facility draws and repayments, supplied IM">
        <textarea aria-label="Instructions JSON" value={instructionText} onChange={e => editInstructions(e.target.value)} rows={16}
          className="w-full rounded border border-line-strong bg-surface-base p-2 font-mono text-xs text-paper" />
      </Accordion>
      {requirements.length > 0 && <Requirements list={requirements} />}
      {ready && <p className="text-xs text-up">Mapping complete. Composed request {ready.slice(0, 12)}… is bound to {bookName} at revision {revision}.</p>}
      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" disabled={busy || !bookName} onClick={() => void check()}>Check mapping</Button>
        <Button variant="secondary" disabled={busy} onClick={() => void savePlan()}>Save plan</Button>
        <Button disabled={busy || !bookName} onClick={() => void run()}>Run</Button>
      </div>
    </CardBody></Card>}

    <Card><CardHeader title="Run" sub="Durable native job · original deadline · independent verification" /><CardBody className="space-y-2">
      <div className="flex flex-wrap items-end gap-2">
        <Field label="Reopen a job"><Input aria-label="Job id" value={reopen} onChange={e => setReopen(e.target.value)} className="w-80" /></Field>
        <Button variant="secondary" size="sm" disabled={busy || !reopen.trim()} onClick={() => void open()}>Reopen</Button>
        {job && (job.status === 'native_queued' || job.status === 'native_running') && <Button variant="ghost" size="sm" onClick={() => void cancel()}>Cancel</Button>}
      </div>
      {job && <div className="space-y-1 text-xs" aria-label="Job status">
        <div className="flex flex-wrap items-center gap-2"><Badge tone={statusTone[job.status] ?? 'neutral'}>{words(job.status.replace('native_', ''))}</Badge>
          <span>Job {job.id}</span><span>attempt {job.attempt}</span><span>deadline {new Date(job.authority.deadline_at * 1000).toLocaleString()}</span>
          {job.published && <Badge tone="up">verified and published</Badge>}</div>
        {job.detail && <p role="status" className="text-danger">{job.detail}</p>}
      </div>}
    </CardBody></Card>

    {receipt && job && <section aria-label="Lifecycle results"><Card><CardHeader title="Results" sub="Independently verified native receipt and published tables" right={
      <Select label="Scenario" value={scenario} options={receipt.workflow.source_receipt.journals.map(j => j.scenario_id)} onChange={setScenario} />} /><CardBody className="space-y-3">
      <div className="flex flex-wrap gap-2 text-xs">
        <Badge tone="up">independent Rust verification passed</Badge>
        <Badge tone="neutral">formal hedge accounting: {receipt.workflow.source_receipt.formal_hedge_accounting ? 'accounted by the formal owner' : 'not selected (economic links only)'}</Badge>
        {journal?.receipt.open_claims && <Badge tone="warning">claims remain open at the horizon</Badge>}
      </div>
      {bridge && <p className="text-xs text-paper-dim" aria-label="Funding need">
        Facility draw <code>{bridge.event_id}</code> adds {money(bridge.cash_movement)} on {iso(bridge.ordinal)}; usable cash before it was {money(beforeBridge)}.
        {bridgeUsers.map(u => <span key={u.event_id}> <code>{u.event_id}</code> pays {money(-u.cash_movement)} and depends on it, leaving usable cash {money(u.usable_cash_after)}.</span>)}
        {(journal?.receipt.policy_decisions ?? []).filter(d => d.kind === 'draw').map(d => <span key={d.decision_id}> The funding policy drew it for <code>{d.trigger}</code>, which needed {money(d.need)} with usable cash at {money(d.usable_cash_before)}.</span>)}
        {saleAfter && <> <code>{saleAfter.event_id}</code> settles only after {saleAfter.depends_on.join(', ')}.</>}
        {vmUsable && <> Posted VM returns on {iso(vmUsable.ordinal)}, raising usable cash to {money(vmUsable.usable_cash_after)}.</>}
        {imUsable && <> Posted IM returns on {iso(imUsable.ordinal)}, raising usable cash to {money(imUsable.usable_cash_after)}.</>}
      </p>}
      <Tabs id="lifecycle-results" tabs={RESULT_TABS.map(t => t.label)} active={labelOf(tab)} onChange={l => setTab(RESULT_TABS.find(t => t.label === l)?.id ?? 'liquidity')} />
      <div {...tabPanelProps('lifecycle-results', labelOf('liquidity'), labelOf(tab))}>{tab === 'liquidity' && <DataTable rows={stepRows} />}</div>
      <div {...tabPanelProps('lifecycle-results', labelOf('obligations'), labelOf(tab))}>{tab === 'obligations' && <DataTable rows={obligationRows} />}</div>
      <div {...tabPanelProps('lifecycle-results', labelOf('collateral'), labelOf(tab))}>{tab === 'collateral' && last && <DataTable rows={[
        { measure: 'Usable cash', value: money(last.usable_cash_after) },
        { measure: 'Restricted cash (segregated received VM)', value: money(last.restricted_cash_after) },
        { measure: 'Facility drawn', value: money(last.facility_drawn_after) },
        { measure: 'Undrawn facility capacity (not cash)', value: money(last.facility_undrawn_after) },
        { measure: 'Pledged securities (quantity)', value: money(last.pledged_quantity_after) },
        { measure: 'Posted VM / received VM', value: `${money(last.posted_vm_after)} / ${money(last.received_vm_after)}` },
        { measure: 'Posted IM / received IM in custody', value: `${money(last.posted_im_after)} / ${money(last.received_im_custody_after)}` },
        { measure: 'Pending receipts / payments', value: `${money(last.pending_receivable_after)} / ${money(last.pending_payable_after)}` },
        { measure: 'Residual securities / derivative assets / derivative liabilities', value: `${money(last.security_quantity_after)} / ${money(last.derivative_assets_after)} / ${money(last.derivative_liabilities_after)}` },
      ]} />}</div>
      <div {...tabPanelProps('lifecycle-results', labelOf('facts'), labelOf(tab))}>{tab === 'facts' && <DataTable rows={factRows} />}</div>
      <div {...tabPanelProps('lifecycle-results', labelOf('decisions'), labelOf(tab))}>{tab === 'decisions' && (decisionRows.length ? <DataTable rows={decisionRows} />
        : <p className="text-xs text-paper-faint">No funding policy ran: the supplied schedule executed as given.</p>)}</div>
      <div {...tabPanelProps('lifecycle-results', labelOf('monthly'), labelOf(tab))}>{tab === 'monthly' && (monthlyRows.length ? <DataTable rows={monthlyRows} />
        : <p className="text-xs text-paper-faint">Monthly reports are produced when a business plan mode is selected.</p>)}</div>
      {TABLES.map(t => <div key={t.id} {...tabPanelProps('lifecycle-results', labelOf(t.id), labelOf(tab))}>{tab === t.id && (pages[t.id] ? <div className="space-y-1">
        <div className="flex items-center justify-end gap-2 text-xs text-paper-dim">
          <span>{pages[t.id].offset + 1}–{Math.min(pages[t.id].offset + pages[t.id].rows.length, pages[t.id].total)} of {pages[t.id].total}</span>
          <Button size="sm" variant="ghost" disabled={busy || pages[t.id].offset === 0} onClick={() => void page(t.id, Math.max(0, pages[t.id].offset - 100))}>Previous</Button>
          <Button size="sm" variant="ghost" disabled={busy || pages[t.id].offset + 100 >= pages[t.id].total} onClick={() => void page(t.id, pages[t.id].offset + 100)}>Next</Button>
          <Button size="sm" variant="secondary" disabled={busy} onClick={() => void download(t.id)}>Download</Button>
        </div>
        <DataTable rows={pages[t.id].rows.map(r => Object.fromEntries(Object.entries(r).map(([k, v]) => [words(k), typeof v === 'object' ? JSON.stringify(v) : v])))} />
      </div> : <p className="text-xs text-paper-faint">Not published by this run.</p>)}</div>)}
    </CardBody></Card></section>}
  </div>;
}
