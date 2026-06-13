/** Hand-rolled shadcn-style primitives, Binance dark theme. */
import clsx from "clsx";
import { useEffect as _ue, useRef as _ur, useState as _us, type ReactNode, type ButtonHTMLAttributes, type InputHTMLAttributes } from "react";

export const Card = ({ className, children }: { className?: string; children: ReactNode }) => (
  <div className={clsx("rounded-xl border border-line bg-surface-1 shadow-sm", className)}>{children}</div>
);
export const CardHeader = ({ title, sub, right }: { title: string; sub?: string; right?: ReactNode }) => (
  <div className="flex items-start justify-between border-b border-line px-3 py-2">
    <div>
      <div className="text-[13px] font-medium text-paper">{title}</div>
      {sub && <div className="mt-0.5 text-[11px] text-paper-faint">{sub}</div>}
    </div>
    {right}
  </div>
);
export const CardBody = ({ className, children }: { className?: string; children: ReactNode }) => (
  <div className={clsx("p-3", className)}>{children}</div>
);

export const Button = ({ className, variant = "default", ...p }:
  ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "default" | "ghost" | "danger" }) => (
  <button
    className={clsx(
      "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-xs font-semibold transition-colors disabled:opacity-40",
      variant === "default" && "bg-brand text-ink hover:bg-brand-dim",
      variant === "ghost" && "border border-line bg-surface-2 text-paper-dim hover:bg-surface-3",
      variant === "danger" && "border border-down/40 bg-down/10 text-down hover:bg-down/20",
      className)}
    {...p}
  />
);

export const Input = ({ className, ...p }: InputHTMLAttributes<HTMLInputElement>) => (
  <input
    className={clsx(
      "h-7 w-full rounded-md border border-line bg-surface-2 px-2 text-xs text-paper num",
      "outline-none focus:border-brand focus:ring-1 focus:ring-brand-deep", className)}
    {...p}
  />
);

export const Badge = ({ tone = "zinc", children }: { tone?: "zinc" | "green" | "red" | "amber"; children: ReactNode }) => (
  <span className={clsx("inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium",
    tone === "green" && "bg-up/15 text-up",
    tone === "red" && "bg-down/15 text-down",
    tone === "amber" && "bg-brand/15 text-brand",
    tone === "zinc" && "bg-surface-3 text-paper-dim")}>{children}</span>
);

export const Stat = ({ label, value, delta }: { label: string; value: string; delta?: string }) => {
  const down = delta?.startsWith("-");
  return (
    <div className="rounded-lg border border-line bg-surface-1 p-3">
      <div className="text-[10px] uppercase tracking-wide text-paper-faint">{label}</div>
      <div className="mt-0.5 text-lg font-semibold text-paper num">{value}</div>
      {delta && (
        <div className={clsx("mt-0.5 text-[11px] num", down ? "text-down" : "text-up")}>
          {/[+-]?\d/.test(delta) && <span aria-hidden className="mr-0.5">{down ? "▼" : "▲"}</span>}{delta}
        </div>
      )}
    </div>
  );
};

/** Spinner: GPU-friendly, honors prefers-reduced-motion (Tailwind motion-reduce). */
export const Spinner = ({ className }: { className?: string }) => (
  <svg className={clsx("h-4 w-4 animate-spin motion-reduce:animate-none text-brand", className)}
    viewBox="0 0 24 24" fill="none" aria-hidden role="presentation">
    <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" strokeWidth="3" />
    <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
  </svg>
);

/** ChartState: consistent loading / error / empty frame for a data panel. */
export function ChartState({ kind, hint, elapsed, error, onRetry }: {
  kind: "loading" | "error" | "empty"; hint?: string; elapsed?: number;
  error?: string | null; onRetry?: () => void;
}) {
  if (kind === "loading")
    return (
      <div className="flex h-full min-h-[14rem] flex-col items-center justify-center gap-2 text-paper-faint">
        <Spinner className="h-5 w-5" />
        <div className="num text-xs text-paper-dim" role="status" aria-live="polite">
          running… {elapsed != null && elapsed > 0 ? `${elapsed.toFixed(0)}s` : ""}
        </div>
      </div>
    );
  if (kind === "error")
    return (
      <div className="flex h-full min-h-[14rem] flex-col items-center justify-center gap-2 px-6 text-center">
        <div className="text-xs font-medium text-down">Run failed</div>
        {error && <div className="text-[11px] leading-relaxed text-paper-faint">{error}</div>}
        {onRetry && <Button variant="ghost" onClick={onRetry}>Try again</Button>}
      </div>
    );
  return (
    <div className="flex h-full min-h-[14rem] items-center justify-center text-xs text-paper-faint">
      {hint ?? "no data"}
    </div>
  );
}

export function DataTable({ rows, cols, maxH = "28rem" }:
  { rows: Record<string, unknown>[]; cols?: string[]; maxH?: string }) {
  if (!rows.length) return <div className="px-3 py-2 text-xs text-paper-faint">no rows</div>;
  const cs = cols ?? Object.keys(rows[0]);
  // A column is numeric if its first non-null value is a number; numeric
  // columns right-align so figures line up on their last digit (scannable).
  const numCol = new Set(cs.filter(c => {
    const row = rows.find(r => r[c] != null);
    return row != null && typeof row[c] === "number";
  }));
  return (
    <div className="overflow-auto" style={{ maxHeight: maxH }}>
      <table className="w-full text-left text-xs tabular-nums">
        <thead className="sticky top-0 z-10 bg-surface-2 text-paper-dim">
          <tr>{cs.map(c => (
            <th key={c} className={clsx("whitespace-nowrap px-2.5 py-1.5 font-medium", numCol.has(c) && "text-right")}>{c}</th>
          ))}</tr>
        </thead>
        <tbody className="divide-y divide-line">
          {rows.map((r, i) => (
            <tr key={i} className="hover:bg-surface-2">
              {cs.map(c => {
                const v = r[c];
                const isNum = typeof v === "number";
                return (
                  <td key={c} className={clsx("whitespace-nowrap px-2.5 py-1",
                    (isNum || numCol.has(c)) && "text-right",
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
  <div className="flex gap-1 rounded-lg border border-line bg-surface-2 p-0.5">
    {tabs.map(t => (
      <button key={t} onClick={() => onChange(t)}
        className={clsx("rounded-md px-2.5 py-1 text-xs font-medium",
          t === active ? "bg-surface-3 text-brand" : "text-paper-dim hover:text-paper")}>
        {t}
      </button>
    ))}
  </div>
);

/** Popover: hand-rolled, outside-click dismiss, brass-trimmed panel. */
export function Popover({ trigger, children, width = "16rem" }:
  { trigger: ReactNode; children: ReactNode; width?: string }) {
  const [open, setOpen] = _us(false);
  const ref = _ur<HTMLDivElement>(null);
  _ue(() => {
    if (!open) return;
    const h = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", h);
    return () => document.removeEventListener("mousedown", h);
  }, [open]);
  return (
    <div ref={ref} className="relative inline-flex">
      <button type="button" onClick={() => setOpen(o => !o)} className="inline-flex items-center">
        {trigger}
      </button>
      {open && (
        <div className="absolute left-0 top-full z-50 mt-1.5 rounded-lg border border-brand/40 bg-surface-1 p-3 text-xs leading-relaxed text-paper-dim shadow-xl"
          style={{ width }}>
          {children}
        </div>
      )}
    </div>
  );
}

/** InfoPop: small brass info mark carrying method/config notes. */
export const InfoPop = ({ children, width }: { children: ReactNode; width?: string }) => (
  <Popover width={width} trigger={
    <span className="ml-1 inline-flex h-3.5 w-3.5 items-center justify-center rounded-full border border-brand/60 text-[9px] leading-none text-brand hover:bg-brand-deep">i</span>
  }>{children}</Popover>
);

/** Accordion: a collapsible section whose collapsed header carries a RICH
 * summary (the detail you need before deciding to open it). Children mount
 * lazily on first open and stay mounted (preserves embedded tool state and
 * avoids recharts 0-size warnings). Controlled (open + onToggle) or internal.
 * Height animates via grid-template-rows; reduced motion respected. */
export function Accordion({ id, title, summary, badge, open, onToggle, defaultOpen = false, children }: {
  id?: string; title: string; summary?: ReactNode; badge?: ReactNode;
  open?: boolean; onToggle?: (v: boolean) => void; defaultOpen?: boolean; children: ReactNode;
}) {
  const [internal, setInternal] = _us(defaultOpen);
  const isOpen = open ?? internal;
  const [mounted, setMounted] = _us(isOpen);
  _ue(() => { if (isOpen) setMounted(true); }, [isOpen]);
  const toggle = () => { const n = !isOpen; if (onToggle) onToggle(n); else setInternal(n); };
  return (
    <div id={id} className="overflow-hidden rounded-xl border border-line bg-surface-1 shadow-sm scroll-mt-24">
      <button type="button" onClick={toggle} aria-expanded={isOpen}
        className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors hover:bg-surface-2">
        <svg viewBox="0 0 16 16" aria-hidden
          className={clsx("h-3.5 w-3.5 shrink-0 text-paper-faint transition-transform duration-200 motion-reduce:transition-none", isOpen && "rotate-90")}>
          <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        <span className="shrink-0 text-[13px] font-medium text-paper">{title}</span>
        {badge}
        <span className="ml-auto min-w-0 truncate text-right text-xs text-paper-faint">{!isOpen && summary}</span>
      </button>
      <div className="grid transition-[grid-template-rows] duration-200 ease-out motion-reduce:transition-none"
        style={{ gridTemplateRows: isOpen ? "1fr" : "0fr" }}>
        <div className="min-h-0 overflow-hidden">
          <div className="border-t border-line p-3">{mounted && children}</div>
        </div>
      </div>
    </div>
  );
}
