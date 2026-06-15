import type { SerializedDockview } from "dockview";

const ACTIVE_LAYOUT_KEY = "workspace.layout.v1";
const NAMED_LAYOUTS_KEY = "workspace.layouts.v1";

export interface NamedLayout {
  name: string;
  layout: SerializedDockview;
  updatedAt: string;
}

export function loadActiveLayout(): SerializedDockview | null {
  return readJson<SerializedDockview>(ACTIVE_LAYOUT_KEY);
}

export function saveActiveLayout(layout: SerializedDockview) {
  writeJson(ACTIVE_LAYOUT_KEY, layout);
}

export function clearActiveLayout() {
  try { localStorage.removeItem(ACTIVE_LAYOUT_KEY); } catch { /* ignore */ }
}

export function loadNamedLayouts(): NamedLayout[] {
  return readJson<NamedLayout[]>(NAMED_LAYOUTS_KEY) ?? [];
}

export function saveNamedLayout(name: string, layout: SerializedDockview): NamedLayout[] {
  const trimmed = name.trim();
  if (!trimmed) return loadNamedLayouts();
  const next = [
    { name: trimmed, layout, updatedAt: new Date().toISOString() },
    ...loadNamedLayouts().filter(item => item.name !== trimmed),
  ];
  writeJson(NAMED_LAYOUTS_KEY, next);
  return next;
}

export function deleteNamedLayout(name: string): NamedLayout[] {
  const next = loadNamedLayouts().filter(item => item.name !== name);
  writeJson(NAMED_LAYOUTS_KEY, next);
  return next;
}

function readJson<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) as T : null;
  } catch {
    return null;
  }
}

function writeJson<T>(key: string, value: T) {
  try { localStorage.setItem(key, JSON.stringify(value)); } catch { /* ignore */ }
}
