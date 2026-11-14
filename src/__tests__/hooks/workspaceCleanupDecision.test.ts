import { describe, expect, it } from "vitest";
import {
	keptReason,
	keptReasonFromError,
	keptRecord,
	recordWorkspaceCleanup,
} from "../../hooks/git/workspaceCleanupDecision";
import type { WorkspaceLifecycleStatus } from "../../stores/workspaceIdentity";

const merged: WorkspaceLifecycleStatus = {
	commitStatus: "merged",
	proofSource: "github_pr",
	tip: "tip-1",
	base: "base-1",
	dirtyFiles: 0,
	dirtyFingerprint: "fp",
	removalSafety: "safe",
};
const reasonOf = (status: WorkspaceLifecycleStatus) => recordWorkspaceCleanup("ws", status, 1_000).decision;

describe("recordWorkspaceCleanup", () => {
	// catches: eligibility derived from the merged label alone.
	it("is eligible only for merged, proven, safe, bound work", () => {
		expect(reasonOf(merged)).toMatchObject({ eligible: true, reason: "merged" });
	});

	// catches: unknown or failed proof read as merged; stale, partial proof still authorising cleanup.
	it.each([
		["inspection error", { ...merged, error: "ref unreadable" }],
		["unknown verdict", { ...merged, commitStatus: "unknown" as const }],
		["missing tip", { ...merged, tip: undefined }],
		["missing base", { ...merged, base: undefined }],
		["missing proof source", { ...merged, proofSource: undefined }],
		["missing fingerprint", { ...merged, dirtyFingerprint: undefined }],
	])("treats %s as uncertain and ineligible", (_case, status) => {
		expect(reasonOf(status)).toMatchObject({ eligible: false, reason: "uncertain" });
	});

	// catches: pushed-only, in-sync or unmerged work reported as merged.
	it.each(["unmerged", "pushed_unmerged", "in_sync", "unrelated_history"] as const)(
		"reports %s as unmerged",
		(commitStatus) => {
			expect(reasonOf({ ...merged, commitStatus })).toMatchObject({ eligible: false, reason: "unmerged" });
		},
	);

	// catches: merged proof overriding dirty, untracked, submodule, session or checkout hazards.
	it.each([
		["dirty files", { dirtyFiles: 1 }],
		["unknown dirt", { dirtyFiles: null }],
		["untracked files", { untrackedFiles: 1 }],
		["submodule commits", { submoduleUnpushedCommits: [{ path: "plugins", count: 1 }] }],
		["live session", { liveSessions: [{ sessionId: "s", name: "agent" }] }],
		["forced removal", { removalSafety: "requires_force" as const }],
		["missing checkout", { missingCheckout: true }],
	])("keeps merged work blocked by %s as safety-blocked", (_case, hazard) => {
		expect(reasonOf({ ...merged, ...hazard })).toMatchObject({ eligible: false, reason: "safety-blocked" });
	});

	it("keeps proof separate from the decision", () => {
		const record = recordWorkspaceCleanup("ws", { ...merged, dirtyFiles: 3 }, 5_000);
		expect(record.proof).toEqual({
			workspaceId: "ws",
			tip: "tip-1",
			base: "base-1",
			source: "github_pr",
			verdict: "merged",
			inspectedAt: 5_000,
		});
		expect(record.decision.eligible).toBe(false);
	});
});

describe("kept reason on the row", () => {
	// catches: a locked merged worktree failing silently, so the row gives no reason.
	it("turns a locked checkout error into a visible reason", () => {
		expect(keptReasonFromError(new Error("worktree_locked:worktree is locked"))).toBe("locked: worktree is locked");
		expect(keptReasonFromError(new Error("disk full"))).toBeNull();
	});

	// catches: showing a keep reason on work that is eligible, or on unmerged work.
	it("reports a reason only for merged work the safety gate refused", () => {
		const eligible = recordWorkspaceCleanup("ws", merged, 1);
		expect(keptReason(eligible)).toBeNull();
		expect(keptReason(keptRecord(eligible, "uncommitted work"))).toBe("uncommitted work");
		const unmerged = recordWorkspaceCleanup("ws", { ...merged, commitStatus: "unmerged" }, 1);
		expect(keptReason(keptRecord(unmerged, "x"))).toBeNull();
	});
});
