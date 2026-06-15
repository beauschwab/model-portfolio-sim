import type { ReactNode } from "react";
import type { IDockviewPanelProps } from "dockview";
import { Badge } from "../components/ui";
import PipelineMonitor from "../components/PipelineMonitor";
import BalanceSheet from "../pages/BalanceSheet";
import Dashboard from "../pages/Dashboard";
import KpisPage from "../pages/Kpis";
import MarketPage from "../pages/Market";
import MorningSheet from "../pages/MorningSheet";
import OptimizerPage from "../pages/Optimizer";
import Positions from "../pages/Positions";
import SettingsPage from "../pages/Settings";
import StrategyPage from "../pages/Strategy";

export type PanelId =
  | "morning"
  | "pipeline"
  | "risk"
  | "kpis"
  | "positions"
  | "market"
  | "strategy"
  | "optimizer"
  | "books"
  | "settings";

export interface WorkspacePanelDef {
  id: PanelId;
  title: string;
  railLabel: string;
  subtitle: string;
  badge?: ReactNode;
  component: () => ReactNode;
}

export const PANEL_DEFS: WorkspacePanelDef[] = [
  { id: "morning", title: "Morning Sheet", railLabel: "AM", subtitle: "ALCO-ready summary · constraints · run notes", component: () => <MorningSheet /> },
  { id: "pipeline", title: "Pipeline", railLabel: "PL", subtitle: "live orchestration · scenario fan-out · path & calc telemetry", component: () => <PipelineMonitor /> },
  { id: "risk", title: "Risk Desk", railLabel: "RD", subtitle: "KRD profile · NII forecast · 9Q stress P&L", component: () => <Dashboard /> },
  { id: "kpis", title: "KPIs", railLabel: "K", subtitle: "EVE · LCR · NSFR · CET1", component: () => <KpisPage /> },
  { id: "positions", title: "Positions", railLabel: "P", subtitle: "side → book → position · indicative client-side derivations", component: () => <Positions /> },
  { id: "market", title: "Market & Scenarios", railLabel: "MK", subtitle: "par curve · 9Q scenario builder", component: () => <MarketPage /> },
  { id: "strategy", title: "Strategy Lab", railLabel: "SL", subtitle: "live allocation sandbox · sub-ms KPI recalc", component: () => <StrategyPage /> },
  { id: "optimizer", title: "Optimizer", railLabel: "O", subtitle: "robust balance-sheet LP · shadow prices", component: () => <OptimizerPage /> },
  { id: "books", title: "Book Editor", railLabel: "BE", subtitle: "6 books · table view + JSON edit", component: () => <BalanceSheet /> },
  {
    id: "settings",
    title: "Assumptions & Settings",
    railLabel: "AS",
    subtitle: "deposit attrition · prepay vector · run config",
    badge: <Badge tone="amber">prepay restart</Badge>,
    component: () => <SettingsPage />,
  },
];

export const PANEL_BY_ID = Object.fromEntries(PANEL_DEFS.map(panel => [panel.id, panel])) as Record<PanelId, WorkspacePanelDef>;

function WorkspacePanel(props: IDockviewPanelProps) {
  const id = props.api.id as PanelId;
  const def = PANEL_BY_ID[id];
  if (!def) {
    return <div className="p-4 text-xs text-paper-faint">Unknown panel: {props.api.id}</div>;
  }
  return (
    <div className="h-full min-h-0 overflow-auto bg-surface p-3">
      {def.component()}
    </div>
  );
}

export const DOCKVIEW_COMPONENTS = {
  workspacePanel: WorkspacePanel,
};
