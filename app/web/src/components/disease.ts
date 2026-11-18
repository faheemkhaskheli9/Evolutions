import type { Disease, HostHealth, Strain } from "../engine/protocol";

/** Colours of the four resistance loci on charts. */
export const LOCUS_COLORS = ["#7fb2ff", "#6fcf8a", "#ffd54a", "#e59cff"];

export interface DiseaseCharts {
  /** Most hosts infected at once between chart points, as a share of hosts (%). */
  prevalence: [number, number][];
  /** Resistant allele frequency at each locus. */
  resistance: [number, number][][];
  /** Mean virulence and transmissibility of the infections (gaps where there were none). */
  virulence: [number, number][];
  transmissibility: [number, number][];
}

/** Chart lines from one species' disease series (see `Disease.series`). */
export function diseaseCharts(series: number[][] | undefined): DiseaseCharts {
  const out: DiseaseCharts = { prevalence: [], resistance: [[], [], [], []], virulence: [], transmissibility: [] };
  for (const r of series ?? []) {
    const [year, n, , peak] = r;
    out.prevalence.push([year, n > 0 ? (100 * peak) / n : 0]);
    for (let i = 0; i < 4; i++) out.resistance[i].push([year, r[5 + i]]);
    if (r[9] >= 0) out.virulence.push([year, r[9]]);
    if (r[10] >= 0) out.transmissibility.push([year, r[10]]);
  }
  return out;
}

/** The species to show first: the one with most hosts infected (else the most deaths, else the first). */
export function defaultDiseaseSpecies(d: Disease): number | null {
  const best = [...d.species].sort((a, b) => b.infected - a.infected || b.deaths - a.deaths || a.id - b.id)[0];
  return best ? best.id : null;
}

/** A whole number with thousands separators ("1,234"). */
export function count(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

/** "12% infected" style share, with "<1%" for small non-zero values. */
export function percent(part: number, whole: number): string {
  if (whole <= 0 || part <= 0) return "0%";
  const p = (100 * part) / whole;
  return p < 1 ? "<1%" : `${Math.round(p)}%`;
}

/** "Escapes R1, R3" or "Escapes none". */
export function escapes(s: Strain): string {
  return s.escape.length ? `Escapes ${s.escape.map((i) => `R${i}`).join(", ")}` : "Escapes none";
}

/** Mean resistant allele frequency over the four loci. */
export function meanResistance(h: HostHealth): number {
  return h.resistance.reduce((a, b) => a + b, 0) / Math.max(1, h.resistance.length);
}
