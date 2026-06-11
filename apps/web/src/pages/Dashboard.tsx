/** Trader dashboard: book stats, KRD profile, NII forecast, 9Q stress. */
import { useEffect, useState } from "react";
import { api, awaitJob, fmt$, type BookName, type Job } from "../lib/api";
import { KrdBar, NiiArea, StressLines } from "../components/charts";
import { Badge, Button, Card, CardBody, CardHeader, Stat } from "../components/ui";

type Row = Record<string, number | string>;
const TENORS = [1, 2, 3, 4, 5, 7, 10, 15, 20, 30];

export default function Dashboard() {
  const [books, setBooks] = useState<Record<string, { positions: number; balance: number }>>({});
  const [risk, setRisk] = useState<Record<string, Row[]> | null>(null);
  const [nii, setNii] = useState<{ monthly: Row[]; summary: Row[] } | null>(null);
  const [stress, setStress] = useState<Record<string, { agg: Row[] }> | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => { api.books().then(setBooks).catch(() => {}); }, []);

  const run = async (kind: "risk" | "nii" | "stress", set: (r: never) => void, booksArg?: BookName[]) => {
    setBusy(kind);
    try {
      const j = await api.run(kind, undefined, booksArg);
      const done: Job = await awaitJob(j.id);
      if (done.status === "done") set(done.result as never);
      else alert(done.detail);
    } finally { setBusy(null); }
  };

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
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        {Object.entries(books).map(([k, v]) => (
          <Stat key={k} label={k} value={fmt$(v.balance)} delta={`${v.positions} positions`} />
        ))}
      </div>

      <div className="flex gap-2">
        <Button disabled={!!busy} onClick={() => run("risk", setRisk as never)}>
          {busy === "risk" ? "running…" : "Run risk (all books)"}
        </Button>
        <Button disabled={!!busy} variant="ghost" onClick={() => run("nii", setNii as never)}>
          {busy === "nii" ? "running…" : "Run NII forecast"}
        </Button>
        <Button disabled={!!busy} variant="ghost" onClick={() => run("stress", setStress as never, ["mbs", "deposits"])}>
          {busy === "stress" ? "running…" : "Run 9Q stress"}
        </Button>
        {totalDv01 !== null && (
          <Badge tone={totalDv01 >= 0 ? "green" : "red"}>net dv01 {fmt$(totalDv01)}/bp</Badge>
        )}
      </div>

      <div className="grid gap-4 xl:grid-cols-2">
        <Card>
          <CardHeader title="KRD profile" sub="$/bp by pillar, stacked by book (fixed-OAS, CRN)" />
          <CardBody>{risk ? <KrdBar data={krdData as never} /> : <Empty hint="run risk" />}</CardBody>
        </Card>
        <Card>
          <CardHeader title="NII forecast" sub="monthly net interest income, 27m horizon"
            right={nii && <Badge tone="green">{fmt$((nii.summary.find(s => s.metric === "nii_annualized_$")?.value as number) ?? 0)}/yr</Badge>} />
          <CardBody>{nii ? <NiiArea data={nii.monthly as never} /> : <Empty hint="run NII" />}</CardBody>
        </Card>
        <Card className="xl:col-span-2">
          <CardHeader title="9Q stress P&L — MBS book" sub="forward-starting parallel shocks, P&L vs base forward value" />
          <CardBody>{stress?.mbs ? <StressLines data={stressData as never} shocks={[-100, 100, 200, 300]} /> : <Empty hint="run stress" />}</CardBody>
        </Card>
      </div>
    </div>
  );
}

const Empty = ({ hint }: { hint: string }) => (
  <div className="flex h-48 items-center justify-center text-xs text-zinc-600">no data — {hint}</div>
);
