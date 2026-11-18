import { describe, expect, it } from "vitest";
import type { SimEvent } from "../engine/protocol";
import { chartMarkers } from "./chartMarkers";
import { nearestMarker } from "./LineChart";

const ev = (year: number, kind: SimEvent["kind"], text: string): SimEvent => ({ year, kind, species: null, text, index: 0 });

describe("chart markers", () => {
  const events = [
    ev(0, "Info", "World created"),
    ev(3, "Nature", "Wildfire around 7°N 153°E burned the vegetation of 5 tiles; it will grow back."),
    ev(5, "Nature", "Drought around 10°S 20°E: up to 50% less rain over ~500 km for 3 years."),
    ev(8, "Nature", "The drought around 10°S 20°E ends."),
    ev(9, "Intervention", "Climate shift: -2.0 °C global temperature."),
    ev(12, "Nature", "Falling seas (sea level -58 m) opened a land bridge: now 1 large landmass, was 2."),
    ev(12.5, "Nature", "Volcanic eruption at 2°S 18°E: up to 2.4 °C cooler over ~2654 km for 2 years."),
    ev(13, "Speciation", "A new species"),
  ];

  it("marks interventions and notable natural events", () => {
    const m = chartMarkers(events);
    expect(m.map((x) => [x.x, x.kind])).toEqual([
      [9, "intervention"],
      [12, "nature"],
      [12.5, "nature"],
    ]);
    expect(chartMarkers(events, { cycle: true }).map((x) => x.x)).toEqual([9, 12]);
  });

  it("keeps routine events when asked, and only the latest", () => {
    expect(chartMarkers(events, { all: true })).toHaveLength(6);
    expect(chartMarkers(events, { all: true, max: 2 }).map((x) => x.x)).toEqual([12, 12.5]);
  });

  it("finds the marker nearest the pointer", () => {
    const m = chartMarkers(events);
    expect(nearestMarker(m, 8.6, 1)?.x).toBe(9);
    expect(nearestMarker(m, 7, 1)).toBeNull();
  });
});
