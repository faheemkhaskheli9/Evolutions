import { describe, expect, it } from "vitest";
import { chartLabel, chartTarget, verdictLabel } from "./why";

describe("why", () => {
  it("routes chart links", () => {
    expect(chartTarget("trait:insulation")).toEqual({ tab: "species", trait: "insulation" });
    expect(chartTarget("population")).toEqual({ tab: "species" });
    expect(chartTarget("popgen")).toEqual({ tab: "species" });
    expect(chartTarget("tree")).toEqual({ tab: "tree" });
    expect(chartTarget(null)).toBeNull();
    expect(chartTarget("nonsense")).toBeNull();
  });

  it("labels links and verdicts", () => {
    expect(chartLabel("tree")).toBe("Show the tree");
    expect(chartLabel("trait:body_mass")).toBe("Show the trait chart");
    expect(chartLabel("population")).toBe("Show the population chart");
    expect(verdictLabel("drift")).toBe("Consistent with drift");
    expect(verdictLabel("selection")).toBe("Selection");
    expect(verdictLabel(null)).toBeNull();
  });
});
