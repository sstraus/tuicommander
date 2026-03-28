import { describe, it, expect } from "vitest";
import { render } from "@solidjs/testing-library";
import { DiffViewer, parseDiff, parseDiffFiles, classifyLine } from "../../components/ui/DiffViewer";

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

describe("classifyLine", () => {
  it("returns header for diff --git lines", () => {
    expect(classifyLine("diff --git a/foo b/foo")).toBe("header");
  });

  it("returns hunk for @@ lines", () => {
    expect(classifyLine("@@ -1,3 +1,4 @@")).toBe("hunk");
  });

  it("returns addition for + lines", () => {
    expect(classifyLine("+new")).toBe("addition");
  });

  it("returns deletion for - lines", () => {
    expect(classifyLine("-old")).toBe("deletion");
  });

  it("returns context for +++ header", () => {
    expect(classifyLine("+++ b/file.ts")).toBe("context");
  });

  it("returns context for --- header", () => {
    expect(classifyLine("--- a/file.ts")).toBe("context");
  });
});

describe("parseDiffFiles", () => {
  it("splits multi-file diff", () => {
    const diff = [
      "diff --git a/a.ts b/a.ts",
      "@@ -1 +1 @@",
      "-old",
      "+new",
      "diff --git a/b.ts b/b.ts",
      "@@ -1 +1 @@",
      "-x",
      "+y",
    ].join("\n");
    const files = parseDiffFiles(diff);
    expect(files).toHaveLength(2);
    expect(files[0].path).toBe("a.ts");
    expect(files[1].path).toBe("b.ts");
  });

  it("counts additions and deletions", () => {
    const diff = "diff --git a/f b/f\n@@ -1 +1,2 @@\n-old\n+new\n+extra";
    const files = parseDiffFiles(diff);
    expect(files[0].additions).toBe(2);
    expect(files[0].deletions).toBe(1);
  });

  it("returns empty for blank diff", () => {
    expect(parseDiffFiles("")).toHaveLength(0);
    expect(parseDiffFiles("  ")).toHaveLength(0);
  });
});

describe("DiffViewer component", () => {
  // Note: @git-diff-view/solid requires Canvas for text measurement,
  // which is not available in jsdom/happy-dom. We test empty states
  // (which don't trigger the library rendering) and verify the
  // component mounts without the library path.

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

  it("renders container with id", () => {
    const { container } = render(() => <DiffViewer diff="" />);
    const el = container.querySelector("#diff-content");
    expect(el).not.toBeNull();
  });
});
