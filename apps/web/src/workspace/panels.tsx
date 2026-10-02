import { lazy, Suspense, Component, type ReactNode } from "react";
import type { IDockviewPanelProps } from "dockview";
import { Badge } from "../components/ui";
const PipelineMonitor = lazy(() => import("../components/PipelineMonitor"));
const BalanceSheet = lazy(() => import("../pages/BalanceSheet"));
const Dashboard = lazy(() => import("../pages/Dashboard"));
const KpisPage = lazy(() => import("../pages/Kpis"));
const Treasury = lazy(() => import("../pages/Treasury"));
const MarketPage = lazy(() => import("../pages/Market"));
const MorningSheet = lazy(() => import("../pages/MorningSheet"));
const OptimizerPage = lazy(() => import("../pages/Optimizer"));
const Positions = lazy(() => import("../pages/Positions"));
const Cohorts = lazy(() => import("../pages/Cohorts"));
const WhatIf = lazy(() => import("../pages/WhatIf"));
const DecisionLab = lazy(() => import("../pages/DecisionLab"));
const BalanceStress = lazy(() => import("../pages/BalanceStress"));
const SettingsPage = lazy(() => import("../pages/Settings"));
const StrategyPage = lazy(() => import("../pages/Strategy"));

export type PanelId =
  | "morning"
  | "pipeline"
  | "risk"
  | "kpis"
  | "treasury"
  | "positions"
  | "cohorts"
  | "whatif"
  | "decision"
  | "balance-stress"
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
  { id: "treasury", title: "Capital & FTP", railLabel: "CF", subtitle: "Capital coverage · internal funding · profitability", component: () => <Treasury /> },
  { id: "positions", title: "Positions", railLabel: "P", subtitle: "side → book → position · indicative client-side derivations", component: () => <Positions /> },
  { id: "cohorts", title: "Tape & Cohorts", railLabel: "TC", subtitle: "loan tapes · behavioral cohorts · lineage · loan analytics", component: () => <Cohorts /> },
  { id: "whatif", title: "Instrument What-if", railLabel: "WI", subtitle: "temporary assumptions · held calibration · incremental risk and earnings", component: () => <WhatIf /> },
  { id: "decision", title: "Decision Lab", railLabel: "DL", subtitle: "assumptions → selective rebuild → optimization → validated allocation", component: () => <DecisionLab /> },
  { id: "balance-stress", title: "Balance-sheet Stress", railLabel: "BS", subtitle: "liquidity · credit · capital · entity constraints · policies", component: () => <BalanceStress /> },
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

class PanelErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    return this.state.failed ? <div role="alert" className="p-4 text-xs">This panel could not load. <button onClick={() => window.location.reload()}>Reload workspace</button></div> : this.props.children;
  }
}

function WorkspacePanel(props: IDockviewPanelProps) {
  const id = props.api.id as PanelId;
  const def = PANEL_BY_ID[id];
  if (!def) {
    return <div className="p-4 text-xs text-paper-faint">Unknown panel: {props.api.id}</div>;
  }
  return (
    <div role="region" aria-label={def.title} className="h-full min-h-0 overflow-auto bg-surface p-3">
      <PanelErrorBoundary><Suspense fallback={<div role="status" className="p-4 text-xs text-paper-faint">Loading panel…</div>}>{def.component()}</Suspense></PanelErrorBoundary>
    </div>
  );
}

export const DOCKVIEW_COMPONENTS = {
  workspacePanel: WorkspacePanel,
};
