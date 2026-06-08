import { describe, expect, it } from "vitest";
import { diskStatUnchanged } from "../../components/CodeEditorPanel/CodeEditorTab";

describe("diskStatUnchanged", () => {
	it("treats a null baseline as changed (forces the first read)", () => {
		expect(diskStatUnchanged(null, { modified_at: 100, size: 10 })).toBe(false);
	});

	it("is unchanged when both mtime and size match", () => {
		expect(diskStatUnchanged({ modifiedAt: 100, size: 10 }, { modified_at: 100, size: 10 })).toBe(true);
	});

	it("is changed when mtime differs", () => {
		expect(diskStatUnchanged({ modifiedAt: 100, size: 10 }, { modified_at: 200, size: 10 })).toBe(false);
	});

	it("is changed when size differs at the same mtime (truncate-rewrite within a tick)", () => {
		expect(diskStatUnchanged({ modifiedAt: 100, size: 10 }, { modified_at: 100, size: 12 })).toBe(false);
	});
});
