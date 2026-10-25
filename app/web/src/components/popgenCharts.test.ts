import { describe, expect, it } from "vitest";
import type { GenSample } from "../engine/protocol";
import { alleleSeries, heterozygositySeries, neSeries } from "./popgenCharts";

const g = (year: number, ne_h: number | null, freqs: (number | null)[]): GenSample => ({
  year,
  ht: 0.8,
  hs: 0.7,
  fst: 0.1,
  ne: 100,
  ne_h,
  generation: 1,
  segregating: 50,
  freqs,
});

describe("population-genetics series", () => {
  it("plots H_T and H_S", () => {
    const s = heterozygositySeries([g(0, null, []), g(1, null, [])], "red");
    expect(s.map((x) => x.points)).toEqual([
      [[0, 0.8], [1, 0.8]],
      [[0, 0.7], [1, 0.7]],
    ]);
  });

  it("leaves out the heterozygosity Ne until it can be estimated", () => {
    expect(neSeries([g(0, null, [])], [], "red").map((x) => x.name)).toEqual(["Census N", "Nₑ (sex ratio)"]);
    const s = neSeries([g(0, null, []), g(1, 250, [])], [{ year: 0, n: 300, mean: [], sd: [], het: 0.8 }], "red");
    expect(s[2].points).toEqual([[1, 250]]);
    expect(s[0].points).toEqual([[0, 300]]);
  });

  it("follows each allele by position and skips untracked samples", () => {
    const s = alleleSeries([g(0, null, [0.2, null]), g(1, null, [0.3, 0.05])], [7, 9], ["a", "b"]);
    expect(s[0]).toMatchObject({ name: "a", points: [[0, 0.2], [1, 0.3]] });
    expect(s[1].points).toEqual([[1, 0.05]]);
    expect(s[0].color).not.toBe(s[1].color);
  });
});
