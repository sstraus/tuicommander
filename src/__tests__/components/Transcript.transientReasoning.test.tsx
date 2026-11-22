// @vitest-environment jsdom
import { cleanup, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeAll, beforeEach, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import { acpTranscript } from "../../stores/acpTranscript";
import type { AcpClientEvent } from "../../types/acp";

beforeAll(async () => {
	const { activityStore } = await import("../../stores/activityStore");
	await activityStore.hydrate();
});
beforeEach(() => acpTranscript.reset());
afterEach(cleanup);

// Catches: the shared renderer shows stale or non-italic reasoning, or removes stored evidence with the row.
it("renders accepted public reasoning transiently in italics", async () => {
	let sequence = 0;
	const feed = (event: AcpClientEvent, sessionId = "chat-a") =>
		acpTranscript.applyFrame({
			kind: "event",
			connectionId: "connection",
			generation: 1,
			sequence: ++sequence,
			sessionId,
			turnId: "turn",
			event,
		});
	const [session, select] = createSignal("chat-a");
	const { container } = render(() => (
		<Transcript
			entries={() => acpTranscript.entries(session())}
			reasoning={() => acpTranscript.reasoning(session(), "connection")}
			busy={() => false}
			emptyMessage="Ready"
			observeToolDuration={false}
		/>
	));
	const row = () => container.querySelector('[aria-label="Turn reasoning"]');
	expect(row()).toBeNull();
	feed({ kind: "promptSent", text: "accepted" });
	expect(row()?.querySelector("em")?.textContent).toBe("Processing…");
	feed({
		kind: "sessionUpdate",
		update: { sessionUpdate: "agent_thought_chunk", content: { type: "text", text: "Compare " } },
	});
	feed({
		kind: "sessionUpdate",
		update: { sessionUpdate: "agent_thought_chunk", content: { type: "text", text: "the inputs." } },
	});
	expect(row()?.querySelector("em")?.textContent).toBe("Compare the inputs.");
	expect(container.querySelector(".reasoningDisclosure")).toBeNull();
	select("chat-b");
	expect(row()).toBeNull();
	feed({
		kind: "sessionUpdate",
		update: { sessionUpdate: "agent_thought_chunk", content: { type: "text", text: " Still A." } },
	});
	expect(row()).toBeNull();
	select("chat-a");
	expect(row()?.textContent).toBe("Compare the inputs. Still A.");
	feed({
		kind: "sessionUpdate",
		update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "The answer." } },
	});
	await Promise.resolve();
	expect(row()).toBeNull();
	expect(container.textContent).not.toContain("Compare the inputs.");
	expect(container.textContent).toContain("The answer.");
	expect(acpTranscript.entries("chat-a").find((entry) => entry.kind === "thought")?.text).toBe(
		"Compare the inputs. Still A.",
	);
});

// Catches: opting ACP into transient reasoning silently removes PTY thought history.
it("keeps the existing thought disclosure for a transcript without the ACP presentation prop", () => {
	const { container } = render(() => (
		<Transcript
			entries={() => [{ id: "thought", kind: "thought", text: "Retained terminal thought" }]}
			busy={() => false}
			emptyMessage=""
			observeToolDuration={false}
		/>
	));
	expect(container.querySelector("details summary")?.textContent).toBe("Thinking");
	expect(container.textContent).toContain("Retained terminal thought");
});
