import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke, listen: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), info: vi.fn(), error: vi.fn() } }));

import { chatViewStore } from "../../stores/chatView";
import { HttpRpcError } from "../../transport/http";
import grokSnapshot from "../__fixtures__/grokChatSnapshot.json";
import { useSessionChatView } from "../components/useSessionChatView";

beforeEach(() => {
	chatViewStore.reset();
	invoke.mockReset();
});
afterEach(cleanup);

function mount(agent: string, exists = true) {
	return render(() => {
		const chat = useSessionChatView({
			sessionId: () => `mobile-${agent}`,
			agentType: () => agent,
			sessionExists: () => exists,
			subscribeWake: () => () => {},
		});
		return (
			<>
				<chat.Toggle />
				<chat.View />
			</>
		);
	});
}

// Catches: mobile excludes supported Codex even when the backend accepts its exact binding.
it("mobile_codex_chat_toggle_for_bound_transcript", async () => {
	invoke.mockResolvedValue({ epoch: 1, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 });
	const view = mount("codex");
	view.getByRole("tab", { name: "Chat" }).click();
	await waitFor(() => expect(view.getByTestId("terminal-chat-view")).toBeTruthy());
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith("chat_view_snapshot", {
			sessionId: "mobile-codex",
			epoch: null,
			fromSeq: 0,
			pendingTexts: [],
		}),
	);
	view.getByRole("tab", { name: "CLI" }).click();
	expect(view.queryByTestId("terminal-chat-view")).toBeNull();
});

// Catches: adding Codex bypasses the backend's refusal and leaves an unbound blank Chat open.
it("returns unbound Codex to CLI with the backend binding reason", async () => {
	invoke.mockRejectedValue(
		new HttpRpcError("chat_view_snapshot", 500, '{"error":"not_bound: no exact Codex hook binding for this terminal"}'),
	);
	const view = mount("codex");
	view.getByRole("tab", { name: "Chat" }).click();
	await waitFor(() => expect(view.getByRole("tab", { name: "CLI" }).getAttribute("aria-selected")).toBe("true"));
	expect(view.container.textContent).toContain("no exact Codex hook binding for this terminal");
});

// Catches: broadening the mobile gate exposes unsupported adapters or ended PTYs.
it.each([
	["gemini", true],
	["codex", false],
	["grok", false],
])("hides Chat for %s with live session %s", (agent, exists) => {
	const view = mount(agent as string, exists as boolean);
	expect(view.queryByRole("tab", { name: "Chat" })).toBeNull();
	expect(invoke).not.toHaveBeenCalled();
});

// Catches: mobile excludes Grok despite a live registry-bound transcript accepted by the backend.
it("mobile_grok_chat_toggle_for_bound_transcript", async () => {
	invoke.mockResolvedValue(grokSnapshot);
	const view = mount("grok");
	view.getByRole("tab", { name: "Chat" }).click();
	await waitFor(() => expect(view.getByText("MOBILE_CHAT_REALITY_OK")).toBeTruthy());
	expect(invoke).toHaveBeenCalledWith("chat_view_snapshot", {
		sessionId: "mobile-grok",
		epoch: null,
		fromSeq: 0,
		pendingTexts: [],
	});
	view.getByRole("tab", { name: "CLI" }).click();
	expect(view.queryByTestId("terminal-chat-view")).toBeNull();
});

// Catches: adding Grok silently accepts an absent native registry binding.
it("returns unbound Grok to CLI with the backend binding reason", async () => {
	invoke.mockRejectedValue(
		new HttpRpcError(
			"chat_view_snapshot",
			500,
			'{"error":"not_bound: no Grok session is registered for this terminal"}',
		),
	);
	const view = mount("grok");
	view.getByRole("tab", { name: "Chat" }).click();
	await waitFor(() => expect(view.getByRole("tab", { name: "CLI" }).getAttribute("aria-selected")).toBe("true"));
	expect(view.container.textContent).toContain("no Grok session is registered for this terminal");
});
