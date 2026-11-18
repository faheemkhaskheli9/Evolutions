import { describe, expect, it } from "vitest";
import { ALL_NATURE, NO_NATURE, anyNature, natureConfig, natureSummary } from "./nature";

describe("nature settings", () => {
  it("sends nothing when nothing is on", () => {
    expect(anyNature(NO_NATURE)).toBe(false);
    expect(anyNature(null)).toBe(false);
    expect(natureConfig(NO_NATURE)).toBeNull();
    expect(natureConfig({ ...NO_NATURE, fires: true })).toEqual({ ...NO_NATURE, fires: true });
  });

  it("copies the cycle so later edits do not change a sent config", () => {
    const n = { ...ALL_NATURE, glacial: { ...ALL_NATURE.glacial! } };
    const cfg = natureConfig(n)!;
    n.glacial.period_years = 5;
    expect(cfg.glacial!.period_years).toBe(100_000);
  });

  it("summarises the settings", () => {
    expect(natureSummary(null)).toBe("None");
    expect(natureSummary(ALL_NATURE)).toBe("Glacial cycle (100,000 years, −6 °C, sea −120 m), droughts, wildfires, harsh winters, volcanic eruptions");
    expect(natureSummary({ ...NO_NATURE, eruptions: true })).toBe("Volcanic eruptions");
  });
});
