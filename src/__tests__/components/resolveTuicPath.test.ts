import { describe, it, expect } from "vitest";
import { resolveTuicPath } from "../../components/PluginPanel/resolveTuicPath";

describe("resolveTuicPath", () => {
  // -------------------------------------------------------------------------
  // Basic edge cases
  // -------------------------------------------------------------------------

  it("returns null for empty path", () => {
    expect(resolveTuicPath("", ["/repo"], "/repo")).toBeNull();
  });

  it("returns null for absolute path not under any repo", () => {
    expect(resolveTuicPath("/other/file.md", ["/repo"], "/repo")).toBeNull();
  });

  it("returns null for relative path when no active repo", () => {
    expect(resolveTuicPath("src/foo.ts", ["/repo"], null)).toBeNull();
  });

  // -------------------------------------------------------------------------
  // Unix absolute paths
  // -------------------------------------------------------------------------

  describe("Unix absolute paths", () => {
    it("resolves absolute path under a registered repo", () => {
      const result = resolveTuicPath("/repo/src/foo.ts", ["/repo"], null);
      expect(result).toEqual({ repoPath: "/repo", relPath: "src/foo.ts" });
    });

    it("picks the longest matching repo", () => {
      const repos = ["/Gits", "/Gits/alpha"];
      const result = resolveTuicPath("/Gits/alpha/src/lib.rs", repos, null);
      expect(result).toEqual({ repoPath: "/Gits/alpha", relPath: "src/lib.rs" });
    });

    it("does not match repo that merely shares a name prefix", () => {
      const result = resolveTuicPath("/repo-fork/src/foo.ts", ["/repo"], "/repo");
      expect(result).toBeNull();
    });

    it("returns empty relPath when path equals repo root", () => {
      const result = resolveTuicPath("/repo", ["/repo"], null);
      expect(result).toEqual({ repoPath: "/repo", relPath: "" });
    });
  });

  // -------------------------------------------------------------------------
  // Unix relative paths
  // -------------------------------------------------------------------------

  describe("Unix relative paths", () => {
    it("resolves relative path against active repo", () => {
      const result = resolveTuicPath("src/foo.ts", ["/repo"], "/repo");
      expect(result).toEqual({ repoPath: "/repo", relPath: "src/foo.ts" });
    });

    it("resolves ./relative path", () => {
      const result = resolveTuicPath("./src/foo.ts", ["/repo"], "/repo");
      expect(result).toEqual({ repoPath: "/repo", relPath: "src/foo.ts" });
    });

    it("resolves parent traversal that stays within repo", () => {
      const result = resolveTuicPath("src/../lib/bar.ts", ["/repo"], "/repo");
      expect(result).toEqual({ repoPath: "/repo", relPath: "lib/bar.ts" });
    });

    it("rejects parent traversal that escapes repo root", () => {
      const result = resolveTuicPath("../../etc/passwd", ["/repo"], "/repo");
      expect(result).toBeNull();
    });
  });

  // -------------------------------------------------------------------------
  // Windows absolute paths
  // -------------------------------------------------------------------------

  describe("Windows absolute paths", () => {
    it("resolves Windows drive-letter path under a registered repo", () => {
      const repos = ["C:\\DATA\\repos\\arcane"];
      const result = resolveTuicPath("C:\\DATA\\repos\\arcane\\src\\main.rs", repos, null);
      expect(result).toEqual({ repoPath: "C:\\DATA\\repos\\arcane", relPath: "src/main.rs" });
    });

    it("resolves Windows path with forward slashes under backslash repo", () => {
      const repos = ["C:\\DATA\\repos\\arcane"];
      const result = resolveTuicPath("C:/DATA/repos/arcane/src/main.rs", repos, null);
      expect(result).toEqual({ repoPath: "C:\\DATA\\repos\\arcane", relPath: "src/main.rs" });
    });

    it("picks longest matching Windows repo", () => {
      const repos = ["C:\\DATA", "C:\\DATA\\repos\\arcane"];
      const result = resolveTuicPath("C:\\DATA\\repos\\arcane\\lib\\util.ts", repos, null);
      expect(result).toEqual({ repoPath: "C:\\DATA\\repos\\arcane", relPath: "lib/util.ts" });
    });

    it("does not match Windows repo that merely shares a name prefix", () => {
      const repos = ["C:\\DATA\\repos\\arcane"];
      const result = resolveTuicPath("C:\\DATA\\repos\\arcane-fork\\src\\foo.ts", repos, null);
      expect(result).toBeNull();
    });

    it("returns empty relPath when Windows path equals repo root", () => {
      const repos = ["C:\\DATA\\repos\\arcane"];
      const result = resolveTuicPath("C:\\DATA\\repos\\arcane", repos, null);
      expect(result).toEqual({ repoPath: "C:\\DATA\\repos\\arcane", relPath: "" });
    });

    it("handles different drive letters", () => {
      const repos = ["C:\\repos\\alpha", "D:\\repos\\beta"];
      expect(resolveTuicPath("D:\\repos\\beta\\src\\lib.rs", repos, null)).toEqual({
        repoPath: "D:\\repos\\beta",
        relPath: "src/lib.rs",
      });
      expect(resolveTuicPath("E:\\repos\\gamma\\foo.ts", repos, null)).toBeNull();
    });
  });

  // -------------------------------------------------------------------------
  // Windows relative paths
  // -------------------------------------------------------------------------

  describe("Windows relative paths", () => {
    it("resolves relative path against Windows active repo", () => {
      const result = resolveTuicPath("src\\main.rs", ["C:\\DATA\\repos\\arcane"], "C:\\DATA\\repos\\arcane");
      expect(result).toEqual({ repoPath: "C:\\DATA\\repos\\arcane", relPath: "src/main.rs" });
    });

    it("resolves forward-slash relative against Windows active repo", () => {
      const result = resolveTuicPath("src/main.rs", ["C:\\DATA\\repos\\arcane"], "C:\\DATA\\repos\\arcane");
      expect(result).toEqual({ repoPath: "C:\\DATA\\repos\\arcane", relPath: "src/main.rs" });
    });

    it("rejects parent traversal that escapes Windows repo root", () => {
      const result = resolveTuicPath("..\\..\\etc\\passwd", ["C:\\DATA\\repos\\arcane"], "C:\\DATA\\repos\\arcane");
      expect(result).toBeNull();
    });
  });

  // -------------------------------------------------------------------------
  // UNC paths
  // -------------------------------------------------------------------------

  describe("UNC paths", () => {
    it("treats UNC path as absolute", () => {
      const repos = ["\\\\server\\share\\repo"];
      const result = resolveTuicPath("\\\\server\\share\\repo\\src\\foo.ts", repos, null);
      expect(result).toEqual({ repoPath: "\\\\server\\share\\repo", relPath: "src/foo.ts" });
    });
  });
});
