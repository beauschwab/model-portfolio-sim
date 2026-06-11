import { NavLink, Route, Routes } from "react-router-dom";
import BalanceSheet from "./pages/BalanceSheet";
import Dashboard from "./pages/Dashboard";
import MarketPage from "./pages/Market";
import SettingsPage from "./pages/Settings";
import KpisPage from "./pages/Kpis";
import StrategyPage from "./pages/Strategy";

const NAV = [
  ["/", "Dashboard"],
  ["/kpis", "KPIs"],
  ["/strategy", "Strategy Lab"],
  ["/balance-sheet", "Balance Sheet"],
  ["/market", "Market & Scenarios"],
  ["/settings", "Assumptions & Settings"],
] as const;

export default function App() {
  return (
    <div className="flex min-h-screen">
      <aside className="w-56 shrink-0 border-r border-line bg-surface-1 p-4">
        <div className="mb-6 flex items-center gap-2">
          <div className="h-6 w-6 rounded bg-brand-deep text-center text-sm font-bold leading-6 text-brand">R</div>
          <div className="text-sm font-semibold tracking-tight">Rates Workbench</div>
        </div>
        <nav className="space-y-1">
          {NAV.map(([to, label]) => (
            <NavLink key={to} to={to} end={to === "/"}
              className={({ isActive }) =>
                `block rounded-md px-3 py-1.5 text-xs font-medium ${isActive ? "bg-surface-3 text-brand" : "text-zinc-400 hover:bg-surface-2 hover:text-zinc-200"}`}>
              {label}
            </NavLink>
          ))}
        </nav>
        <div className="mt-8 border-t border-line pt-4 text-[10px] leading-relaxed text-zinc-600">
          mbs-risk v0.9 · LMM Monte Carlo · fixed-OAS / CRN · synthetic WFC-1Q26-proportional book
        </div>
      </aside>
      <main className="flex-1 overflow-auto p-6">
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/kpis" element={<KpisPage />} />
          <Route path="/strategy" element={<StrategyPage />} />
          <Route path="/balance-sheet" element={<BalanceSheet />} />
          <Route path="/market" element={<MarketPage />} />
          <Route path="/settings" element={<SettingsPage />} />
        </Routes>
      </main>
    </div>
  );
}
