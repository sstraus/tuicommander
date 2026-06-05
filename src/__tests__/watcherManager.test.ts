import { describe, expect, it } from "vitest";
import { buildTrigger, isGitTrigger, watcherFormReady } from "../components/WatcherManager/WatcherManager";

describe("WatcherManager form logic", () => {
	it("isGitTrigger is true for PR triggers only", () => {
		expect(isGitTrigger("pr_pushed")).toBe(true);
		expect(isGitTrigger("pr_opened")).toBe(true);
		expect(isGitTrigger("idle")).toBe(false);
		expect(isGitTrigger("command_done")).toBe(false);
	});

	it("buildTrigger sets authored_by_others for PR triggers and is static otherwise", () => {
		expect(buildTrigger("pr_pushed", true)).toEqual({ type: "pr_pushed", authored_by_others: true });
		expect(buildTrigger("pr_pushed", false)).toEqual({ type: "pr_pushed", authored_by_others: false });
		expect(buildTrigger("pr_opened", true)).toEqual({ type: "pr_opened", authored_by_others: true });
		expect(buildTrigger("pr_opened", false)).toEqual({ type: "pr_opened", authored_by_others: false });
		expect(buildTrigger("idle", true)).toEqual({ type: "idle" });
		expect(buildTrigger("command_done_fail", false)).toEqual({ type: "command_done", on_failure_only: true });
	});

	it("watcherFormReady requires an action (prompt or instructions)", () => {
		const base = { triggerKey: "idle" as const, repoPath: "" };
		expect(watcherFormReady({ ...base, promptId: "p1", instructions: "" })).toBe(true);
		expect(watcherFormReady({ ...base, promptId: "", instructions: "do it" })).toBe(true);
		expect(watcherFormReady({ ...base, promptId: "", instructions: "   " })).toBe(false);
		expect(watcherFormReady({ ...base, promptId: "", instructions: "" })).toBe(false);
	});

	it("watcherFormReady requires a repo for the pr_pushed trigger", () => {
		// Action present but no repo → not ready.
		expect(watcherFormReady({ promptId: "pr-review", instructions: "", triggerKey: "pr_pushed", repoPath: "" })).toBe(
			false,
		);
		// Action + repo → ready.
		expect(
			watcherFormReady({ promptId: "pr-review", instructions: "", triggerKey: "pr_pushed", repoPath: "/repo" }),
		).toBe(true);
	});
});
