/** Shared motion primitives for the engine-alive feel.
 *
 * Everything here degrades to instant/static under prefers-reduced-motion:
 * tweens snap to target, the number-flash is suppressed, and the View
 * Transition helper runs the update synchronously. Motion is a signal of
 * state (a number moving = the engine recomputed), never decoration. */
import { useEffect, useRef, useState, type CSSProperties } from "react";

/* ── reduced-motion ───────────────────────────────────────────────────── */
export function prefersReducedMotion() {
  return typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function useReducedMotion() {
  const [r, setR] = useState(prefersReducedMotion);
  useEffect(() => {
    const m = matchMedia("(prefers-reduced-motion: reduce)");
    const h = () => setR(m.matches);
    m.addEventListener("change", h);
    return () => m.removeEventListener("change", h);
  }, []);
  return r;
}

/* ── number tween (ease-out-expo toward a moving target) ───────────────── */
export function useTween(target: number, reduced: boolean, ms = 650) {
  const [val, setVal] = useState(target);
  const from = useRef(target);
  const t0 = useRef(0);
  const raf = useRef(0);
  const cur = useRef(target);
  useEffect(() => {
    if (reduced) { cur.current = target; setVal(target); return; }
    from.current = cur.current;
    t0.current = performance.now();
    const tick = (now: number) => {
      const p = Math.min(1, (now - t0.current) / ms);
      const e = 1 - Math.pow(2, -10 * p); // ease-out-expo
      const v = from.current + (target - from.current) * (p >= 1 ? 1 : e);
      cur.current = v; setVal(v);
      if (p < 1) raf.current = requestAnimationFrame(tick);
    };
    cancelAnimationFrame(raf.current);
    raf.current = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf.current);
  }, [target, reduced, ms]);
  return val;
}

/* ── formatters ───────────────────────────────────────────────────────── */
export const compact = (n: number) =>
  new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: Math.abs(n) < 1e4 ? 0 : 1 }).format(n);
export const full = (n: number) => Math.round(n).toLocaleString();

/* ── TweenNumber: a value that counts to its target and flashes on change ─ */
export function TweenNumber({
  value, format, className, style, flash = true, title,
}: {
  value: number;
  format: (n: number) => string;
  className?: string;
  style?: CSSProperties;
  flash?: boolean;
  title?: string;
}) {
  const reduced = useReducedMotion();
  const v = useTween(value, reduced);
  const prev = useRef(value);
  const [dir, setDir] = useState<"up" | "down" | null>(null);
  useEffect(() => {
    if (!flash || reduced) { prev.current = value; return; }
    if (value > prev.current) setDir("up");
    else if (value < prev.current) setDir("down");
    prev.current = value;
    const id = setTimeout(() => setDir(null), 700);
    return () => clearTimeout(id);
  }, [value, flash, reduced]);
  return (
    <span
      className={className}
      style={style}
      title={title}
      data-flash={dir ?? undefined}
    >
      {format(v)}
    </span>
  );
}

/* ── View Transition helper (shared-element morphs; graceful fallback) ──── */
type VTDocument = Document & { startViewTransition?: (cb: () => void) => unknown };
export function startViewTransition(update: () => void) {
  const d = document as VTDocument;
  if (typeof d.startViewTransition === "function" && !prefersReducedMotion()) {
    d.startViewTransition(update);
  } else {
    update();
  }
}
