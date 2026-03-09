import { describe, it, expect } from "vitest";
import { classifyDroppedFile } from "../useFileDrop";

describe("classifyDroppedFile", () => {
  it("classifies .md files as markdown", () => {
    expect(classifyDroppedFile("/Users/me/docs/README.md")).toBe("markdown");
  });

  it("classifies .mdx files as markdown", () => {
    expect(classifyDroppedFile("/Users/me/docs/page.mdx")).toBe("markdown");
  });

  it("classifies .MD (uppercase) as markdown", () => {
    expect(classifyDroppedFile("/Users/me/CHANGELOG.MD")).toBe("markdown");
  });

  it("classifies .ts files as editor", () => {
    expect(classifyDroppedFile("/Users/me/src/index.ts")).toBe("editor");
  });

  it("classifies .json files as editor", () => {
    expect(classifyDroppedFile("/Users/me/package.json")).toBe("editor");
  });

  it("classifies files without extension as editor", () => {
    expect(classifyDroppedFile("/Users/me/Makefile")).toBe("editor");
  });

  it("classifies dotfiles as editor", () => {
    expect(classifyDroppedFile("/Users/me/.gitignore")).toBe("editor");
  });

  it("classifies .MDX (uppercase) as markdown", () => {
    expect(classifyDroppedFile("/Users/me/page.MDX")).toBe("markdown");
  });
});
