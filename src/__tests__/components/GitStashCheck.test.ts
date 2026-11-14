import { beforeEach, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: h.invoke }));

import { checkStash } from "../../components/GitPanel/stashCheck";
import type { SavedPrompt } from "../../stores/promptLibrary";

const prompt = { id: "smart-stash-check" } as SavedPrompt;
const facts = { stash_ref: "stash@{0}", hash: "abc123", message: "wip", all_in_head: false };
const verdictRun = (verdict: string) => async () => ({
	ok: true,
	output: `noise {"verdict":"${verdict}","reason":"r"} noise`,
});
const dropCalls = () => h.invoke.mock.calls.filter(([c]) => c === "git_stash_drop" || c === "git_stash_check_drop");

beforeEach(() => {
	h.invoke.mockReset();
	h.invoke.mockImplementation(async (command: string) =>
		command === "git_stash_facts"
			? facts
			: {
					dropped: false,
					reason: "not proven: HEAD lacks the stash's version of a.txt",
					archive_ref: null,
					restore: null,
				},
	);
});

// RED: a stash with a hunk missing from HEAD is dropped automatically.
it("catches a redundant verdict dropping a stash the backend cannot prove, or bypassing the guarded command", async () => {
	const r = await checkStash("/repo", "stash@{0}", prompt, verdictRun("redundant"));
	expect(h.invoke.mock.calls.some(([c]) => c === "git_stash_drop")).toBe(false);
	expect(h.invoke).toHaveBeenCalledWith("git_stash_check_drop", {
		path: "/repo",
		stashRef: "stash@{0}",
		hash: "abc123",
		verdict: "redundant",
	});
	expect(r.dropped).toBe(false);
	expect(r.summary).toContain("HEAD lacks");
});

it.each(["useful", "unsure", "bogus"])("catches verdict %s reaching any drop command", async (verdict) => {
	const r = await checkStash("/repo", "stash@{0}", prompt, verdictRun(verdict));
	expect(dropCalls()).toHaveLength(0);
	expect(r.dropped).toBe(false);
});
