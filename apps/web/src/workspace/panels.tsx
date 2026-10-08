import { lazy, Suspense, Component, type ReactNode } from "react";
import type { IDockviewPanelProps } from "dockview";
import { Activity, BookOpen, ChartLine, Compass, Gauge, Landmark, Layers, Settings2, ShieldAlert, Sunrise, Table2, Workflow, type LucideIcon } from "lucide-react";
import { Badge } from "../components/ui";
const PipelineMonitor = lazy(() => import("../components/PipelineMonitor"));
const BalanceSheet = lazy(() => import("../pages/BalanceSheet"));
const Dashboard = lazy(() => import("../pages/Dashboard"));
const KpisPage = lazy(() => import("../pages/Kpis"));
const Treasury = lazy(() => import("../pages/Treasury"));
const MarketPage = lazy(() => import("../pages/Market"));
const MorningSheet = lazy(() => import("../pages/MorningSheet"));
const Positions = lazy(() => import("../pages/Positions"));
const Cohorts = lazy(() => import("../pages/Cohorts"));
const DecideWorkspace = lazy(() => import("../pages/Decide"));
const StressWorkspace = lazy(() => import("../pages/Stress"));
const SettingsPage = lazy(() => import("../pages/Settings"));

export type PanelId =
  | "morning"
  | "pipeline"
  | "risk"
  | "kpis"
  | "treasury"
  | "positions"
  | "cohorts"
  | "decide"
  | "stress"
  | "market"
  | "books"
  | "settings";

/** Rail sections, in the order a desk uses them. `system` panels open from
 * the top bar's run status, not from the rail. */
export type PanelGroup = "home" | "monitor" | "decide" | "data" | "system";
export const RAIL_GROUPS: { id: PanelGroup; label: string }[] = [
  { id: "home", label: "Home" },
  { id: "monitor", label: "Monitor" },
  { id: "decide", label: "Decide" },
  { id: "data", label: "Data" },
];

export interface WorkspacePanelDef {
  id: PanelId;
  group: PanelGroup;
  title: string;
  /** Lucide icon for the activity rail. */
  icon: LucideIcon;
  subtitle: string;
  badge?: ReactNode;
  component: () => ReactNode;
}

export const PANEL_DEFS: WorkspacePanelDef[] = [
  { id: "morning", group: "home", title: "Morning Sheet", icon: Sunrise, subtitle: "ALCO-ready summary · constraints · run notes", component: () => <MorningSheet /> },
  { id: "pipeline", group: "system", title: "Pipeline", icon: Workflow, subtitle: "Live orchestration · scenario fan-out · path & calc telemetry", component: () => <PipelineMonitor /> },
  { id: "stress", group: "monitor", title: "Stress", icon: ShieldAlert, subtitle: "9Q rate shocks · balance-sheet liquidity, credit and capital", component: () => <StressWorkspace /> },
  { id: "risk", group: "monitor", title: "Risk Desk", icon: Activity, subtitle: "KRD profile · NII forecast · 9Q stress P&L", component: () => <Dashboard /> },
  { id: "kpis", group: "monitor", title: "KPIs", icon: Gauge, subtitle: "EVE · LCR · NSFR · CET1", component: () => <KpisPage /> },
  { id: "treasury", group: "monitor", title: "Capital & FTP", icon: Landmark, subtitle: "Capital coverage · internal funding · profitability", component: () => <Treasury /> },
  { id: "decide", group: "decide", title: "Decide", icon: Compass, subtitle: "Sandbox → optimize → validate · what-if", component: () => <DecideWorkspace /> },
  { id: "positions", group: "data", title: "Positions", icon: Table2, subtitle: "Side → book → position · indicative client-side derivations", component: () => <Positions /> },
  { id: "cohorts", group: "data", title: "Tape & Cohorts", icon: Layers, subtitle: "Loan tapes · behavioral cohorts · lineage · loan analytics", component: () => <Cohorts /> },
  { id: "market", group: "data", title: "Market & Scenarios", icon: ChartLine, subtitle: "Par curve · 9Q scenario builder", component: () => <MarketPage /> },
  { id: "books", group: "data", title: "Book Editor", icon: BookOpen, subtitle: "6 books · table view + JSON edit", component: () => <BalanceSheet /> },
  {
    id: "settings",
    group: "data",
    title: "Assumptions & Settings",
    icon: Settings2,
    subtitle: "Deposit attrition · prepay vector · run config",
    badge: <Badge tone="warning">Prepay restart</Badge>,
    component: () => <SettingsPage />,
  },
];

export const PANEL_BY_ID = Object.fromEntries(PANEL_DEFS.map(panel => [panel.id, panel])) as Record<PanelId, WorkspacePanelDef>;

class PanelErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    return this.state.failed ? <div role="alert" className="p-4 text-sm">This panel could not load. <button onClick={() => window.location.reload()}>Reload workspace</button></div> : this.props.children;
  }
}

function WorkspacePanel(props: IDockviewPanelProps) {
  const id = props.api.id as PanelId;
  const def = PANEL_BY_ID[id];
  if (!def) {
    return <div className="p-4 text-sm text-paper-faint">Unknown panel: {props.api.id}</div>;
  }
  return (
    <div role="region" aria-label={def.title} className="h-full min-h-0 overflow-auto bg-surface p-3">
      <PanelErrorBoundary><Suspense fallback={<div role="status" className="p-4 text-sm text-paper-faint">Loading panel…</div>}>{def.component()}</Suspense></PanelErrorBoundary>
    </div>
  );
}

export const DOCKVIEW_COMPONENTS = {
  workspacePanel: WorkspacePanel,
};
