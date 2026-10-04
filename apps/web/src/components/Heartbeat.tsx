/** Compute heartbeat: live path-evaluation throughput as a filled canvas trace.
 *
 * Samples are (elapsed_s, cumulative path_evaluations); we draw the derivative
 * (path-evals / second) as a soft-filled accent trace with a leading dot. It
 * repaints when samples arrive (no decorative animation loop). Colors come from
 * the Aperture tokens. DPR-aware + ResizeObserver.
 *
 * Two visual densities: `rail` (compact, for the masthead) and the default
 * full panel (for solve consoles). */
import { useEffect, useMemo, useRef } from "react";
import { compact } from "./motion";
import { token } from "../lib/tokens";

export type Sample = { t: number; pe: number };

export function Heartbeat({
  samples, running, variant = "panel",
}: {
  samples: Sample[];
  running: boolean;
  reduced?: boolean; // kept for callers; the trace no longer animates
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
    if (!running) peak.current = 0;
    const draw = () => {
      const dpr = Math.min(devicePixelRatio || 1, 2);
      const w = box.clientWidth, h = box.clientHeight;
      if (w === 0 || h === 0) return;
      if (cv.width !== w * dpr || cv.height !== h * dpr) { cv.width = w * dpr; cv.height = h * dpr; }
      const ctx = cv.getContext("2d");
      if (!ctx) return;
      const accent = token("--accent"), soft = token("--yellow-softer"), muted = token("--text-tertiary");
      const font = (px: number) => `${px}px Inter, system-ui, sans-serif`;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      if (!rail) {
        ctx.strokeStyle = token("--border-subtle"); ctx.lineWidth = 1;
        ctx.beginPath(); ctx.moveTo(0, h - 0.5); ctx.lineTo(w, h - 0.5); ctx.stroke();
      }
      const mx = Math.max(peak.current, ...rates, 1); peak.current = mx;
      if (rates.length < 1) {
        if (!rail) {
          ctx.fillStyle = muted; ctx.font = font(11);
          ctx.fillText(running ? "Awaiting telemetry…" : "Path-evaluations / s", 8, h / 2);
        }
        return;
      }
      const n = rates.length;
      const x = (i: number) => (n === 1 ? w / 2 : (i / (n - 1)) * w);
      const pad = rail ? 2 : 4;
      const y = (r: number) => h - pad - (r / mx) * (h - pad * 2 - (rail ? 1 : 4));
      // flat soft fill + a crisp accent line; no gradient, glow or looping pulse
      ctx.beginPath(); ctx.moveTo(0, h);
      rates.forEach((r, i) => ctx.lineTo(x(i), y(r)));
      ctx.lineTo(x(n - 1), h); ctx.closePath(); ctx.fillStyle = soft; ctx.fill();
      ctx.beginPath();
      rates.forEach((r, i) => (i ? ctx.lineTo(x(i), y(r)) : ctx.moveTo(x(i), y(r))));
      ctx.strokeStyle = accent; ctx.lineWidth = rail ? 1.25 : 1.5; ctx.stroke();
      ctx.beginPath(); ctx.arc(x(n - 1), y(rates[n - 1]), rail ? 1.8 : 2.5, 0, Math.PI * 2); ctx.fillStyle = accent; ctx.fill();
      if (!rail) {
        ctx.fillStyle = muted; ctx.font = font(10);
        ctx.fillText(`Peak ${compact(mx)} path-evals/s`, 8, 12);
      }
    };
    draw();
    const ro = new ResizeObserver(draw);
    ro.observe(box);
    return () => ro.disconnect();
  }, [rates, running, rail]);

  return (
    <div
      ref={wrap}
      className={rail
        ? "relative h-9 w-full overflow-hidden"
        : "relative h-28 w-full overflow-hidden rounded-md border border-line bg-surface-base"}
    >
      <canvas ref={ref} className="block h-full w-full" />
    </div>
  );
}
