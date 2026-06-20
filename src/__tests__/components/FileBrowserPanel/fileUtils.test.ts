import { describe, expect, it } from "vitest";
import { fileTooltip } from "../../../components/FileBrowserPanel/fileUtils";
import type { DirEntry } from "../../../types/fs";

function entry(overrides: Partial<DirEntry>): DirEntry {
	return {
		name: "forced-interception.md",
		path: "ai-governance/MCPProxy/forced-interception.md",
		is_dir: false,
		is_ignored: false,
		size: 16000,
		modified_at: 0,
		git_status: "",
		...overrides,
	} as DirEntry;
}

describe("fileTooltip", () => {
	it("returns just the path when modified_at is missing", () => {
		const e = entry({ modified_at: 0 });
		expect(fileTooltip(e)).toBe(e.path);
	});

	it("appends a relative last-modified line, treating modified_at as seconds", () => {
		// modified_at is seconds since the epoch — 5 minutes ago.
		const e = entry({ modified_at: Math.floor(Date.now() / 1000) - 5 * 60 });
		const tip = fileTooltip(e);
		expect(tip.startsWith(`${e.path}\nModified `)).toBe(true);
		expect(tip).toContain("5m ago");
	});
});
