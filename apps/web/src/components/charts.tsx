/** Recharts wrappers + the shared chart theme for the Aperture dark surface.
 *
 * Colors are CSS custom properties (src/styles/aperture.css), which SVG
 * presentation attributes resolve. Series order follows --viz-1/2/3
 * (yellow → sky → violet). Green/red are reserved for direction and are never
 * used as categorical series colors. Minimal gridlines, muted axes, no gradients. */
import {
  Area, AreaChart, Bar, BarChart, CartesianGrid, Legend, Line, LineChart,
  ResponsiveContainer, Tooltip, XAxis, YAxis,
} from "recharts";

/** Categorical series order; never green/red. */
export const VIZ = [
  "var(--viz-1)", "var(--viz-2)", "var(--viz-3)", "var(--sky-600)", "var(--gray-300)", "var(--yellow-700)",
] as const;
export const REF = "var(--gray-300)"; // reference / baseline series

export const chartAxis = {
  stroke: "var(--border-default)", tickLine: false,
  tick: { fill: "var(--text-tertiary)", fontSize: 10 },
} as const;
export const chartGrid = { stroke: "var(--border-subtle)", strokeDasharray: "2 4", vertical: false } as const;
export const chartTip = {
  contentStyle: {
    background: "var(--surface-overlay)", border: "1px solid var(--border-default)",
    borderRadius: 1, fontSize: 11, boxShadow: "var(--shadow-md)", color: "var(--text-primary)",
  },
  labelStyle: { color: "var(--text-secondary)" },
  itemStyle: { fontVariantNumeric: "tabular-nums" },
  cursor: { stroke: "var(--border-strong)", fill: "var(--yellow-softer)" },
} as const;
export const chartLegend = { wrapperStyle: { fontSize: 11, color: "var(--text-secondary)" } } as const;
export const axisLabel = (value: string) => ({ value, fontSize: 10, fill: "var(--text-tertiary)", dy: 12 });
/** Restrained motion: no long chart entrance animations. */
export const still = { isAnimationActive: false } as const;

/** Shock palette: ordered by direction and magnitude (sky for falling rates,
 * yellow for rising), so color encodes the scenario, not a gain/loss judgment. */
function shockColor(s: number, shocks: number[]): string {
  if (s === 0) return REF;
  const same = shocks.filter(x => Math.sign(x) === Math.sign(s)).map(Math.abs).sort((a, b) => a - b);
  const rank = same.indexOf(Math.abs(s));
  const ramp = s < 0 ? ["var(--sky-400)", "var(--sky-500)", "var(--sky-600)"] : ["var(--yellow-300)", "var(--yellow-500)", "var(--yellow-700)"];
  return ramp[Math.min(rank, ramp.length - 1)];
}

export function KrdBar({ data }: { data: { tenor: string; [k: string]: number | string }[] }) {
  const keys = data.length ? Object.keys(data[0]).filter(k => k !== "tenor") : [];
  return (
    <ResponsiveContainer width="100%" height={260}>
      <BarChart data={data} stackOffset="sign">
        <CartesianGrid {...chartGrid} />
        <XAxis dataKey="tenor" {...chartAxis} />
        <YAxis {...chartAxis} tickFormatter={v => `${(v / 1000).toFixed(0)}k`} />
        <Tooltip {...chartTip} formatter={(v: number) => `$${v.toLocaleString()}/bp`} />
        <Legend {...chartLegend} />
        {keys.map((k, i) => <Bar key={k} dataKey={k} stackId="a" fill={VIZ[i % VIZ.length]} {...still} />)}
      </BarChart>
    </ResponsiveContainer>
  );
}

export function NiiArea({ data }: { data: Record<string, number>[] }) {
  return (
    <ResponsiveContainer width="100%" height={260}>
      <AreaChart data={data}>
        <CartesianGrid {...chartGrid} />
        <XAxis dataKey="month" {...chartAxis} />
        <YAxis {...chartAxis} tickFormatter={v => `${(v / 1e6).toFixed(0)}M`} />
        <Tooltip {...chartTip} formatter={(v: number) => `$${(v / 1e6).toFixed(1)}M`} />
        <Area type="monotone" dataKey="nii" name="NII" stroke="var(--viz-1)" fill="var(--yellow-softer)" strokeWidth={1.5} {...still} />
        <Line type="monotone" dataKey="interest_income" name="Interest income" stroke="var(--viz-2)" dot={false} {...still} />
      </AreaChart>
    </ResponsiveContainer>
  );
}

export function StressLines({ data, shocks }: { data: Record<string, number>[]; shocks: number[] }) {
  return (
    <ResponsiveContainer width="100%" height={260}>
      <LineChart data={data}>
        <CartesianGrid {...chartGrid} />
        <XAxis dataKey="horizon_m" {...chartAxis} label={axisLabel("Months forward")} />
        <YAxis {...chartAxis} tickFormatter={v => `${(v / 1e6).toFixed(0)}M`} />
        <Tooltip {...chartTip} formatter={(v: number) => `$${(v / 1e6).toFixed(1)}M`} />
        <Legend {...chartLegend} />
        {shocks.map(s => (
          <Line key={s} type="monotone" dataKey={`${s}`} name={`${s > 0 ? "+" : s < 0 ? "−" : ""}${Math.abs(s)}bp`}
            stroke={shockColor(s, shocks)} dot={false} strokeWidth={1.5} {...still} />
        ))}
      </LineChart>
    </ResponsiveContainer>
  );
}

export function CurveChart({ data }: { data: { tenor: number; base: number; scenario?: number }[] }) {
  return (
    <ResponsiveContainer width="100%" height={220}>
      <LineChart data={data}>
        <CartesianGrid {...chartGrid} />
        <XAxis dataKey="tenor" {...chartAxis} scale="log" domain={[1, 30]} ticks={[1, 2, 5, 10, 20, 30]} />
        <YAxis {...chartAxis} domain={["auto", "auto"]} tickFormatter={v => `${(v * 100).toFixed(1)}%`} />
        <Tooltip {...chartTip} formatter={(v: number) => `${(v * 100).toFixed(3)}%`} />
        <Line type="monotone" dataKey="base" name="Base" stroke={REF} strokeDasharray="3 3" dot={{ r: 2, fill: REF }} strokeWidth={1.5} {...still} />
        <Line type="monotone" dataKey="scenario" name="Scenario" stroke="var(--viz-1)" dot={{ r: 2, fill: "var(--viz-1)" }} strokeWidth={1.5} {...still} />
      </LineChart>
    </ResponsiveContainer>
  );
}

export function ScenarioPath({ data }: { data: Record<string, number>[] }) {
  return (
    <ResponsiveContainer width="100%" height={240}>
      <LineChart data={data}>
        <CartesianGrid {...chartGrid} />
        <XAxis dataKey="quarter" {...chartAxis} label={axisLabel("Quarter")} />
        <YAxis yAxisId="l" {...chartAxis} tickFormatter={v => `${(v / 1e6).toFixed(0)}M`} />
        <YAxis yAxisId="r" orientation="right" {...chartAxis} tickFormatter={v => `${v.toFixed(1)}%`} />
        <Tooltip {...chartTip} />
        <Legend {...chartLegend} />
        <Line yAxisId="l" dataKey="nii_annualized" name="NII (ann.)" stroke="var(--viz-1)" dot={{ r: 2, fill: "var(--viz-1)" }} strokeWidth={1.5} {...still} />
        <Line yAxisId="r" dataKey="nim_pct" name="NIM %" stroke="var(--viz-2)" dot={{ r: 2, fill: "var(--viz-2)" }} strokeWidth={1.5} {...still} />
      </LineChart>
    </ResponsiveContainer>
  );
}
