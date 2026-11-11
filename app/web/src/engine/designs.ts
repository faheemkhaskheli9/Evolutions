// Designed species (PRD FR-S2): helpers for the species designer, and the designs kept
// in this browser ("My species"). The engine checks every design; nothing here decides
// whether a design is possible.

import type { Catalog, FounderGroup, SpeciesDesign, TraitMeta } from "./protocol";

const STORE = "evolutions-designs";

/** Slider positions run from 0 to this. */
export const SLIDER_STEPS = 1000;

/** A key made from a name: lowercase letters, digits and dashes, starting "my-", not in `taken`. */
export function designKey(name: string, taken: Iterable<string>): string {
  const used = new Set(taken);
  const slug =
    name
      .normalize("NFKD")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 30) || "species";
  const base = slug.startsWith("my-") ? slug : `my-${slug}`;
  if (!used.has(base)) return base;
  for (let i = 2; ; i++) if (!used.has(`${base}-${i}`)) return `${base}-${i}`;
}

/** A design with a library species' values, named and keyed so it does not clash with `taken`. */
export function designFromLibrary(catalog: Catalog, key: string, taken: Iterable<string>): SpeciesDesign {
  const s = catalog.species.find((x) => x.key === key) ?? catalog.species[0];
  const traits: Record<string, number> = {};
  for (const t of catalog.traits) traits[t.key] = s?.traits?.[t.key] ?? t.default;
  const name = `My ${s?.name.toLowerCase() ?? "species"}`;
  return { key: designKey(name, taken), name, description: "", color: s?.color ?? [150, 150, 150], traits, variation: 1 };
}

/** Slider position (0–SLIDER_STEPS) of a trait value: log scale for `log` traits. */
export function toSlider(t: TraitMeta, v: number): number {
  const x = Math.min(t.max, Math.max(t.min, v));
  const f = t.log ? Math.log(x / t.min) / Math.log(t.max / t.min) : (x - t.min) / (t.max - t.min);
  return Math.round(f * SLIDER_STEPS);
}

/** The trait value at a slider position, rounded to three significant digits. */
export function fromSlider(t: TraitMeta, pos: number): number {
  const f = Math.min(1, Math.max(0, pos / SLIDER_STEPS));
  const v = t.log ? t.min * Math.pow(t.max / t.min, f) : t.min + f * (t.max - t.min);
  return Math.min(t.max, Math.max(t.min, Number(v.toPrecision(3))));
}

/** A trait value for display, with its unit. */
export function formatTrait(t: TraitMeta, v: number): string {
  if (t.key === "body_mass") return v < 1 ? `${Math.round(v * 1000)} g` : `${Number(v.toPrecision(3))} kg`;
  if (t.unit === "0–1") return v.toFixed(2);
  return `${Number(v.toPrecision(3))} ${t.unit}`;
}

/** The designs the founder groups use. */
export function designsUsed(founders: FounderGroup[], designs: SpeciesDesign[]): SpeciesDesign[] {
  return designs.filter((d) => founders.some((f) => f.species === d.key));
}

/** `a` with `b`'s designs added; a design in `b` replaces one in `a` with the same key. */
export function mergeDesigns(a: SpeciesDesign[], b: SpeciesDesign[]): SpeciesDesign[] {
  return [...a.filter((d) => !b.some((e) => e.key === d.key)), ...b];
}

/** Whether two designs are the same species (the engine refuses two different designs with one key). */
export const sameDesign = (a: SpeciesDesign, b: SpeciesDesign) => JSON.stringify(normal(a)) === JSON.stringify(normal(b));

/** A design in the engine's normal form: fields in order, trait keys sorted, no empty description. */
export function normal(d: SpeciesDesign): SpeciesDesign {
  const traits: Record<string, number> = {};
  for (const k of Object.keys(d.traits).sort()) traits[k] = d.traits[k];
  const out: SpeciesDesign = { key: d.key, name: d.name, color: d.color, traits, variation: d.variation };
  if (d.description) out.description = d.description;
  return out;
}

/** The designs kept in this browser; none if storage is unavailable. */
export function loadDesigns(): SpeciesDesign[] {
  try {
    const v = JSON.parse(localStorage.getItem(STORE) ?? "[]") as unknown;
    return Array.isArray(v) ? (v as SpeciesDesign[]).filter((d) => d && typeof d.key === "string" && d.traits) : [];
  } catch {
    return [];
  }
}

export function storeDesigns(list: SpeciesDesign[]) {
  try {
    localStorage.setItem(STORE, JSON.stringify(list));
  } catch {
    // Storage is full or blocked: the designs last until the page is closed.
  }
}
