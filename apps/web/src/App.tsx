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
          <div className="flex shrink-0 items-center justify-between border-b border-line bg-surface-base px-3.5 py-1">
            <div className="flex min-w-0 items-center gap-2 text-xs text-paper-faint">
              <span className="eyebrow">Workspace</span>
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
