/** Standard EVE/NII rate shock report (`rate-shock-1`): one row per scenario,
 * limit status per metric, and the policy that defines both. */
import { useEffect, useMemo, useRef, useState } from "react";
import { api, awaitJob, errorText, fmt$, rowsOf, type RateShockPolicy, type RateShockResult, type Row } from "../lib/api";
import { useEngineData } from "../lib/engine";
import { Badge, Button, Card, CardBody, CardHeader, InfoPop, Spinner, type Tone } from "../components/ui";

const METRICS = [["eve", "EVE"], ["nii_12m", "NII 12m"], ["nii_24m", "NII 24m"]] as const;
const STATUS: Record<string, { tone: Tone; label: string }> = {
  within_limit: { tone: "up", label: "Within limit" },
  breach: { tone: "danger", label: "Breach" },
  unavailable: { tone: "warning", label: "Unavailable" },
  not_applicable: { tone: "neutral", label: "Not applicable" },
  unconfigured: { tone: "neutral", label: "No limit set" },
};
const pct = (v: unknown) => typeof v === "number" ? `${v >= 0 ? "+" : ""}${v.toFixed(2)}%` : "—";
const money = (v: unknown) => typeof v === "number" ? fmt$(v) : "—";
const title = (name: string) => name.replaceAll("_", " ").replace(/(\d+)p(\d+)/, "$1.$2").replace(/ (\d[\d.]*)$/, " $1 bp");

function download(rows: Row[], columns: string[]) {
  const csv = [columns, ...rows.map(r => columns.map(c => r[c] ?? ""))]
    .map(row => row.map(v => {
      const cell = typeof v === "string" && /^[=+@-]/.test(v) ? `'${v}` : String(v);
      return `"${cell.replaceAll('"', '""')}"`;
    }).join(",")).join("\r\n");
  const url = URL.createObjectURL(new Blob([csv], { type: "text/csv;charset=utf-8" }));
  const link = document.createElement("a"); link.href = url; link.download = "rate-shocks.csv"; link.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

export default function RateShocks() {
  const { revision } = useEngineData();
  const [policy, setPolicy] = useState<RateShockPolicy | null>(null);
  const [draft, setDraft] = useState("");
  const [result, setResult] = useState<{ data: RateShockResult; revision: number } | null>(null);
  const [stage, setStage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  // The saved policy the draft was seeded from and the workspace revision it was read at. A refresh
  // replaces the draft only while it has no unsaved edits (a cleared draft counts as an edit). Under
  // unsaved edits the base advances only if the saved policy is unchanged, so saving a draft written
  // against a policy that changed elsewhere is a 409 conflict, never a silent overwrite.
  const base = useRef<{ text: string; revision: number } | null>(null);
  const latest = useRef(draft);
  latest.current = draft;

  useEffect(() => {
    let live = true;
    api.rateShockPolicy().then(p => {
      if (!live) return;
      const text = JSON.stringify(p.policy, null, 2);
      const prev = base.current;
      setPolicy(p.policy);
      if (prev === null || latest.current === prev.text) {
        setDraft(text);
        base.current = { text, revision: p.revision };
      } else if (text === prev.text) {
        base.current = { text, revision: p.revision };
      }
    }).catch(e => live && setError(errorText(e)));
    return () => { live = false; };
  }, [revision]);

  async function run() {
    setError(null); setStage("Queued");
    try {
      const job = await awaitJob((await api.runRateShocks(revision)).id, s => setStage(s.progress?.stage ?? s.status));
      if (job.status === "error") throw new Error(job.detail ?? "The rate shock report did not compute.");
      setResult({ data: job.result as RateShockResult, revision });
    } catch (e) { setError(errorText(e)); } finally { setStage(null); }
  }

  async function save() {
    setError(null); setSaved(null);
    try {
      const res = await api.putRateShockPolicy(JSON.parse(draft) as RateShockPolicy, base.current?.revision ?? revision);
      base.current = { text: draft, revision: res.revision };
      setSaved("Policy saved. Run the report again to apply it.");
    } catch (e) { setError(errorText(e)); }
  }

  const rows = useMemo(() => result ? rowsOf<Row>(result.data.rows) : [], [result]);
  const scenarios = useMemo(() => [...new Set(rows.map(r => String(r.scenario)))], [rows]);
  const cell = (scenario: string, metric: string) => rows.find(r => r.scenario === scenario && r.metric === metric);
  const stale = !!result && result.revision !== revision;
  const overall = result ? STATUS[result.data.status] ?? STATUS.unconfigured : null;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={run} disabled={!!stage}>{stage ? "Running…" : "Run rate shocks"}</Button>
        {stage && <span className="flex items-center gap-2 text-sm text-paper-dim"><Spinner />{stage}</span>}
        {overall && <Badge tone={overall.tone} dot>{overall.label}</Badge>}
        {stale && <Badge tone="warning">Inputs changed since this run</Badge>}
        <Button variant="secondary" disabled={!rows.length}
          onClick={() => download(rows, ["scenario", "metric", "base", "value", "delta", "delta_pct", "applicability", "max_decline_pct", "headroom_pct", "status", "unavailable_reason"])}>
          Export CSV
        </Button>
        <p className="text-xs text-paper-faint">Full revaluation at fixed base OAS on shared draws. Ramps are income only.</p>
      </div>
      {error && <p role="alert" className="text-sm text-danger">{error}</p>}

      {result && (
        <Card className={stale ? "opacity-60 transition-opacity duration-base" : undefined}>
          <CardHeader title="EVE and NII by scenario" sub={`Base EVE ${money(result.data.base_eve)} including hedges ${money(result.data.base_hedge_value)} · ${result.data.horizon_months}-month projection · change as % of base`}
            right={<InfoPop width="20rem">{Object.values(result.data.conventions).map((c, i) => <p key={i} className="mb-1.5 last:mb-0">{c}</p>)}</InfoPop>} />
          <CardBody className="p-0">
            <div className="overflow-x-auto">
              <table aria-label="Rate shock report" className="w-full text-left text-sm num">
                <thead><tr className="text-xs text-paper-faint">
                  <th className="border-b border-line px-3 py-2 font-medium">Scenario</th>
                  {METRICS.map(([key, label]) => <th key={key} colSpan={3} className="border-b border-l border-line px-3 py-2 font-medium">{label}</th>)}
                </tr><tr className="text-xs text-paper-faint">
                  <th className="border-b border-line px-3 py-1.5" />
                  {METRICS.map(([key]) => [
                    <th key={`${key}-d`} className="border-b border-l border-line px-3 py-1.5 text-right font-medium">Change</th>,
                    <th key={`${key}-p`} className="border-b border-line px-3 py-1.5 text-right font-medium">%</th>,
                    <th key={`${key}-s`} className="border-b border-line px-3 py-1.5 font-medium">Limit</th>,
                  ])}
                </tr></thead>
                <tbody>{scenarios.map(name => (
                  <tr key={name}>
                    <td className="whitespace-nowrap border-b border-line/50 px-3 py-1.5">{title(name)}</td>
                    {METRICS.map(([key]) => {
                      const r = cell(name, key);
                      const status = STATUS[String(r?.status)] ?? STATUS.unconfigured;
                      const limit = typeof r?.max_decline_pct === "number" ? ` −${r.max_decline_pct}%` : "";
                      return [
                        <td key={`${key}-d`} className="whitespace-nowrap border-b border-l border-line/50 px-3 py-1.5 text-right">{money(r?.delta)}</td>,
                        <td key={`${key}-p`} className="whitespace-nowrap border-b border-line/50 px-3 py-1.5 text-right">{pct(r?.delta_pct)}</td>,
                        <td key={`${key}-s`} className="whitespace-nowrap border-b border-line/50 px-3 py-1.5" title={r?.unavailable_reason ? String(r.unavailable_reason) : undefined}>
                          <Badge tone={status.tone}>{status.label}{limit}</Badge></td>,
                      ];
                    })}
                  </tr>))}
                </tbody>
              </table>
            </div>
          </CardBody>
        </Card>
      )}
      {!result && !stage && <p className="text-sm text-paper-faint">No report yet. Run rate shocks to revalue the saved balance sheet under the policy set.</p>}

      <Card>
        <CardHeader title="Shock policy and limits"
          sub="Scenario sizes, floor and board limits. A limit is the largest permitted decline, as % of the base metric. No limit is never a pass." />
        <CardBody className="space-y-2">
          <textarea aria-label="Rate shock policy" spellCheck={false} value={draft} onChange={e => setDraft(e.target.value)}
            className="h-64 w-full rounded border border-line bg-surface p-2 font-mono text-xs" />
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="secondary" onClick={save} disabled={!policy || draft === JSON.stringify(policy, null, 2)}>Save policy</Button>
            {saved && <span className="text-sm text-paper-dim">{saved}</span>}
            <span className="text-xs text-paper-faint">Example limit: {`{"metric":"eve","scenario":"parallel_up_200","applicability":"internal_limit","max_decline_pct":15}`}</span>
          </div>
        </CardBody>
      </Card>
    </div>
  );
}
