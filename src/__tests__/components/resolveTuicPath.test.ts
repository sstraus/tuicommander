import { describe, expect, it } from "vitest";
import { resolveTuicPath } from "../../components/PluginPanel/resolveTuicPath";

describe("resolveTuicPath", () => {
	// -------------------------------------------------------------------------
	// Basic edge cases
	// -------------------------------------------------------------------------

	it("returns null for empty path", () => {
		expect(resolveTuicPath("", ["/repo"], "/repo")).toBeNull();
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
			const result = resolveTuicPath("/repo-fork/src/foo.ts", ["/repo"], null);
			expect(result).toEqual({ repoPath: "", relPath: "/repo-fork/src/foo.ts", external: true });
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

		it("rejects mid-path traversal that escapes repo root", () => {
			const result = resolveTuicPath("src/../../secret.txt", ["/repo"], "/repo");
			expect(result).toBeNull();
		});

		it("drops the trailing slash of a directory path", () => {
			const result = resolveTuicPath("src/", ["/repo"], "/repo");
			expect(result).toEqual({ repoPath: "/repo", relPath: "src" });
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
			expect(result).toMatchObject({ external: true });
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
			expect(resolveTuicPath("E:\\repos\\gamma\\foo.ts", repos, null)).toMatchObject({ external: true });
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

	// -------------------------------------------------------------------------
	// Absolute paths outside every registered repo (#2666-17b6)
	// -------------------------------------------------------------------------

	describe("external absolute paths", () => {
		// Catches: owner report cannot open an accessible external reference file.
		it("plugin_panel_absolute_external_file_opens_and_edits: resolves to an external file owned by the asking panel's repo", () => {
			expect(resolveTuicPath("/other/file.md", ["/repo"], "/repo")).toEqual({
				repoPath: "/repo",
				relPath: "/other/file.md",
				external: true,
			});
		});

		it("works with no focused repo and no panel repo", () => {
			expect(resolveTuicPath("/other/file.md", [], null)).toEqual({
				repoPath: "",
				relPath: "/other/file.md",
				external: true,
			});
		});

		// Catches: downloaded panel-relative content opens another repo after focus changes.
		it("panel_relative_link_cannot_follow_foreign_focused_repo: the base stays the asking panel's repo", () => {
			expect(resolveTuicPath("docs/a.md", ["/panel", "/focused"], "/panel")).toEqual({
				repoPath: "/panel",
				relPath: "docs/a.md",
			});
			expect(resolveTuicPath("/elsewhere/a.md", ["/panel", "/focused"], "/panel")).toMatchObject({
				repoPath: "/panel",
				external: true,
			});
		});

		it("does not mark a path under a registered repo as external", () => {
			expect(resolveTuicPath("/repo/a.md", ["/repo"], null)).not.toHaveProperty("external");
		});

		// Catches: "/repo/../etc/x" passing the prefix check and opening as an in-repo file.
		it("normalizes traversal before classifying", () => {
			expect(resolveTuicPath("/repo/../etc/x.md", ["/repo"], "/repo")).toEqual({
				repoPath: "/repo",
				relPath: "/etc/x.md",
				external: true,
			});
			expect(resolveTuicPath("/other/../repo/a.md", ["/repo"], null)).toEqual({
				repoPath: "/repo",
				relPath: "a.md",
			});
		});

		it("resolves Windows drive paths outside the repo, keeping backslashes when no dot segment is present", () => {
			expect(resolveTuicPath("D:\\ref\\a.md", ["C:\\repo"], "C:\\repo")).toEqual({
				repoPath: "C:\\repo",
				relPath: "D:\\ref\\a.md",
				external: true,
			});
			expect(resolveTuicPath("C:\\repo\\..\\ref\\a.md", ["C:\\repo"], "C:\\repo")).toEqual({
				repoPath: "C:\\repo",
				relPath: "C:/ref/a.md",
				external: true,
			});
		});

		it("resolves UNC paths outside the repo without dropping the share root", () => {
			expect(resolveTuicPath("\\\\srv\\share\\a.md", ["C:\\repo"], "C:\\repo")).toEqual({
				repoPath: "C:\\repo",
				relPath: "\\\\srv\\share\\a.md",
				external: true,
			});
			expect(resolveTuicPath("\\\\srv\\share\\x\\..\\a.md", [], null)).toEqual({
				repoPath: "",
				relPath: "//srv/share/a.md",
				external: true,
			});
		});
	});
});
