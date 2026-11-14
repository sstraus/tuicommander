import type { SmartPromptResult } from "../../hooks/useSmartPrompts";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import type { SavedPrompt } from "../../stores/promptLibrary";

/** Mirrors `StashFacts` in tuic-git `stash_check.rs`. */
export interface StashFacts {
	stash_ref: string;
	hash: string;
	message: string;
	all_in_head: boolean;
}

/** Mirrors `StashCheckOutcome` in tuic-git `stash_check.rs`. */
interface StashCheckOutcome {
	dropped: boolean;
	reason: string;
	archive_ref: string | null;
	restore: string | null;
}

export interface StashCheckResult {
	stashRef: string;
	verdict: string;
	dropped: boolean;
	/** One line for the user: why it was kept, or where the archive is and how to restore. */
	summary: string;
}

type Verdict = "redundant" | "obsolete" | "useful" | "unsure";

function parseVerdict(output: string | undefined): { verdict: Verdict; reason: string } {
	const unsure = { verdict: "unsure" as Verdict, reason: "the prompt returned no usable verdict" };
	const match = output?.match(/\{[\s\S]*\}/);
	if (!match) return unsure;
	try {
		const parsed = JSON.parse(match[0]) as { verdict?: string; reason?: string };
		const verdict = ["redundant", "obsolete", "useful", "unsure"].includes(parsed.verdict ?? "")
			? (parsed.verdict as Verdict)
			: "unsure";
		return { verdict, reason: parsed.reason ?? "" };
	} catch {
		return unsure;
	}
}

/**
 * Check one stash: git computes the facts, the smart prompt classifies, and
 * only the backend decides whether to drop (after archiving the commit).
 * Anything but a proven redundant/obsolete verdict keeps the stash.
 */
export async function checkStash(
	repoPath: string,
	stashRef: string,
	prompt: SavedPrompt,
	run: (prompt: SavedPrompt, vars: Record<string, string>) => Promise<SmartPromptResult>,
): Promise<StashCheckResult> {
	const facts = await invoke<StashFacts>("git_stash_facts", { path: repoPath, stashRef });
	const result = await run(prompt, { stash_facts: JSON.stringify(facts, null, 2) });
	if (!result.ok) {
		return { stashRef, verdict: "unsure", dropped: false, summary: `kept: ${result.reason ?? "check failed"}` };
	}
	const { verdict, reason } = parseVerdict(result.output);
	if (verdict !== "redundant" && verdict !== "obsolete") {
		return { stashRef, verdict, dropped: false, summary: `kept (${verdict}): ${reason}` };
	}
	const outcome = await invoke<StashCheckOutcome>("git_stash_check_drop", {
		path: repoPath,
		stashRef,
		hash: facts.hash,
		verdict,
	});
	if (!outcome.dropped) {
		return { stashRef, verdict, dropped: false, summary: `kept (${verdict}, not proven): ${outcome.reason}` };
	}
	appLogger.info("git", `Dropped ${stashRef} after check; archived as ${outcome.archive_ref}`);
	return {
		stashRef,
		verdict,
		dropped: true,
		summary: `dropped (${verdict}); archived as ${outcome.archive_ref}; restore: ${outcome.restore}`,
	};
}
