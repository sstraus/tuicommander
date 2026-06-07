import { describe, expect, it } from "vitest";

import type { BranchDetail } from "../../components/GitPanel/types";
import { branchListsEqual } from "../../utils/branchListsEqual";

function branch(overrides: Partial<BranchDetail> = {}): BranchDetail {
	return {
		name: "main",
		is_current: true,
		is_remote: false,
		is_main: true,
		is_merged: false,
		ahead: 0,
		behind: 0,
		upstream: "origin/main",
		last_commit_date: "2026-06-05",
		last_commit_message: "init",
		last_commit_author: "Boss",
		base_ahead: null,
		base_behind: null,
		base_branch: null,
		...overrides,
	};
}

describe("branchListsEqual", () => {
	it("returns true for the same reference", () => {
		const list = [branch()];
		expect(branchListsEqual(list, list)).toBe(true);
	});

	it("returns true for distinct but value-equal lists", () => {
		expect(
			branchListsEqual(
				[branch(), branch({ name: "dev", is_current: false })],
				[branch(), branch({ name: "dev", is_current: false })],
			),
		).toBe(true);
	});

	it("returns false when lengths differ", () => {
		expect(branchListsEqual([branch()], [branch(), branch({ name: "dev" })])).toBe(false);
	});

	it("returns false when any field differs", () => {
		expect(branchListsEqual([branch({ ahead: 0 })], [branch({ ahead: 1 })])).toBe(false);
		expect(branchListsEqual([branch({ is_current: true })], [branch({ is_current: false })])).toBe(false);
		expect(branchListsEqual([branch({ upstream: "origin/main" })], [branch({ upstream: null })])).toBe(false);
		expect(branchListsEqual([branch({ last_commit_message: "a" })], [branch({ last_commit_message: "b" })])).toBe(
			false,
		);
		expect(branchListsEqual([branch({ base_branch: null })], [branch({ base_branch: "main" })])).toBe(false);
	});

	it("is order-sensitive (different ordering is not equal)", () => {
		const a = [branch({ name: "main" }), branch({ name: "dev" })];
		const b = [branch({ name: "dev" }), branch({ name: "main" })];
		expect(branchListsEqual(a, b)).toBe(false);
	});

	it("treats two empty lists as equal", () => {
		expect(branchListsEqual([], [])).toBe(true);
	});
});
