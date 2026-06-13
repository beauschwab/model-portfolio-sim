/** Top-level KPI board: EVE & duration gap, LCR, NSFR, CET1 projection.
 * Weight tables are stylized (the calibration seam) — labels say so. */
import { useState } from "react";
import { Line, LineChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { api, awaitJob, fmt$ } from "../lib/api";
import { Badge, Button, Card, CardBody, CardHeader, DataTable, Stat, InfoPop } from "../components/ui";

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
  const [k, setK] = useState<Kpis | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async () => {
    setBusy(true);
    try {
      const j = await api.run("kpis");
      const done = await awaitJob(j.id);
      if (done.status === "done") setK(done.result as Kpis);
      else alert(done.detail);
    } finally { setBusy(false); }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-3">
        <Button disabled={busy} onClick={run}>{busy ? "running…" : "Compute KPIs"}</Button>
        <span className="text-[11px] text-paper-faint">
          parallel dv01s by full revaluation · stylized 12 CFR 249 / NSFR / standardized-RWA weights (calibration seam)
        </span>
      </div>

      {k && (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-6">
            <Stat label="EVE" value={fmt$(k.eve.eve_$)} delta={`net dv01 ${fmt$(k.eve.dv01_net_$)}/bp`} />
            <Stat label="Duration gap" value={`${k.eve.duration_gap_y.toFixed(2)}y`}
              delta={`A ${k.eve.dur_assets_y.toFixed(2)} / L ${k.eve.dur_liab_y.toFixed(2)}`} />
            <Stat label="LCR" value={`${k.lcr.lcr_pct.toFixed(0)}%`} delta={`HQLA ${fmt$(k.lcr.hqla_$)}`} />
            <Stat label="NSFR" value={`${k.nsfr.nsfr_pct.toFixed(0)}%`} delta={`ASF ${fmt$(k.nsfr.asf_$)}`} />
            <Stat label="CET1 (t0)" value={`${k.capital.cet1_path[0].cet1_ratio_pct.toFixed(2)}%`}
              delta={`RWA ${fmt$(k.capital.rwa_total_$)} (${k.capital.rwa_density_pct.toFixed(0)}% density)`} />
            <div className="rounded-lg border border-line bg-surface-1 p-3">
              <div className="flex items-center text-[10px] uppercase tracking-wide text-paper-faint">IRRBB outlier
                <InfoPop width="15rem">Supervisory outlier test: worst parallel-shock ΔEVE beyond 15% of equity draws review. First-order from parallel dv01; convexity lives in the 9Q stress pack. LCR/NSFR/RWA weights are stylized module data — swap in internal mappings before relying on levels.</InfoPop>
              </div>
              <div className="mt-1"><Badge tone={k.eve.irrbb_outlier ? "red" : "green"}>
                {k.eve.irrbb_worst_pct_eve.toFixed(1)}% EVE worst shock {k.eve.irrbb_outlier ? "(>15%)" : ""}
              </Badge></div>
              <div className="mt-1 text-[10px] text-paper-faint">no swap hedge book modeled — the real one hedges this</div>
            </div>
          </div>

          <div className="grid gap-3 xl:grid-cols-2">
            <Card>
              <CardHeader title="ΔEVE by parallel shock" sub="first-order (parallel dv01); convexity in the 9Q stress pack" />
              <CardBody className="p-0"><DataTable rows={k.eve.sensitivity} /></CardBody>
            </Card>
            <Card>
              <CardHeader title="CET1 projection (9Q)" sub={k.capital.note} />
              <CardBody>
                <ResponsiveContainer width="100%" height={230}>
                  <LineChart data={k.capital.cet1_path}>
                    <CartesianGrid stroke="#2b3139" strokeDasharray="3 3" />
                    <XAxis dataKey="quarter" stroke="#707a8a" fontSize={10} />
                    <YAxis stroke="#707a8a" fontSize={10} domain={["auto", "auto"]} tickFormatter={v => `${v.toFixed(1)}%`} />
                    <Tooltip contentStyle={{ background: "#1e2329", border: "1px solid #2b3139", borderRadius: 8, fontSize: 11 }}
                      formatter={(v: number) => `${v.toFixed(2)}%`} />
                    <Line dataKey="cet1_ratio_pct" stroke="#fcd535" dot strokeWidth={1.5} />
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
