import { describe, it, expect } from "vitest";
import { pathBasename, pathDirname, pathParts, replaceBasename } from "../../utils/pathUtils";

describe("pathBasename", () => {
  it("extracts basename from Unix paths", () => {
    expect(pathBasename("/foo/bar/baz.txt")).toBe("baz.txt");
    expect(pathBasename("docs/readme.md")).toBe("readme.md");
    expect(pathBasename("file.txt")).toBe("file.txt");
  });

  it("extracts basename from Windows paths", () => {
    expect(pathBasename("C:\\Users\\foo\\bar.txt")).toBe("bar.txt");
    expect(pathBasename("docs\\readme.md")).toBe("readme.md");
  });

  it("handles mixed separators", () => {
    expect(pathBasename("foo/bar\\baz.txt")).toBe("baz.txt");
  });

  it("handles empty/root paths", () => {
    expect(pathBasename("")).toBe("");
    expect(pathBasename("/")).toBe("");
  });
});

describe("pathDirname", () => {
  it("extracts directory from Unix paths", () => {
    expect(pathDirname("/foo/bar/baz.txt")).toBe("/foo/bar");
    expect(pathDirname("docs/readme.md")).toBe("docs");
  });

  it("extracts directory from Windows paths", () => {
    expect(pathDirname("C:\\Users\\foo\\bar.txt")).toBe("C:\\Users\\foo");
    expect(pathDirname("docs\\readme.md")).toBe("docs");
  });

  it("returns empty for files without directory", () => {
    expect(pathDirname("file.txt")).toBe("");
    expect(pathDirname("")).toBe("");
  });
});

describe("pathParts", () => {
  it("splits Unix paths", () => {
    expect(pathParts("foo/bar/baz")).toEqual(["foo", "bar", "baz"]);
  });

  it("splits Windows paths", () => {
    expect(pathParts("foo\\bar\\baz")).toEqual(["foo", "bar", "baz"]);
  });

  it("handles single segment", () => {
    expect(pathParts("file.txt")).toEqual(["file.txt"]);
  });
});

describe("replaceBasename", () => {
  it("replaces last segment in Unix path", () => {
    expect(replaceBasename("docs/old.md", "new.md")).toBe("docs/new.md");
  });

  it("replaces last segment in Windows path", () => {
    expect(replaceBasename("docs\\old.md", "new.md")).toBe("docs\\new.md");
  });

  it("handles single-segment path", () => {
    expect(replaceBasename("old.md", "new.md")).toBe("new.md");
  });
});
