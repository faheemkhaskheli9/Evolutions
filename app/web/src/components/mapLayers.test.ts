import { describe, expect, it } from "vitest";
import { ALLELE_RAMP, BIOME_COLORS, TEMP_RAMP, VEG_RAMP, type RGB } from "../util";
import { demePixels, scalarPixels, snowPixels, terrainPixels, worldCopies } from "./mapLayers";

const px = (data: Uint8ClampedArray, i: number) => Array.from(data.slice(i * 4, i * 4 + 4));
const f32 = (...v: number[]) => new Float32Array(v);
const rgba = (c: RGB, a: number) => [...c.map(Math.round), a];

describe("terrainPixels", () => {
  const world = { w: 4, h: 1 };
  // Tiles: deep sea, shallow sea, river, flat grassland.
  const elevation = f32(-4000, -100, 50, 50);
  const biome = f32(0, 0, 6, 6);
  const water = f32(1, 1, 3, 0);

  it("shades the sea by depth", () => {
    const p = terrainPixels(world, elevation, biome, water);
    expect(px(p, 0)).toEqual([22, 52, 98, 255]);
    expect(px(p, 1)[2]).toBeGreaterThan(px(p, 0)[2]);
  });
  it("draws rivers as water, or as the land they cross", () => {
    expect(px(terrainPixels(world, elevation, biome, water), 2)).toEqual([62, 122, 178, 255]);
    expect(px(terrainPixels(world, elevation, biome, water, false), 2)).toEqual(rgba(BIOME_COLORS[6], 255));
  });
  it("uses the biome colour on flat lowland, lit on slopes facing east", () => {
    const flat = terrainPixels({ w: 2, h: 1 }, f32(50, 50), f32(6, 6), f32(0, 0));
    expect(px(flat, 0)).toEqual(rgba(BIOME_COLORS[6], 255));
    // The last column's east neighbour wraps to the first, here the sea (taken as 0 m).
    expect(px(terrainPixels(world, elevation, biome, water), 3)[0]).toBeGreaterThan(BIOME_COLORS[6][0]);
  });
  it("is opaque everywhere", () => {
    const p = terrainPixels(world, elevation, biome, water);
    for (let i = 0; i < 4; i++) expect(p[i * 4 + 3]).toBe(255);
  });
});

describe("scalarPixels", () => {
  const world = { w: 3, h: 1 };
  const water = f32(1, 0, 0);

  it("leaves the sea clear", () => {
    expect(px(scalarPixels(world, f32(10, 10, 10), "temperature", water), 0)).toEqual([0, 0, 0, 0]);
  });
  it("maps temperature from -25 °C to 35 °C", () => {
    const p = scalarPixels(world, f32(0, -25, 35), "temperature", water);
    expect(px(p, 1)).toEqual(rgba(TEMP_RAMP[0], 200));
    expect(px(p, 2)).toEqual(rgba(TEMP_RAMP[TEMP_RAMP.length - 1], 200));
  });
  it("leaves tiles with no vegetation, or without the species, clear", () => {
    expect(px(scalarPixels(world, f32(0, -1, 1), "vegetation", water), 1)).toEqual([0, 0, 0, 0]);
    expect(px(scalarPixels(world, f32(0, -1, 1), "alleles", water), 1)).toEqual([0, 0, 0, 0]);
  });
  it("uses the allele ramp for allele frequency and the vegetation ramp otherwise", () => {
    expect(px(scalarPixels(world, f32(0, 0, 1), "alleles", water), 2)).toEqual(rgba(ALLELE_RAMP[ALLELE_RAMP.length - 1], 200));
    expect(px(scalarPixels(world, f32(0, 0, 1), "vegetation", water), 2)).toEqual(rgba(VEG_RAMP[VEG_RAMP.length - 1], 200));
  });
});

describe("demePixels", () => {
  const world = { w: 3, h: 1 };
  const density = f32(0, 10, 1000);
  const dominant = new Uint32Array([0, 7, 9]);
  const colors = new Map<number, RGB>([[7, [10, 20, 30]]]);

  it("leaves empty tiles clear", () => {
    expect(px(demePixels(world, density, dominant, colors, true), 0)[3]).toBe(0);
  });
  it("heat map: the densest tile gets the top colour", () => {
    expect(px(demePixels(world, density, dominant, colors, true), 2)).toEqual([255, 200, 60, 220]);
  });
  it("species map: dominant species colour, more opaque where denser, white if unknown", () => {
    const p = demePixels(world, density, dominant, colors, false);
    expect(px(p, 1).slice(0, 3)).toEqual([10, 20, 30]);
    expect(px(p, 2)).toEqual([255, 255, 255, 180]);
    expect(px(p, 1)[3]).toBeLessThan(180);
  });
});

describe("snowPixels", () => {
  it("draws snow on land and lakes but not the sea, as opaque as it is deep", () => {
    const p = snowPixels({ w: 3, h: 1 }, f32(1, 1, 0.5), f32(1, 2, 0));
    expect(px(p, 0)[3]).toBe(0);
    expect(px(p, 1)).toEqual([244, 247, 252, 225]);
    expect(px(p, 2)[3]).toBe(112); // 112.5, rounded half to even
  });
  it("tolerates a short snow array", () => {
    expect(() => snowPixels({ w: 4, h: 1 }, f32(1), f32(0, 0, 0, 0))).not.toThrow();
  });
});

// Phase 5 web bug 3: panning more than two world widths, or zooming out, left blank strips.
describe("worldCopies", () => {
  const covers = (ox: number, span: number, width: number) => {
    const ks = worldCopies(ox, span, width);
    const left = ox + ks[0] * span;
    const right = ox + (ks[ks.length - 1] + 1) * span;
    return left <= 0 && right >= width;
  };
  it("covers the view however far it is panned", () => {
    for (const ox of [0, 250, -250, 5000, -5000, 123_456, -98_765]) expect(covers(ox, 400, 1000)).toBe(true);
  });
  it("covers the view when zoomed out to a world narrower than the view", () => {
    expect(covers(100, 120, 1000)).toBe(true);
    expect(worldCopies(100, 120, 1000).length).toBeGreaterThanOrEqual(8);
  });
  it("draws no copies wholly off screen", () => {
    expect(worldCopies(0, 1000, 1000).map((k) => k + 0)).toEqual([0]); // k + 0 turns -0 into 0
    expect(worldCopies(-300, 1000, 1000).map((k) => k + 0)).toEqual([0, 1]);
  });
});
