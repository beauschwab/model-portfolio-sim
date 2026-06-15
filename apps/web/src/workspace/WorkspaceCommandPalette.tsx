import { useEffect, useMemo, useRef, useState } from "react";
import clsx from "clsx";
import { useEngine } from "../lib/engine";
import { PANEL_DEFS, type PanelId } from "./panels";
import { useWorkspace } from "./Workspace";

interface Action {
  id: string;
  label: string;
  group: string;
  hint?: string;
  run: () => void;
  disabled?: boolean;
}

function fuzzy(q: string, text: string): number | null {
  if (!q) return 0;
  const target = text.toLowerCase();
  let ti = 0, score = 0, streak = 0;
  for (const ch of q.toLowerCase()) {
    const found = target.indexOf(ch, ti);
    if (found < 0) return null;
    score += found - ti + (found > ti ? 2 : 0);
    streak = found === ti ? streak + 1 : 0;
    score -= streak;
    ti = found + 1;
  }
  return score;
}

export function WorkspaceCommandPalette() {
  const engine = useEngine();
  const workspace = useWorkspace();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setOpen(value => !value);
      } else if (event.key === "Escape") {
        setOpen(false);
      }
    };
    const onOpen = () => setOpen(true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("palette:open", onOpen);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("palette:open", onOpen);
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setSelected(0);
    requestAnimationFrame(() => inputRef.current?.focus());
  }, [open]);

  const close = () => setOpen(false);

  const actions = useMemo<Action[]>(() => {
    const list: Action[] = [];
    const runKinds: [string, string, string][] = [
      ["kpis", "Run KPI sheet", "EVE · LCR · NSFR · CET1"],
      ["risk", "Run risk (KRD profile)", "fixed-OAS · CRN · all books"],
      ["nii", "Run NII forecast", "LMM Monte Carlo · 27m"],
      ["stress", "Run 9Q stress", "forward parallel shocks"],
    ];
    for (const [kind, label, hint] of runKinds) {
      list.push({
        id: `run-${kind}`,
        label,
        hint,
        group: "Run",
        disabled: engine.running,
        run: () => {
          close();
          void engine.run(kind, kind === "stress" ? { books: ["mbs", "deposits"] } : undefined);
        },
      });
    }

    for (const panel of PANEL_DEFS) {
      list.push({
        id: `open-${panel.id}`,
        label: `Open panel · ${panel.title}`,
        hint: panel.subtitle,
        group: "Panels",
        run: () => { close(); workspace.openPanel(panel.id as PanelId); },
      });
    }

    for (const name of Object.keys(engine.scenarios)) {
      const scenario = engine.scenarios[name];
      const last = scenario?.ust10y_bp?.[scenario.ust10y_bp.length - 1] ?? 0;
      list.push({
        id: `scenario-${name}`,
        label: `Scenario · ${name}`,
        hint: `10y ${last >= 0 ? "+" : ""}${last}bp`,
        group: "Scenario",
        disabled: name === engine.active,
        run: () => { close(); engine.setActive(name); },
      });
    }

    list.push({ id: "layout-reset", label: "Reset dock layout", group: "Layouts", run: () => { close(); workspace.resetLayout(); } });
    for (const item of workspace.namedLayouts) {
      list.push({
        id: `layout-${item.name}`,
        label: `Apply layout · ${item.name}`,
        hint: new Date(item.updatedAt).toLocaleString(),
        group: "Layouts",
        run: () => { close(); workspace.applyLayout(item.layout); },
      });
    }
    return list;
  }, [engine, workspace]);

  const filtered = useMemo(() => {
    const scored = actions
      .map(action => ({ action, score: fuzzy(query, `${action.group} ${action.label} ${action.hint ?? ""}`) }))
      .filter(item => item.score !== null) as { action: Action; score: number }[];
    scored.sort((a, b) => a.score - b.score);
    return scored.map(item => item.action);
  }, [actions, query]);

  useEffect(() => { setSelected(0); }, [query]);
  useEffect(() => {
    const element = listRef.current?.querySelector<HTMLElement>(`[data-index="${selected}"]`);
    element?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  if (!open) return null;

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "ArrowDown") { event.preventDefault(); setSelected(value => Math.min(value + 1, filtered.length - 1)); }
    else if (event.key === "ArrowUp") { event.preventDefault(); setSelected(value => Math.max(value - 1, 0)); }
    else if (event.key === "Enter") {
      event.preventDefault();
      const action = filtered[selected];
      if (action && !action.disabled) action.run();
    }
  };

  let group = "";
  return (
    <div className="fixed inset-0 z-[1000] flex items-start justify-center px-4 pt-[12vh]" role="dialog" aria-modal="true" aria-label="Command palette">
      <button aria-hidden tabIndex={-1} className="absolute inset-0 cursor-default bg-ink/70 backdrop-blur-sm" onClick={close} />
      <div className="palette-pop relative w-full max-w-xl overflow-hidden rounded-xl border border-brand/30 bg-surface-1 shadow-2xl">
        <div className="flex items-center gap-2.5 border-b border-line px-4">
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="#929aa5" strokeWidth="1.5" aria-hidden>
            <circle cx="7" cy="7" r="5" /><path d="M11 11l3 3" strokeLinecap="round" />
          </svg>
          <input
            ref={inputRef}
            value={query}
            onChange={event => setQuery(event.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Open panels, run jobs, switch scenarios, apply layouts..."
            className="h-12 w-full bg-transparent text-sm text-paper outline-none placeholder:text-paper-dim"
          />
          <kbd className="hidden shrink-0 rounded border border-line px-1.5 py-0.5 text-[10px] text-paper-faint sm:block">esc</kbd>
        </div>
        <div ref={listRef} className="max-h-[52vh] overflow-auto py-1.5">
          {filtered.length === 0 && <div className="px-4 py-8 text-center text-xs text-paper-faint">No matching actions</div>}
          {filtered.map((action, index) => {
            const header = action.group !== group ? action.group : null;
            group = action.group;
            return (
              <div key={action.id}>
                {header && <div className="px-4 pb-1 pt-2.5 text-[10px] font-medium uppercase tracking-wider text-paper-faint">{header}</div>}
                <button
                  data-index={index}
                  disabled={action.disabled}
                  onMouseMove={() => setSelected(index)}
                  onClick={() => !action.disabled && action.run()}
                  className={clsx(
                    "flex w-full items-center gap-3 px-4 py-2 text-left text-sm",
                    action.disabled && "opacity-40",
                    index === selected ? "bg-surface-3 text-paper" : "text-paper-dim hover:bg-surface-2 hover:text-paper",
                  )}
                >
                  <span className="truncate">{action.label}</span>
                  {action.hint && <span className="num ml-auto shrink-0 truncate text-[11px] text-paper-faint">{action.hint}</span>}
                </button>
              </div>
            );
          })}
        </div>
        <div className="flex items-center gap-3 border-t border-line px-4 py-2 text-[10px] text-paper-faint">
          <span><kbd className="rounded border border-line px-1">↑</kbd><kbd className="ml-0.5 rounded border border-line px-1">↓</kbd> move</span>
          <span><kbd className="rounded border border-line px-1">↵</kbd> run</span>
          <span className="ml-auto">{engine.running ? "engine busy — runs queued" : "ready"}</span>
        </div>
      </div>
    </div>
  );
}
