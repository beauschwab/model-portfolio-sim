/** Recharts wrappers themed for the dark surface. */
import {
  Area, AreaChart, Bar, BarChart, CartesianGrid, Legend, Line, LineChart,
  ResponsiveContainer, Tooltip, XAxis, YAxis,
} from "recharts";

const axis = { stroke: "#3f3f46", fontSize: 10, tickLine: false } as const;
const grid = { stroke: "#1f1f23", strokeDasharray: "3 3" } as const;
const tip = {
  contentStyle: { background: "#101012", border: "1px solid #27272a", borderRadius: 8, fontSize: 11 },
  labelStyle: { color: "#a1a1aa" },
} as const;

export function KrdBar({ data }: { data: { tenor: string; [k: string]: number | string }[] }) {
  const keys = data.length ? Object.keys(data[0]).filter(k => k !== "tenor") : [];
  const palette = ["#3ecf8e", "#60a5fa", "#f59e0b", "#f87171", "#a78bfa"];
  return (
    <ResponsiveContainer width="100%" height={260}>
      <BarChart data={data} stackOffset="sign">
        <CartesianGrid {...grid} />
        <XAxis dataKey="tenor" {...axis} />
        <YAxis {...axis} tickFormatter={v => `${(v / 1000).toFixed(0)}k`} />
        <Tooltip {...tip} formatter={(v: number) => `$${v.toLocaleString()}/bp`} />
        <Legend wrapperStyle={{ fontSize: 11 }} />
        {keys.map((k, i) => <Bar key={k} dataKey={k} stackId="a" fill={palette[i % palette.length]} />)}
      </BarChart>
    </ResponsiveContainer>
  );
}

export function NiiArea({ data }: { data: Record<string, number>[] }) {
  return (
    <ResponsiveContainer width="100%" height={260}>
      <AreaChart data={data}>
        <defs>
          <linearGradient id="nii" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#3ecf8e" stopOpacity={0.35} />
            <stop offset="100%" stopColor="#3ecf8e" stopOpacity={0} />
          </linearGradient>
        </defs>
        <CartesianGrid {...grid} />
        <XAxis dataKey="month" {...axis} />
        <YAxis {...axis} tickFormatter={v => `${(v / 1e6).toFixed(0)}M`} />
        <Tooltip {...tip} formatter={(v: number) => `$${(v / 1e6).toFixed(1)}M`} />
        <Area type="monotone" dataKey="nii" stroke="#3ecf8e" fill="url(#nii)" strokeWidth={1.5} />
        <Line type="monotone" dataKey="interest_income" stroke="#60a5fa" dot={false} />
      </AreaChart>
    </ResponsiveContainer>
  );
}

export function StressLines({ data, shocks }: { data: Record<string, number>[]; shocks: number[] }) {
  const palette: Record<string, string> = { "-100": "#60a5fa", "100": "#f59e0b", "200": "#f87171", "300": "#dc2626" };
  return (
    <ResponsiveContainer width="100%" height={260}>
      <LineChart data={data}>
        <CartesianGrid {...grid} />
        <XAxis dataKey="horizon_m" {...axis} label={{ value: "months fwd", fontSize: 10, fill: "#71717a", dy: 12 }} />
        <YAxis {...axis} tickFormatter={v => `${(v / 1e6).toFixed(0)}M`} />
        <Tooltip {...tip} formatter={(v: number) => `$${(v / 1e6).toFixed(1)}M`} />
        <Legend wrapperStyle={{ fontSize: 11 }} />
        {shocks.map(s => (
          <Line key={s} type="monotone" dataKey={`${s}`} name={`${s > 0 ? "+" : ""}${s}bp`}
            stroke={palette[String(s)] ?? "#a78bfa"} dot={false} strokeWidth={1.5} />
        ))}
      </LineChart>
    </ResponsiveContainer>
  );
}

export function CurveChart({ data }: { data: { tenor: number; base: number; scenario?: number }[] }) {
  return (
    <ResponsiveContainer width="100%" height={220}>
      <LineChart data={data}>
        <CartesianGrid {...grid} />
        <XAxis dataKey="tenor" {...axis} scale="log" domain={[1, 30]} ticks={[1, 2, 5, 10, 20, 30]} />
        <YAxis {...axis} domain={["auto", "auto"]} tickFormatter={v => `${(v * 100).toFixed(1)}%`} />
        <Tooltip {...tip} formatter={(v: number) => `${(v * 100).toFixed(3)}%`} />
        <Line type="monotone" dataKey="base" stroke="#71717a" dot={{ r: 2 }} strokeWidth={1.5} />
        <Line type="monotone" dataKey="scenario" stroke="#3ecf8e" dot={{ r: 2 }} strokeWidth={1.5} />
      </LineChart>
    </ResponsiveContainer>
  );
}

export function ScenarioPath({ data }: { data: Record<string, number>[] }) {
  return (
    <ResponsiveContainer width="100%" height={240}>
      <LineChart data={data}>
        <CartesianGrid {...grid} />
        <XAxis dataKey="quarter" {...axis} label={{ value: "quarter", fontSize: 10, fill: "#71717a", dy: 12 }} />
        <YAxis yAxisId="l" {...axis} tickFormatter={v => `${(v / 1e6).toFixed(0)}M`} />
        <YAxis yAxisId="r" orientation="right" {...axis} tickFormatter={v => `${v.toFixed(1)}%`} />
        <Tooltip {...tip} />
        <Legend wrapperStyle={{ fontSize: 11 }} />
        <Line yAxisId="l" dataKey="nii_annualized" name="NII (ann.)" stroke="#3ecf8e" dot strokeWidth={1.5} />
        <Line yAxisId="r" dataKey="nim_pct" name="NIM %" stroke="#60a5fa" dot strokeWidth={1.5} />
      </LineChart>
    </ResponsiveContainer>
  );
}
