/** Recalculation graph: which downstream results depend on which inputs.
 *
 * Leaves are the input nodes the API fingerprints on `/state` (a content hash
 * per slice of the committed snapshot: each book, the market, settings, product
 * assumptions, scenarios, cohort workflow state, and a catch-all context node).
 * Result nodes are the engine outputs the screens show. When inputs change, only
 * results whose inputs hash differently are recomputed; risk and stress are kept
 * per book, so editing one book recomputes that book's slice only.
 *
 * Edges here must be conservative: a missing edge leaves a figure silently out of
 * date. They mirror what each server run reads (see `run_kpis`, `run_nii`,
 * `run_risk_all`, `run_stress_all` in apps/api/app/store.py). */
import type { BookName } from "./api";

export type ResultKind = "kpis" | "risk" | "nii" | "stress";
/** Fingerprint per input node, as served by `/state`. */
export type InputNodes = Record<string, string>;

export const ALL_BOOKS: BookName[] = ["mbs", "loans", "debt", "deposits", "cds", "mm"];
/** Books the risk run values one by one (money-market rows carry no risk run). */
export const RISK_BOOKS: BookName[] = ["mbs", "loans", "debt", "deposits", "cds"];
/** Books the 9Q stress runs. */
export const STRESS_BOOKS: BookName[] = ["mbs", "deposits"];

/** Inputs every base result reads. `context` covers equity, hedges, deposit and
 * MBS histories, the valuation date, programs, and any snapshot key added later. */
const SHARED = ["market", "settings", "assumptions:other", "context"];
const PRODUCT_ASSUMPTIONS: Partial<Record<string, string>> = { deposits: "assumptions:deposits", cds: "assumptions:cds" };

/** Input nodes a result node depends on. Scenarios feed no base result, and
 * tapes/cohort rules feed none until a publish changes a book. */
export function dependsOn(node: string): string[] {
  const [kind, book] = node.split(":");
  if (kind === "kpis" || kind === "nii") {
    return [...SHARED, ...ALL_BOOKS.map(b => `books:${b}`), "assumptions:deposits", "assumptions:cds"];
  }
  if (book === "hedges") return SHARED;            // hedge risk reads the hedge book (context) and the market
  const product = PRODUCT_ASSUMPTIONS[book];
  return [...SHARED, `books:${book}`, ...(product ? [product] : [])];
}

/** Result nodes for a requested result. A full risk run also values the hedge
 * book; a run for some books does not. */
export function nodesFor(kind: ResultKind, books?: BookName[]): string[] {
  if (kind === "risk") {
    const scope = books ?? RISK_BOOKS;
    const all = RISK_BOOKS.every(b => scope.includes(b));
    return [...scope.filter(b => RISK_BOOKS.includes(b)).map(b => `risk:${b}`), ...(all ? ["risk:hedges"] : [])];
  }
  if (kind === "stress") return (books ?? STRESS_BOOKS).map(b => `stress:${b}`);
  return [kind];
}

/** Inputs of `node` whose fingerprint differs between two revisions, or null
 * when either revision's fingerprints are unknown (treat as changed). */
export function changedInputs(node: string, then: InputNodes | undefined, now: InputNodes | undefined): string[] | null {
  if (!then || !now) return null;
  return dependsOn(node).filter(d => then[d] !== now[d]);
}
