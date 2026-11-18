import { describe, expect, it } from "vitest";
import type { Disease, HostHealth, Strain } from "../engine/protocol";
import { count, defaultDiseaseSpecies, diseaseCharts, escapes, meanResistance, percent } from "./disease";

const host = (id: number, infected: number, deaths = 0): HostHealth => ({
  id,
  name: `s${id}`,
  n: 100,
  infected,
  recovered: 0,
  resistance: [0.1, 0.2, 0.3, 0.4],
  virulence: -1,
  transmissibility: -1,
  deaths,
});

describe("disease charts", () => {
  it("turns the series into prevalence, resistance and strain traits", () => {
    const c = diseaseCharts([
      [1, 200, 10, 50, 0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 3],
      [2, 0, 0, 0, 0, 0.1, 0.2, 0.3, 0.4, -1, -1, 3],
    ]);
    expect(c.prevalence).toEqual([
      [1, 25],
      [2, 0],
    ]);
    expect(c.resistance[3]).toEqual([
      [1, 0.4],
      [2, 0.4],
    ]);
    // No infections, no virulence point.
    expect(c.virulence).toEqual([[1, 0.5]]);
    expect(c.transmissibility).toEqual([[1, 0.6]]);
    expect(diseaseCharts(undefined).prevalence).toEqual([]);
  });

  it("shows the most infected species first", () => {
    const d: Disease = { active: true, cases: 0, deaths: 0, strains: [], lineages: [], species: [host(0, 0, 9), host(1, 5), host(2, 5)], series: {} };
    expect(defaultDiseaseSpecies(d)).toBe(1);
    expect(defaultDiseaseSpecies({ ...d, species: [host(0, 0, 1), host(3, 0, 9)] })).toBe(3);
    expect(defaultDiseaseSpecies({ ...d, species: [] })).toBeNull();
  });

  it("formats shares, escapes and resistance", () => {
    expect(count(95)).toBe("95");
    expect(count(1234567.4)).toBe("1,234,567");
    expect(percent(0, 10)).toBe("0%");
    expect(percent(1, 1000)).toBe("<1%");
    expect(percent(25, 100)).toBe("25%");
    const s = { escape: [1, 3] } as Strain;
    expect(escapes(s)).toBe("Escapes R1, R3");
    expect(escapes({ escape: [] } as unknown as Strain)).toBe("Escapes none");
    expect(meanResistance(host(0, 0))).toBeCloseTo(0.25);
  });
});
