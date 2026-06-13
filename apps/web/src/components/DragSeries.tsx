/** DragSeries: a per-quarter scenario leg you SHAPE by dragging points.
 * Pointer events on an SVG; values clamp to [min,max] and snap to step.
 * Click an empty column to set it; drag to sculpt; the path is the leg. */
import { useMemo, useRef, useState } from "react";

export default function DragSeries({ values, onChange, min, max, step, unit, color = "#fcd535", n = 9 }: {
  values: number[]; onChange: (v: number[]) => void;
  min: number; max: number; step: number; unit: string; color?: string; n?: number;
}) {
  const W = 460, H = 110, PX = 26, PY = 12;
  const svg = useRef<SVGSVGElement>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const vals = useMemo(() => Array.from({ length: n }, (_, i) =>
    values.length ? values[Math.min(i, values.length - 1)] : 0), [values, n]);
  const x = (i: number) => PX + (i * (W - 2 * PX)) / (n - 1);
  const y = (v: number) => H - PY - ((v - min) / (max - min)) * (H - 2 * PY);
  const fromY = (py: number) => {
    const raw = min + ((H - PY - py) / (H - 2 * PY)) * (max - min);
    return Math.round(Math.max(min, Math.min(max, raw)) / step) * step;
  };
  const apply = (i: number, clientX: number, clientY: number) => {
    const r = svg.current!.getBoundingClientRect();
    const py = ((clientY - r.top) / r.height) * H;
    const next = [...vals];
    next[i] = fromY(py);
    onChange(next);
    return void clientX;
  };
  const nearest = (clientX: number) => {
    const r = svg.current!.getBoundingClientRect();
    const px = ((clientX - r.left) / r.width) * W;
    let bi = 0, bd = 1e9;
    for (let i = 0; i < n; i++) { const d = Math.abs(x(i) - px); if (d < bd) { bd = d; bi = i; } }
    return bi;
  };
  const path = vals.map((v, i) => `${i ? "L" : "M"}${x(i)},${y(v)}`).join(" ");
  const zy = min < 0 && max > 0 ? y(0) : null;
  return (
    <svg ref={svg} viewBox={`0 0 ${W} ${H}`} className="w-full touch-none select-none"
      onPointerDown={e => { const i = nearest(e.clientX); setDrag(i); apply(i, e.clientX, e.clientY); (e.target as Element).setPointerCapture?.(e.pointerId); }}
      onPointerMove={e => { if (drag !== null) apply(drag, e.clientX, e.clientY); }}
      onPointerUp={() => setDrag(null)} onPointerLeave={() => setDrag(null)}>
      {Array.from({ length: n }, (_, i) => (
        <g key={i}>
          <line x1={x(i)} x2={x(i)} y1={PY} y2={H - PY} stroke="#2b3139" strokeWidth="1" />
          <text x={x(i)} y={H - 1} textAnchor="middle" fontSize="7.5" fill="#707a8a">Q{i + 1}</text>
        </g>
      ))}
      {zy !== null && <line x1={PX} x2={W - PX} y1={zy} y2={zy} stroke="#2b3139" strokeDasharray="3 3" />}
      <path d={path} fill="none" stroke={color} strokeWidth="1.5" />
      {vals.map((v, i) => (
        <g key={i} className="cursor-ns-resize">
          <circle cx={x(i)} cy={y(v)} r={drag === i ? 6 : 4} fill="#0b0e11" stroke={color} strokeWidth="1.5" />
          {(drag === i) && (
            <text x={x(i)} y={y(v) - 9} textAnchor="middle" fontSize="8.5" fill="#eaecef" className="num">
              {v}{unit}
            </text>
          )}
        </g>
      ))}
    </svg>
  );
}
