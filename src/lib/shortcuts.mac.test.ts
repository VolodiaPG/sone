import { describe, expect, it, vi } from "vitest";

vi.mock("./platform", () => ({ isMac: true }));

const { formatCombo } = await import("./shortcuts");

describe("formatCombo on macOS", () => {
  it("uses the macOS glyphs and order", () => {
    expect(
      formatCombo({ code: "KeyK", mod: true, shift: true, alt: true }),
    ).toBe("⌥⇧⌘K");
  });

  it("shows Command-comma for settings", () => {
    expect(
      formatCombo({ code: "Comma", mod: true, shift: false, alt: false }),
    ).toBe("⌘,");
  });
});
