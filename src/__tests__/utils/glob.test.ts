import { describe, it, expect } from "vitest";
import { globToRegex } from "../../utils/glob";

describe("globToRegex", () => {
  describe("* wildcard (matches anything except /)", () => {
    it("matches a filename segment", () => {
      const re = globToRegex("*.ts");
      expect(re.test("foo.ts")).toBe(true);
    });

    it("does not match path separators", () => {
      const re = globToRegex("src/*.ts");
      expect(re.test("src/foo.ts")).toBe(true);
      expect(re.test("src/sub/foo.ts")).toBe(false);
    });

    it("matches empty string (zero chars)", () => {
      const re = globToRegex("*.ts");
      expect(re.test(".ts")).toBe(true);
    });

    it("* matches zero or more non-slash characters in a segment", () => {
      const re = globToRegex("src/*.ts");
      // Must have exactly one segment between src/ and .ts
      expect(re.test("src/foo.ts")).toBe(true);
      expect(re.test("src/foo/bar.ts")).toBe(false);
    });
  });

  describe("** wildcard (matches any path including /)", () => {
    it("matches across multiple path segments", () => {
      const re = globToRegex("src/**/*.ts");
      expect(re.test("src/foo.ts")).toBe(true);
      expect(re.test("src/a/b/c/foo.ts")).toBe(true);
    });

    it("matches without trailing segment too", () => {
      const re = globToRegex("**/*.ts");
      expect(re.test("foo.ts")).toBe(true);
      expect(re.test("a/b/foo.ts")).toBe(true);
    });
  });

  describe("? wildcard (matches single non-/ char)", () => {
    it("matches exactly one character", () => {
      const re = globToRegex("foo?.ts");
      expect(re.test("fooX.ts")).toBe(true);
      expect(re.test("foo.ts")).toBe(false);
      expect(re.test("fooXY.ts")).toBe(false);
    });

    it("does not match path separator", () => {
      const re = globToRegex("src/?oo.ts");
      expect(re.test("src/foo.ts")).toBe(true);
      expect(re.test("src//oo.ts")).toBe(false);
    });
  });

  describe("regex special character escaping", () => {
    it("escapes dots literally", () => {
      const re = globToRegex("foo.ts");
      expect(re.test("footts")).toBe(false);
      expect(re.test("foo.ts")).toBe(true);
    });

    it("escapes + literally", () => {
      const re = globToRegex("a+b.ts");
      expect(re.test("a+b.ts")).toBe(true);
      expect(re.test("aab.ts")).toBe(false);
    });

    it("escapes square brackets literally", () => {
      const re = globToRegex("[abc].ts");
      expect(re.test("[abc].ts")).toBe(true);
      expect(re.test("a.ts")).toBe(false);
    });

    it("escapes parentheses literally", () => {
      const re = globToRegex("(test).ts");
      expect(re.test("(test).ts")).toBe(true);
      expect(re.test("test.ts")).toBe(false);
    });

    it("escapes backslash literally", () => {
      const re = globToRegex("a\\b");
      expect(re.test("a\\b")).toBe(true);
    });
  });

  describe("case insensitivity", () => {
    it("matches case-insensitively", () => {
      const re = globToRegex("*.TS");
      expect(re.test("foo.ts")).toBe(true);
      expect(re.test("foo.Ts")).toBe(true);
    });
  });

  describe("combined patterns", () => {
    it("handles complex pattern with multiple wildcards", () => {
      const re = globToRegex("src/**/__tests__/*.test.ts");
      expect(re.test("src/__tests__/foo.test.ts")).toBe(true);
      expect(re.test("src/components/__tests__/bar.test.ts")).toBe(true);
      expect(re.test("src/__tests__/foo.spec.ts")).toBe(false);
    });
  });
});
