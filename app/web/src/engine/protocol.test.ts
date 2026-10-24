import { describe, expect, it } from "vitest";
import { exportFiles, inspectReply } from "./protocol";

// Phase 5 web bug 2: after New world or Open, queued Inspector requests for the old animal
// were answered by the new engine, showing an unrelated animal with the same id.
describe("inspectReply", () => {
  const data = { x: 1, y: 2 };

  it("accepts a reply for the animal being inspected", () => {
    expect(inspectReply(7, { id: 7, data })).toBe("update");
  });
  it("ignores a reply for another animal", () => {
    expect(inspectReply(7, { id: 3, data })).toBe("stale");
  });
  it("ignores replies once nothing is inspected (a new world clears the inspector)", () => {
    expect(inspectReply(null, { id: 7, data })).toBe("stale");
    expect(inspectReply(null, { id: 7, data: null })).toBe("stale");
  });
  it("reports an animal that died or left the simulated area", () => {
    expect(inspectReply(7, { id: 7, data: null })).toBe("gone");
  });
});

describe("exportFiles", () => {
  const engine = {
    species_csv_files: () =>
      JSON.stringify([
        { name: "species_1_Ovis_a.csv", text: "year,n\n0,10\n" },
        { name: "species_12_Ovis_b.csv", text: "year,n\n0,3\n" },
      ]),
    chronicle_csv: () => "year,kind,species,species_name,text\n",
    newick: () => "(A:1.0);",
    settings_json: () => "{}",
  };

  it("exports every species' time series as CSV", () => {
    const files = exportFiles(engine, "species");
    expect(files.map((f) => f.name)).toEqual(["species_1_Ovis_a.csv", "species_12_Ovis_b.csv"]);
    expect(files.every((f) => f.mime === "text/csv")).toBe(true);
  });
  it("picks one species by id, not by id prefix", () => {
    expect(exportFiles(engine, "species", 1).map((f) => f.name)).toEqual(["species_1_Ovis_a.csv"]);
    expect(exportFiles(engine, "species", 12).map((f) => f.name)).toEqual(["species_12_Ovis_b.csv"]);
  });
  it("names the other files", () => {
    expect(exportFiles(engine, "chronicle")[0]).toMatchObject({ name: "chronicle.csv", mime: "text/csv" });
    expect(exportFiles(engine, "newick")[0]).toEqual({ name: "phylogeny.nwk", text: "(A:1.0);", mime: "text/plain" });
    expect(exportFiles(engine, "settings")[0]).toMatchObject({ name: "settings.json", mime: "application/json" });
  });
});
