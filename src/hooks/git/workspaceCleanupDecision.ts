import type {
	WorkspaceCleanupReason,
	WorkspaceCleanupRecord,
	WorkspaceLifecycleStatus,
} from "../../stores/workspaceIdentity";

const withDetail = (reason: WorkspaceCleanupReason, detail: string) => ({ eligible: false as const, reason, detail });

/**
 * Turn one fresh backend inspection into the two separate facts the UI and the
 * sweep need: what proof exists (exact tip, base, source, when) and whether
 * cleanup may proceed. The backend still revalidates at removal; this verdict
 * only decides whether to ask it.
 */
export function recordWorkspaceCleanup(
	workspaceId: string,
	status: WorkspaceLifecycleStatus,
	inspectedAt: number,
): WorkspaceCleanupRecord {
	const proof = {
		workspaceId,
		tip: status.tip ?? null,
		base: status.base ?? null,
		source: status.proofSource ?? null,
		verdict: status.commitStatus,
		inspectedAt,
	};
	return { proof, decision: decide(status, proof) };
}

function decide(
	status: WorkspaceLifecycleStatus,
	proof: WorkspaceCleanupRecord["proof"],
): WorkspaceCleanupRecord["decision"] {
	const fallback = (status.warnings ?? []).join("; ");
	if (status.error || status.commitStatus === "unknown") {
		return withDetail("uncertain", status.error ?? (fallback || "completion proof is unknown"));
	}
	if (status.commitStatus !== "merged") {
		return withDetail("unmerged", fallback || "work is not integrated");
	}
	// null means the inspection failed; it is never the same as zero.
	if (
		status.removalSafety !== "safe" ||
		status.dirtyFiles !== 0 ||
		(status.untrackedFiles ?? 0) > 0 ||
		(status.submoduleUnpushedCommits?.length ?? 0) > 0 ||
		status.missingCheckout ||
		(status.liveSessions?.length ?? 0) > 0
	) {
		return withDetail("safety-blocked", fallback || "removal needs review");
	}
	if (!proof.tip || !proof.base || !proof.source) {
		return withDetail("uncertain", "completion proof does not name tip, base and source");
	}
	if (!status.dirtyFingerprint) {
		return withDetail("uncertain", "checkout state cannot be bound to the inspection");
	}
	return { eligible: true, reason: "merged", detail: "merged and safe to archive" };
}

/** The same proof, now refused by the safety gate: the row says why it stays. */
export function keptRecord(record: WorkspaceCleanupRecord, detail: string): WorkspaceCleanupRecord {
	return { proof: record.proof, decision: withDetail("safety-blocked", detail) };
}

/** Why a cleanup attempt failed, when the failure is a keep (a locked checkout), else null. */
export function keptReasonFromError(error: unknown): string | null {
	const message = error instanceof Error ? error.message : String(error);
	const locked = /worktree_locked:\s*(.*)/s.exec(message);
	if (locked) return `locked${locked[1].trim() ? `: ${locked[1].trim()}` : ""}`;
	return /\blocked\b/i.test(message) ? "locked" : null;
}

/** Row text for a merged worktree the sweep refused to touch; null when nothing was refused. */
export function keptReason(record: WorkspaceCleanupRecord | undefined): string | null {
	if (!record || record.decision.eligible || record.decision.reason !== "safety-blocked") return null;
	return record.proof.verdict === "merged" ? record.decision.detail : null;
}
