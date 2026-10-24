import { describe, expect, it } from "vitest";
import { inspectReply } from "./protocol";

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
