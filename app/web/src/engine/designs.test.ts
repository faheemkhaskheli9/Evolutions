import { describe, expect, it } from "vitest";
import { designFromLibrary, designKey, designsUsed, formatTrait, fromSlider, mergeDesigns, normal, sameDesign, toSlider } from "./designs";
import type { Catalog, SpeciesDesign, TraitMeta } from "./protocol";

const mass: TraitMeta = { key: "body_mass", label: "Adult body mass", unit: "kg", min: 0.01, max: 6000, log: true, default: 5, class: "Morphology" };
const ins: TraitMeta = { key: "insulation", label: "Insulation", unit: "0–1", min: 0.01, max: 0.99, log: false, default: 0.45, class: "Morphology" };

const catalog = {
  species: [{ key: "hare", name: "Steppe hare", description: "", color: [1, 2, 3], traits: { body_mass: 2, insulation: 0.45 } }],
  traits: [mass, ins],
} as unknown as Catalog;

describe("designs", () => {
  it("makes unique keys from names", () => {
    expect(designKey("Snow Grazer!", [])).toBe("my-snow-grazer");
    expect(designKey("Snow grazer", ["my-snow-grazer"])).toBe("my-snow-grazer-2");
    expect(designKey("my hare", [])).toBe("my-hare");
    expect(designKey("???", [])).toBe("my-species");
  });

  it("copies a library species", () => {
    const d = designFromLibrary(catalog, "hare", ["my-steppe-hare"]);
    expect(d.traits).toEqual({ body_mass: 2, insulation: 0.45 });
    expect(d.key).toBe("my-steppe-hare-2");
    expect(d.color).toEqual([1, 2, 3]);
    expect(d.variation).toBe(1);
  });

  it("maps sliders on a log scale for log traits", () => {
    expect(toSlider(mass, 0.01)).toBe(0);
    expect(toSlider(mass, 6000)).toBe(1000);
    expect(fromSlider(mass, toSlider(mass, 2))).toBeCloseTo(2, 1);
    expect(fromSlider(ins, 500)).toBe(0.5);
    expect(toSlider(ins, 2)).toBe(1000);
    expect(formatTrait(mass, 0.05)).toBe("50 g");
    expect(formatTrait(mass, 60)).toBe("60 kg");
    expect(formatTrait(ins, 0.456)).toBe("0.46");
  });

  it("finds, merges and compares designs", () => {
    const a: SpeciesDesign = { key: "my-a", name: "A", color: [0, 0, 0], traits: { z: 1, a: 2 }, variation: 1, description: "" };
    const b: SpeciesDesign = { ...a, key: "my-b" };
    expect(designsUsed([{ species: "my-b", count: 2, diversity: 1 }], [a, b])).toEqual([b]);
    const a2 = { ...a, name: "A2" };
    expect(mergeDesigns([a, b], [a2]).map((d) => d.name)).toEqual(["A", "A2"]);
    expect(Object.keys(normal(a).traits)).toEqual(["a", "z"]);
    expect("description" in normal(a)).toBe(false);
    expect(sameDesign(a, { ...a, traits: { a: 2, z: 1 } })).toBe(true);
    expect(sameDesign(a, a2)).toBe(false);
  });
});
