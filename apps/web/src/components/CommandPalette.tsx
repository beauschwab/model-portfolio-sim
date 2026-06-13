/** ⌘K command palette — speed-of-thought control for the whole desk.
 *
 * A quant should never reach for the mouse to run a job, compose a tile, or
 * switch the active scenario. The palette indexes every such action and
 * filters by subsequence match; ↑/↓ move, Enter runs, Esc closes. It renders
 * in a fixed overlay (outside any tile's overflow/stacking context) and opens
 * on ⌘K / Ctrl-K or the masthead trigger. Reduced-motion safe: the fade is
 * disabled globally by the index.css kill-switch. */
import { useEffect, useMemo, useRef, useState } from "react";
import clsx from "clsx";
import { useNavigate } from "react-router-dom";
import { useEngine } from "../lib/engine";
import { useTiles } from "./Tiles";

interface Action {
  id: string;
  label: string;
  group: string;
  hint?: string;
  run: () => void;
  disabled?: boolean;
}

/** subsequence fuzzy match → score (lower is better), or null if no match */
function fuzzy(q: string, text: string): number | null {
  if (!q) return 0;
  const t = text.toLowerCase();
  let ti = 0, score = 0, streak = 0;
  for (const ch of q.toLowerCase()) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return null;
    score += found - ti + (found > ti ? 2 : 0);
    streak = found === ti ? streak + 1 : 0;
    score -= streak; // reward consecutive hits
    ti = found + 1;
  }
  return score;
}

export function CommandPalette() {
  const engine = useEngine();
  const tiles = useTiles();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen(o => !o);
      } else if (e.key === "Escape") {
        setOpen(false);
      }
    };
    const onOpen = () => setOpen(true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("palette:open", onOpen);
    return () => { window.removeEventListener("keydown", onKey); window.removeEventListener("palette:open", onOpen); };
  }, []);

  useEffect(() => {
    if (open) { setQ(""); setSel(0); requestAnimationFrame(() => inputRef.current?.focus()); }
  }, [open]);

  const close = () => setOpen(false);

  const actions = useMemo<Action[]>(() => {
    const runKinds: [string, string, string][] = [
      ["run-kpis", "Run KPI sheet", "EVE · LCR · NSFR · CET1"],
      ["run-risk", "Run risk (KRD profile)", "fixed-OAS · CRN · all books"],
      ["run-nii", "Run NII forecast", "LMM Monte Carlo · 27m"],
      ["run-stress", "Run 9Q stress", "forward parallel shocks"],
    ];
    const a: Action[] = runKinds.map(([id, label, hint]) => ({
      id, label, hint, group: "Run",
      disabled: engine.running,
      run: () => {
        close();
        const kind = id.replace("run-", "");
        void engine.run(kind, kind === "stress" ? { books: ["mbs", "deposits"] } : undefined);
      },
    }));

    for (const def of tiles.hidden) {
      a.push({
        id: `add-${def.id}`, label: `Add tile · ${def.title}`, hint: def.subtitle,
        group: "Tiles", run: () => { close(); tiles.add(def.id); },
      });
    }
    for (const id of tiles.shown) {
      const def = tiles.defs[id];
      if (!def) continue;
      a.push({
        id: `focus-${id}`, label: `Expand tile · ${def.title}`, group: "Tiles",
        run: () => { close(); tiles.toggleExpand(id); },
      });
    }
    a.push({ id: "reset-layout", label: "Reset desk layout", group: "Tiles", run: () => { close(); tiles.reset(); } });

    for (const name of Object.keys(engine.scenarios)) {
      const sc = engine.scenarios[name];
      const last = sc?.ust10y_bp?.[sc.ust10y_bp.length - 1] ?? 0;
      a.push({
        id: `scn-${name}`, label: `Scenario · ${name}`, hint: `10y ${last >= 0 ? "+" : ""}${last}bp`,
        group: "Scenario", disabled: name === engine.active,
        run: () => { close(); engine.setActive(name); },
      });
    }

    const pages: [string, string][] = [
      ["/risk", "Risk Desk"], ["/kpis", "KPIs"], ["/strategy", "Strategy Lab"],
      ["/optimizer", "Optimizer"], ["/positions", "Positions"], ["/balance-sheet", "Book Editor"],
      ["/market", "Market & Scenarios"], ["/settings", "Assumptions & Settings"],
    ];
    for (const [to, label] of pages) {
      a.push({ id: `nav-${to}`, label: `Open page · ${label}`, hint: to, group: "Navigate", run: () => { close(); navigate(to); } });
    }
    return a;
  }, [engine, tiles, navigate]);

  const filtered = useMemo(() => {
    const scored = actions
      .map(act => ({ act, s: fuzzy(q, `${act.label} ${act.hint ?? ""} ${act.group}`) }))
      .filter(x => x.s !== null) as { act: Action; s: number }[];
    scored.sort((a, b) => a.s - b.s);
    return scored.map(x => x.act);
  }, [actions, q]);

  useEffect(() => { setSel(0); }, [q]);
  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(`[data-idx="${sel}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  if (!open) return null;

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") { e.preventDefault(); setSel(s => Math.min(s + 1, filtered.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setSel(s => Math.max(s - 1, 0)); }
    else if (e.key === "Enter") {
      e.preventDefault();
      const act = filtered[sel];
      if (act && !act.disabled) act.run();
    }
  };

  let lastGroup = "";

  return (
    <div className="fixed inset-0 z-[1000] flex items-start justify-center px-4 pt-[12vh]" role="dialog" aria-modal="true" aria-label="Command palette">
      <button aria-hidden tabIndex={-1} className="absolute inset-0 cursor-default bg-ink/70 backdrop-blur-sm" onClick={close} />
      <div className="palette-pop relative w-full max-w-xl overflow-hidden rounded-xl border border-brand/30 bg-surface-1 shadow-2xl">
        <div className="flex items-center gap-2.5 border-b border-line px-4">
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="#707a8a" strokeWidth="1.5" aria-hidden>
            <circle cx="7" cy="7" r="5" /><path d="M11 11l3 3" strokeLinecap="round" />
          </svg>
          <input
            ref={inputRef}
            value={q}
            onChange={e => setQ(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Run a job, add a tile, switch scenario…"
            className="h-12 w-full bg-transparent text-sm text-paper outline-none placeholder:text-paper-faint"
          />
          <kbd className="hidden shrink-0 rounded border border-line px-1.5 py-0.5 text-[10px] text-paper-faint sm:block">esc</kbd>
        </div>
        <div ref={listRef} className="max-h-[52vh] overflow-auto py-1.5">
          {filtered.length === 0 && (
            <div className="px-4 py-8 text-center text-xs text-paper-faint">No matching actions</div>
          )}
          {filtered.map((act, i) => {
            const header = act.group !== lastGroup ? act.group : null;
            lastGroup = act.group;
            return (
              <div key={act.id}>
                {header && <div className="px-4 pb-1 pt-2.5 text-[10px] font-medium uppercase tracking-wider text-paper-faint">{header}</div>}
                <button
                  data-idx={i}
                  disabled={act.disabled}
                  onMouseMove={() => setSel(i)}
                  onClick={() => !act.disabled && act.run()}
                  className={clsx(
                    "flex w-full items-center gap-3 px-4 py-2 text-left text-sm",
                    act.disabled && "opacity-40",
                    i === sel ? "bg-surface-3 text-paper" : "text-paper-dim",
                  )}
                >
                  <span className="truncate">{act.label}</span>
                  {act.hint && <span className="num ml-auto shrink-0 truncate text-[11px] text-paper-faint">{act.hint}</span>}
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
