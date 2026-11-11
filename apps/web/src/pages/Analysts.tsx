import { useCallback, useEffect, useRef, useState } from 'react';
import { FileSearch, MessageSquare, Square } from 'lucide-react';
import { analystApi, api, type AnalystConversation, type AnalystEvidence, type AnalystEvidenceSummary, type AnalystRole, type AnalystStatus, type AnalystWorkflow } from '../lib/api';
import { Badge, Button, Card, CardBody, CardHeader } from '../components/ui';

const WORKFLOWS: Array<{ id: AnalystWorkflow; title: string; prompt: string }> = [
  { id: 'morning-review', title: 'Morning review', prompt: 'Review the latest completed runs. Identify stale or missing results, operational exceptions, and the next actions supported by evidence.' },
  { id: 'explain-change', title: 'Explain a change', prompt: 'Compare these two job IDs: [first job] and [second job]. Check compatibility and explain only differences supported by published evidence.' },
  { id: 'job-recovery', title: 'Recover job visibility', prompt: 'Find this original job ID or idempotency key: [identifier]. Report its status, evidence, and next action without resubmitting or cancelling it.' },
];
const fieldClass = 'w-full rounded-sm border border-line-strong bg-surface-base p-2 text-base text-paper focus:border-brand';

function Evidence({ conversation, item }: { conversation: string; item: AnalystEvidenceSummary }) {
  const [value, setValue] = useState<AnalystEvidence | null>(null);
  const [error, setError] = useState('');
  return <details className="mt-2 rounded-sm border border-line p-2" onToggle={event => {
    if (event.currentTarget.open && !value) void analystApi.evidence(conversation, item.id).then(setValue).catch(e => setError(String(e)));
  }}>
    <summary className="cursor-pointer break-all text-xs text-paper">[{item.id}] {item.tool}</summary>
    {error && <p role="alert" className="text-sm text-danger">{error}</p>}
    <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap break-all text-xs text-paper-dim">{value ? JSON.stringify({ arguments: value.arguments, result: value.result, sha256: value.sha256 }, null, 2) : 'Loading evidence…'}</pre>
  </details>;
}

export default function Analysts() {
  const [status, setStatus] = useState<AnalystStatus | null>(null);
  const [sessions, setSessions] = useState<AnalystConversation[]>([]);
  const [conversation, setConversation] = useState<AnalystConversation | null>(null);
  const [selected, setSelected] = useState('');
  const [role, setRole] = useState<AnalystRole>('ops');
  const [workflow, setWorkflow] = useState<AnalystWorkflow>('morning-review');
  const [prompt, setPrompt] = useState(WORKFLOWS[0].prompt);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const request = useRef<{ request_id: string; workflow: AnalystWorkflow; prompt: string; expected_revision: number } | null>(null);
  const generation = useRef(0);
  const active = conversation?.turns?.find(t => t.status === 'running');

  useEffect(() => {
    let live = true;
    void analystApi.status().then(value => {
      if (!live) return;
      setStatus(value);
      if (value.available || value.history_available) void analystApi.list().then(items => { if (live) setSessions(items); })
        .catch(e => { if (live) setError(String(e)); });
    }).catch(e => { if (live) setError(String(e)); });
    return () => { live = false; };
  }, []);

  const refresh = useCallback(async (id: string, version: number) => {
    const value = await analystApi.get(id);
    if (generation.current === version) setConversation(value);
    return value;
  }, []);

  useEffect(() => {
    if (!selected) return;
    const version = ++generation.current;
    let live = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const value = await refresh(selected, version);
        if (live && value.turns?.some(t => t.status === 'running')) timer = setTimeout(() => { void poll(); }, 1000);
      } catch (e) { if (live) setError(String(e)); }
    };
    void poll();
    return () => { live = false; clearTimeout(timer); ++generation.current; };
  }, [selected, busy, refresh]);

  async function send() {
    if (!prompt.trim() || busy || active) return;
    setBusy(true); setError('');
    try {
      let item = conversation;
      if (!selected || !item) {
        item = await analystApi.create(role, WORKFLOWS.find(w => w.id === workflow)!.title);
        setSessions(items => [item!, ...items]);
        setConversation(item); setSelected(item.id);
      }
      if (!request.current || request.current.prompt !== prompt || request.current.workflow !== workflow) {
        const state = await api.state();
        request.current = { request_id: crypto.randomUUID(), workflow, prompt, expected_revision: state.revision };
      }
      // An uncertain HTTP outcome retains exactly the same request ID, prompt and revision.
      await analystApi.ask(item.id, request.current);
      request.current = null;
      await refresh(item.id, generation.current);
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }

  function newConversation() {
    ++generation.current; setSelected(''); setConversation(null); request.current = null; setError('');
  }

  return <div className="mx-auto flex max-w-6xl flex-col gap-3">
    <Card>
      <CardHeader title="Analysts" sub="Review results, explain changes, and recover job visibility" right={<Badge tone="ai">Read-only pilot</Badge>} />
      <CardBody>
        <p className="text-sm text-paper-dim">Answers cite captured evidence. Saved books, financial jobs, and publication remain under your control.</p>
        {!status && !error && <p role="status" className="mt-3 text-paper-faint">Checking analyst availability…</p>}
        {status && !status.available && <div role="status" className="mt-3 border-l-2 border-line-strong pl-3">
          <p className="font-medium text-paper-heading">Analysts are not connected</p>
          <p className="mt-1 text-sm text-paper-dim">{status.reason}. Your workspace and existing analyses remain available.</p>
        </div>}
        {status?.available && <p className="mt-2 text-xs text-paper-faint">{status.provider} · {status.model}</p>}
      </CardBody>
    </Card>
    <div className="grid gap-3 lg:grid-cols-[240px_minmax(0,1fr)]">
      <Card><CardBody className="flex flex-col gap-3">
        <label className="text-sm text-paper-dim">Analyst
          <select aria-label="Analyst role" className={fieldClass + ' mt-1'} value={conversation?.role ?? role}
            disabled={!!selected || busy} onChange={e => setRole(e.target.value as AnalystRole)}>
            <option value="ops">Operations Analyst</option><option value="strategy">Portfolio Analyst</option>
          </select>
        </label>
        <Button variant="secondary" disabled={busy} onClick={newConversation}><MessageSquare size={14} /> New conversation</Button>
        <div className="eyebrow mt-2">Conversations</div>
        {sessions.length === 0 && <p className="text-sm text-paper-faint">Your conversations will appear here.</p>}
        {sessions.map(s => <button key={s.id} className={`rounded-sm border p-2 text-left text-sm ${selected === s.id ? 'border-line-strong bg-surface-2 text-paper-heading' : 'border-transparent text-paper-dim hover:bg-surface-2'}`}
          aria-pressed={selected === s.id} disabled={busy} onClick={() => { request.current = null; setConversation(null); setSelected(s.id); setError(''); }}>
          {s.title}<span className="mt-1 block text-xs text-paper-faint">{s.role === 'ops' ? 'Operations' : 'Portfolio'}</span>
        </button>)}
      </CardBody></Card>
      <div className="flex min-w-0 flex-col gap-3">
        {conversation?.turns?.map(turn => <Card key={turn.id}><CardBody>
          <p className="whitespace-pre-wrap text-base font-medium text-paper-heading">{turn.prompt}</p>
          <div className="my-3 flex flex-wrap items-center gap-2"><Badge tone={turn.status === 'error' ? 'danger' : 'neutral'}>{turn.status}</Badge>
            <span className="text-xs text-paper-faint">Revision {turn.revision} · Draft explanation</span></div>
          <div className="whitespace-pre-wrap break-words text-base leading-relaxed text-paper">{turn.answer || (turn.status === 'running' ? 'Reading workspace evidence…' : 'No answer was completed.')}</div>
          {turn.detail && <p className="mt-3 text-sm text-paper-dim">{turn.detail}</p>}
          {(turn.evidence?.length ?? 0) > 0 && <details className="mt-4 border-t border-line pt-3"><summary className="cursor-pointer text-sm text-paper-dim">Evidence ({turn.evidence.length})</summary>
            {turn.evidence.map(e => <Evidence key={e.id} conversation={conversation.id} item={e} />)}
          </details>}
        </CardBody></Card>)}
        <Card><CardBody>
          <div className="mb-3 flex flex-wrap gap-2">{WORKFLOWS.map(w => <Button key={w.id} variant="secondary" size="sm"
            aria-pressed={workflow === w.id} disabled={busy || !!active} onClick={() => { setWorkflow(w.id); setPrompt(w.prompt); request.current = null; }}>
            <FileSearch size={14} />{w.title}</Button>)}</div>
          <label htmlFor="analyst-prompt" className="sr-only">Ask your analyst</label>
          <textarea id="analyst-prompt" className={fieldClass + ' min-h-28 resize-y'} maxLength={8000} value={prompt}
            disabled={busy || !!active} onChange={e => setPrompt(e.target.value)} />
          {error && <p role="alert" className="mt-2 break-words text-sm text-danger">{error}</p>}
          <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
            <p className="text-xs text-paper-faint">Stopping an answer does not cancel financial jobs.</p>
            {active && conversation ? <Button variant="secondary" onClick={() => {
              const id = conversation.id, version = generation.current;
              void analystApi.stop(id, active.id).then(() => refresh(id, version)).catch(e => {
                if (generation.current === version) setError(String(e));
              });
            }}><Square size={14} /> Stop answer</Button> : <Button disabled={!status?.available || busy || !prompt.trim() || (!!selected && !conversation)} onClick={() => { void send(); }}>
              {busy ? 'Starting…' : 'Ask analyst'}</Button>}
          </div>
        </CardBody></Card>
      </div>
    </div>
  </div>;
}
