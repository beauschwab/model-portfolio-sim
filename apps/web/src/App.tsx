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
          <div className="flex shrink-0 justify-end border-b border-line bg-surface-base px-3.5 py-1">
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
