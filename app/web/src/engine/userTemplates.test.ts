import "fake-indexeddb/auto";
import { describe, expect, it } from "vitest";
import { listTemplates, removeTemplate, storeTemplate, templateFileName } from "./userTemplates";

const file = (name: string, seed = 1) =>
  JSON.stringify({ format: "evolutions-template", version: 1, name, description: "", base: "blank", seed, realism: "Accelerated", gen: {}, founders: [] });

describe("user template store", () => {
  it("keeps templates newest first, replaces duplicates and removes them", async () => {
    expect(await listTemplates()).toEqual([]);
    const a = await storeTemplate(file("A"), 100);
    const b = await storeTemplate(file("B"), 200);
    expect((await listTemplates()).map((t) => t.template.name)).toEqual(["B", "A"]);
    const again = await storeTemplate(file("A"), 300);
    expect(again.id).toBe(a.id);
    expect((await listTemplates()).map((t) => t.template.name)).toEqual(["A", "B"]);
    await removeTemplate(b.id);
    expect((await listTemplates()).map((t) => t.template.name)).toEqual(["A"]);
  });
});

describe("templateFileName", () => {
  it("makes a safe file name", () => {
    expect(templateFileName("Lesson 3: Island dwarfism!")).toBe("lesson-3-island-dwarfism.evotemplate");
    expect(templateFileName("Évolution")).toBe("evolution.evotemplate");
    expect(templateFileName("../..")).toBe("template.evotemplate");
  });
});
