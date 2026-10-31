import { beforeEach, expect, it } from "vitest";
import { aiChatDraft } from "../../components/AIChatPanel/draft";

beforeEach(() => aiChatDraft.reset());
it("restores an acknowledged withdrawal to its source tab — catches tab switches losing or misrouting the prompt", () => {
	aiChatDraft.activate("source");
	aiChatDraft.activate("other");
	aiChatDraft.set("other draft");
	aiChatDraft.restoreRecalled("source", "withdrawn prompt");
	expect(aiChatDraft.text()).toBe("other draft");
	aiChatDraft.activate("source");
	expect(aiChatDraft.text()).toBe("withdrawn prompt");
});
