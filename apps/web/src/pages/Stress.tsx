/** Stress — rate shocks and balance-sheet stress in one place. The 9Q rate-shock
 * P&L used to sit on Risk Desk; it now runs here, next to the balance-sheet
 * engine it is compared against. */
import { useEffect, useMemo, useState } from "react";
import { useEngineData } from "../lib/engine";
import { rowsOf, type Job, type Table } from "../lib/api";
import { StressLines } from "../components/charts";
import { Button, Card, CardBody, CardHeader, ChartState, Spinner, Tabs, tabPanelProps } from "../components/ui";
import BalanceStress from "./BalanceStress";

type Row = Record<string, number | string>;
type RunState = "idle" | "running" | "done" | "error";

function RateShocks() {
  const engine = useEngineData();
  const [stress, setStress] = useState<Record<string, { agg: Table }> | null>(null);
  const [state, setState] = useState<RunState>("idle");
  const [error, setError] = useState<string | null>(null);
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => { setStress(null); }, [engine.revision]);
  useEffect(() => {
    if (state !== "running") { setElapsed(0); return; }
    const t0 = Date.now();
    const id = setInterval(() => setElapsed((Date.now() - t0) / 1000), 250);
    return () => clearInterval(id);
  }, [state]);

  const run = async () => {
    setState("running"); setError(null);
    try {
      const done: Job = await engine.run("stress", { books: ["mbs", "deposits"] });
      if (done.status === "done") {
        setStress(done.result as never);
        setState("done");
      } else {
        setError(done.detail ?? "engine returned no result");
        setState("error");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setState("error");
    }
  };

  const data = useMemo(() => stress?.mbs
    ? Object.values(
        rowsOf(stress.mbs.agg).reduce((acc: Record<number, Row>, r) => {
          const h = r.horizon_m as number;
          acc[h] ??= { horizon_m: h };
          acc[h][String(r.shock_bp)] = (r["pnl_$"] ?? r["eve_pnl_$"]) as number;
          return acc;
        }, {}))
    : [], [stress]);

  return (
    <div className="space-y-3">
      <Button disabled={state === "running"} onClick={run}>
        {state === "running" ? <><Spinner /> running {elapsed.toFixed(0)}s</> : "Run 9Q stress"}
      </Button>
      <Card>
        <CardHeader title="9Q stress P&L — MBS book" sub="Forward-starting parallel shocks, P&L vs base forward value" />
        <CardBody>
          {stress?.mbs
            ? <StressLines data={data as never} shocks={[-100, 100, 200, 300]} />
            : <ChartState kind={state === "running" ? "loading" : state === "error" ? "error" : "empty"}
                hint="Run 9Q stress to populate" elapsed={elapsed} error={error} onRetry={run} />}
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
