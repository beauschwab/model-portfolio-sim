/** UI primitives following the Aperture Risk design system (see DESIGN.md):
 * near-square 1px radii, hairline borders over shadows, one yellow accent,
 * green/red only for direction, Inter tabular figures for numbers. */
import clsx from "clsx";
import { ChevronRight, Info } from "lucide-react";
import { useEffect as _ue, useRef as _ur, useState as _us, type KeyboardEvent, type ReactNode, type ButtonHTMLAttributes, type InputHTMLAttributes } from "react";

/** Panel: the standard surface — surface-1, 1px default border, top inset highlight, no shadow at rest. */
export const Card = ({ className, children }: { className?: string; children: ReactNode }) => (
  <div className={clsx("rounded-md border border-line-strong bg-surface-1 shadow-inset-top", className)}>{children}</div>
);
export const CardHeader = ({ title, sub, right }: { title: string; sub?: string; right?: ReactNode }) => (
  <div className="flex min-h-[44px] flex-wrap items-center justify-between gap-x-3 gap-y-2 border-b border-line px-3.5 py-2.5">
    <div className="min-w-0 flex-1 basis-48">
      <div className="truncate text-md font-semibold tracking-tight text-paper-heading">{title}</div>
      {sub && <div className="mt-0.5 text-xs text-paper-faint">{sub}</div>}
    </div>
    {right && <div className="flex shrink-0 items-center gap-1.5">{right}</div>}
  </div>
);
export const CardBody = ({ className, children }: { className?: string; children: ReactNode }) => (
  <div className={clsx("p-3.5", className)}>{children}</div>
);

type ButtonVariant = "primary" | "default" | "secondary" | "ghost" | "danger";
/** Button: primary (yellow, one per surface), secondary, ghost, danger. `default` is primary. */
export const Button = ({ className, variant = "primary", size = "md", ...p }:
  ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant; size?: "sm" | "md" }) => (
  <button
    className={clsx(
      "inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-sm border leading-none",
      "transition-[background-color,border-color,filter] duration-fast",
      "hover:brightness-110 active:translate-y-px disabled:cursor-not-allowed disabled:opacity-45 disabled:hover:brightness-100",
      size === "sm" ? "h-control-sm px-2.5 text-sm" : "h-control px-3 text-base",
      (variant === "primary" || variant === "default") && "border-brand-press bg-brand font-semibold text-ink active:bg-brand-press",
      variant === "secondary" && "border-line-strong bg-surface-2 font-medium text-paper active:bg-surface-active",
      variant === "ghost" && "border-transparent bg-transparent font-medium text-paper-dim hover:text-paper",
      variant === "danger" && "border-danger/35 bg-danger/15 font-semibold text-danger",
      className)}
    {...p}
  />
);

/** Input: base-surface field, default border, yellow border + soft ring on focus. */
export const Input = ({ className, ...p }: InputHTMLAttributes<HTMLInputElement>) => (
  <input
    className={clsx(
      "num h-control w-full rounded-sm border border-line-strong bg-surface-base px-2.5 text-base text-paper",
      "outline-none transition-[border-color,box-shadow] duration-fast placeholder:text-paper-faint",
      "focus:border-brand focus:shadow-focus focus-visible:outline-none", className)}
    {...p}
  />
);

export type Tone = "neutral" | "accent" | "up" | "down" | "warning" | "danger" | "info" | "ai";
const TONE: Record<Tone, string> = {
  neutral: "bg-surface-2 text-paper-dim border-paper-dim/20",
  accent: "bg-brand/15 text-brand-hover border-brand/25",
  up: "bg-up/15 text-up border-up/25",
  down: "bg-down/15 text-down border-down/25",
  warning: "bg-warning/15 text-warning border-warning/25",
  danger: "bg-danger/15 text-danger border-danger/25",
  info: "bg-info/15 text-info border-info/25",
  ai: "bg-ai/15 text-ai border-ai/25",
};
/** Badge: squared status label; tone carries the semantics, `dot` adds a status dot. */
export const Badge = ({ tone = "neutral", dot, children }: { tone?: Tone; dot?: boolean; children: ReactNode }) => (
  <span className={clsx("num inline-flex items-center gap-1.5 whitespace-nowrap rounded-sm border px-2 py-px text-xs font-semibold", TONE[tone])}>
    {dot && <span aria-hidden className="h-1.5 w-1.5 shrink-0 rounded-full bg-current" />}
    {children}
  </span>
);

/** A variance against an explicit reference. `value` drives direction; `invert`
 * marks metrics where an increase is unfavorable (cost, VaR, ΔEVE). */
export type Variance = { value: number; text: string; ref: string; invert?: boolean };

/** Stat: a KPI tile. Color is semantic only — a delta is green/red only when it is
 * a real variance to a stated reference. Neutral context goes in `detail`. */
export const Stat = ({ label, value, variance, detail }:
  { label: string; value: string; variance?: Variance; detail?: ReactNode }) => {
  const favorable = variance && variance.value !== 0 ? (variance.invert ? variance.value < 0 : variance.value > 0) : null;
  return (
    <div className="rounded-md border border-line-strong bg-surface-1 px-3.5 py-3 shadow-inset-top">
      <div className="eyebrow truncate">{label}</div>
      <div className="num mt-1 text-xl font-semibold tracking-display text-paper-heading">{value}</div>
      {variance && (
        <div className="num mt-1 flex items-center gap-1.5 whitespace-nowrap text-xs">
          <span className={clsx("font-semibold", favorable == null ? "text-paper-faint" : favorable ? "text-up" : "text-down")}>
            <span aria-hidden className="mr-0.5 text-[8px]">{variance.value > 0 ? "▲" : variance.value < 0 ? "▼" : "●"}</span>
            {variance.text}
          </span>
          <span className="truncate text-paper-faint">vs {variance.ref}</span>
        </div>
      )}
      {detail && <div className="num mt-0.5 truncate text-xs text-paper-faint">{detail}</div>}
    </div>
  );
};

/** Spinner: honors prefers-reduced-motion (Tailwind motion-reduce). */
export const Spinner = ({ className }: { className?: string }) => (
  <svg className={clsx("h-4 w-4 animate-spin motion-reduce:animate-none text-brand", className)}
    viewBox="0 0 24 24" fill="none" aria-hidden role="presentation">
    <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" strokeWidth="3" />
    <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
  </svg>
);

/** ChartState: consistent loading / error / empty frame for a data panel.
 * Empty states say what will appear and how to populate it. */
export function ChartState({ kind, hint, elapsed, error, onRetry }: {
  kind: "loading" | "error" | "empty"; hint?: string; elapsed?: number;
  error?: string | null; onRetry?: () => void;
}) {
  if (kind === "loading")
    return (
      <div className="flex h-full min-h-[14rem] flex-col items-center justify-center gap-2 text-paper-faint">
        <Spinner className="h-5 w-5" />
        <div className="num text-sm text-paper-dim" role="status" aria-live="polite">
          Running… {elapsed != null && elapsed > 0 ? `${elapsed.toFixed(0)}s` : ""}
        </div>
      </div>
    );
  if (kind === "error")
    return (
      <div className="flex h-full min-h-[14rem] flex-col items-center justify-center gap-2 px-6 text-center">
        <div className="text-sm font-semibold text-danger">Run failed</div>
        {error && <div className="text-xs leading-relaxed text-paper-faint">{error}</div>}
        {onRetry && <Button variant="secondary" onClick={onRetry}>Try again</Button>}
      </div>
    );
  return (
    <div className="flex h-full min-h-[14rem] items-center justify-center px-6 text-center text-sm text-paper-faint">
      {hint ?? "No data yet — run the analysis to populate this panel."}
    </div>
  );
}

/** DataTable: dense terminal table — uppercase micro headers, 28px rows,
 * right-aligned tabular numerics, sticky header. */
export function DataTable({ rows, cols, maxH = "28rem" }:
  { rows: Record<string, unknown>[]; cols?: string[]; maxH?: string }) {
  const [page, setPage] = _us(0);
  const pageSize = 100;
  const lastPage = Math.max(0, Math.ceil(rows.length / pageSize) - 1);
  const currentPage = Math.min(page, lastPage);
  _ue(() => setPage(0), [rows]);
  if (!rows.length) return <div className="px-3 py-2 text-sm text-paper-faint">No rows.</div>;
  const cs = cols ?? Object.keys(rows[0]);
  // A column is numeric if its first non-null value is a number; numeric
  // columns right-align so figures line up on their last digit (scannable).
  const numCol = new Set(cs.filter(c => {
    const row = rows.find(r => r[c] != null);
    return row != null && typeof row[c] === "number";
  }));
  return (
    <div>
      {rows.length > pageSize && <nav aria-label="Table pages" className="flex items-center justify-end gap-2 border-b border-line px-3 py-1.5 text-sm text-paper-dim">
        <span className="num">{currentPage * pageSize + 1}–{Math.min((currentPage + 1) * pageSize, rows.length)} of {rows.length}</span>
        <Button size="sm" variant="ghost" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}>Previous</Button>
        <Button size="sm" variant="ghost" disabled={currentPage === lastPage} onClick={() => setPage(currentPage + 1)}>Next</Button>
      </nav>}
      <div className="overflow-auto" style={{ maxHeight: maxH }}>
      <table className="w-full border-collapse text-left text-sm tabular-nums">
        <thead className="sticky top-0 z-10 bg-surface-base">
          <tr>{cs.map(c => (
            <th key={c} className={clsx("h-row whitespace-nowrap border-b border-line-strong px-3 eyebrow", numCol.has(c) && "text-right")}>{c}</th>
          ))}</tr>
        </thead>
        <tbody>
          {rows.slice(currentPage * pageSize, (currentPage + 1) * pageSize).map((r, i) => (
            <tr key={i} className="h-row border-b border-line transition-colors duration-fast hover:bg-surface-2">
              {cs.map(c => {
                const v = r[c];
                const isNum = typeof v === "number";
                return (
                  <td key={c} className={clsx("whitespace-nowrap px-3",
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
    </div>
  );
}

const tabSlug = (t: string) => t.toLowerCase().replace(/[^a-z0-9]+/g, "-");
/** Props for the panel a tab reveals, when `Tabs` is given the same `id`. */
export const tabPanelProps = (id: string, tab: string, active: string) => ({
  role: "tabpanel" as const, id: `${id}-panel-${tabSlug(tab)}`,
  "aria-labelledby": `${id}-tab-${tabSlug(tab)}`, hidden: tab !== active,
});

/** Tabs: Aperture segmented control for compact inline view switching. With an
 * `id`, each tab controls the panel rendered with `tabPanelProps(id, …)`. Arrow
 * keys, Home and End move between tabs; only the selected tab is in tab order. */
export const Tabs = ({ tabs, active, onChange, id }:
  { tabs: string[]; active: string; onChange: (t: string) => void; id?: string }) => {
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const i = tabs.indexOf(active);
    const next = e.key === "ArrowRight" ? (i + 1) % tabs.length : e.key === "ArrowLeft" ? (i - 1 + tabs.length) % tabs.length
      : e.key === "Home" ? 0 : e.key === "End" ? tabs.length - 1 : -1;
    if (next < 0) return;
    e.preventDefault();
    onChange(tabs[next]);
    (e.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next])?.focus();
  };
  return (
    <div role="tablist" onKeyDown={onKeyDown} className="inline-flex gap-0.5 rounded-sm border border-line-strong bg-surface-base p-0.5">
      {tabs.map(t => (
        <button key={t} type="button" role="tab" aria-selected={t === active} tabIndex={t === active ? 0 : -1}
          id={id ? `${id}-tab-${tabSlug(t)}` : undefined} aria-controls={id ? `${id}-panel-${tabSlug(t)}` : undefined}
          onClick={() => onChange(t)}
          className={clsx("rounded-sm px-2.5 py-1 text-sm font-semibold transition-colors duration-fast",
            t === active ? "bg-brand text-ink" : "text-paper-dim hover:text-paper")}>
          {t}
        </button>
      ))}
    </div>
  );
};

/** Popover: overlay surface with a subtle blur and tight dark shadow; outside-click dismiss. */
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
      <button type="button" onClick={() => setOpen(o => !o)} aria-expanded={open} className="inline-flex items-center">
        {trigger}
      </button>
      {open && (
        <div className="absolute left-0 top-full z-50 mt-1.5 rounded-lg border border-line-strong bg-surface-overlay/95 p-3 text-sm leading-relaxed text-paper-dim shadow-md backdrop-blur"
          style={{ width }}>
          {children}
        </div>
      )}
    </div>
  );
}

/** InfoPop: a muted info icon carrying method/config notes. */
export const InfoPop = ({ children, width }: { children: ReactNode; width?: string }) => (
  <Popover width={width} trigger={
    <Info aria-label="More information" className="ml-1 h-3.5 w-3.5 text-paper-faint transition-colors duration-fast hover:text-paper" strokeWidth={1.5} />
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
    <div id={id} className="scroll-mt-24 overflow-hidden rounded-md border border-line-strong bg-surface-1 shadow-inset-top">
      <button type="button" onClick={toggle} aria-expanded={isOpen}
        className="flex w-full items-center gap-2.5 px-3.5 py-2.5 text-left transition-colors duration-fast hover:bg-surface-2">
        <ChevronRight aria-hidden strokeWidth={1.5}
          className={clsx("h-3.5 w-3.5 shrink-0 text-paper-faint transition-transform duration-base motion-reduce:transition-none", isOpen && "rotate-90")} />
        <span className="shrink-0 text-md font-semibold tracking-tight text-paper-heading">{title}</span>
        {badge}
        <span className="ml-auto min-w-0 truncate text-right text-sm text-paper-faint">{!isOpen && summary}</span>
      </button>
      <div className="grid transition-[grid-template-rows] duration-base motion-reduce:transition-none"
        style={{ gridTemplateRows: isOpen ? "1fr" : "0fr" }}>
        <div className="min-h-0 overflow-hidden">
          <div className="border-t border-line p-3.5">{mounted && children}</div>
        </div>
      </div>
    </div>
  );
}
