// Map images shared by the flat map and the 3D world: one pixel per tile.
import type { WorldInfo } from "../engine/protocol";
import { ALLELE_RAMP, BIOME_COLORS, RAIN_RAMP, ramp, TEMP_RAMP, VEG_RAMP, type RGB } from "../util";

export type Layer = "terrain" | "temperature" | "rainfall" | "vegetation" | "population" | "alleles";

/**
 * Terrain image: biome colours with hillshading, water by depth. With `rivers` false,
 * river tiles show the land they run through (the 3D world draws rivers as water).
 */
export function buildTerrain(world: WorldInfo, elevation: Float32Array, biome: Float32Array, water: Float32Array, rivers = true) {
  const { w, h } = world;
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  const img = ctx.createImageData(w, h);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      let col: RGB;
      const wt = water[i];
      if (wt === 1) {
        const depth = Math.min(1, -elevation[i] / 4000);
        col = ramp([[64, 120, 170], [22, 52, 98]], depth);
      } else if (wt === 2 || (wt === 3 && rivers)) {
        col = [62, 122, 178];
      } else {
        col = BIOME_COLORS[biome[i] | 0] ?? [120, 120, 120];
        // Relief shading against neighbouring land (the sea floor would exaggerate coasts).
        const e = elevation[i];
        const ex = Math.max(0, elevation[y * w + ((x + 1) % w)]);
        const ey = Math.max(0, elevation[Math.min(h - 1, y + 1) * w + x]);
        const shade = Math.max(-0.35, Math.min(0.35, ((e - ex) + (e - ey)) / 900));
        const high = Math.min(0.35, Math.max(0, (e - 2000) / 5000));
        col = [
          col[0] * (1 + shade) + (255 - col[0]) * high,
          col[1] * (1 + shade) + (255 - col[1]) * high,
          col[2] * (1 + shade) + (255 - col[2]) * high,
        ];
      }
      img.data[i * 4] = col[0];
      img.data[i * 4 + 1] = col[1];
      img.data[i * 4 + 2] = col[2];
      img.data[i * 4 + 3] = 255;
    }
  }
  ctx.putImageData(img, 0, 0);
  return c;
}

export function buildScalar(world: WorldInfo, data: Float32Array, layer: Layer, water: Float32Array) {
  const { w, h } = world;
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  const img = ctx.createImageData(w, h);
  for (let i = 0; i < w * h; i++) {
    if (water[i] === 1) continue;
    const v = data[i];
    let col: RGB;
    if (layer === "temperature") col = ramp(TEMP_RAMP, (v + 25) / 60);
    else if (layer === "rainfall") col = ramp(RAIN_RAMP, Math.sqrt(Math.max(0, v) / 3500));
    else {
      // Vegetation: -1 where nothing grows. Allele frequency: -1 where the species is absent.
      if (v < 0) continue;
      col = ramp(layer === "alleles" ? ALLELE_RAMP : VEG_RAMP, v);
    }
    img.data[i * 4] = col[0];
    img.data[i * 4 + 1] = col[1];
    img.data[i * 4 + 2] = col[2];
    img.data[i * 4 + 3] = 200;
  }
  ctx.putImageData(img, 0, 0);
  return c;
}

export function buildDemes(world: WorldInfo, density: Float32Array, dominant: Uint32Array, colors: Map<number, RGB>, heat: boolean) {
  const { w, h } = world;
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  const img = ctx.createImageData(w, h);
  let max = 0;
  for (let i = 0; i < density.length; i++) max = Math.max(max, density[i]);
  const lmax = Math.log1p(max) || 1;
  for (let i = 0; i < w * h; i++) {
    const d = density[i];
    if (d <= 0) continue;
    const t = Math.log1p(d) / lmax;
    const col: RGB = heat ? ramp([[30, 30, 80], [180, 40, 120], [255, 200, 60]], t) : colors.get(dominant[i]) ?? [255, 255, 255];
    img.data[i * 4] = col[0];
    img.data[i * 4 + 1] = col[1];
    img.data[i * 4 + 2] = col[2];
    img.data[i * 4 + 3] = heat ? 220 : 50 + 130 * t;
  }
  ctx.putImageData(img, 0, 0);
  return c;
}

/** Snow lying on land and lakes (not the open sea): white, as opaque as the cover is deep. */
export function buildSnow(world: WorldInfo, snow: Float32Array, water: Float32Array) {
  const { w, h } = world;
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  const img = ctx.createImageData(w, h);
  for (let i = 0; i < Math.min(w * h, snow.length); i++) {
    if (water[i] === 1 || snow[i] <= 0) continue;
    img.data[i * 4] = 244;
    img.data[i * 4 + 1] = 247;
    img.data[i * 4 + 2] = 252;
    img.data[i * 4 + 3] = 225 * snow[i];
  }
  ctx.putImageData(img, 0, 0);
  return c;
}
