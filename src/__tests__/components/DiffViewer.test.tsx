import { describe, it, expect } from "vitest";
import { render } from "@solidjs/testing-library";
import { DiffViewer, parseDiff } from "../../components/ui/DiffViewer";

describe("parseDiff", () => {
  it("classifies addition lines", () => {
    const lines = parseDiff("+added line");
    expect(lines[0].type).toBe("addition");
  });

  it("classifies deletion lines", () => {
    const lines = parseDiff("-removed line");
    expect(lines[0].type).toBe("deletion");
  });

  it("classifies header lines", () => {
    const lines = parseDiff("diff --git a/foo b/foo");
    expect(lines[0].type).toBe("header");
  });

  it("classifies hunk lines", () => {
    const lines = parseDiff("@@ -1,3 +1,4 @@");
    expect(lines[0].type).toBe("hunk");
  });

  it("classifies context lines", () => {
    const lines = parseDiff("unchanged context line");
    expect(lines[0].type).toBe("context");
  });

  it("does not classify +++ as addition", () => {
    const lines = parseDiff("+++ b/file.ts");
    expect(lines[0].type).toBe("context");
  });

  it("does not classify --- as deletion", () => {
    const lines = parseDiff("--- a/file.ts");
    expect(lines[0].type).toBe("context");
  });
});

describe("DiffViewer", () => {
  it("renders diff lines with correct CSS classes", () => {
    const diff = "diff --git a/f b/f\n@@ -1 +1 @@\n-old\n+new\n context";
    const { container } = render(() => <DiffViewer diff={diff} />);
    const lines = container.querySelectorAll(".diff-line");
    expect(lines.length).toBe(5);
    expect(lines[0].classList.contains("header")).toBe(true);
    expect(lines[1].classList.contains("hunk")).toBe(true);
    expect(lines[2].classList.contains("deletion")).toBe(true);
    expect(lines[3].classList.contains("addition")).toBe(true);
    expect(lines[4].classList.contains("context")).toBe(true);
  });

  it("shows empty message when diff is empty", () => {
    const { container } = render(() => <DiffViewer diff="" />);
    const empty = container.querySelector(".diff-empty");
    expect(empty).not.toBeNull();
    expect(empty!.textContent).toBe("No changes");
  });

  it("shows custom empty message", () => {
    const { container } = render(() => (
      <DiffViewer diff="  " emptyMessage="Nothing to show" />
    ));
    const empty = container.querySelector(".diff-empty");
    expect(empty!.textContent).toBe("Nothing to show");
  });

  it("renders diff content text", () => {
    const { container } = render(() => <DiffViewer diff="+hello world" />);
    const line = container.querySelector(".diff-line.addition");
    expect(line).not.toBeNull();
    expect(line!.textContent).toBe("+hello world");
  });
});
