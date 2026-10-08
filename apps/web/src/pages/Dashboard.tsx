import { useEngineData, type ResultKind } from "../lib/engine";
/** Risk Desk: book stats, KRD profile and NII forecast. These are downstream of
 * the inputs, so they recompute on their own when an assumption changes; the
 * badge says when a figure is out of date. Rate stress lives in Stress. */
import { useEffect, useMemo, useState } from "react";
import { api, colOf, fmt$, rowsOf, type Table } from "../lib/api";
import { KrdBar, NiiArea } from "../components/charts";
import { Badge, Button, Card, CardBody, CardHeader, ChartState } from "../components/ui";

type Row = Record<string, number | string>;
const bookSign = (book: string) => ["debt", "deposits", "cds"].includes(book) ? -1 : 1;
const TENORS = [1, 2, 3, 4, 5, 7, 10, 15, 20, 30];

/** Freshness of a downstream result: updating, out of date, or current. */
export function Freshness({ kind }: { kind: ResultKind }) {
  const { pending, isStale, errors, results, autoRecalc } = useEngineData();
  if (pending.includes(kind)) return <Badge tone="accent" dot>Updating</Badge>;
  if (errors[kind]) return <Badge tone="danger">Failed</Badge>;
  if (!results[kind]) return null;
  if (isStale(kind)) return <Badge tone="warning">{autoRecalc ? "Out of date" : "Out of date · refresh"}</Badge>;
  return null;
}

export default function Dashboard() {
  const engine = useEngineData();
  const [books, setBooks] = useState<Record<string, { positions: number; balance: number }>>({});
  useEffect(() => { api.books().then(setBooks).catch(() => {}); }, [engine.revision]);

  const risk = engine.results.risk?.value as Record<string, Table> | undefined;
  const nii = engine.results.nii?.value as { monthly: Table; summary: Table } | undefined;
  const riskFailed = engine.errors.risk;
  const niiFailed = engine.errors.nii;

  const krdData = useMemo(() => risk
    ? TENORS.map(t => {
        const row: Row = { tenor: `${t}y` };
        for (const [book, table] of Object.entries(risk))
          row[book] = bookSign(book) * colOf(table, `krd01_${t}y`).reduce((a, x) => a + (x ?? 0), 0);
        return row;
      })
    : [], [risk]);

  const totalDv01 = useMemo(() => risk
    ? Object.entries(risk).reduce((a, [book, table]) => a + bookSign(book) * colOf(table, "dv01").reduce((s, x) => s + (x ?? 0), 0), 0)
    : null, [risk]);

  const niiAnnualized = useMemo(() => nii
    ? (rowsOf(nii.summary).find(s => s.metric === "nii_annualized_$")?.value as number) ?? 0
    : 0, [nii]);

  const dim = (kind: ResultKind) => (engine.isStale(kind) && engine.results[kind] ? "opacity-60 transition-opacity duration-base" : "");

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
        <Button variant="secondary" disabled={engine.pending.includes("risk")} onClick={() => engine.request("risk")}>Refresh risk</Button>
        <Button variant="secondary" disabled={engine.pending.includes("nii")} onClick={() => engine.request("nii")}>Refresh NII</Button>
        {totalDv01 !== null && (
          <Badge tone="neutral">net dv01 {risk?.hedges ? "(incl. hedges) " : "(selected books) "}{fmt$(totalDv01)}/bp</Badge>
        )}
        {niiAnnualized !== 0 && <Badge tone="up">NII {fmt$(niiAnnualized)}/yr</Badge>}
      </div>

      <div className="grid gap-3 xl:grid-cols-2">
        <Card>
          <CardHeader title="KRD profile" sub="$/bp by pillar, stacked by book (fixed-OAS, CRN)"
            right={<Freshness kind="risk" />} />
          <CardBody className={dim("risk")}>
            {risk
              ? <KrdBar data={krdData as never} />
              : <ChartState kind={engine.pending.includes("risk") ? "loading" : riskFailed ? "error" : "empty"}
                  hint="Risk populates on load, and again whenever an assumption changes." error={riskFailed}
                  onRetry={() => engine.request("risk")} />}
          </CardBody>
        </Card>
        <Card>
          <CardHeader title="NII forecast" sub="Monthly net interest income, 27m horizon"
            right={<Freshness kind="nii" />} />
          <CardBody className={dim("nii")}>
            {nii
              ? <NiiArea data={rowsOf(nii.monthly) as never} />
              : <ChartState kind={engine.pending.includes("nii") ? "loading" : niiFailed ? "error" : "empty"}
                  hint="Forecast populates on load, and again whenever an assumption changes." error={niiFailed}
                  onRetry={() => engine.request("nii")} />}
          </CardBody>
        </Card>
      </div>
    </div>
  );
}
