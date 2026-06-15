/** Positions — the hierarchical balance sheet. Side → book → position
 * drill-down; spot balances edit via slider popovers and AUTO-BALANCE
 * into a brass "Cash & ST funding" plug on the opposite side; a
 * segmented control swaps the right-hand column group between Summary
 * (yield/OAD/sparkline), Fwd Balance, Fwd NII, and KRD heat. All
 * derived figures are INDICATIVE client-side approximations (each ⓘ
 * says so) — engine-grade numbers come from Risk Desk / NII runs. */
import { useEffect, useMemo, useState } from "react";
import { api, fmt$, rowsOf, type BookName, type Row } from "../lib/api";
import { Badge, Button, Card, CardBody, CardHeader, InfoPop, Popover, Spinner } from "../components/ui";

type View = "summary" | "fwd balance" | "fwd nii" | "krd";
const VIEWS: View[] = ["summary", "fwd balance", "fwd nii", "krd"];
const QTRS = [1, 2, 3, 4, 5, 6, 7, 8, 9];
const PILLARS = ["2y", "5y", "10y", "30y"];
const SHORT = 0.0365;

type Pos = { id: string; bal: number; bal0: number; yld: number; oad: number; decay: number; book: string; side: 1 | -1 };

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

const Spark = ({ v, color = "#fcd535" }: { v: number[]; color?: string }) => {
  const [lo, hi] = [Math.min(...v), Math.max(...v)];
  const pts = v.map((x, i) => `${(i / (v.length - 1)) * 64},${18 - ((x - lo) / Math.max(hi - lo, 1e-9)) * 14}`).join(" ");
  return <svg width="68" height="20"><polyline points={pts} fill="none" stroke={color} strokeWidth="1.25" /></svg>;
};
const Heat = ({ v, max, sign = 1 }: { v: number; max: number; sign?: number }) => (
  <div className="num rounded px-1.5 py-0.5 text-right text-[11px]"
    style={{ background: `rgba(${sign * v >= 0 ? "14,203,129" : "246,70,93"},${Math.min(0.85, Math.abs(v) / Math.max(max, 1e-9))})`, color: "#eaecef" }}>
    {Math.abs(v) >= 1e6 ? (v / 1e6).toFixed(1) + "M" : (v / 1e3).toFixed(0) + "k"}
  </div>
);
const Trend = ({ now, was }: { now: number; was: number }) => {
  const d = now - was;
  if (Math.abs(d) < Math.abs(was) * 1e-6) return null;
  return <span className={`num ml-1 text-[10px] ${d > 0 ? "text-up" : "text-down"}`}>{d > 0 ? "▲" : "▼"}{fmt$(Math.abs(d))}</span>;
};

/** Balance editor popover: slider 0–2× with before/after bars. */
function BalEdit({ p, onSet }: { p: Pos; onSet: (v: number) => void }) {
  const [m, setM] = useState(p.bal / p.bal0);
  return (
    <Popover width="15rem" trigger={
      <span className="num cursor-pointer underline decoration-dotted decoration-brand/60 hover:text-brand">{fmt$(p.bal)}</span>}>
      <div className="space-y-2">
        <div className="text-[10px] uppercase tracking-wide text-paper-faint">Spot balance — balances into the cash/ST-funding plug</div>
        <input type="range" min={0} max={2} step={0.05} value={m} className="w-full accent-[#fcd535]"
          onChange={e => { const x = Number(e.target.value); setM(x); onSet(p.bal0 * x); }} />
        <div className="flex items-end gap-2">
          {[["was", p.bal0], ["now", p.bal0 * m]].map(([l, v]) => (
            <div key={String(l)} className="flex-1">
              <div className="h-10 rounded-sm bg-surface-3"><div className="rounded-sm bg-brand/70" style={{ height: `${Math.min(100, (Number(v) / (2 * p.bal0)) * 100)}%`, marginTop: "auto" }} /></div>
              <div className="num mt-1 text-[10px] text-paper-faint">{String(l)} {fmt$(Number(v))}</div>
            </div>
          ))}
        </div>
        <div className="num text-xs text-paper">{(m * 100).toFixed(0)}% of booked</div>
      </div>
    </Popover>
  );
}

export default function Positions() {
  const [pos, setPos] = useState<Pos[]>([]);
  const [loading, setLoading] = useState(true);
  const [open, setOpen] = useState<Record<string, boolean>>({ Assets: true, Liabilities: true });
  const [view, setView] = useState<View>("summary");

  useEffect(() => {
    (async () => {
      const names = ["mbs", "loans", "mm", "debt", "deposits", "cds"] as BookName[];
      const fetched = await Promise.all(names.map(b => api.book(b).then(rowsOf).catch(() => [] as Row[])));
      const out: Pos[] = [];
      names.forEach((b, bi) => {
        for (const r of fetched[bi]) {
          const side: 1 | -1 = b === "debt" || b === "deposits" || b === "cds" ? -1 : b === "mm" ? (String(r.side) === "asset" ? 1 : -1) : 1;
          const d = derive(b, r);
          out.push({ ...d, bal0: d.bal, book: b, side });
        }
      });
      setPos(out);
      setLoading(false);
    })();
  }, []);

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

  const agg = (ps: Pos[]) => ({
    bal: ps.reduce((s, p) => s + p.bal, 0), bal0: ps.reduce((s, p) => s + p.bal0, 0),
    yld: ps.reduce((s, p) => s + p.yld * p.bal, 0) / Math.max(1, ps.reduce((s, p) => s + p.bal, 0)),
    oad: ps.reduce((s, p) => s + p.oad * p.bal, 0) / Math.max(1, ps.reduce((s, p) => s + p.bal, 0)),
    balQ: QTRS.map((_, i) => ps.reduce((s, p) => s + balPath(p)[i], 0)),
    niiQ: QTRS.map((_, i) => ps.reduce((s, p) => s + niiPath(p)[i], 0)),
    krd: PILLARS.map((_, i) => ps.reduce((s, p) => s + krdSplit(p)[i] * p.side, 0)),
  });

  const Cols = ({ a, p }: { a: ReturnType<typeof agg>; p?: Pos }) => view === "summary" ? (
    <>
      <td className="num px-2 text-right text-xs">{(a.yld * 100).toFixed(2)}%</td>
      <td className="num px-2 text-right text-xs">{a.oad.toFixed(2)}y
        {p && <InfoPop width="14rem">Indicative OAD: heuristic by product (coupon-adjusted for MBS, term-scaled for schedule paper, segment table for NMDs). Engine KRDs from a Risk Desk run supersede this.</InfoPop>}</td>
      <td className="px-2"><Spark v={a.balQ} /></td>
      <td className="px-2"><Spark v={a.niiQ} color="#2dbdb6" /></td>
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
          <div key={l} className="rounded-lg border border-line bg-surface-1 p-3">
            <div className="text-[10px] uppercase tracking-wide text-paper-faint">{l}</div>
            <div className="num mt-0.5 text-lg text-paper">{fmt$(t.bal)}<Trend now={t.bal} was={t.bal0} /></div>
            <div className="num mt-0.5 text-[11px] text-paper-faint">{(t.yld * 100).toFixed(2)}% · {t.oad.toFixed(2)}y OAD</div>
          </div>
        ))}
        <div className="rounded-lg border border-brand/50 bg-surface-1 p-3">
          <div className="flex items-center text-[10px] uppercase tracking-wide text-brand">Cash / ST-funding plug
            <InfoPop width="14rem">Your edits balance here: grow assets and the plug turns to short-term funding (liability); shrink them and the book holds cash. Priced at the short rate either way — the carry consequence of every resize.</InfoPop></div>
          <div className={`num mt-0.5 text-lg ${plug > 0 ? "text-down" : "text-up"}`}>{plug === 0 ? "—" : fmt$(Math.abs(plug))}</div>
          <div className="text-[11px] text-paper-faint">{plug > 0 ? "ST funding raised" : plug < 0 ? "cash held" : "balanced as booked"}</div>
        </div>
        <div className="rounded-lg border border-line bg-surface-1 p-3">
          <div className="text-[10px] uppercase tracking-wide text-paper-faint">View</div>
          <div className="mt-2 flex gap-1 rounded-lg border border-line bg-surface-2 p-0.5">
            {VIEWS.map(v => (
              <button key={v} onClick={() => setView(v)}
                className={`rounded-md px-2 py-1 text-[10px] font-medium capitalize ${v === view ? "bg-surface-3 text-brand" : "text-paper-faint hover:text-paper"}`}>{v}</button>
            ))}
          </div>
        </div>
      </div>

      <Card>
        <CardHeader title="Positions" sub="drill side → book → position; balances edit via slider popovers and auto-balance into the plug; figures are indicative — Risk Desk runs are authoritative"
          right={<Badge tone="zinc">{pos.length} positions</Badge>} />
        <CardBody className="overflow-auto p-0">
          <table className="w-full text-left text-xs tabular-nums">
            <thead className="sticky top-0 bg-surface-2 text-paper-faint">
              <tr>
                <th className="px-2.5 py-1.5">name</th>
                <th className="px-2 py-1.5 text-right">balance</th>
                {view === "summary"
                  ? ["yield", "OAD", "fwd bal", "fwd nii"].map(h => <th key={h} className="px-2 py-1.5 text-right">{h}</th>)
                  : (view === "krd" ? PILLARS : QTRS.map(q => `Q${q}`)).map(h => <th key={String(h)} className="px-1 py-1.5 text-right">{String(h)}</th>)}
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {loading ? (
                <tr><td colSpan={6} className="py-10">
                  <div className="flex items-center justify-center gap-2 text-xs text-paper-faint"><Spinner /> loading positions…</div>
                </td></tr>
              ) : (["Assets", "Liabilities"] as const).map(sideL => (
                <SideRows key={sideL} label={sideL} books={groups[sideL]} open={open} setOpen={setOpen}
                  agg={agg} Cols={Cols} setPos={setPos} />
              ))}
            </tbody>
          </table>
        </CardBody>
      </Card>
    </div>
  );

  function SideRows({ label, books, open, setOpen, agg, Cols, setPos }: any) {
    const all = (Object.values(books) as Pos[][]).flat();
    if (!all.length) return null;
    const a = agg(all);
    return (
      <>
        <tr className="bg-surface-1 font-medium">
          <td className="cursor-pointer px-2.5 py-1.5 text-paper" onClick={() => setOpen({ ...open, [label]: !open[label] })}>
            <span className="mr-1 text-brand">{open[label] ? "▾" : "▸"}</span>{label}
          </td>
          <td className="num px-2 text-right text-paper">{fmt$(a.bal)}<Trend now={a.bal} was={a.bal0} /></td>
          <Cols a={a} />
        </tr>
        {open[label] && Object.entries(books).map(([b, ps]: [string, any]) => {
          const ab = agg(ps); const key = `${label}:${b}`;
          return (
            <BookGroup key={b} bk={key} b={b} ps={ps} ab={ab} />
          );
        })}
      </>
    );
    function BookGroup({ bk, b, ps, ab }: any) {
      return (
        <>
          <tr className="hover:bg-surface-1">
            <td className="cursor-pointer px-2.5 py-1 pl-7 text-paper-dim" onClick={() => setOpen({ ...open, [bk]: !open[bk] })}>
              <span className="mr-1 text-brand">{open[bk] ? "▾" : "▸"}</span>{b}
              <span className="ml-2 text-[10px] text-paper-faint">{ps.length}</span>
            </td>
            <td className="num px-2 text-right">{fmt$(ab.bal)}<Trend now={ab.bal} was={ab.bal0} /></td>
            <Cols a={ab} />
          </tr>
          {open[bk] && ps.map((p: Pos) => (
            <tr key={p.id} className="hover:bg-surface-1" style={{ contentVisibility: "auto", containIntrinsicSize: "auto 26px" }}>
              <td className="px-2.5 py-1 pl-12 text-paper-faint">{p.id}</td>
              <td className="px-2 text-right">
                <BalEdit p={p} onSet={v => setPos((xs: Pos[]) => xs.map(x => x.id === p.id && x.book === p.book ? { ...x, bal: v } : x))} />
              </td>
              <Cols a={agg([p])} p={p} />
            </tr>
          ))}
        </>
      );
    }
  }
}
