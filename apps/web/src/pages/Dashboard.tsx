import { useEngineData } from "../lib/engine";
/** Trader dashboard: book stats, KRD profile, NII forecast, 9Q stress. */
import { useEffect, useMemo, useState } from "react";
import { api, awaitJob, colOf, fmt$, rowsOf, type BookName, type Job, type Table } from "../lib/api";
import { KrdBar, NiiArea, StressLines } from "../components/charts";
import { Badge, Button, Card, CardBody, CardHeader, ChartState, Spinner } from "../components/ui";

type Row = Record<string, number | string>;
type Kind = "risk" | "nii" | "stress";
type RunState = "idle" | "running" | "done" | "error";
const bookSign = (book: string) => ["debt", "deposits", "cds"].includes(book) ? -1 : 1;
const TENORS = [1, 2, 3, 4, 5, 7, 10, 15, 20, 30];

export default function Dashboard() {
  const engine = useEngineData();
  const [books, setBooks] = useState<Record<string, { positions: number; balance: number }>>({});
  const [risk, setRisk] = useState<Record<string, Table> | null>(null);
  const [nii, setNii] = useState<{ monthly: Table; summary: Table } | null>(null);
  const [stress, setStress] = useState<Record<string, { agg: Table }> | null>(null);
  const [status, setStatus] = useState<Record<Kind, RunState>>({ risk: "idle", nii: "idle", stress: "idle" });
  const [errors, setErrors] = useState<Record<Kind, string | null>>({ risk: null, nii: null, stress: null });
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    api.books().then(setBooks).catch(() => {});
    setRisk(null); setNii(null); setStress(null);
  }, [engine.revision]);

  const anyRunning = Object.values(status).some(s => s === "running");
  useEffect(() => {
    if (!anyRunning) { setElapsed(0); return; }
    const t0 = Date.now();
    const id = setInterval(() => setElapsed((Date.now() - t0) / 1000), 250);
    return () => clearInterval(id);
  }, [anyRunning]);

  const SETTERS: Record<Kind, (r: never) => void> = { risk: setRisk as never, nii: setNii as never, stress: setStress as never };
  const ARGS: Record<Kind, BookName[] | undefined> = { risk: undefined, nii: undefined, stress: ["mbs", "deposits"] };

  const run = async (kind: Kind) => {
    setStatus(s => ({ ...s, [kind]: "running" }));
    setErrors(e => ({ ...e, [kind]: null }));
    try {
      const done: Job = await engine.run(kind, { books: ARGS[kind] });
      if (done.status === "done") {
        SETTERS[kind](done.result as never);
        setStatus(s => ({ ...s, [kind]: "done" }));
      } else {
        setErrors(e => ({ ...e, [kind]: done.detail ?? "engine returned no result" }));
        setStatus(s => ({ ...s, [kind]: "error" }));
      }
    } catch (err) {
      setErrors(e => ({ ...e, [kind]: err instanceof Error ? err.message : String(err) }));
      setStatus(s => ({ ...s, [kind]: "error" }));
    }
  };

  const runAll = () => { void Promise.all([run("risk"), run("nii"), run("stress")]); };

  // columnar aggregation straight off the Arrow tables
  const krdData = useMemo(() => risk
    ? TENORS.map(t => {
        const row: Row = { tenor: `${t}y` };
        for (const [book, table] of Object.entries(risk))
          row[book] = bookSign(book) * colOf(table, `krd01_${t}y`).reduce((a, x) => a + (x ?? 0), 0);
        return row;
      })
    : [], [risk]);

  const stressData = useMemo(() => stress?.mbs
    ? Object.values(
        rowsOf(stress.mbs.agg).reduce((acc: Record<number, Row>, r) => {
          const h = r.horizon_m as number;
          acc[h] ??= { horizon_m: h };
          acc[h][String(r.shock_bp)] = (r["pnl_$"] ?? r["eve_pnl_$"]) as number;
          return acc;
        }, {}))
    : [], [stress]);

  const totalDv01 = useMemo(() => risk
    ? Object.entries(risk).reduce((a, [book, table]) => a + bookSign(book) * colOf(table, "dv01").reduce((s, x) => s + (x ?? 0), 0), 0)
    : null, [risk]);

  const niiAnnualized = useMemo(() => nii
    ? (rowsOf(nii.summary).find(s => s.metric === "nii_annualized_$")?.value as number) ?? 0
    : 0, [nii]);

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-stretch divide-x divide-line overflow-hidden rounded-md border border-line-strong bg-surface-1 shadow-inset-top">
        {Object.entries(books).map(([k, v]) => (
          <div key={k} className="min-w-[116px] flex-1 px-4 py-2.5">
            <div className="eyebrow">{k}</div>
            <div className="num text-lg font-semibold text-paper">{fmt$(v.balance)}</div>
            <div className="num text-xs text-paper-faint">{v.positions} pos</div>
          </div>
        ))}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Button disabled={anyRunning} onClick={runAll}>
          {anyRunning ? <><Spinner /> running {elapsed.toFixed(0)}s</> : "Run all"}
        </Button>
        <Button disabled={anyRunning} variant="secondary" onClick={() => run("risk")}>Risk</Button>
        <Button disabled={anyRunning} variant="secondary" onClick={() => run("nii")}>NII</Button>
        <Button disabled={anyRunning} variant="secondary" onClick={() => run("stress")}>9Q stress</Button>
        {totalDv01 !== null && (
          <Badge tone="neutral">net dv01 {risk?.hedges ? "(incl. hedges) " : "(selected books) "}{fmt$(totalDv01)}/bp</Badge>
        )}
      </div>

      <div className="grid gap-3 xl:grid-cols-2">
        <Card>
          <CardHeader title="KRD profile" sub="$/bp by pillar, stacked by book (fixed-OAS, CRN)" />
          <CardBody>
            {risk
              ? <KrdBar data={krdData as never} />
              : <ChartState kind={status.risk === "running" ? "loading" : status.risk === "error" ? "error" : "empty"}
                  hint="Run risk to populate" elapsed={elapsed} error={errors.risk} onRetry={() => run("risk")} />}
          </CardBody>
        </Card>
        <Card>
          <CardHeader title="NII forecast" sub="Monthly net interest income, 27m horizon"
            right={nii && <Badge tone="up">{fmt$(niiAnnualized)}/yr</Badge>} />
          <CardBody>
            {nii
              ? <NiiArea data={rowsOf(nii.monthly) as never} />
              : <ChartState kind={status.nii === "running" ? "loading" : status.nii === "error" ? "error" : "empty"}
                  hint="Run NII to populate" elapsed={elapsed} error={errors.nii} onRetry={() => run("nii")} />}
          </CardBody>
        </Card>
        <Card className="xl:col-span-2">
          <CardHeader title="9Q stress P&L — MBS book" sub="Forward-starting parallel shocks, P&L vs base forward value" />
          <CardBody>
            {stress?.mbs
              ? <StressLines data={stressData as never} shocks={[-100, 100, 200, 300]} />
              : <ChartState kind={status.stress === "running" ? "loading" : status.stress === "error" ? "error" : "empty"}
                  hint="Run 9Q stress to populate" elapsed={elapsed} error={errors.stress} onRetry={() => run("stress")} />}
          </CardBody>
        </Card>
      </div>
    </div>
  );
}

