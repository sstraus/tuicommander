import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { sendCommand } from "../../utils/sendCommand";
import { IdeasOverlay } from "../components/IdeasOverlay";

const idea = { id: "idea-1", text: `Refactor ${"the session store ".repeat(40)}`, createdAt: 0, repoDisplayName: null };
vi.mock("../../transport", () => ({ rpc: vi.fn(async () => ({ language: "en" })) }));
vi.mock("../../utils/sendCommand", () => ({ sendCommand: vi.fn(async () => {}) }));
vi.mock("../../stores/ideas", () => ({
	ideasStore: { getFilteredIdeas: () => [idea], addIdea: vi.fn(), markUsed: vi.fn(), removeIdea: vi.fn() },
}));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

afterEach(() => {
	cleanup();
	vi.clearAllMocks();
});

// Catches: the idea is sent with agentType null, so a long line on a Claude session skips the Claude adapter.
it("sends an idea with the agent type of the session", async () => {
	render(() => <IdeasOverlay sessionId="s1" repoPath="/repo" agentType="claude" onDismiss={() => {}} />);
	fireEvent.click(screen.getByRole("button", { name: "Send to terminal" }));
	await Promise.resolve();
	expect(vi.mocked(sendCommand)).toHaveBeenCalledOnce();
	const [, text, agentType, , submit, sessionId] = vi.mocked(sendCommand).mock.calls[0];
	expect([text, agentType, submit, sessionId]).toEqual([idea.text, "claude", true, "s1"]);
});
