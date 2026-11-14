import { describe, expect, it } from "vitest";
import { mapCommandToHttp } from "../transport";

describe("remove_orphan_worktree transport binding (critic-1188 r2)", () => {
	// Catches: confirmedSessions dropped (or renamed) between the Tauri args and the HTTP body,
	// so a remote client's reviewed session list never reaches the backend guard.
	it("forwards the reviewed session ids unchanged", () => {
		expect(
			mapCommandToHttp("remove_orphan_worktree", {
				repoPath: "/r",
				worktreePath: "/r/wt",
				safeOnly: false,
				confirmedSessions: ["s1", "s2"],
				expectedFingerprint: "reviewed",
			}),
		).toEqual({
			method: "POST",
			path: "/repo/remove-orphan",
			body: {
				repoPath: "/r",
				worktreePath: "/r/wt",
				safeOnly: false,
				confirmedSessions: ["s1", "s2"],
				expectedFingerprint: "reviewed",
			},
		});
	});

	// Catches: an omitted list serialised as undefined/null (serde Vec rejects null with 422).
	it("sends an empty array, not null, when no sessions were reviewed", () => {
		const mapped = mapCommandToHttp("remove_orphan_worktree", { repoPath: "/r", worktreePath: "/r/wt" });
		expect((mapped as { body: unknown }).body).toEqual({
			repoPath: "/r",
			worktreePath: "/r/wt",
			safeOnly: false,
			confirmedSessions: [],
		});
	});
});
