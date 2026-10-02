/** Risk settings + model assumption overrides (the documented surfaces). */
import { useEffect, useState } from "react";
import { api, type Settings } from "../lib/api";
import { Badge, Button, Card, CardBody, CardHeader, Input, InfoPop } from "../components/ui";

export default function SettingsPage() {
  const [s, setS] = useState<Settings | null>(null);
  const [asm, setAsm] = useState<Record<string, unknown> | null>(null);
  const [segText, setSegText] = useState("");

  useEffect(() => {
    api.settings().then(setS);
    api.assumptions().then(a => { setAsm(a); setSegText(JSON.stringify(a.deposit_segments, null, 1)); });
  }, []);

  if (!s || !asm) return null;
  const saveSettings = async () => {
    try { await api.putSettings(s); alert("settings saved"); }
    catch (e) { alert(String(e)); }
  };
  const saveSegments = async () => {
    try { await api.putAssumptions({ deposit_segments: JSON.parse(segText) }); alert("applied"); }
    catch (e) { alert(String(e)); }
  };

  return (
    <div className="grid gap-3 xl:grid-cols-2">
      <Card>
        <CardHeader title="Risk & scenario settings" right={<Button onClick={saveSettings}>Save</Button>} />
        <CardBody className="space-y-3">
          <label className="flex items-center gap-3 text-xs text-paper-dim">
            <span className="w-56">Product simulation backend</span>
            <select aria-label="Product simulation backend" value={s.compute_backend ?? 'rust'}
              className="rounded-md border border-line bg-surface-2 p-2 text-paper"
              onChange={e => setS({ ...s, compute_backend: e.target.value as 'python' | 'rust' })}>
              <option value="rust">Rust (production default)</option>
              {s.compute_backend === 'python' && <option value="python" disabled>Python (deprecated — select Rust)</option>}
            </select>
          </label>
          <p className="text-[11px] text-paper-faint">Rust is required for production calculations. Python calculation is deprecated and retained only for independent engine validation. Select Rust and rebuild older strategy libraries and sessions. Missing native libraries cause an error; calculations never silently fall back to Python.</p>
          {([["n_paths", "Monte Carlo paths"], ["n_paths_base", "MBS base calibration paths"], ["n_threads", "Compute threads (0 = all)"], ["seed", "CRN seed"], ["horizon_months", "NII/stress horizon (months)"]] as const).map(([k, label]) => (
            <div key={k} className="flex items-center gap-3">
              <div className="flex w-56 items-center text-xs text-paper-dim">{label}
                <InfoPop>{k === "n_paths" ? "Paths per sensitivity and non-MBS calibration. Common random numbers keep central differences stable." : k === "n_paths_base" ? "Paths for MBS base OAS calibration and scenario spot marks; sensitivity paths are configured separately." : k === "n_threads" ? "Set for every job. Zero restores all threads supported by the server process." : k === "seed" ? "One shared random draw set per scenario family. Changing the seed changes every number coherently." : "Months for NII, stress, and strategy forecasts. 27 = nine quarters."}</InfoPop>
              </div>
              <Input type="number" value={s[k]} onChange={e => setS({ ...s, [k]: parseInt(e.target.value) || 0 })} />
            </div>
          ))}
          <div className="flex items-center gap-3">
            <div className="flex w-56 items-center text-xs text-paper-dim">stress shocks (bp)
              <InfoPop>Forward-starting parallel shocks for the 9Q stress pack. Include ±100 to get the forward dv01 profile.</InfoPop></div>
            <Input value={s.shocks_bp.join(", ")}
              onChange={e => setS({ ...s, shocks_bp: e.target.value.split(",").map(x => parseFloat(x)).filter(n => !isNaN(n)) })} />
          </div>
          <div className="pt-2 text-[11px] text-paper-faint">
            One CRN object per run; central differences are deltas of means under shared draws — changing the seed between bump sides destroys them (engine invariant 2).
          </div>
        </CardBody>
      </Card>

      <Card>
        <CardHeader title="Deposit attrition segments" sub="base decay / flight amp / S-curve B / g0 per segment — the panel-fit seam"
          right={<Button onClick={saveSegments}>Apply</Button>} />
        <CardBody>
          <textarea className="h-56 w-full rounded-md border border-line bg-surface-2 p-3 font-mono text-[11px] text-paper-dim outline-none"
            value={segText} onChange={e => setSegText(e.target.value)} />
        </CardBody>
      </Card>

      <Card className="xl:col-span-2">
        <CardHeader title="Prepay model vector" sub="read-only via API" right={<Badge tone="amber">restart required</Badge>} />
        <CardBody>
          <div className="grid grid-cols-3 gap-2 lg:grid-cols-9">
            {(asm.prepay as { names: string[]; vector: number[] }).names.map((n, i) => (
              <div key={n} className="rounded-lg border border-line bg-surface-2 p-2">
                <div className="text-[10px] text-paper-faint">{n}</div>
                <div className="num text-sm text-paper">{(asm.prepay as { vector: number[] }).vector[i]}</div>
              </div>
            ))}
          </div>
          <div className="mt-3 text-[11px] text-paper-faint">{String(asm.note)}</div>
        </CardBody>
      </Card>
    </div>
  );
}
