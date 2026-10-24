import { describe, expect, it } from "vitest";
import { chartRange, niceTicks, xTickLabel, yAxisTicks, type Series } from "./LineChart";

const series = (points: [number, number][], band?: number[]): Series[] => [{ name: "s", color: "#000", points, band }];

describe("niceTicks", () => {
  it("uses steps of 1, 2 or 5 × 10ⁿ inside the range", () => {
    expect(niceTicks(0, 100, 3)).toEqual([0, 50, 100]);
    expect(niceTicks(0, 1000, 4)).toEqual([0, 500, 1000]);
    expect(niceTicks(3, 17, 4)).toEqual([5, 10, 15]);
    expect(niceTicks(-0.3, 0.3, 4)).toEqual([-0.2, 0, 0.2]);
  });
  it("returns exact zero, not -0 or 1e-17", () => {
    const t = niceTicks(-1, 1, 3);
    expect(t).toContain(0);
    expect(Object.is(t[1], 0)).toBe(true);
    for (const v of niceTicks(-0.7, 0.9, 5)) expect(Math.abs(v) < 1e-12 ? v === 0 : true).toBe(true);
  });
  it("falls back to one tick for an empty range", () => {
    expect(niceTicks(5, 5)).toEqual([5]);
    expect(niceTicks(0, NaN)).toEqual([0]);
  });
});

// Phase 5 web bug 11: charts with one sample, year ticks and log-scale ticks.
describe("chart regressions", () => {
  it("gives a single sample a non-empty range", () => {
    const r = chartRange(series([[100, 40]]), false);
    expect(r.xmax).toBeGreaterThan(r.xmin);
    expect(r.ymax).toBeGreaterThan(r.ymin);
    expect(r.ymin).toBeLessThan(40);
    expect(r.ymax).toBeGreaterThan(40);
  });
  it("gives an empty chart a unit range", () => {
    expect(chartRange([], false)).toEqual({ xmin: 0, xmax: 1, ymin: 0, ymax: 1 });
  });
  it("includes the ± band", () => {
    const r = chartRange(series([[0, 10], [1, 10]], [5, 5]), false);
    expect(r.ymin).toBeLessThan(5);
    expect(r.ymax).toBeGreaterThan(15);
  });
  it("labels year ticks as whole years, not 0.0 or 1,001", () => {
    const span = 1000;
    const labels = niceTicks(1, 1001, 4).map((t) => xTickLabel(t, span));
    expect(labels).toEqual(["500", "1,000"]);
    expect(xTickLabel(0, span)).toBe("0");
  });
  it("keeps decimals on short spans", () => {
    expect(xTickLabel(0.25, 0.5)).toBe("0.25");
    expect(xTickLabel(2.5, 5)).toBe("2.5");
  });
  it("puts log-scale ticks on powers of ten", () => {
    const r = chartRange(series([[0, 1], [1, 20_000]]), true);
    const ticks = yAxisTicks(r.ymin, r.ymax, true);
    expect(ticks.length).toBeGreaterThanOrEqual(2);
    for (const t of ticks) expect(Number.isInteger(t)).toBe(true);
    expect(ticks.map((t) => 10 ** t)).not.toContain(11_535);
  });
  it("falls back to three ticks on a log scale narrower than a decade", () => {
    const r = chartRange(series([[0, 200], [1, 400]]), true);
    const ticks = yAxisTicks(r.ymin, r.ymax, true);
    expect(ticks).toHaveLength(3);
    expect(ticks[0]).toBe(r.ymin);
    expect(ticks[2]).toBe(r.ymax);
  });
});
