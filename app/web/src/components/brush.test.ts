import { describe, expect, it } from "vitest";
import { extendStroke, strokePoints } from "./brush";

describe("brush strokes", () => {
  it("fills the gap left by a fast drag", () => {
    const pts: [number, number][] = [];
    extendStroke(pts, 0, 0, 2);
    extendStroke(pts, 0.5, 0, 2); // closer than half the radius: skipped
    extendStroke(pts, 5, 0, 2);
    expect(pts.length).toBe(6);
    for (let i = 1; i < pts.length; i++) expect(pts[i][0] - pts[i - 1][0]).toBeLessThanOrEqual(1);
    expect(pts[pts.length - 1]).toEqual([5, 0]);
  });

  it("starts the stroke on the map and keeps it in bounds", () => {
    const out = strokePoints(
      [
        [130, -2],
        [125, 10],
        [140, 70],
      ],
      128,
      64,
    );
    expect(out).toEqual([
      [2, 0],
      [-3, 10],
      [12, 64],
    ]);
  });

  it("thins long strokes to the engine's limit", () => {
    const pts = Array.from({ length: 10_000 }, (_, i) => [i / 100, 1] as [number, number]);
    const out = strokePoints(pts, 128, 64);
    expect(out.length).toBeLessThanOrEqual(4096);
    expect(out[out.length - 1][0]).toBeCloseTo(99.99);
  });
});
