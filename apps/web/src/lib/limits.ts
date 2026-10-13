/** Management limits shown across the constraint ledger and KPI tiles.
 * Display references only — the engine computes the metrics; these are the
 * lines the desk reads them against. One place so every surface agrees. */
import type { Variance } from "../components/ui";

export type Limit = { label: string; limit: number; sense: "floor" | "ceiling"; unit: "%" | "y" };

export const LIMITS = {
  eve200: { label: "EVE sensitivity (+200bp)", limit: 15, sense: "ceiling", unit: "%" },
  lcr: { label: "Liquidity coverage", limit: 110, sense: "floor", unit: "%" },
  nsfr: { label: "Stable funding", limit: 100, sense: "floor", unit: "%" },
  cet1: { label: "CET1, end of plan", limit: 10, sense: "floor", unit: "%" },
  durationGap: { label: "Duration gap", limit: 2.0, sense: "ceiling", unit: "y" },
} as const satisfies Record<string, Limit>;

/** Distance to the limit; positive = inside it. */
export const headroom = (value: number, l: Limit) => l.sense === "floor" ? value - l.limit : l.limit - value;

/** Within 8% of the limit: caution (warning tone), not yet a breach. */
export const isTight = (value: number, l: Limit) => headroom(value, l) < 0.08 * Math.abs(l.limit);

export const limitRef = (l: Limit) => `${l.sense === "floor" ? "≥" : "≤"} ${l.limit}${l.unit} limit`;

/** Headroom as a Stat variance: favorable when inside the limit. */
export function limitVariance(value: number, l: Limit, digits = 1): Variance {
  const room = headroom(value, l);
  const unit = l.unit === "%" ? " pts" : "y";
  return { value: room, text: `${room >= 0 ? "+" : "−"}${Math.abs(room).toFixed(digits)}${unit}`, ref: limitRef(l) };
}
