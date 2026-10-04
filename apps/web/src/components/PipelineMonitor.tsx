/** PipelineMonitor — live orchestration view of the active run.
 *
 * Renders the engine's structured progress as a stepper-like tree:
 * position build → branch per scenario (book / market / quarter) → rate-path
 * generation → cashflow + OAS calculation. The tree, the four realtime
 * counters (positions, paths, cashflow calcs, OAS solves), and the throughput
 * heartbeat all read from `useEngine()` telemetry, which the backend emits at
 * phase boundaries (numba kernels are opaque); counters tween between polls for
 * a smooth realtime feel. Everything degrades to static under reduced motion. */
import { Check, X } from "lucide-react";
import { useMemo } from "react";
import clsx from "clsx";
import type { NodeKind, NodeStatus, PipelineNode } from "../lib/api";
import { useEngine } from "../lib/engine";
import { Card, CardBody, CardHeader, ChartState, Badge } from "./ui";
import { Heartbeat } from "./Heartbeat";
import { TweenNumber, compact, full, useReducedMotion } from "./motion";

const KIND_GLYPH: Record<NodeKind, string> = {
  build: "▦", branch: "⎇", paths: "∿", cashflow: "Σ", oas: "%", reduce: "↓", solve: "✦",
};
const KIND_LABEL: Record<NodeKind, string> = {
  build: "build", branch: "branch", paths: "paths", cashflow: "cashflow",
  oas: "oas", reduce: "reduce", solve: "solve",
};

/** The headline stat for a node: paths for path nodes, calcs elsewhere. */
function nodeStat(n: PipelineNode): string | null {
  const s = n.stat ?? {};
  if (n.kind === "paths" && s.paths) return `${compact(s.paths)} paths`;
  if (s.calcs) return `${compact(s.calcs)} calcs`;
  if (s.units) return `${full(s.units)} units`;
  if (s.records) return `${full(s.records)} pos`;
  return null;
}

function StatusDot({ status }: { status: NodeStatus }) {
  if (status === "running")
    return <span className="h-2 w-2 shrink-0 rounded-full bg-brand" />;
  if (status === "done")
    return <Check aria-label="Done" className="h-3 w-3 shrink-0 text-up" strokeWidth={2} />;
  if (status === "error")
    return <X aria-label="Failed" className="h-3 w-3 shrink-0 text-danger" strokeWidth={2} />;
  return <span className="h-2.5 w-2.5 shrink-0 rounded-full border border-line" />;
}

function NodeRow({ node, depth }: { node: PipelineNode; depth: number }) {
  const stat = nodeStat(node);
  const dur = node.t0 != null && node.t1 != null ? node.t1 - node.t0 : null;
  const running = node.status === "running";
  return (
    <div
      className={clsx(
        "log-in relative flex items-center gap-2 rounded-md py-1 pr-2",
        running && "bg-brand/5",
      )}
      style={{ paddingLeft: `${0.25 + depth * 1.1}rem` }}
    >
      <StatusDot status={node.status} />
      <span aria-hidden className={clsx(
        "w-4 shrink-0 text-center text-sm",
        running ? "text-brand" : node.status === "done" ? "text-paper-dim" : "text-paper-faint",
      )}>{KIND_GLYPH[node.kind]}</span>
      <span className={clsx(
        "shrink-0 text-sm font-medium",
        node.status === "pending" ? "text-paper-faint" : "text-paper",
      )}>{node.label}</span>
      {node.detail && <span className="min-w-0 truncate text-xs text-paper-faint">· {node.detail}</span>}
      <span className="ml-auto flex shrink-0 items-center gap-2">
        {stat && <span className="num text-xs text-paper-dim">{stat}</span>}
        {dur != null && <span className="num text-2xs text-paper-faint">{dur.toFixed(2)}s</span>}
        {!stat && !dur && <span className="eyebrow">{KIND_LABEL[node.kind]}</span>}
      </span>
    </div>
  );
}

/** Render the flat node list as a tree by parent pointer, depth-first. */
function Tree({ nodes }: { nodes: PipelineNode[] }) {
  const byParent = useMemo(() => {
    const m = new Map<string | null, PipelineNode[]>();
    for (const n of nodes) {
      const k = n.parent ?? null;
      (m.get(k) ?? m.set(k, []).get(k)!).push(n);
    }
    return m;
  }, [nodes]);

  const rows: JSX.Element[] = [];
  const walk = (parent: string | null, depth: number) => {
    for (const n of byParent.get(parent) ?? []) {
      rows.push(<NodeRow key={n.id} node={n} depth={depth} />);
      walk(n.id, depth + 1);
    }
  };
  walk(null, 0);
  return <div className="space-y-0.5">{rows}</div>;
}

function CounterCard({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-md border border-line-strong bg-surface-1 shadow-inset-top p-3">
      <div className="eyebrow">{label}</div>
      <TweenNumber
        value={value}
        format={full}
        className="num mt-0.5 block text-lg font-semibold text-paper"
        title={full(value)}
      />
    </div>
  );
}

export default function PipelineMonitor() {
  const { running, activeKind, stage, pct, elapsed, samples, nodes, stats, plan, log } = useEngine();
  const reduced = useReducedMotion();
  const hasRun = nodes.length > 0 || running;

  return (
    <div className="space-y-3">
      <Card>
        <CardHeader
          title="Orchestration pipeline"
          sub="Position build → scenario fan-out → rate paths → cashflow + OAS"
          right={
            <div className="flex items-center gap-2">
              {activeKind && <Badge tone={running ? "accent" : "neutral"}>{activeKind}</Badge>}
              {running && <Badge tone="up">Live</Badge>}
            </div>
          }
        />
        <CardBody className="space-y-3">
          {/* progress bar */}
          <div>
            <div className="mb-1 flex items-center justify-between text-xs text-paper-faint">
              <span className="truncate text-paper-dim">{stage || (running ? "Starting…" : "idle")}</span>
              <span className="num shrink-0">{pct.toFixed(0)}% · {elapsed.toFixed(1)}s</span>
            </div>
            <div className="relative h-1.5 w-full overflow-hidden rounded-sm bg-surface-2">
              <div className="h-full rounded-sm bg-brand transition-[width] duration-base" style={{ width: `${pct}%` }} />
            </div>
            <div className="mt-1 flex gap-3 text-2xs text-paper-faint">
              {plan.crn_seed != null && <span className="num">seed {plan.crn_seed}</span>}
              {plan.monte_carlo_paths != null && <span className="num">{plan.monte_carlo_paths} MC paths</span>}
              {plan.horizon_months != null && <span className="num">{plan.horizon_months}m horizon</span>}
              {plan.records != null && <span className="num">{full(plan.records)} records</span>}
            </div>
          </div>

          {/* realtime counters */}
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
            <CounterCard label="Positions run" value={stats.records ?? 0} />
            <CounterCard label="Paths generated" value={stats.paths_generated ?? 0} />
            <CounterCard label="Cashflow calcs" value={stats.cashflow_calcs ?? 0} />
            <CounterCard label="OAS solves" value={stats.oas_calcs ?? 0} />
          </div>
        </CardBody>
      </Card>

      <Card>
        <CardHeader title="Pipeline tree" sub="Live parallel execution · status per step" />
        <CardBody>
          {hasRun
            ? <Tree nodes={nodes} />
            : <ChartState kind="empty" hint="Run a sheet to watch the pipeline build." />}
        </CardBody>
      </Card>

      <Card>
        <CardHeader title="Throughput" sub="Path-evaluations / second" />
        <CardBody>
          <Heartbeat samples={samples} running={running} reduced={reduced} variant="panel" />
        </CardBody>
      </Card>

      {log.length > 0 && (
        <Card>
          <CardHeader title="Run log" sub="Streamed engine messages" />
          <CardBody>
            <div className="max-h-44 space-y-0.5 overflow-auto font-mono text-xs leading-relaxed">
              {log.map((l, i) => (
                <div key={i} className="log-in flex gap-2 text-paper-dim">
                  <span className="num shrink-0 text-paper-faint">{l.t.toFixed(2)}s</span>
                  <span className="min-w-0">{l.msg}</span>
                </div>
              ))}
            </div>
          </CardBody>
        </Card>
      )}
    </div>
  );
}
