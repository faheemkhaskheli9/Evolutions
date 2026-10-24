import { describe, expect, it } from "vitest";
import type { SpeciesInfo } from "../engine/protocol";
import { pickSpecies } from "./RunUntil";

const sp = (id: number, extinct_year: number | null = null): SpeciesInfo => ({
  id,
  name: `S${id}`,
  common_name: `s${id}`,
  color: [0, 0, 0],
  parent: null,
  origin_year: 0,
  extinct_year,
  split_cause: null,
  n: 10,
  mean: null,
  het: null,
  origin_mean: null,
});

// Phase 5 web bug 6: Run until could send a remembered species that had since gone extinct.
describe("pickSpecies", () => {
  it("keeps a living species", () => {
    expect(pickSpecies([sp(1), sp(2)], 2)).toMatchObject({ sp: 2, speciesId: 2 });
  });
  it("falls back to all life for an extinct species", () => {
    expect(pickSpecies([sp(1), sp(2, 500)], 2)).toMatchObject({ sp: "all", speciesId: 1 });
  });
  it("falls back to all life for a species from an earlier world", () => {
    expect(pickSpecies([sp(1)], 9)).toMatchObject({ sp: "all", speciesId: 1 });
  });
  it("has no single species once all life is extinct", () => {
    const r = pickSpecies([sp(1, 10)], 1);
    expect(r).toMatchObject({ sp: "all", speciesId: null });
    expect(r.alive).toEqual([]);
  });
});
