import { describe, expect, it } from "vitest";
import type { SavedPrompt } from "../stores/promptLibrary";
import { handleWatcherFire, type WatcherFireDeps, type WatcherFirePayload } from "../stores/watcherFire";

function makePrompt(id: string): SavedPrompt {
	return { id, name: id, content: `content of ${id}`, executionMode: "inject" } as SavedPrompt;
}

function makeDeps(prompts: Record<string, SavedPrompt> = {}): WatcherFireDeps & {
	calls: Record<string, unknown[][]>;
} {
	const calls: Record<string, unknown[][]> = {
		setActiveSession: [],
		executeSmartPrompt: [],
		sendAssisted: [],
		startAgent: [],
		notifyReview: [],
		warn: [],
	};
	return {
		calls,
		getSmartById: (id) => prompts[id],
		setActiveSession: (s) => {
			calls.setActiveSession.push([s]);
		},
		executeSmartPrompt: async (p) => {
			calls.executeSmartPrompt.push([p]);
		},
		sendAssisted: async (c, s) => {
			calls.sendAssisted.push([c, s]);
		},
		startAgent: async (s, g) => {
			calls.startAgent.push([s, g]);
		},
		notifyReview: (n) => {
			calls.notifyReview.push([n]);
		},
		warn: (m) => {
			calls.warn.push([m]);
		},
	};
}

const base: WatcherFirePayload = { ruleId: "r1", sessionId: "sess-1", context: "" };

describe("handleWatcherFire", () => {
	it("resolves prompt_id and runs executeSmartPrompt against the session (terminal)", async () => {
		const prompt = makePrompt("smart-1");
		const deps = makeDeps({ "smart-1": prompt });
		await handleWatcherFire({ ...base, promptId: "smart-1" }, deps);

		expect(deps.calls.setActiveSession).toEqual([["sess-1"]]);
		expect(deps.calls.executeSmartPrompt).toEqual([[prompt]]);
		expect(deps.calls.sendAssisted).toHaveLength(0);
		expect(deps.calls.startAgent).toHaveLength(0);
	});

	it("falls back to startAgent with instructions when prompt_id absent (terminal)", async () => {
		const deps = makeDeps();
		await handleWatcherFire({ ...base, instructions: "do the thing" }, deps);

		expect(deps.calls.startAgent).toEqual([["sess-1", "do the thing"]]);
		expect(deps.calls.executeSmartPrompt).toHaveLength(0);
		expect(deps.calls.sendAssisted).toHaveLength(0);
	});

	it("routes a PR fire (repoPath set) through the assisted gate, not executeSmartPrompt", async () => {
		const prompt = makePrompt("pr-review");
		const deps = makeDeps({ "pr-review": prompt });
		await handleWatcherFire({ ...base, promptId: "pr-review", repoPath: "/repo", headRefOid: "oid9" }, deps);

		// Assisted conversation runs the prompt content — never the unrestricted inject path.
		expect(deps.calls.sendAssisted).toEqual([["content of pr-review", "sess-1"]]);
		expect(deps.calls.executeSmartPrompt).toHaveLength(0);
		expect(deps.calls.startAgent).toHaveLength(0);
	});

	it("adds a review_started notification entry for a PR fire with prNumber + branch", async () => {
		const prompt = makePrompt("pr-review");
		const deps = makeDeps({ "pr-review": prompt });
		await handleWatcherFire(
			{ ...base, promptId: "pr-review", repoPath: "/repo", headRefOid: "oid9", prNumber: 42, branch: "feat/x" },
			deps,
		);

		expect(deps.calls.notifyReview).toEqual([
			[{ repoPath: "/repo", branch: "feat/x", prNumber: 42, title: "Reviewing PR #42" }],
		]);
	});

	it("does not notify for a terminal fire", async () => {
		const prompt = makePrompt("smart-1");
		const deps = makeDeps({ "smart-1": prompt });
		await handleWatcherFire({ ...base, promptId: "smart-1" }, deps);
		expect(deps.calls.notifyReview).toHaveLength(0);
	});

	it("PR fire with only instructions also goes assisted", async () => {
		const deps = makeDeps();
		await handleWatcherFire({ ...base, repoPath: "/repo", instructions: "review it" }, deps);
		expect(deps.calls.sendAssisted).toEqual([["review it", "sess-1"]]);
	});

	it("warns when prompt_id does not resolve and there is no fallback", async () => {
		const deps = makeDeps();
		await handleWatcherFire({ ...base, promptId: "missing" }, deps);
		expect(deps.calls.warn.length).toBeGreaterThan(0);
		expect(deps.calls.executeSmartPrompt).toHaveLength(0);
		expect(deps.calls.startAgent).toHaveLength(0);
	});
});
