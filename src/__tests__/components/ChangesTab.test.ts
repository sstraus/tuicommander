import { describe, expect, it } from "vitest";
import { isDirEntry } from "../../components/GitPanel/ChangesTab";

describe("isDirEntry", () => {
	// `git status` collapses a wholly-untracked directory into a single
	// trailing-slash entry (e.g. `providers/`). That's not a file, so it has no
	// diff — clicking it must NOT open an empty "No changes" diff tab.
	it("treats collapsed untracked directories as directory entries", () => {
		expect(isDirEntry("providers/")).toBe(true);
		expect(isDirEntry("src/components/")).toBe(true);
	});

	it("treats Windows-style trailing separators as directory entries", () => {
		expect(isDirEntry("providers\\")).toBe(true);
	});

	it("treats real file paths as non-directory entries", () => {
		expect(isDirEntry("providers/index.ts")).toBe(false);
		expect(isDirEntry("README.md")).toBe(false);
		expect(isDirEntry("src\\main.ts")).toBe(false);
	});
});
