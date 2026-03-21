import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const tsx = readFileSync(
  resolve(__dirname, "../components/QuestionBanner.tsx"),
  "utf-8",
);

const css = readFileSync(
  resolve(__dirname, "../components/QuestionBanner.module.css"),
  "utf-8",
);

describe("QuestionBanner is tap-to-navigate only (no Yes/No buttons)", () => {
  it("does not contain Yes/No reply buttons", () => {
    // The banner should be a simple notification that navigates to the terminal.
    // Yes/No buttons are useless: the question is truncated and multi-choice
    // prompts can't be answered with Yes/No.
    expect(tsx).not.toContain("yesBtn");
    expect(tsx).not.toContain("noBtn");
    expect(tsx).not.toContain('"yes"');
    expect(tsx).not.toContain('"no"');
  });

  it("does not contain sendReply function", () => {
    expect(tsx).not.toContain("sendReply");
  });

  it("entire banner navigates to the terminal on tap", () => {
    // The banner item should call onNavigate when tapped
    expect(tsx).toContain("onNavigate");
  });

  it("CSS does not contain Yes/No button styles", () => {
    expect(css).not.toContain(".yesBtn");
    expect(css).not.toContain(".noBtn");
  });
});
