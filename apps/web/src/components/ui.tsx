/** Hand-rolled shadcn-style primitives, Supabase dark theme. */
import clsx from "clsx";
import { type ReactNode, type ButtonHTMLAttributes, type InputHTMLAttributes } from "react";

export const Card = ({ className, children }: { className?: string; children: ReactNode }) => (
  <div className={clsx("rounded-xl border border-line bg-surface-1 shadow-sm", className)}>{children}</div>
);
export const CardHeader = ({ title, sub, right }: { title: string; sub?: string; right?: ReactNode }) => (
  <div className="flex items-start justify-between border-b border-line px-4 py-3">
    <div>
      <div className="text-sm font-medium text-zinc-100">{title}</div>
      {sub && <div className="mt-0.5 text-xs text-zinc-500">{sub}</div>}
    </div>
    {right}
  </div>
);
export const CardBody = ({ className, children }: { className?: string; children: ReactNode }) => (
  <div className={clsx("p-4", className)}>{children}</div>
);

export const Button = ({ className, variant = "default", ...p }:
  ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "default" | "ghost" | "danger" }) => (
  <button
    className={clsx(
      "inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-xs font-medium transition-colors disabled:opacity-40",
      variant === "default" && "bg-brand-deep text-brand hover:bg-emerald-900",
      variant === "ghost" && "border border-line bg-surface-2 text-zinc-300 hover:bg-surface-3",
      variant === "danger" && "border border-red-900 bg-red-950 text-red-300 hover:bg-red-900",
      className)}
    {...p}
  />
);

export const Input = ({ className, ...p }: InputHTMLAttributes<HTMLInputElement>) => (
  <input
    className={clsx(
      "h-8 w-full rounded-md border border-line bg-surface-2 px-2 text-xs text-zinc-200 num",
      "outline-none focus:border-brand-dim focus:ring-1 focus:ring-brand-deep", className)}
    {...p}
  />
);

export const Badge = ({ tone = "zinc", children }: { tone?: "zinc" | "green" | "red" | "amber"; children: ReactNode }) => (
  <span className={clsx("inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium",
    tone === "green" && "bg-emerald-950 text-brand",
    tone === "red" && "bg-red-950 text-red-300",
    tone === "amber" && "bg-amber-950 text-amber-300",
    tone === "zinc" && "bg-surface-3 text-zinc-400")}>{children}</span>
);

export const Stat = ({ label, value, delta }: { label: string; value: string; delta?: string }) => (
  <div className="rounded-xl border border-line bg-surface-1 p-4">
    <div className="text-[11px] uppercase tracking-wide text-zinc-500">{label}</div>
    <div className="mt-1 text-xl font-semibold text-zinc-100 num">{value}</div>
    {delta && <div className={clsx("mt-0.5 text-xs num", delta.startsWith("-") ? "text-down" : "text-up")}>{delta}</div>}
  </div>
);

export function DataTable({ rows, cols, maxH = "28rem" }:
  { rows: Record<string, unknown>[]; cols?: string[]; maxH?: string }) {
  if (!rows.length) return <div className="p-4 text-xs text-zinc-500">no rows</div>;
  const cs = cols ?? Object.keys(rows[0]);
  return (
    <div className="overflow-auto" style={{ maxHeight: maxH }}>
      <table className="w-full text-left text-xs">
        <thead className="sticky top-0 bg-surface-2 text-zinc-400">
          <tr>{cs.map(c => <th key={c} className="whitespace-nowrap px-3 py-2 font-medium">{c}</th>)}</tr>
        </thead>
        <tbody className="divide-y divide-line">
          {rows.map((r, i) => (
            <tr key={i} className="hover:bg-surface-2">
              {cs.map(c => {
                const v = r[c];
                const isNum = typeof v === "number";
                return (
                  <td key={c} className={clsx("whitespace-nowrap px-3 py-1.5",
                    isNum && "num", isNum && (v as number) < 0 && "text-down")}>
                    {isNum ? (Math.abs(v as number) >= 1000 ? (v as number).toLocaleString(undefined, { maximumFractionDigits: 0 }) : (v as number).toFixed(Math.abs(v as number) < 10 ? 3 : 1)) : String(v)}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export const Tabs = ({ tabs, active, onChange }:
  { tabs: string[]; active: string; onChange: (t: string) => void }) => (
  <div className="flex gap-1 rounded-lg border border-line bg-surface-2 p-1">
    {tabs.map(t => (
      <button key={t} onClick={() => onChange(t)}
        className={clsx("rounded-md px-3 py-1 text-xs font-medium",
          t === active ? "bg-surface-3 text-brand" : "text-zinc-400 hover:text-zinc-200")}>
        {t}
      </button>
    ))}
  </div>
);
