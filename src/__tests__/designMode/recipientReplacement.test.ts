import { expect, it } from "vitest";
// @ts-expect-error plain MV3 module
import { deliverDraft } from "../../../browser-extension/design-mode/cards.js";

// Catches: an exited recipient permanently poisoning a draft despite choosing another live agent.
it("sends retained picks to the chosen replacement after the recipient exited", async () => {
	let stored = {
		selectedSessionId: "exited",
		selectionInitialized: true,
		picks: [{ id: "pick", comment: "Increase spacing", raw: {} }],
	};
	const save = async (draft: typeof stored) => {
		stored = draft;
	};
	await expect(
		deliverDraft(
			stored,
			async () => {
				throw new Error("Selected agent exited or belongs to another repository. Choose another agent.");
			},
			save,
			"first-attempt",
		),
	).rejects.toThrow("Selected agent exited");
	stored = { ...stored, selectedSessionId: "replacement" };
	const requests: { sessionId: string }[] = [];
	await deliverDraft(
		stored,
		async (batch: { sessionId: string }) => {
			requests.push(batch);
			return { accepted: true, typed: false, queued: 1 };
		},
		save,
		"replacement-attempt",
	);
	expect(requests).toEqual([expect.objectContaining({ sessionId: "replacement" })]);
});
