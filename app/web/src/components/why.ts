import type { Why } from "../engine/protocol";

/** Where a "Why?" panel's chart link goes: the Species panel (optionally charting a trait), or the tree. */
export type ChartTarget = { tab: "species"; trait?: string } | { tab: "tree" };

export function chartTarget(chart: string | null): ChartTarget | null {
  if (!chart) return null;
  if (chart === "tree") return { tab: "tree" };
  if (chart.startsWith("trait:")) return { tab: "species", trait: chart.slice(6) };
  if (chart === "population" || chart === "popgen") return { tab: "species" };
  return null;
}

/** The link's label. */
export function chartLabel(chart: string | null): string {
  if (chart === "tree") return "Show the tree";
  if (chart?.startsWith("trait:")) return "Show the trait chart";
  if (chart === "popgen") return "Show the population-genetics charts";
  return "Show the population chart";
}

const VERDICTS: Record<NonNullable<Why["verdict"]>, string> = {
  selection: "Selection",
  drift: "Consistent with drift",
  mixed: "Selection and drift",
};

export function verdictLabel(v: Why["verdict"]): string | null {
  return v ? VERDICTS[v] : null;
}
