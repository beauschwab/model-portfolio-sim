/** Decide — the strategy tools as one workspace, in the order a desk uses them:
 * sandbox (slide allocations), optimize (robust LP), validate (decision session),
 * with what-if alongside. Each step mounts on first visit and then stays
 * mounted, so a built session or an optimizer result survives switching steps. */
import { useState } from "react";
import { Tabs, tabPanelProps } from "../components/ui";
import StrategySandbox from "./Strategy";
import Optimize from "./Optimizer";
import Validate from "./DecisionLab";
import WhatIf from "./WhatIf";

const STEPS = [
  { id: "Sandbox", Step: StrategySandbox },
  { id: "Optimize", Step: Optimize },
  { id: "Validate", Step: Validate },
  { id: "What-if", Step: WhatIf },
] as const;

export default function Decide() {
  const [active, setActive] = useState<string>("Sandbox");
  const [visited, setVisited] = useState<Set<string>>(() => new Set(["Sandbox"]));
  const choose = (id: string) => {
    setActive(id);
    setVisited(v => (v.has(id) ? v : new Set(v).add(id)));
  };
  return (
    <div className="space-y-3">
      <Tabs id="decide" tabs={STEPS.map(s => s.id)} active={active} onChange={choose} />
      {STEPS.map(({ id, Step }) => (
        <div key={id} {...tabPanelProps("decide", id, active)}>{visited.has(id) && <Step />}</div>
      ))}
    </div>
  );
}
