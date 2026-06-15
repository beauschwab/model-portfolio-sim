import { Masthead } from "./components/Masthead";
import { EngineProvider } from "./lib/engine";
import { ActivityRail, LayoutMenu, WorkspaceProvider, WorkspaceSurface } from "./workspace/Workspace";
import { WorkspaceCommandPalette } from "./workspace/WorkspaceCommandPalette";

export default function App() {
  return (
    <EngineProvider>
      <WorkspaceProvider>
        <div className="flex h-screen min-h-screen flex-col overflow-hidden bg-surface text-paper">
          <Masthead />
          <div className="flex shrink-0 items-center justify-between border-b border-line bg-surface-1/80 px-3 py-1.5">
            <div className="flex min-w-0 items-center gap-2 text-[11px] text-paper-faint">
              <span className="font-medium text-paper-dim">Dock workspace</span>
              <span className="hidden sm:inline">Drag tabs to split left/right/top/bottom, or drop into a tab group.</span>
            </div>
            <LayoutMenu />
          </div>
          <div className="flex min-h-0 flex-1">
            <ActivityRail />
            <WorkspaceSurface />
          </div>
          <WorkspaceCommandPalette />
        </div>
      </WorkspaceProvider>
    </EngineProvider>
  );
}
