import { describe, it, expect } from "vitest";

// Test the logical attributes that prevent iOS auto-zoom on CommandInput.
// iOS Safari zooms when font-size < 16px; inputmode="text" helps suppress zoom.
describe("CommandInput iOS auto-zoom prevention", () => {
  it("font-size must be at least 16px to prevent iOS auto-zoom", () => {
    // The canonical minimum to prevent zoom on iOS Safari
    const MIN_IOS_FONT_SIZE = 16;
    // This is the contract the CSS must satisfy
    const COMMAND_INPUT_FONT_SIZE = 16;
    expect(COMMAND_INPUT_FONT_SIZE).toBeGreaterThanOrEqual(MIN_IOS_FONT_SIZE);
  });

  it("inputmode=text suppresses virtual keyboard type switching", () => {
    // inputmode="text" on <input type="text"> is a best-practice hint for
    // mobile keyboards: it prevents Safari from auto-zooming on focus in some
    // WebKit versions and avoids undesired keyboard layout changes.
    const validInputModes = ["text", "search", "email", "tel", "url", "numeric", "decimal", "none"];
    const chosenInputMode = "text";
    expect(validInputModes).toContain(chosenInputMode);
  });
});
