import { describe, it, expect } from "vitest";
import { CODING_EXT, filePathRegex, fileUrlRegex } from "../../components/Terminal/linkProvider";

describe("linkProvider regexes", () => {
  describe("filePathRegex", () => {
    function matchAll(text: string): string[] {
      const re = filePathRegex();
      const results: string[] = [];
      let m: RegExpExecArray | null;
      while ((m = re.exec(text)) !== null) results.push(m[1]);
      return results;
    }

    it("matches absolute paths", () => {
      expect(matchAll("open /usr/local/bin/test.rs")).toEqual(["/usr/local/bin/test.rs"]);
    });

    it("matches relative ./ paths", () => {
      expect(matchAll("edit ./src/main.ts")).toEqual(["./src/main.ts"]);
    });

    it("matches relative ../ paths", () => {
      expect(matchAll("see ../lib/utils.py")).toEqual(["../lib/utils.py"]);
    });

    it("matches ~/ home paths", () => {
      expect(matchAll("cat ~/Documents/notes.md")).toEqual(["~/Documents/notes.md"]);
    });

    it("matches word/ relative paths", () => {
      expect(matchAll("src/components/App.tsx is the entry")).toEqual(["src/components/App.tsx"]);
    });

    it("matches :line suffix", () => {
      expect(matchAll("error at /app/main.go:42")).toEqual(["/app/main.go:42"]);
    });

    it("matches :line:col suffix", () => {
      expect(matchAll("/src/index.ts:10:5 error")).toEqual(["/src/index.ts:10:5"]);
    });

    it("matches multiple paths in one line", () => {
      expect(matchAll("/a/b.rs and ./c/d.py")).toEqual(["/a/b.rs", "./c/d.py"]);
    });

    it("does not match bare filenames without path separator", () => {
      expect(matchAll("main.rs")).toEqual([]);
    });

    it("matches paths with @ in segments", () => {
      expect(matchAll("node_modules/@scope/pkg/index.js")).toEqual(["node_modules/@scope/pkg/index.js"]);
    });
  });

  describe("fileUrlRegex", () => {
    function matchAll(text: string): string[] {
      const re = fileUrlRegex();
      const results: string[] = [];
      let m: RegExpExecArray | null;
      while ((m = re.exec(text)) !== null) results.push(m[1]);
      return results;
    }

    it("matches file:///absolute/path", () => {
      expect(matchAll("open file:///home/user/test.rs")).toEqual(["/home/user/test.rs"]);
    });

    it("matches file://absolute/path (bare)", () => {
      expect(matchAll("see file:///tmp/out.log")).toEqual(["/tmp/out.log"]);
    });

    it("does not match file:// without absolute path", () => {
      expect(matchAll("file://relative/path.rs")).toEqual([]);
    });

    it("stops at whitespace", () => {
      expect(matchAll("file:///a/b.txt next")).toEqual(["/a/b.txt"]);
    });
  });

  describe("CODING_EXT", () => {
    it("includes common extensions", () => {
      for (const ext of ["rs", "ts", "tsx", "js", "py", "go", "md", "json", "yaml", "css", "html"]) {
        expect(CODING_EXT).toContain(ext);
      }
    });
  });
});
