import { describe, expect, it } from "vitest";
import { parseClamped } from "./NumberInput";

// Phase 5 web bug 7: number boxes clamped on every keystroke, so deleting "100" left "2".
// The box now keeps what is typed and clamps it only on blur or Enter.
describe("parseClamped", () => {
  it("clamps to the range and rounds", () => {
    expect(parseClamped("100", 5, 2, 50)).toBe(50);
    expect(parseClamped("1", 5, 2, 50)).toBe(2);
    expect(parseClamped("7.6", 5, 2, 50)).toBe(8);
  });
  it("keeps the old value for an empty box or text that isn't a number", () => {
    expect(parseClamped("", 5, 2, 50)).toBe(5);
    expect(parseClamped("  ", 5, 2, 50)).toBe(5);
    expect(parseClamped("abc", 5, 2, 50)).toBe(5);
  });
});
