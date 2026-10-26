import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("../../../invoke", () => ({
	invoke: invokeMock,
	listen: vi.fn(async () => () => {}),
}));
vi.mock("../../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), info: vi.fn() } }));

import { ViewModeToggle } from "../../../components/Terminal/TerminalChatView";
import { useSessionChatView } from "../../../mobile/components/useSessionChatView";
import { chatViewStore } from "../../../stores/chatView";
import { terminalsStore } from "../../../stores/terminals";

beforeEach(() => {
	chatViewStore.reset();
	invokeMock.mockReset();
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
});

// Catches: the desktop toggle still rejecting ego after exact session discovery.
it("ego desktop toggle selects chat for its bound terminal", () => {
	const id = terminalsStore.add({
		sessionId: "ego-desktop",
		fontSize: 14,
		name: "ego",
		cwd: null,
		awaitingInput: null,
		agentType: "ego",
		agentSessionId: "own-session",
	});
	const view = render(() => <ViewModeToggle terminalId={id} />);
	fireEvent.click(view.getByRole("tab", { name: "Chat" }));
	expect(terminalsStore.get(id)?.viewMode).toBe("chat");
});

// Catches: mobile hardcoding Claude, or an unreadable ego export leaving an empty
// conversation instead of returning to CLI and retaining the backend reason.
it("ego mobile chat uses the shared wake and displays the unreadable reason", async () => {
	invokeMock.mockRejectedValue(new Error("not_bound: ego transcript v1 header missing"));
	const subscribeWake = vi.fn(async (_sessionId: string, _wake: () => void) => () => {});
	const Harness = () => {
		const chat = useSessionChatView({
			sessionId: () => "ego-mobile",
			agentType: () => "ego",
			sessionExists: () => true,
			subscribeWake,
		});
		return (
			<>
				<chat.Toggle />
				<chat.View />
			</>
		);
	};
	const view = render(() => <Harness />);
	fireEvent.click(view.getByRole("button", { name: "Chat" }));
	await waitFor(() => expect(view.getByRole("status").textContent).toContain("ego transcript v1 header missing"));
	expect(subscribeWake).toHaveBeenCalledOnce();
	expect(subscribeWake.mock.calls[0][0]).toBe("ego-mobile");
	expect(view.getByRole("button", { name: "CLI" }).getAttribute("aria-pressed")).toBe("true");
});
