import { LayoutGrid, Trash2 } from "lucide-react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { DockviewReact, type DockviewApi, type DockviewReadyEvent, type SerializedDockview } from "dockview";
import clsx from "clsx";
import { Button, Popover } from "../components/ui";
import { clearActiveLayout, deleteNamedLayout, loadActiveLayout, loadNamedLayouts, saveActiveLayout, saveNamedLayout, type NamedLayout } from "./layouts";
import { DOCKVIEW_COMPONENTS, PANEL_BY_ID, PANEL_DEFS, RAIL_GROUPS, type PanelId } from "./panels";

interface WorkspaceState {
  api: DockviewApi | null;
  activePanel: PanelId | null;
  namedLayouts: NamedLayout[];
  onReady: (event: DockviewReadyEvent) => void;
  openPanel: (id: PanelId) => void;
  resetLayout: () => void;
  saveLayoutAs: (name: string) => void;
  applyLayout: (layout: SerializedDockview) => void;
  deleteLayout: (name: string) => void;
}

const Ctx = createContext<WorkspaceState | null>(null);

/** Restore a saved layout only if every panel in it still exists. Layouts saved
 * before panels were merged or removed fall back to the default instead of
 * opening blank panels. */
function restoreLayout(dockApi: DockviewApi, layout: SerializedDockview) {
  dockApi.fromJSON(layout);
  if (dockApi.panels.some(panel => !PANEL_BY_ID[panel.id as PanelId])) {
    throw new Error("layout references retired panels");
  }
}

export function useWorkspace() {
  const value = useContext(Ctx);
  if (!value) throw new Error("useWorkspace must be used within WorkspaceProvider");
  return value;
}

export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const [api, setApi] = useState<DockviewApi | null>(null);
  const [activePanel, setActivePanel] = useState<PanelId | null>(null);
  const [namedLayouts, setNamedLayouts] = useState<NamedLayout[]>(() => loadNamedLayouts());
  const suppressSave = useRef(false);

  const addPanel = useCallback((dockApi: DockviewApi, id: PanelId, options?: Partial<Parameters<DockviewApi["addPanel"]>[0]>) => {
    const def = PANEL_BY_ID[id];
    const existing = dockApi.getPanel(id);
    if (existing) {
      existing.api.setActive();
      return existing;
    }
    return dockApi.addPanel({
      id,
      title: def.title,
      component: "workspacePanel",
      params: { subtitle: def.subtitle },
      ...options,
    });
  }, []);

  const buildDefaultLayout = useCallback((dockApi: DockviewApi) => {
    dockApi.clear();
    addPanel(dockApi, "morning");
    addPanel(dockApi, "risk", { position: { referencePanel: "morning", direction: "within" } });
    dockApi.getPanel("morning")?.api.setActive();
  }, [addPanel]);

  const openPanel = useCallback((id: PanelId) => {
    if (!api) return;
    const reference = api.activePanel?.id ?? api.panels[0]?.id;
    addPanel(api, id, reference ? { position: { referencePanel: reference, direction: "within" } } : undefined);
  }, [addPanel, api]);

  const resetLayout = useCallback(() => {
    if (!api) return;
    suppressSave.current = true;
    clearActiveLayout();
    buildDefaultLayout(api);
    saveActiveLayout(api.toJSON());
    suppressSave.current = false;
  }, [api, buildDefaultLayout]);

  const applyLayout = useCallback((layout: SerializedDockview) => {
    if (!api) return;
    suppressSave.current = true;
    try { restoreLayout(api, layout); } catch { buildDefaultLayout(api); }
    saveActiveLayout(api.toJSON());
    suppressSave.current = false;
  }, [api, buildDefaultLayout]);

  const saveLayoutAs = useCallback((name: string) => {
    if (!api) return;
    setNamedLayouts(saveNamedLayout(name, api.toJSON()));
  }, [api]);

  const deleteLayout = useCallback((name: string) => {
    setNamedLayouts(deleteNamedLayout(name));
  }, []);

  const onReady = useCallback((event: DockviewReadyEvent) => {
    const dockApi = event.api;
    setApi(dockApi);

    const saved = loadActiveLayout();
    suppressSave.current = true;
    try {
      if (saved) restoreLayout(dockApi, saved);
      else buildDefaultLayout(dockApi);
    } catch {
      buildDefaultLayout(dockApi);
    }
    saveActiveLayout(dockApi.toJSON());
    suppressSave.current = false;

    const layoutDisposable = dockApi.onDidLayoutChange(() => {
      if (suppressSave.current) return;
      saveActiveLayout(dockApi.toJSON());
    });
    const activeDisposable = dockApi.onDidActivePanelChange(panel => setActivePanel((panel?.id as PanelId | undefined) ?? null));
    setActivePanel((dockApi.activePanel?.id as PanelId | undefined) ?? null);

    return () => {
      layoutDisposable.dispose();
      activeDisposable.dispose();
    };
  }, [buildDefaultLayout]);

  const value = useMemo<WorkspaceState>(() => ({
    api,
    activePanel,
    namedLayouts,
    onReady,
    openPanel,
    resetLayout,
    saveLayoutAs,
    applyLayout,
    deleteLayout,
  }), [api, activePanel, namedLayouts, onReady, openPanel, resetLayout, saveLayoutAs, applyLayout, deleteLayout]);

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function WorkspaceSurface() {
  const { onReady } = useWorkspace();
  return (
    <div className="min-h-0 flex-1 bg-surface">
      <DockviewReact
        className="rates-dockview dockview-theme-rates"
        components={DOCKVIEW_COMPONENTS}
        disableFloatingGroups
        dndStrategy="pointer"
        noPanelsOverlay="watermark"
        getTabContextMenuItems={() => ["close", "closeOthers", "closeAll"]}
        onReady={onReady}
      />
    </div>
  );
}

export function ActivityRail() {
  const { activePanel, openPanel } = useWorkspace();
  return (
    <aside className="flex w-14 shrink-0 flex-col items-center gap-1 border-r border-line bg-[var(--gray-1000)] py-2.5" aria-label="Workspace panels">
      {RAIL_GROUPS.map((group, gi) => (
        <div key={group.id} className="flex w-full flex-col items-center gap-1">
          <div className={clsx("mb-0.5 w-full border-t border-line pt-1.5 text-center text-2xs font-semibold uppercase tracking-normal text-paper-faint", gi === 0 && "border-t-0 pt-0")}>{group.label}</div>
          {PANEL_DEFS.filter(panel => panel.group === group.id).map(panel => (
        <button
          key={panel.id}
          type="button"
          title={`${panel.title} — ${panel.subtitle}`}
          aria-label={`Open ${panel.title}`}
          aria-current={activePanel === panel.id ? "page" : undefined}
          onClick={() => openPanel(panel.id)}
          className={clsx(
            "group relative grid h-[38px] w-[38px] place-items-center rounded-sm transition-colors duration-fast",
            activePanel === panel.id
              ? "bg-brand/15 text-brand-hover"
              : "text-paper-faint hover:bg-surface-2 hover:text-paper",
          )}
        >
          <panel.icon aria-hidden className="h-[18px] w-[18px]" strokeWidth={1.5} />
          {activePanel === panel.id && <span className="absolute -left-[9px] top-[9px] bottom-[9px] w-0.5 bg-brand" />}
        </button>
          ))}
        </div>
      ))}
    </aside>
  );
}

export function LayoutMenu() {
  const { namedLayouts, saveLayoutAs, applyLayout, deleteLayout, resetLayout } = useWorkspace();
  const [name, setName] = useState("");
  return (
    <Popover width="18rem" trigger={
      <span className="inline-flex h-control-sm items-center gap-1.5 rounded-sm border border-line-strong bg-surface-2 px-2.5 text-sm font-medium text-paper-dim transition-colors duration-fast hover:bg-surface-3 hover:text-paper">
        <LayoutGrid aria-hidden className="h-3.5 w-3.5" strokeWidth={1.5} />Layouts
      </span>
    }>
      <div className="space-y-3">
        <div>
          <div className="mb-1 eyebrow">Save current layout</div>
          <div className="flex gap-2">
            <input
              value={name}
              onChange={event => setName(event.target.value)}
              placeholder="ALCO pack"
              className="h-control min-w-0 flex-1 rounded-sm border border-line-strong bg-surface-base px-2 text-sm text-paper outline-none placeholder:text-paper-dim focus:border-brand"
            />
            <Button variant="secondary" onClick={() => { saveLayoutAs(name); setName(""); }}>Save</Button>
          </div>
        </div>
        <div>
          <div className="mb-1 eyebrow">Named layouts</div>
          <div className="max-h-44 space-y-1 overflow-auto">
            {namedLayouts.length === 0 && <div className="rounded-md bg-surface-2 px-2 py-2 text-sm text-paper-faint">No saved layouts yet.</div>}
            {namedLayouts.map(item => (
              <div key={item.name} className="flex items-center gap-1 rounded-md bg-surface-2 p-1">
                <button type="button" onClick={() => applyLayout(item.layout)} className="min-w-0 flex-1 truncate px-2 py-1 text-left text-sm text-paper-dim hover:text-brand">
                  {item.name}
                </button>
                <button type="button" onClick={() => deleteLayout(item.name)} className="grid h-6 w-6 place-items-center rounded-sm text-paper-faint hover:bg-surface-3 hover:text-danger" aria-label={`Delete ${item.name}`}><Trash2 aria-hidden className="h-3.5 w-3.5" strokeWidth={1.5} /></button>
              </div>
            ))}
          </div>
        </div>
        <button type="button" onClick={resetLayout} className="w-full rounded-sm border border-line-strong bg-surface-2 px-2 py-1.5 text-sm font-medium text-paper-dim hover:bg-surface-3 hover:text-brand">
          Reset to default desk
        </button>
      </div>
    </Popover>
  );
}

