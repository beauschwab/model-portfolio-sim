import { useEngineData } from "../lib/engine";
/** Top-level KPI board: EVE & duration gap, LCR, NSFR, CET1 projection.
 * Weight tables are stylized (the calibration seam) — labels say so. */
import { Line, LineChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { chartAxis, chartGrid, chartTip, still } from "../components/charts";
import { LIMITS, limitVariance } from "../lib/limits";
import { api, awaitJob, fmt$ } from "../lib/api";
import { Badge, Card, CardBody, CardHeader, DataTable, Stat, InfoPop } from "../components/ui";

type Kpis = {
  eve: { eve_$: number; duration_gap_y: number; dur_assets_y: number; dur_liab_y: number;
    dv01_net_$: number; irrbb_outlier: boolean; irrbb_worst_pct_eve: number;
    sensitivity: Record<string, number | string>[] };
  lcr: { lcr_pct: number; hqla_$: number; net_outflows_$: number };
  nsfr: { nsfr_pct: number; asf_$: number; rsf_$: number };
  capital: { rwa_total_$: number; rwa_density_pct: number;
    cet1_path: { quarter: number; cet1_ratio_pct: number; cet1_$: number; drivers: string }[]; note: string };
};

export default function KpisPage() {
  const engine = useEngineData();
  const k = engine.kpis;

  return (
    <div className="space-y-3">
      <p className="text-xs text-paper-faint">
        {k ? "Parallel dv01s by full revaluation · " : "No results yet. Use Run sheet in the top bar. "}
        stylized 12 CFR 249 / NSFR / standardized-RWA weights (calibration seam)
      </p>

      {k && (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-6">
            <Stat label="EVE" value={fmt$(k.eve.eve_$)} detail={`Net dv01 ${fmt$(k.eve.dv01_net_$)}/bp`} />
            <Stat label="Duration gap" value={`${k.eve.duration_gap_y.toFixed(2)}y`}
              variance={limitVariance(k.eve.duration_gap_y, LIMITS.durationGap, 2)}
              detail={`Assets ${k.eve.dur_assets_y.toFixed(2)}y · liabilities ${k.eve.dur_liab_y.toFixed(2)}y`} />
            <Stat label="LCR" value={`${k.lcr.lcr_pct.toFixed(0)}%`} variance={limitVariance(k.lcr.lcr_pct, LIMITS.lcr, 0)}
              detail={`HQLA ${fmt$(k.lcr.hqla_$)}`} />
            <Stat label="NSFR" value={`${k.nsfr.nsfr_pct.toFixed(0)}%`} variance={limitVariance(k.nsfr.nsfr_pct, LIMITS.nsfr, 0)}
              detail={`ASF ${fmt$(k.nsfr.asf_$)}`} />
            <Stat label="CET1 (t0)" value={`${k.capital.cet1_path[0].cet1_ratio_pct.toFixed(2)}%`}
              variance={limitVariance(k.capital.cet1_path[0].cet1_ratio_pct, LIMITS.cet1, 2)}
              detail={`RWA ${fmt$(k.capital.rwa_total_$)} · ${k.capital.rwa_density_pct.toFixed(0)}% density`} />
            <div className="rounded-md border border-line-strong bg-surface-1 px-3.5 py-3 shadow-inset-top">
              <div className="eyebrow flex items-center">IRRBB outlier
                <InfoPop width="15rem">Supervisory outlier test: worst parallel-shock ΔEVE beyond 15% of equity draws review. First-order from parallel dv01; convexity lives in the 9Q stress pack. LCR/NSFR/RWA weights are stylized module data — swap in internal mappings before relying on levels.</InfoPop>
              </div>
              <div className="mt-1"><Badge tone={k.eve.irrbb_outlier ? "danger" : "up"}>
                {k.eve.irrbb_worst_pct_eve.toFixed(1)}% EVE worst shock {k.eve.irrbb_outlier ? "(>15%)" : ""}
              </Badge></div>
              <div className="mt-1 text-xs text-paper-faint">No swap hedge book modeled; a live book would hedge this.</div>
            </div>
          </div>

          <div className="grid gap-3 xl:grid-cols-2">
            <Card>
              <CardHeader title="ΔEVE by parallel shock" sub="First-order (parallel dv01); convexity in the 9Q stress pack" />
              <CardBody className="p-0"><DataTable rows={k.eve.sensitivity} /></CardBody>
            </Card>
            <Card>
              <CardHeader title="CET1 projection (9Q)" sub={k.capital.note} />
              <CardBody>
                <ResponsiveContainer width="100%" height={230}>
                  <LineChart data={k.capital.cet1_path}>
                    <CartesianGrid {...chartGrid} />
                    <XAxis dataKey="quarter" {...chartAxis} />
                    <YAxis {...chartAxis} domain={["auto", "auto"]} tickFormatter={v => `${v.toFixed(1)}%`} />
                    <Tooltip {...chartTip}
                      formatter={(v: number) => `${v.toFixed(2)}%`} />
                    <Line dataKey="cet1_ratio_pct" name="CET1 %" stroke="var(--viz-1)" dot={{ r: 2, fill: "var(--viz-1)" }} strokeWidth={1.5} {...still} />
                  </LineChart>
                </ResponsiveContainer>
              </CardBody>
            </Card>
          </div>
        </>
      )}
    </div>
  );
}
