/** Positions — the hierarchical balance sheet. Side → book → position
 * drill-down; spot balances edit via slider popovers and AUTO-BALANCE
 * into a brass "Cash & ST funding" plug on the opposite side; a
 * segmented control swaps the right-hand column group between Summary
 * (yield/OAD/sparkline), Fwd Balance, Fwd NII, and KRD heat. All
 * derived figures are INDICATIVE client-side approximations (each ⓘ
 * says so) — engine-grade numbers come from Risk Desk / NII runs. */
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { SlidersHorizontal } from "lucide-react";
import { api, errorText, fmt$, rowsOf, type BookName, type Row } from "../lib/api";
import { Badge, Button, Card, CardBody, CardHeader, InfoPop, Input, Popover, Spinner } from "../components/ui";

type View = "summary" | "fwd balance" | "fwd nii" | "krd";
const VIEWS: View[] = ["summary", "fwd balance", "fwd nii", "krd"];
const QTRS = [1, 2, 3, 4, 5, 6, 7, 8, 9];
const PILLARS = ["2y", "5y", "10y", "30y"];
const SHORT = 0.0365;

type Pos = { id: string; bal: number; bal0: number; yld: number; oad: number; decay: number; book: string; side: 1 | -1; segment?: string };

/** Each book's balance column, and the key that identifies a row in it. */
const BAL_FIELD: Record<BookName, string> = { mbs: "current_face", loans: "face", debt: "face", deposits: "balance", cds: "balance", mm: "balance" };
const rowKey = (book: string) => (book === "mbs" ? "cusip" : "id");
const DEPOSIT_FIELDS: [string, string][] = [["base", "Monthly base decay"], ["amp", "Flight amplitude"], ["b", "Flight B"], ["g0", "Flight floor g0"]];
const CD_FIELDS = ["Base annual withdrawal", "Amplitude", "B", "g0", "Annual cap"];

function derive(book: string, r: Row): Omit<Pos, "book" | "side" | "bal0"> {
  const n = (k: string) => Number(r[k] ?? 0);
  if (book === "mbs") return { id: String(r.cusip), bal: n("current_face"), yld: n("net_coupon"), oad: Math.min(6.5, 7.5 - 60 * Math.max(0, n("net_coupon") - 0.03)), decay: 0.10 };
  if (book === "loans" || book === "debt") {
    const fl = n("is_float") === 1;
    const yrs = Math.max(0.3, (new Date(String(r.maturity)).getTime() - Date.now()) / 3.15e10);
    return { id: String(r.id), bal: n("face"), yld: fl ? SHORT + n("coupon_or_spread") : n("coupon_or_spread"), oad: fl ? 0.2 : Math.min(yrs * 0.85, 8), decay: 1 / Math.max(1, yrs) };
  }
  if (book === "deposits") {
    const seg = String(r.segment);
    const oad = { DDA: 2.2, NOW: 1.3, SAV: 0.9, MMDA: 0.8 }[seg] ?? 1.0;
    const dec = { DDA: 0.18, NOW: 0.25, SAV: 0.28, MMDA: 0.34 }[seg] ?? 0.25;
    return { id: String(r.id), bal: n("balance"), yld: n("rate_paid"), oad, decay: dec };
  }
  if (book === "cds") {
    const yrs = Math.max(0.2, (new Date(String(r.maturity)).getTime() - Date.now()) / 3.15e10);
    return { id: String(r.id), bal: n("balance"), yld: n("rate"), oad: yrs * 0.9, decay: 1 / Math.max(1, yrs) };
  }
  return { id: String(r.id), bal: n("balance"), yld: Math.max(0, SHORT + n("spread_bp") / 1e4), oad: 0.08, decay: 0 };
}

const balPath = (p: Pos) => QTRS.map(q => p.bal * Math.pow(1 - p.decay, q * 0.25));
const niiPath = (p: Pos) => balPath(p).map(b => (b * p.yld) / 4);
const krdSplit = (p: Pos) => {
  const w = p.oad <= 1.5 ? [0.7, 0.3, 0, 0] : p.oad <= 3.5 ? [0.2, 0.6, 0.2, 0] : p.oad <= 6 ? [0.05, 0.3, 0.55, 0.1] : [0, 0.15, 0.45, 0.4];
  return w.map(x => (x * p.oad * p.bal) / 1e4);
};

const Spark = ({ v, color = "var(--viz-1)" }: { v: number[]; color?: string }) => {
  const [lo, hi] = [Math.min(...v), Math.max(...v)];
  const pts = v.map((x, i) => `${(i / (v.length - 1)) * 64},${18 - ((x - lo) / Math.max(hi - lo, 1e-9)) * 14}`).join(" ");
  return <svg width="68" height="20"><polyline points={pts} fill="none" stroke={color} strokeWidth="1.25" /></svg>;
};
const Heat = ({ v, max, sign = 1 }: { v: number; max: number; sign?: number }) => (
  <div className="num rounded px-1.5 py-0.5 text-right text-xs"
    style={{ background: `color-mix(in srgb, var(${sign * v >= 0 ? "--up-500" : "--down-500"}) ${Math.round(Math.min(0.85, Math.abs(v) / Math.max(max, 1e-9)) * 100)}%, transparent)`, color: "var(--text-heading)" }}>
    {Math.abs(v) >= 1e6 ? (v / 1e6).toFixed(1) + "M" : (v / 1e3).toFixed(0) + "k"}
  </div>
);
const Trend = ({ now, was }: { now: number; was: number }) => {
  const d = now - was;
  if (d === 0 || Math.abs(d) < Math.abs(was) * 1e-6) return null;
  return <span className={`num ml-1 text-2xs ${d > 0 ? "text-up" : "text-down"}`}>{d > 0 ? "▲" : "▼"}{fmt$(Math.abs(d))}</span>;
};

/** Product assumptions for a row's class. Edits apply to every position in that
 * class; a single position cannot be overridden yet, because the engine has no
 * per-position behavior input. Values save to the engine, which then recomputes. */
function AssumptionEdit({ p, assumptions, onSaved }: { p: Pos; assumptions: Row | null; onSaved: () => void }) {
  const [draft, setDraft] = useState<number[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const seg = p.segment ?? "";
  const current: number[] = p.book === "deposits"
    ? DEPOSIT_FIELDS.map(([k]) => Number(((assumptions?.deposit_segments as Record<string, Record<string, number>> | undefined)?.[seg] ?? {})[k] ?? NaN))
    : p.book === "cds" ? ((assumptions?.cd_ew_params as number[] | undefined) ?? [])
    : [];
  const started = draft.length === current.length && draft.length > 0;
  const values = started ? draft : current;

  const save = async () => {
    setError(null); setSaving(true);
    try {
      if (p.book === "deposits") {
        const changed: Record<string, number> = {};
        DEPOSIT_FIELDS.forEach(([k], i) => { if (values[i] !== current[i]) changed[k] = values[i]; });
        if (Object.keys(changed).length) await api.putAssumptions({ deposit_segments: { [seg]: changed } });
      } else if (p.book === "cds") {
        if (values.some((v, i) => v !== current[i])) await api.putAssumptions({ cd_ew_params: values });
      }
      setDraft([]);
      onSaved();
    } catch (e) {
      setError(errorText(e));
    } finally { setSaving(false); }
  };

  const editable = p.book === "deposits" || p.book === "cds";
  return (
    <Popover width="17rem" trigger={
      <span title="Product assumptions" className="inline-flex h-5 w-5 cursor-pointer items-center justify-center rounded-sm text-paper-faint hover:bg-surface-2 hover:text-paper">
        <SlidersHorizontal aria-label="Product assumptions" className="h-3 w-3" strokeWidth={1.5} />
      </span>}>
      <div className="space-y-2">
        <div className="eyebrow">{p.book === "deposits" ? `${seg} deposits` : p.book === "cds" ? "CD product" : p.book === "mbs" ? "MBS prepayment" : p.book.toUpperCase()}</div>
        {p.book === "deposits" && <p className="text-xs text-paper-faint">Applies to every {seg} balance. A single account cannot be overridden yet.</p>}
        {p.book === "cds" && <p className="text-xs text-paper-faint">Applies to all CD positions.</p>}
        {p.book === "mbs" && <p className="text-xs text-paper-faint">Prepay vector is read-only here. Changing it needs an engine restart.</p>}
        {!editable && p.book !== "mbs" && <p className="text-xs text-paper-faint">Rate and term are book fields. Edit them in Book Editor.</p>}
        {!assumptions && editable && <div className="text-xs text-paper-faint">Loading…</div>}
        {assumptions && editable && (p.book === "deposits" ? DEPOSIT_FIELDS : CD_FIELDS.map(l => [l, l] as [string, string])).map(([, label], i) => (
          <label key={label} className="flex items-center gap-2">
            <span className="w-32 text-xs text-paper-dim">{label}</span>
            <Input type="number" step="any" min={0} value={Number.isFinite(values[i]) ? values[i] : ""}
              onChange={e => { const next = [...values]; next[i] = Number(e.target.value); setDraft(next); }} />
          </label>
        ))}
        {p.book === "mbs" && assumptions && (
          <div className="num text-2xs text-paper-faint">{((assumptions.prepay as { names: string[]; vector: number[] } | undefined)?.names ?? []).map((n, i) =>
            <div key={n} className="flex justify-between"><span>{n}</span><span>{Number((assumptions.prepay as { vector: number[] }).vector[i]).toPrecision(4)}</span></div>)}</div>
        )}
        {error && <div role="alert" className="text-xs text-danger">{error}</div>}
        {editable && <Button size="sm" disabled={saving || !assumptions} onClick={save}>{saving ? "Saving…" : "Save assumptions"}</Button>}
      </div>
    </Popover>
  );
}

/** Balance editor popover: slider 0–2× with before/after bars. */
function BalEdit({ p, onSet }: { p: Pos; onSet: (v: number) => void }) {
  const [m, setM] = useState(p.bal / p.bal0);
  return (
    <Popover width="15rem" trigger={
      <span className="num cursor-pointer underline decoration-dotted decoration-brand/60 hover:text-brand">{fmt$(p.bal)}</span>}>
      <div className="space-y-2">
        <div className="eyebrow">Spot balance — balances into the cash/ST-funding plug</div>
        <input type="range" min={0} max={2} step={0.05} value={m} className="w-full accent-brand"
          onChange={e => { const x = Number(e.target.value); setM(x); onSet(p.bal0 * x); }} />
        <div className="flex items-end gap-2">
          {[["was", p.bal0], ["now", p.bal0 * m]].map(([l, v]) => (
            <div key={String(l)} className="flex-1">
              <div className="h-10 rounded-sm bg-surface-3"><div className="rounded-sm bg-brand/70" style={{ height: `${Math.min(100, (Number(v) / (2 * p.bal0)) * 100)}%`, marginTop: "auto" }} /></div>
              <div className="num mt-1 text-2xs text-paper-faint">{String(l)} {fmt$(Number(v))}</div>
            </div>
          ))}
        </div>
        <div className="num text-sm text-paper">{(m * 100).toFixed(0)}% of booked</div>
      </div>
    </Popover>
  );
}

export default function Positions() {
  const [pos, setPos] = useState<Pos[]>([]);
  const [loading, setLoading] = useState(true);
  const [revision, setRevision] = useState(0);
  const [assumptions, setAssumptions] = useState<Row | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  /** Rows as stored in each book, so an edit writes the whole book back. */
  const rawRows = useRef<Partial<Record<BookName, Row[]>>>({});
  /** Balance each position had when first loaded this session. The plug and the
   * trend read against it, so they keep meaning "since you started editing". */
  const baseline = useRef(new Map<string, number>());
  const saveTimers = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  const loadedOnce = useRef(false);
  const [open, setOpen] = useState<Record<string, boolean>>({ Assets: true, Liabilities: true });
  const [view, setView] = useState<View>("summary");

  useEffect(() => {
    const refresh = () => setRevision(value => value + 1);
    window.addEventListener('engine:inputs-changed', refresh);
    return () => window.removeEventListener('engine:inputs-changed', refresh);
  }, []);

  useEffect(() => {
    api.assumptions().then(setAssumptions).catch(() => setAssumptions(null));
  }, [revision]);

  useEffect(() => {
    let active = true;
    // show the spinner only on first load: a refetch after an edit must not unmount
    // the popover the user is still dragging
    if (!loadedOnce.current) setLoading(true);
    (async () => {
      const names = ["mbs", "loans", "mm", "debt", "deposits", "cds"] as BookName[];
      const fetched = await Promise.all(names.map(b => api.book(b).then(rowsOf).catch(() => [] as Row[])));
      const out: Pos[] = [];
      names.forEach((b, bi) => {
        rawRows.current[b] = fetched[bi];
        for (const r of fetched[bi]) {
          const side: 1 | -1 = b === "debt" || b === "deposits" || b === "cds" ? -1 : b === "mm" ? (String(r.side) === "asset" ? 1 : -1) : 1;
          const d = derive(b, r);
          const key = `${b}:${d.id}`;
          if (!baseline.current.has(key)) baseline.current.set(key, d.bal);
          out.push({ ...d, bal0: baseline.current.get(key)!, book: b, side, segment: b === "deposits" ? String(r.segment) : undefined });
        }
      });
      if (active) { setPos(out); setLoading(false); loadedOnce.current = true; }
    })();
    return () => { active = false; };
  }, [revision]);

  /** Write a balance edit back to its book after a short pause. The book write
   * raises the inputs-changed event, so the engine recomputes everything
   * downstream and this grid reloads. */
  const commitBalance = (p: Pos, value: number) => {
    const k = `${p.book}:${p.id}`;
    clearTimeout(saveTimers.current.get(k));
    saveTimers.current.set(k, setTimeout(async () => {
      const key = rowKey(p.book), field = BAL_FIELD[p.book as BookName];
      const rows = rawRows.current[p.book as BookName] ?? [];
      const next = rows.map(r => (String(r[key]) === p.id ? { ...r, [field]: value } : r));
      try {
        setSaveError(null);
        await api.putBook(p.book as BookName, next);
      } catch (e) {
        setSaveError(errorText(e));
        setRevision(v => v + 1);   // reload the book so the grid shows what was saved, not the rejected value
      }
    }, 600));
  };

  const plug = useMemo(() => {
    const a = pos.filter(p => p.side > 0).reduce((s, p) => s + p.bal, 0);
    const l = pos.filter(p => p.side < 0).reduce((s, p) => s + p.bal, 0);
    const a0 = pos.filter(p => p.side > 0).reduce((s, p) => s + p.bal0, 0);
    const l0 = pos.filter(p => p.side < 0).reduce((s, p) => s + p.bal0, 0);
    return (a - l) - (a0 - l0);          // >0 needs ST funding; <0 holds cash
  }, [pos]);

  const groups = useMemo(() => {
    const g: Record<string, Record<string, Pos[]>> = { Assets: {}, Liabilities: {} };
    for (const p of pos) (g[p.side > 0 ? "Assets" : "Liabilities"][p.book] ??= []).push(p);
    return g;
  }, [pos]);

  const maxNii = useMemo(() => Math.max(1, ...pos.map(p => niiPath(p)[0])), [pos]);
  const maxBal = useMemo(() => Math.max(1, ...pos.map(p => p.bal)), [pos]);
  const maxKrd = useMemo(() => Math.max(1, ...pos.flatMap(p => krdSplit(p))), [pos]);

  const derived = useMemo(() => new Map(pos.map(p => [p, {
    balQ: balPath(p), niiQ: niiPath(p), krd: krdSplit(p),
  }])), [pos]);
  const agg = (ps: Pos[]) => {
    const total = ps.reduce((s, p) => s + p.bal, 0);
    const balQ = QTRS.map(() => 0), niiQ = QTRS.map(() => 0), krd = PILLARS.map(() => 0);
    let yld = 0, oad = 0, bal0 = 0;
    for (const p of ps) {
      const d = derived.get(p)!;
      bal0 += p.bal0; yld += p.yld * p.bal; oad += p.oad * p.bal;
      for (let i = 0; i < QTRS.length; i++) { balQ[i] += d.balQ[i]; niiQ[i] += d.niiQ[i]; }
      for (let i = 0; i < PILLARS.length; i++) krd[i] += d.krd[i] * p.side;
    }
    return { bal: total, bal0, yld: yld / Math.max(1, total), oad: oad / Math.max(1, total), balQ, niiQ, krd };
  };

  const renderCols = ({ a, p }: { a: ReturnType<typeof agg>; p?: Pos }) => view === "summary" ? (
    <>
      <td className="num px-2 text-right text-sm">{(a.yld * 100).toFixed(2)}%</td>
      <td className="num px-2 text-right text-sm">{a.oad.toFixed(2)}y
        {p && <InfoPop width="14rem">Indicative OAD: heuristic by product (coupon-adjusted for MBS, term-scaled for schedule paper, segment table for NMDs). Engine KRDs from a Risk Desk run supersede this.</InfoPop>}</td>
      <td className="px-2"><Spark v={a.balQ} /></td>
      <td className="px-2"><Spark v={a.niiQ} color="var(--viz-2)" /></td>
    </>
  ) : view === "fwd balance" ? (
    <>{a.balQ.map((v, i) => <td key={i} className="px-0.5"><Heat v={v} max={maxBal} /></td>)}</>
  ) : view === "fwd nii" ? (
    <>{a.niiQ.map((v, i) => <td key={i} className="px-0.5"><Heat v={v} max={maxNii * 4} /></td>)}</>
  ) : (
    <>{a.krd.map((v, i) => <td key={i} className="px-0.5"><Heat v={v} max={maxKrd} sign={1} /></td>)}</>
  );

  const totals = { A: agg(pos.filter(p => p.side > 0)), L: agg(pos.filter(p => p.side < 0)) };

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {([["Assets", totals.A], ["Liabilities", totals.L]] as const).map(([l, t]) => (
          <div key={l} className="rounded-md border border-line-strong bg-surface-1 shadow-inset-top p-3">
            <div className="eyebrow">{l}</div>
            <div className="num mt-0.5 text-lg text-paper">{fmt$(t.bal)}<Trend now={t.bal} was={t.bal0} /></div>
            <div className="num mt-0.5 text-xs text-paper-faint">{(t.yld * 100).toFixed(2)}% · {t.oad.toFixed(2)}y OAD</div>
          </div>
        ))}
        <div className="rounded-md border border-brand/40 bg-surface-1 p-3 shadow-inset-top">
          <div className="eyebrow flex items-center">Cash / ST-funding plug
            <InfoPop width="14rem">Your edits balance here: grow assets and the plug turns to short-term funding (liability); shrink them and the book holds cash. Priced at the short rate either way — the carry consequence of every resize.</InfoPop></div>
          <div className="num mt-0.5 text-lg text-paper-heading">{plug === 0 ? "—" : fmt$(Math.abs(plug))}</div>
          <div className="text-xs text-paper-faint">{plug > 0 ? "ST funding raised" : plug < 0 ? "cash held" : "balanced as booked"}</div>
        </div>
        <div className="rounded-md border border-line-strong bg-surface-1 shadow-inset-top p-3">
          <div className="eyebrow">View</div>
          <div className="mt-2 flex gap-1 rounded-lg border border-line bg-surface-2 p-0.5">
            {VIEWS.map(v => (
              <button key={v} onClick={() => setView(v)}
                className={`rounded-sm px-2 py-1 text-2xs font-semibold capitalize ${v === view ? "bg-brand text-ink" : "text-paper-faint hover:text-paper"}`}>{v}</button>
            ))}
          </div>
        </div>
      </div>

      {saveError && <div role="alert" className="text-sm text-danger">Could not save the balance: {saveError}</div>}
      <Card>
        <CardHeader title="Positions" sub="Drill side → book → position; balances edit via slider popovers and auto-balance into the plug; figures are indicative — Risk Desk runs are authoritative"
          right={<Badge tone="neutral">{pos.length} positions</Badge>} />
        <CardBody className="overflow-auto p-0">
          <table className="w-full text-left text-sm tabular-nums">
            <thead className="sticky top-0 bg-surface-base text-2xs font-semibold uppercase tracking-wide text-paper-faint">
              <tr>
                <th className="px-2.5 py-1.5">Name</th>
                <th className="px-2 py-1.5 text-right">Balance</th>
                {view === "summary"
                  ? ["yield", "OAD", "fwd bal", "fwd nii"].map(h => <th key={h} className="px-2 py-1.5 text-right">{h}</th>)
                  : (view === "krd" ? PILLARS : QTRS.map(q => `Q${q}`)).map(h => <th key={String(h)} className="px-1 py-1.5 text-right">{String(h)}</th>)}
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {loading ? (
                <tr><td colSpan={6} className="py-10">
                  <div className="flex items-center justify-center gap-2 text-sm text-paper-faint"><Spinner /> loading positions…</div>
                </td></tr>
              ) : (["Assets", "Liabilities"] as const).map(sideL => (
                <Fragment key={sideL}>{renderSideRows({ label: sideL, books: groups[sideL], open, setOpen, agg, renderCols, setPos })}</Fragment>
              ))}
            </tbody>
          </table>
        </CardBody>
      </Card>
    </div>
  );

  function renderSideRows({ label, books, open, setOpen, agg, renderCols, setPos }: any) {
    const all = (Object.values(books) as Pos[][]).flat();
    if (!all.length) return null;
    const a = agg(all);
    return (
      <>
        <tr className="bg-surface-1 font-medium">
          <td className="cursor-pointer px-2.5 py-1.5 text-paper" onClick={() => setOpen({ ...open, [label]: !open[label] })}>
            <span className="mr-1 text-paper-faint">{open[label] ? "▾" : "▸"}</span>{label}
          </td>
          <td className="num px-2 text-right text-paper">{fmt$(a.bal)}<Trend now={a.bal} was={a.bal0} /></td>
          {renderCols({ a })}
        </tr>
        {open[label] && Object.entries(books).map(([b, ps]: [string, any]) => {
          const ab = agg(ps); const key = `${label}:${b}`;
          return (
            <Fragment key={b}>{renderBookGroup({ bk: key, b, ps, ab })}</Fragment>
          );
        })}
      </>
    );
    function renderBookGroup({ bk, b, ps, ab }: any) {
      return (
        <>
          <tr className="hover:bg-surface-1">
            <td className="cursor-pointer px-2.5 py-1 pl-7 text-paper-dim" onClick={() => setOpen({ ...open, [bk]: !open[bk] })}>
              <span className="mr-1 text-paper-faint">{open[bk] ? "▾" : "▸"}</span>{b}
              <span className="ml-2 text-2xs text-paper-faint">{ps.length}</span>
            </td>
            <td className="num px-2 text-right">{fmt$(ab.bal)}<Trend now={ab.bal} was={ab.bal0} /></td>
            {renderCols({ a: ab })}
          </tr>
          {open[bk] && ps.map((p: Pos) => (
            <tr key={p.id} className="hover:bg-surface-1" style={{ contentVisibility: "auto", containIntrinsicSize: "auto 26px" }}>
              <td className="px-2.5 py-1 pl-12 text-paper-faint">{p.id}</td>
              <td className="px-2 text-right">
                <span className="inline-flex items-center justify-end gap-1">
                  <BalEdit p={p} onSet={v => { setPos((xs: Pos[]) => xs.map(x => x.id === p.id && x.book === p.book ? { ...x, bal: v } : x)); commitBalance(p, v); }} />
                  <AssumptionEdit p={p} assumptions={assumptions} onSaved={() => setRevision(v => v)} />
                </span>
              </td>
              {renderCols({ a: agg([p]), p })}
            </tr>
          ))}
        </>
      );
    }
  }
}
