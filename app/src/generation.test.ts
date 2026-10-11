// Name rules shared with the core (settlements::MAX_NAME_LENGTH, counted in
// characters after trimming).
import { describe, expect, it } from "vitest";
import { MAX_NAME_LENGTH, nameProblem } from "./components/generation";

describe("settlement names", () => {
  it("requires a name and trims spaces", () => {
    expect(nameProblem("   ")).toBe("Enter a name for the settlement.");
    expect(nameProblem("  Ashford  ")).toBeNull();
  });

  it("counts characters as the core does, not UTF-16 units", () => {
    expect(nameProblem("😀".repeat(MAX_NAME_LENGTH))).toBeNull();
    expect(nameProblem("😀".repeat(MAX_NAME_LENGTH + 1))).not.toBeNull();
    expect(nameProblem("x".repeat(MAX_NAME_LENGTH + 1))).not.toBeNull();
  });
});
