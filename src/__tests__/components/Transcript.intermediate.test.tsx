// @vitest-environment jsdom
import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";

afterEach(cleanup);

const tool = (id: string): AcpTranscriptEntry => ({
	id,
	kind: "tool",
	call: { toolCallId: id, title: "Read", status: "completed" },
});
const text = (id: string): AcpTranscriptEntry => ({ id, kind: "agent", text: `Text ${id}` });
const answer = (id: string): AcpTranscriptEntry => ({ id, kind: "agent", text: `💬 Answer ${id}` });
const user = (id: string): AcpTranscriptEntry => ({ id, kind: "user", text: `Prompt ${id}` });

function mutedTexts(entries: AcpTranscriptEntry[], muteIntermediate?: boolean): string[] {
	const { container } = render(() => (
		<Transcript
			entries={() => entries}
			busy={() => false}
			emptyMessage="Empty"
			observeToolDuration={false}
			muteIntermediate={muteIntermediate}
		/>
	));
	return Array.from(container.querySelectorAll("[data-intermediate]")).map(
		(node) => node.querySelector("#markdown-content")?.textContent?.trim() ?? "",
	);
}

describe("intermediate assistant text", () => {
	// Catches: the muted style landing on the final answer or on every block of the turn.
	it("mutes only text that a tool call follows in the same turn", () => {
		expect(mutedTexts([user("u"), text("a"), tool("t"), text("b")])).toEqual(["Text a"]);
	});

	// Catches: a status line written before the last tool call of a turn staying at answer weight.
	it("mutes every text block before a tool call, not only the nearest one", () => {
		expect(mutedTexts([user("u"), text("a"), text("b"), tool("t"), text("c")])).toEqual(["Text a", "Text b"]);
	});

	// Catches: a turn's closing answer being muted because the next turn uses tools.
	it("does not carry a finished turn's answer into the next turn's tool calls", () => {
		expect(mutedTexts([user("u1"), text("a"), user("u2"), tool("t"), text("b")])).toEqual([]);
	});

	// Catches: a 💬 answer written before a later tool call rendered grey on the green answer block.
	it("never mutes a 💬 answer, even when a tool call follows it", () => {
		expect(mutedTexts([user("u"), answer("a"), text("b"), tool("t"), text("c")])).toEqual(["Text b"]);
	});

	// Catches: the answer exemption being keyed on the whole turn instead of the marked message.
	it("still mutes a plain message in the same turn as an answer", () => {
		expect(mutedTexts([user("u"), text("a"), answer("b"), tool("t")])).toEqual(["Text a"]);
	});

	// Catches: a text block with no tool call after it being muted.
	it("mutes nothing when the turn has no tool call after the text", () => {
		expect(mutedTexts([user("u"), tool("t"), text("a")])).toEqual([]);
	});

	// Catches: the Appearance > Chat switch having no effect.
	it("keeps every block at normal weight when muting is off", () => {
		expect(mutedTexts([user("u"), text("a"), tool("t"), text("b")], false)).toEqual([]);
	});

	// Catches: muting hiding the block from copy.
	it("keeps the copy action on a muted block", () => {
		const { container } = render(() => (
			<Transcript
				entries={() => [user("u"), text("a"), tool("t"), text("b")]}
				busy={() => false}
				emptyMessage="Empty"
				observeToolDuration={false}
			/>
		));
		const muted = container.querySelector("[data-intermediate]");
		expect(muted?.querySelector('button[aria-label="Assistant message actions"]')).not.toBeNull();
	});
});

// Settle the shared UI persistence owner imported by the control utilities.
import { activityStore } from "../../stores/activityStore";

beforeAll(() => activityStore.hydrate());
