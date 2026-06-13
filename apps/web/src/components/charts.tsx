/** Recharts wrappers themed for the Binance dark surface. */
import {
  Area, AreaChart, Bar, BarChart, CartesianGrid, Legend, Line, LineChart,
  ResponsiveContainer, Tooltip, XAxis, YAxis,
} from "recharts";

const axis = { stroke: "#707a8a", fontSize: 10, tickLine: false } as const;
const grid = { stroke: "#2b3139", strokeDasharray: "3 3" } as const;
const tip = {
  contentStyle: { background: "#1e2329", border: "1px solid #2b3139", borderRadius: 8, fontSize: 11 },
  labelStyle: { color: "#929aa5" },
} as const;

export function KrdBar({ data }: { data: { tenor: string; [k: string]: number | string }[] }) {
  const keys = data.length ? Object.keys(data[0]).filter(k => k !== "tenor") : [];
  const palette = ["#fcd535", "#2dbdb6", "#0ecb81", "#f6465d", "#3b82f6"];
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
            <stop offset="0%" stopColor="#fcd535" stopOpacity={0.35} />
            <stop offset="100%" stopColor="#fcd535" stopOpacity={0} />
          </linearGradient>
        </defs>
        <CartesianGrid {...grid} />
        <XAxis dataKey="month" {...axis} />
        <YAxis {...axis} tickFormatter={v => `${(v / 1e6).toFixed(0)}M`} />
        <Tooltip {...tip} formatter={(v: number) => `$${(v / 1e6).toFixed(1)}M`} />
        <Area type="monotone" dataKey="nii" stroke="#fcd535" fill="url(#nii)" strokeWidth={1.5} />
        <Line type="monotone" dataKey="interest_income" stroke="#2dbdb6" dot={false} />
      </AreaChart>
    </ResponsiveContainer>
  );
}

export function StressLines({ data, shocks }: { data: Record<string, number>[]; shocks: number[] }) {
  const palette: Record<string, string> = { "-100": "#0ecb81", "100": "#fcd535", "200": "#f6465d", "300": "#b3344a" };
  return (
    <ResponsiveContainer width="100%" height={260}>
      <LineChart data={data}>
        <CartesianGrid {...grid} />
        <XAxis dataKey="horizon_m" {...axis} label={{ value: "months fwd", fontSize: 10, fill: "#707a8a", dy: 12 }} />
        <YAxis {...axis} tickFormatter={v => `${(v / 1e6).toFixed(0)}M`} />
        <Tooltip {...tip} formatter={(v: number) => `$${(v / 1e6).toFixed(1)}M`} />
        <Legend wrapperStyle={{ fontSize: 11 }} />
        {shocks.map(s => (
          <Line key={s} type="monotone" dataKey={`${s}`} name={`${s > 0 ? "+" : ""}${s}bp`}
            stroke={palette[String(s)] ?? "#3b82f6"} dot={false} strokeWidth={1.5} />
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
        <Line type="monotone" dataKey="base" stroke="#707a8a" dot={{ r: 2 }} strokeWidth={1.5} />
        <Line type="monotone" dataKey="scenario" stroke="#fcd535" dot={{ r: 2 }} strokeWidth={1.5} />
      </LineChart>
    </ResponsiveContainer>
  );
}

export function ScenarioPath({ data }: { data: Record<string, number>[] }) {
  return (
    <ResponsiveContainer width="100%" height={240}>
      <LineChart data={data}>
        <CartesianGrid {...grid} />
        <XAxis dataKey="quarter" {...axis} label={{ value: "quarter", fontSize: 10, fill: "#707a8a", dy: 12 }} />
        <YAxis yAxisId="l" {...axis} tickFormatter={v => `${(v / 1e6).toFixed(0)}M`} />
        <YAxis yAxisId="r" orientation="right" {...axis} tickFormatter={v => `${v.toFixed(1)}%`} />
        <Tooltip {...tip} />
        <Legend wrapperStyle={{ fontSize: 11 }} />
        <Line yAxisId="l" dataKey="nii_annualized" name="NII (ann.)" stroke="#fcd535" dot strokeWidth={1.5} />
        <Line yAxisId="r" dataKey="nim_pct" name="NIM %" stroke="#2dbdb6" dot strokeWidth={1.5} />
      </LineChart>
    </ResponsiveContainer>
  );
}
