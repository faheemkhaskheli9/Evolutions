// Series for the Species panel's population-genetics charts (PRD FR-O7).

import type { GenSample, Sample } from "../engine/protocol";
import type { Series } from "./LineChart";

/** Colours for allele trajectories, in pick order. */
export const ALLELE_COLORS = ["#e4572e", "#2e86ab", "#76b041", "#a14ebf", "#f3a712"];
/** Alleles that can be charted at once. */
export const MAX_ALLELES = ALLELE_COLORS.length;

/** H_T and H_S over time. */
export function heterozygositySeries(g: GenSample[], color: string): Series[] {
  return [
    { name: "Whole species (H_T)", color, points: g.map((s) => [s.year, s.ht]) },
    { name: "Within patches (H_S)", color: "#8a8f98", points: g.map((s) => [s.year, s.hs]) },
  ];
}

/** Census size and the two effective-size estimates; samples without an estimate are skipped. */
export function neSeries(g: GenSample[], metrics: Sample[], color: string): Series[] {
  const out: Series[] = [
    { name: "Census N", color: "#8a8f98", points: metrics.filter((m) => m.n > 0).map((m) => [m.year, m.n]) },
    { name: "Nₑ (sex ratio)", color, points: g.filter((s) => s.ne > 0).map((s) => [s.year, s.ne]) },
  ];
  const fromHet = g.flatMap((s): [number, number][] => (s.ne_h !== null ? [[s.year, s.ne_h]] : []));
  if (fromHet.length) out.push({ name: "Nₑ (heterozygosity loss)", color: "#e4572e", points: fromHet });
  return out;
}

/** One series per requested allele (in `uids` order), skipping samples where it was not tracked. */
export function alleleSeries(g: GenSample[], uids: number[], names: string[]): Series[] {
  return uids.map((uid, k) => ({
    name: names[k] ?? `allele ${uid}`,
    color: ALLELE_COLORS[k % ALLELE_COLORS.length],
    points: g.flatMap((s): [number, number][] => {
      const f = s.freqs[k];
      return f === null || f === undefined ? [] : [[s.year, f]];
    }),
  }));
}
