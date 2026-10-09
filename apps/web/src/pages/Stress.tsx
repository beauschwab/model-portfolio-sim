/** Stress — rate shocks and balance-sheet stress in one place. The 9Q rate-shock
 * P&L used to sit on Risk Desk; it now runs here, next to the balance-sheet
 * engine it is compared against. */
import { useEffect, useMemo, useState } from "react";
import { useEngineData } from "../lib/engine";
import { Freshness } from "./Dashboard";
import { rowsOf, type Job, type Table } from "../lib/api";
import { StressLines } from "../components/charts";
import { Badge, Button, Card, CardBody, CardHeader, ChartState, Spinner, Tabs, tabPanelProps } from "../components/ui";
import BalanceStress from "./BalanceStress";

type Row = Record<string, number | string>;
type RunState = "idle" | "running" | "done" | "error";

function RateShocks() {
  const engine = useEngineData();
  const stress = engine.results.stress?.value as Record<string, { agg: Table }> | undefined;
  const pending = engine.pending.includes("stress");
  const failed = engine.errors.stress;
  const [requested, setRequested] = useState(false);

  // The 9Q stress is heavy, so it runs when asked and then follows input changes
  // like the other downstream results.
  const run = () => { setRequested(true); engine.request("stress", ["mbs", "deposits"]); };
  useEffect(() => { if (stress || pending) setRequested(true); }, [stress, pending]);

  const data = useMemo(() => stress?.mbs
    ? Object.values(
        rowsOf(stress.mbs.agg).reduce((acc: Record<number, Row>, r) => {
          const h = r.horizon_m as number;
          acc[h] ??= { horizon_m: h };
          acc[h][String(r.shock_bp)] = (r["pnl_$"] ?? r["eve_pnl_$"]) as number;
          return acc;
        }, {}))
    : [], [stress]);

  const stale = engine.isStale("stress") && !!stress;
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <Button disabled={pending} onClick={run}>
          {pending ? <><Spinner /> Updating</> : stress ? "Refresh 9Q stress" : "Run 9Q stress"}
        </Button>
        {stale && !engine.autoRecalc && <Badge tone="warning">Out of date</Badge>}
      </div>
      <Card>
        <CardHeader title="9Q stress P&L — MBS book" sub="Forward-starting parallel shocks, P&L vs base forward value"
          right={<Freshness kind="stress" />} />
        <CardBody className={stale ? "opacity-60 transition-opacity duration-base" : ""}>
          {stress?.mbs
            ? <StressLines data={data as never} shocks={[-100, 100, 200, 300]} />
            : <ChartState kind={pending ? "loading" : failed ? "error" : "empty"}
                hint={requested ? "Stress populates when the run finishes." : "Run 9Q stress to populate this chart."}
                error={failed} onRetry={run} />}
        </CardBody>
      </Card>
    </div>
  );
}

export default function Stress() {
  const [active, setActive] = useState("Rate shocks");
  const [balanceMounted, setBalanceMounted] = useState(false);
  const choose = (id: string) => {
    setActive(id);
    if (id === "Balance sheet") setBalanceMounted(true);
  };
  return (
    <div className="space-y-3">
      <Tabs id="stress" tabs={["Rate shocks", "Balance sheet"]} active={active} onChange={choose} />
      <div {...tabPanelProps("stress", "Rate shocks", active)}><RateShocks /></div>
      <div {...tabPanelProps("stress", "Balance sheet", active)}>{balanceMounted && <BalanceStress />}</div>
    </div>
  );
}
