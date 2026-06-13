import { NavLink, Route, Routes } from "react-router-dom";
import type { ReactNode } from "react";
import BalanceSheet from "./pages/BalanceSheet";
import Positions from "./pages/Positions";
import Dashboard from "./pages/Dashboard";
import MorningSheet from "./pages/MorningSheet";
import MarketPage from "./pages/Market";
import SettingsPage from "./pages/Settings";
import KpisPage from "./pages/Kpis";
import StrategyPage from "./pages/Strategy";
import OptimizerPage from "./pages/Optimizer";
import Workbench from "./pages/Workbench";

const NAV = [
  ["/risk", "Risk Desk"],
  ["/kpis", "KPIs"],
  ["/strategy", "Strategy Lab"],
  ["/optimizer", "Optimizer"],
  ["/positions", "Positions"],
  ["/balance-sheet", "Book Editor"],
  ["/market", "Market & Scenarios"],
  ["/settings", "Assumptions & Settings"],
  ["/morning", "Morning Sheet"],
] as const;

/** Standalone deep-link layout: the classic sidebar shell, kept so any tile
 * can be popped out to its own route. The home route (/) is the composable
 * Workbench, which is its own full-bleed shell. */
function PageShell({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-screen">
      <aside className="w-56 shrink-0 border-r border-line bg-surface-1 p-4">
        <NavLink to="/" className="mb-6 flex items-center gap-2">
          <div className="h-6 w-6 rounded-sm border border-brand text-center font-display text-sm leading-6 text-brand">R</div>
          <div className="font-display text-sm font-medium tracking-tight text-paper">Rates Workbench</div>
        </NavLink>
        <NavLink to="/" className="mb-3 block rounded-md border border-line bg-surface-2 px-3 py-1.5 text-xs font-medium text-brand hover:bg-surface-3">
          ← Workbench
        </NavLink>
        <nav className="space-y-1">
          {NAV.map(([to, label]) => (
            <NavLink key={to} to={to}
              className={({ isActive }) =>
                `block rounded-md px-3 py-1.5 text-xs font-medium ${isActive ? "bg-surface-3 text-brand" : "text-paper-dim hover:bg-surface-2 hover:text-paper"}`}>
              {label}
            </NavLink>
          ))}
        </nav>
        <div className="mt-8 border-t border-line pt-4 text-[10px] leading-relaxed text-paper-faint">
          mbs-risk v0.9 · LMM Monte Carlo · fixed-OAS / CRN · synthetic WFC-1Q26-proportional book
        </div>
      </aside>
      <main className="flex-1 overflow-auto p-4">{children}</main>
    </div>
  );
}

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Workbench />} />
      <Route path="/risk" element={<PageShell><Dashboard /></PageShell>} />
      <Route path="/kpis" element={<PageShell><KpisPage /></PageShell>} />
      <Route path="/strategy" element={<PageShell><StrategyPage /></PageShell>} />
      <Route path="/optimizer" element={<PageShell><OptimizerPage /></PageShell>} />
      <Route path="/positions" element={<PageShell><Positions /></PageShell>} />
      <Route path="/balance-sheet" element={<PageShell><BalanceSheet /></PageShell>} />
      <Route path="/market" element={<PageShell><MarketPage /></PageShell>} />
      <Route path="/settings" element={<PageShell><SettingsPage /></PageShell>} />
      <Route path="/morning" element={<PageShell><MorningSheet /></PageShell>} />
    </Routes>
  );
}
