import { describe, it, expect } from "vitest";
import { resolveTuicPath } from "../resolveTuicPath";

describe("resolveTuicPath", () => {
  const repos = ["/Users/me/code/repoA", "/Users/me/code/repoB"];

  describe("absolute paths (backward compatible)", () => {
    it("resolves an absolute path inside a known repo", () => {
      const result = resolveTuicPath("/Users/me/code/repoA/src/main.ts", repos, null);
      expect(result).toEqual({
        repoPath: "/Users/me/code/repoA",
        relPath: "src/main.ts",
      });
    });

    it("resolves an absolute path matching repo root exactly", () => {
      const result = resolveTuicPath("/Users/me/code/repoA", repos, null);
      expect(result).toEqual({
        repoPath: "/Users/me/code/repoA",
        relPath: "",
      });
    });

    it("returns null for an absolute path not in any repo", () => {
      const result = resolveTuicPath("/unknown/path/file.ts", repos, null);
      expect(result).toBeNull();
    });
  });

  describe("relative paths — resolved against active repo", () => {
    it("resolves a simple relative path", () => {
      const result = resolveTuicPath("README.md", repos, "/Users/me/code/repoA");
      expect(result).toEqual({
        repoPath: "/Users/me/code/repoA",
        relPath: "README.md",
      });
    });

    it("resolves a nested relative path", () => {
      const result = resolveTuicPath("src/App.tsx", repos, "/Users/me/code/repoB");
      expect(result).toEqual({
        repoPath: "/Users/me/code/repoB",
        relPath: "src/App.tsx",
      });
    });

    it("returns null when no active repo is set", () => {
      const result = resolveTuicPath("README.md", repos, null);
      expect(result).toBeNull();
    });
  });

  describe("path traversal guard", () => {
    it("blocks ../ that escapes repo root", () => {
      const result = resolveTuicPath("../../../etc/passwd", repos, "/Users/me/code/repoA");
      expect(result).toBeNull();
    });

    it("blocks ../ at the start", () => {
      const result = resolveTuicPath("../other/file.ts", repos, "/Users/me/code/repoA");
      expect(result).toBeNull();
    });

    it("blocks mid-path traversal that escapes root", () => {
      const result = resolveTuicPath("src/../../secret.txt", repos, "/Users/me/code/repoA");
      expect(result).toBeNull();
    });

    it("allows ../ that stays within repo root", () => {
      const result = resolveTuicPath("src/../README.md", repos, "/Users/me/code/repoA");
      expect(result).toEqual({
        repoPath: "/Users/me/code/repoA",
        relPath: "README.md",
      });
    });

    it("allows ./relative paths", () => {
      const result = resolveTuicPath("./src/main.ts", repos, "/Users/me/code/repoA");
      expect(result).toEqual({
        repoPath: "/Users/me/code/repoA",
        relPath: "src/main.ts",
      });
    });
  });

  describe("edge cases", () => {
    it("returns null for empty path", () => {
      expect(resolveTuicPath("", repos, "/Users/me/code/repoA")).toBeNull();
    });

    it("handles trailing slashes in relative paths", () => {
      const result = resolveTuicPath("src/", repos, "/Users/me/code/repoA");
      expect(result).toEqual({
        repoPath: "/Users/me/code/repoA",
        relPath: "src",
      });
    });

    it("picks the longest matching repo for absolute paths", () => {
      const nestedRepos = ["/Users/me/code", "/Users/me/code/repoA"];
      const result = resolveTuicPath("/Users/me/code/repoA/file.ts", nestedRepos, null);
      expect(result).toEqual({
        repoPath: "/Users/me/code/repoA",
        relPath: "file.ts",
      });
    });
  });
});
