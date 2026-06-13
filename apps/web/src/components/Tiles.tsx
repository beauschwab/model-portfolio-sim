/** Composable tile system for the Workbench.
 *
 * A tile is a self-contained working surface (risk, KPIs, positions, the
 * optimizer…). The grid lets a quant compose their desk: drag to reorder,
 * cycle a tile's width, expand one to fill the surface, and add/remove tiles
 * from the command palette. Layout (order, size, which tiles are shown)
 * persists to localStorage so the desk you arrange is the desk you return to.
 *
 * Reorders and size changes morph via the View Transitions API (each tile
 * carries a stable view-transition-name), degrading to instant under reduced
 * motion or unsupported browsers. */
import {
  createContext, useCallback, useContext, useEffect, useMemo, useRef, useState,
  type ReactNode,
} from "react";
import clsx from "clsx";
import { startViewTransition } from "./motion";

export type TileSize = "sm" | "md" | "lg" | "full";
const SIZES: TileSize[] = ["sm", "md", "lg", "full"];
const COLSPAN: Record<TileSize, string> = {
  sm: "md:col-span-4",
  md: "md:col-span-6",
  lg: "md:col-span-8",
  full: "md:col-span-12",
};

export interface TileDef {
  id: string;
  title: string;
  subtitle?: string;
  badge?: ReactNode;
  defaultSize: TileSize;
  defaultShown?: boolean;
  render: () => ReactNode;
}

interface Layout { order: string[]; size: Record<string, TileSize>; hidden: string[] }

interface TilesState {
  defs: Record<string, TileDef>;
  shown: string[];
  hidden: TileDef[];
  size: Record<string, TileSize>;
  expanded: string | null;
  add: (id: string) => void;
  remove: (id: string) => void;
  cycleSize: (id: string) => void;
  setSize: (id: string, s: TileSize) => void;
  toggleExpand: (id: string) => void;
  reorder: (dragId: string, overId: string) => void;
  reset: () => void;
}

const Ctx = createContext<TilesState | null>(null);
const STORE_KEY = "workbench.layout.v2";

function loadLayout(): Layout | null {
  try {
    const raw = localStorage.getItem(STORE_KEY);
    return raw ? (JSON.parse(raw) as Layout) : null;
  } catch { return null; }
}

export function TilesProvider({ tiles, children }: { tiles: TileDef[]; children: ReactNode }) {
  const defs = useMemo(() => Object.fromEntries(tiles.map(t => [t.id, t])), [tiles]);

  const initial = useMemo<Layout>(() => {
    const stored = loadLayout();
    const known = new Set(tiles.map(t => t.id));
    const defaultShownOrder = tiles.filter(t => t.defaultShown !== false).map(t => t.id);
    const defaultHidden = tiles.filter(t => t.defaultShown === false).map(t => t.id);
    const baseSize = Object.fromEntries(tiles.map(t => [t.id, t.defaultSize]));
    if (!stored) return { order: defaultShownOrder, size: baseSize, hidden: defaultHidden };
    // merge: keep stored order/size/hidden for known tiles, fold in any new tiles
    const order = stored.order.filter(id => known.has(id));
    const hidden = stored.hidden.filter(id => known.has(id));
    const placed = new Set([...order, ...hidden]);
    for (const t of tiles) {
      if (placed.has(t.id)) continue;
      (t.defaultShown === false ? hidden : order).push(t.id);
    }
    return { order, size: { ...baseSize, ...stored.size }, hidden };
  }, [tiles, defs]);

  const [order, setOrder] = useState<string[]>(initial.order);
  const [hidden, setHidden] = useState<string[]>(initial.hidden);
  const [size, setSizeMap] = useState<Record<string, TileSize>>(initial.size);
  const [expanded, setExpanded] = useState<string | null>(null);

  useEffect(() => {
    try { localStorage.setItem(STORE_KEY, JSON.stringify({ order, size, hidden })); } catch { /* ignore */ }
  }, [order, size, hidden]);

  const add = useCallback((id: string) => startViewTransition(() => {
    setHidden(h => h.filter(x => x !== id));
    setOrder(o => (o.includes(id) ? o : [...o, id]));
  }), []);

  const remove = useCallback((id: string) => startViewTransition(() => {
    setExpanded(e => (e === id ? null : e));
    setOrder(o => o.filter(x => x !== id));
    setHidden(h => (h.includes(id) ? h : [id, ...h]));
  }), []);

  const setSize = useCallback((id: string, s: TileSize) => startViewTransition(() => {
    setSizeMap(m => ({ ...m, [id]: s }));
  }), []);

  const cycleSize = useCallback((id: string) => startViewTransition(() => {
    setSizeMap(m => {
      const cur = m[id] ?? "md";
      const next = SIZES[(SIZES.indexOf(cur) + 1) % SIZES.length];
      return { ...m, [id]: next };
    });
  }), []);

  const toggleExpand = useCallback((id: string) => startViewTransition(() => {
    setExpanded(e => (e === id ? null : id));
  }), []);

  const reorder = useCallback((dragId: string, overId: string) => {
    if (dragId === overId) return;
    startViewTransition(() => {
      setOrder(o => {
        const from = o.indexOf(dragId), to = o.indexOf(overId);
        if (from < 0 || to < 0) return o;
        const next = [...o];
        next.splice(from, 1);
        next.splice(to, 0, dragId);
        return next;
      });
    });
  }, []);

  const reset = useCallback(() => startViewTransition(() => {
    setExpanded(null);
    setOrder(tiles.filter(t => t.defaultShown !== false).map(t => t.id));
    setHidden(tiles.filter(t => t.defaultShown === false).map(t => t.id));
    setSizeMap(Object.fromEntries(tiles.map(t => [t.id, t.defaultSize])));
  }), [tiles]);

  const value = useMemo<TilesState>(() => ({
    defs,
    shown: order,
    hidden: hidden.map(id => defs[id]).filter(Boolean),
    size, expanded,
    add, remove, cycleSize, setSize, toggleExpand, reorder, reset,
  }), [defs, order, hidden, size, expanded, add, remove, cycleSize, setSize, toggleExpand, reorder, reset]);

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useTiles() {
  const c = useContext(Ctx);
  if (!c) throw new Error("useTiles must be used within TilesProvider");
  return c;
}

/* ── the grid ─────────────────────────────────────────────────────────── */
export function TileGrid() {
  const { defs, shown, size, expanded, cycleSize, remove, toggleExpand, reorder } = useTiles();
  const dragId = useRef<string | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);

  // When a tile is expanded, it alone fills the surface.
  const visible = expanded ? [expanded] : shown;

  return (
    <div className="grid grid-cols-1 gap-3 md:grid-cols-12">
      {visible.map(id => {
        const def = defs[id];
        if (!def) return null;
        const tileSize: TileSize = expanded ? "full" : (size[id] ?? def.defaultSize);
        return (
          <section
            key={id}
            style={{ viewTransitionName: `tile-${id}` }}
            onDragOver={e => { if (dragId.current && dragId.current !== id) { e.preventDefault(); setOver(id); } }}
            onDrop={e => {
              e.preventDefault();
              if (dragId.current && dragId.current !== id) reorder(dragId.current, id);
              dragId.current = null; setDragging(null); setOver(null);
            }}
            className={clsx(
              "group/tile flex min-w-0 flex-col overflow-hidden rounded-xl border bg-surface-1 shadow-sm transition-colors",
              COLSPAN[tileSize],
              over === id && dragging !== id ? "border-brand/60" : "border-line",
              dragging === id && "opacity-60",
            )}
          >
            <header
              draggable={!expanded}
              onDragStart={e => {
                if (expanded) return;
                dragId.current = id; setDragging(id);
                e.dataTransfer.effectAllowed = "move";
                try { e.dataTransfer.setData("text/plain", id); } catch { /* some browsers */ }
              }}
              onDragEnd={() => { dragId.current = null; setDragging(null); setOver(null); }}
              className={clsx(
                "flex shrink-0 items-center gap-2 border-b border-line px-3.5 py-2.5",
                !expanded && "cursor-grab active:cursor-grabbing",
              )}
            >
              {!expanded && (
                <span aria-hidden className="text-paper-faint/60 transition-colors group-hover/tile:text-paper-faint" title="drag to reorder">
                  <svg width="10" height="14" viewBox="0 0 10 14" fill="currentColor">
                    <circle cx="2" cy="2" r="1.2" /><circle cx="8" cy="2" r="1.2" />
                    <circle cx="2" cy="7" r="1.2" /><circle cx="8" cy="7" r="1.2" />
                    <circle cx="2" cy="12" r="1.2" /><circle cx="8" cy="12" r="1.2" />
                  </svg>
                </span>
              )}
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <h2 className="truncate text-sm font-medium text-paper">{def.title}</h2>
                  {def.badge}
                </div>
                {def.subtitle && <p className="truncate text-[11px] text-paper-faint">{def.subtitle}</p>}
              </div>
              <div className="ml-auto flex shrink-0 items-center gap-0.5">
                {!expanded && (
                  <TileBtn label={`width: ${tileSize}`} onClick={() => cycleSize(id)}>
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
                      <path d="M2 5h12M2 11h12" strokeLinecap="round" />
                      <path d="M5 8H3M13 8h-2" strokeLinecap="round" />
                    </svg>
                  </TileBtn>
                )}
                <TileBtn label={expanded ? "restore" : "expand"} onClick={() => toggleExpand(id)}>
                  {expanded ? (
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
                      <path d="M9 7l4-4M13 3v3M13 3h-3M7 9l-4 4M3 13v-3M3 13h3" />
                    </svg>
                  ) : (
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
                      <path d="M3 7V3h4M13 9v4H9M3 3l4 4M13 13l-4-4" />
                    </svg>
                  )}
                </TileBtn>
                {!expanded && (
                  <TileBtn label="hide tile" onClick={() => remove(id)}>
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
                      <path d="M4 4l8 8M12 4l-8 8" />
                    </svg>
                  </TileBtn>
                )}
              </div>
            </header>
            <div className={clsx("min-h-0 flex-1 overflow-auto p-3", expanded && "min-h-[70vh]")}>
              {def.render()}
            </div>
          </section>
        );
      })}
    </div>
  );
}

function TileBtn({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      className="inline-flex h-6 w-6 items-center justify-center rounded text-paper-faint transition-colors hover:bg-surface-3 hover:text-paper focus-visible:bg-surface-3"
    >
      {children}
    </button>
  );
}
