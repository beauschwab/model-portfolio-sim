/** Trader dashboard: book stats, KRD profile, NII forecast, 9Q stress. */
import { useEffect, useState } from "react";
import { api, awaitJob, fmt$, type BookName, type Job } from "../lib/api";
import { KrdBar, NiiArea, StressLines } from "../components/charts";
import { Badge, Button, Card, CardBody, CardHeader, ChartState, Spinner } from "../components/ui";

type Row = Record<string, number | string>;
type Kind = "risk" | "nii" | "stress";
type RunState = "idle" | "running" | "done" | "error";
const TENORS = [1, 2, 3, 4, 5, 7, 10, 15, 20, 30];

export default function Dashboard() {
  const [books, setBooks] = useState<Record<string, { positions: number; balance: number }>>({});
  const [risk, setRisk] = useState<Record<string, Row[]> | null>(null);
  const [nii, setNii] = useState<{ monthly: Row[]; summary: Row[] } | null>(null);
  const [stress, setStress] = useState<Record<string, { agg: Row[] }> | null>(null);
  const [status, setStatus] = useState<Record<Kind, RunState>>({ risk: "idle", nii: "idle", stress: "idle" });
  const [errors, setErrors] = useState<Record<Kind, string | null>>({ risk: null, nii: null, stress: null });
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => { api.books().then(setBooks).catch(() => {}); }, []);

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
      const j = await api.run(kind, undefined, ARGS[kind]);
      const done: Job = await awaitJob(j.id);
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

  const krdData = risk
    ? TENORS.map(t => {
        const row: Row = { tenor: `${t}y` };
        for (const [book, rows] of Object.entries(risk))
          row[book] = rows.reduce((a, r) => a + ((r[`krd01_${t}y`] as number) ?? 0), 0);
        return row;
      })
    : [];

  const stressData = stress?.mbs
    ? Object.values(
        stress.mbs.agg.reduce((acc: Record<number, Row>, r) => {
          const h = r.horizon_m as number;
          acc[h] ??= { horizon_m: h };
          acc[h][String(r.shock_bp)] = (r["pnl_$"] ?? r["eve_pnl_$"]) as number;
          return acc;
        }, {}))
    : [];

  const totalDv01 = risk
    ? Object.values(risk).flat().reduce((a, r) => a + ((r.dv01 as number) ?? 0), 0)
    : null;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-stretch divide-x divide-line overflow-hidden rounded-xl border border-line bg-surface-1">
        {Object.entries(books).map(([k, v]) => (
          <div key={k} className="min-w-[116px] flex-1 px-4 py-2.5">
            <div className="text-[10px] uppercase tracking-wide text-paper-faint">{k}</div>
            <div className="num text-base font-semibold text-paper">{fmt$(v.balance)}</div>
            <div className="num text-[11px] text-paper-faint">{v.positions} pos</div>
          </div>
        ))}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Button disabled={anyRunning} onClick={runAll}>
          {anyRunning ? <><Spinner /> running {elapsed.toFixed(0)}s</> : "Run all"}
        </Button>
        <Button disabled={anyRunning} variant="ghost" onClick={() => run("risk")}>Risk</Button>
        <Button disabled={anyRunning} variant="ghost" onClick={() => run("nii")}>NII</Button>
        <Button disabled={anyRunning} variant="ghost" onClick={() => run("stress")}>9Q stress</Button>
        {totalDv01 !== null && (
          <Badge tone={totalDv01 >= 0 ? "green" : "red"}>net dv01 {fmt$(totalDv01)}/bp</Badge>
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
          <CardHeader title="NII forecast" sub="monthly net interest income, 27m horizon"
            right={nii && <Badge tone="green">{fmt$((nii.summary.find(s => s.metric === "nii_annualized_$")?.value as number) ?? 0)}/yr</Badge>} />
          <CardBody>
            {nii
              ? <NiiArea data={nii.monthly as never} />
              : <ChartState kind={status.nii === "running" ? "loading" : status.nii === "error" ? "error" : "empty"}
                  hint="Run NII to populate" elapsed={elapsed} error={errors.nii} onRetry={() => run("nii")} />}
          </CardBody>
        </Card>
        <Card className="xl:col-span-2">
          <CardHeader title="9Q stress P&L — MBS book" sub="forward-starting parallel shocks, P&L vs base forward value" />
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

