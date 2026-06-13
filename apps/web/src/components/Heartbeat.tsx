/** Compute heartbeat: live path-evaluation throughput as a filled canvas trace.
 *
 * Samples are (elapsed_s, cumulative path_evaluations); we draw the derivative
 * (path-evals / second) as a brand-gradient area with a glowing, pulsing
 * leading dot. The rAF loop runs only while `running` and motion is allowed;
 * under reduced motion it paints one static frame. DPR-aware + ResizeObserver.
 *
 * Two visual densities: `rail` (compact, for the masthead) and the default
 * full panel (for solve consoles). */
import { useEffect, useMemo, useRef } from "react";
import { compact } from "./motion";

export type Sample = { t: number; pe: number };

export function Heartbeat({
  samples, running, reduced, variant = "panel",
}: {
  samples: Sample[];
  running: boolean;
  reduced: boolean;
  variant?: "panel" | "rail";
}) {
  const ref = useRef<HTMLCanvasElement>(null);
  const wrap = useRef<HTMLDivElement>(null);
  const peak = useRef(0);
  const rail = variant === "rail";

  const rates = useMemo(() => {
    const out: number[] = [];
    for (let i = 1; i < samples.length; i++) {
      const dt = samples[i].t - samples[i - 1].t;
      const dpe = samples[i].pe - samples[i - 1].pe;
      out.push(dt > 1e-6 ? Math.max(0, dpe / dt) : 0);
    }
    return out;
  }, [samples]);

  useEffect(() => {
    const cv = ref.current, box = wrap.current;
    if (!cv || !box) return;
    let alive = true;
    let raf = 0;
    if (!running) peak.current = 0;
    const draw = (pulse: number) => {
      const dpr = Math.min(devicePixelRatio || 1, 2);
      const w = box.clientWidth, h = box.clientHeight;
      if (w === 0 || h === 0) { if (alive && running && !reduced) raf = requestAnimationFrame(draw); return; }
      if (cv.width !== w * dpr || cv.height !== h * dpr) { cv.width = w * dpr; cv.height = h * dpr; }
      const ctx = cv.getContext("2d");
      if (!ctx) return;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      if (!rail) {
        ctx.strokeStyle = "#2b3139"; ctx.lineWidth = 1;
        ctx.beginPath(); ctx.moveTo(0, h - 0.5); ctx.lineTo(w, h - 0.5); ctx.stroke();
      }
      const mx = Math.max(peak.current, ...rates, 1); peak.current = mx;
      if (rates.length < 1) {
        if (!rail) {
          ctx.fillStyle = "#707a8a"; ctx.font = "11px 'JetBrains Mono', monospace";
          ctx.fillText(running ? "awaiting telemetry…" : "path-evaluations / s", 8, h / 2);
        }
        if (alive && running && !reduced) raf = requestAnimationFrame(draw);
        return;
      }
      const n = rates.length;
      const x = (i: number) => (n === 1 ? w / 2 : (i / (n - 1)) * w);
      const pad = rail ? 2 : 4;
      const y = (r: number) => h - pad - (r / mx) * (h - pad * 2 - (rail ? 1 : 4));
      const grad = ctx.createLinearGradient(0, 0, 0, h);
      grad.addColorStop(0, rail ? "rgba(252,213,53,0.34)" : "rgba(252,213,53,0.28)");
      grad.addColorStop(1, "rgba(252,213,53,0.02)");
      ctx.beginPath(); ctx.moveTo(0, h);
      rates.forEach((r, i) => ctx.lineTo(x(i), y(r)));
      ctx.lineTo(x(n - 1), h); ctx.closePath(); ctx.fillStyle = grad; ctx.fill();
      ctx.beginPath();
      rates.forEach((r, i) => (i ? ctx.lineTo(x(i), y(r)) : ctx.moveTo(x(i), y(r))));
      ctx.strokeStyle = "#fcd535"; ctx.lineWidth = rail ? 1.25 : 1.5;
      ctx.shadowColor = "rgba(252,213,53,0.7)"; ctx.shadowBlur = running ? (rail ? 6 : 8) : 0;
      ctx.stroke(); ctx.shadowBlur = 0;
      const lx = x(n - 1), ly = y(rates[n - 1]);
      if (running && !reduced) {
        const p = (Math.sin(pulse / 320) + 1) / 2;
        ctx.beginPath(); ctx.arc(lx, ly, (rail ? 2 : 3) + p * (rail ? 2 : 3), 0, Math.PI * 2);
        ctx.fillStyle = `rgba(252,213,53,${0.18 + p * 0.22})`; ctx.fill();
      }
      ctx.beginPath(); ctx.arc(lx, ly, rail ? 1.8 : 2.5, 0, Math.PI * 2); ctx.fillStyle = "#fcd535"; ctx.fill();
      if (!rail) {
        ctx.fillStyle = "#707a8a"; ctx.font = "10px 'JetBrains Mono', monospace";
        ctx.fillText(`peak ${compact(mx)} path-evals/s`, 8, 12);
      }
      if (alive && running && !reduced) raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    const ro = new ResizeObserver(() => draw(performance.now()));
    ro.observe(box);
    return () => { alive = false; cancelAnimationFrame(raf); ro.disconnect(); };
  }, [rates, running, reduced, rail]);

  return (
    <div
      ref={wrap}
      className={rail
        ? "relative h-9 w-full overflow-hidden"
        : "relative h-28 w-full overflow-hidden rounded-lg border border-line bg-ink/40"}
    >
      <canvas ref={ref} className="block h-full w-full" />
    </div>
  );
}
