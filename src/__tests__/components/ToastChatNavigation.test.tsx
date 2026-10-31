import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const waitFor = vi.waitFor;

const { sendCommand } = vi.hoisted(() => ({ sendCommand: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("../../hooks/usePty", () => ({
	usePty: () => ({ sendCommand, getKittyFlags: async () => 0, listQueuedCommands: async () => [] }),
}));
// Substitute the grid renderer, not Terminal, Chat, the composer or navigation.
vi.mock("../../components/Terminal/CanvasTerminal", () => ({ default: () => <div /> }));
vi.mock("../../hooks/useFileDrop", () => ({ useFileDrop: () => ({ isDragging: () => false, attachTo: () => {} }) }));
vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	isTauri: () => false,
	subscribePty: async () => Object.assign(() => {}, { pause() {}, resume() {} }),
}));
vi.mock("../../invoke", () => ({
	listen: async () => () => {},
	invoke: async (command: string, args?: { sessionId?: string }) => {
		if (command === "chat_view_snapshot")
			return {
				epoch: 1,
				nextSeq: 1,
				reset: true,
				unknownRows: 0,
				malformedRows: 0,
				updates: [
					{
						sessionUpdate: "agent_message_chunk",
						messageId: "answer",
						content: { type: "text", text: `Conversation ${args?.sessionId}` },
					},
				],
			};
		if (command === "get_shell_state") return "idle";
		return null;
	},
}));

import { TerminalArea } from "../../components/TerminalArea";
import { ToastContainer } from "../../components/ToastContainer/ToastContainer";
import { navigateFromNativeNotice } from "../../services/nativeNotificationNavigation";
import { activityStore } from "../../stores/activityStore";
import { appLogger } from "../../stores/appLogger";
import { chatViewStore } from "../../stores/chatView";
import { paneLayoutStore } from "../../stores/paneLayout";
import { progressStore } from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { paneLayoutKey, savedPaneLayouts } from "../../stores/savedPaneLayouts";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";

beforeEach(() => {
	vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval"] });
	vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	vi.spyOn(appLogger, "info").mockImplementation(() => {});
	vi.spyOn(appLogger, "warn").mockImplementation(() => {});
	vi.spyOn(appLogger, "error").mockImplementation(() => {});
	vi.spyOn(globalThis, "requestAnimationFrame").mockReturnValue(1);
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
	for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
	paneLayoutStore.reset();
	savedPaneLayouts.clear();
	chatViewStore.reset();
	progressStore.resetForTests();
	sendCommand.mockClear();
});
afterEach(async () => {
	cleanup();
	for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
	activityStore._testCancelPendingSave();
	repositoriesStore._testCancelPendingSave();
	paneLayoutStore._testCancelPendingSave();
	await vi.runOnlyPendingTimersAsync();
	vi.useRealTimers();
	const errors = vi.mocked(appLogger.error).mock.calls;
	vi.restoreAllMocks();
	expect(errors).toEqual([]);
});

function terminal(repo: string, session: string) {
	const id = terminalsStore.add({
		sessionId: session,
		cwd: repo,
		name: session,
		fontSize: 13,
		awaitingInput: null,
		agentType: "claude",
		agentSessionId: `agent-${session}`,
	});
	repositoriesStore.addTerminalToWorkspace(repo, "main", id);
	terminalsStore.setViewMode(id, "chat");
	return id;
}
function setup(destination: boolean) {
	for (const path of ["/chat-x", "/chat-y"]) {
		repositoriesStore.add({ path, displayName: path });
		repositoriesStore.setWorkspace(path, "main", { worktreePath: path });
		repositoriesStore.setActiveWorkspace(path, "main");
	}
	const a = terminal("/chat-x", "session-A");
	const b = destination ? terminal("/chat-y", "session-B") : null;
	repositoriesStore.setActive("/chat-x");
	terminalsStore.setActive(a);
	const view = render(() => (
		<>
			<TerminalArea
				onTerminalFocus={(id) => terminalsStore.setActive(id)}
				onCloseTab={() => {}}
				onOpenFilePath={() => {}}
				onContextMenu={() => {}}
			/>
			<ToastContainer />
		</>
	));
	return { ...view, a, b };
}
function report(sessionId: string) {
	progressStore.presentLive({
		repo_path: "/chat-y",
		payload: {
			entry: { id: 2579, project: "/chat-y", ptyId: sessionId, createdAtMs: 1, type: "done", text: "Ready to review" },
		},
	});
}
async function sendFromVisibleChat(container: HTMLElement) {
	await waitFor(() => expect(container.querySelector(".terminal-pane.active .cm-editor")).not.toBeNull());
	const pane = container.querySelector<HTMLElement>(".terminal-pane.active")!;
	await waitFor(() => expect(pane.textContent).toContain("Conversation session-B"));
	expect(pane.textContent).not.toContain("Conversation session-A");
	const editor = EditorView.findFromDOM(pane.querySelector<HTMLElement>(".cm-editor")!)!;
	editor.dispatch({ changes: { from: 0, insert: "Review this repository" } });
	fireEvent.click(pane.querySelector<HTMLButtonElement>('button[aria-label="Send (Enter)"]')!);
	await waitFor(() =>
		expect(sendCommand).toHaveBeenCalledExactlyOnceWith("session-B", "Review this repository", "claude"),
	);
}

// Catches: the live progress navigation changes paths but leaves Chat sending to A.
it("live_progress_origin_shows_and_sends_to_its_chat", async () => {
	const view = setup(true);
	report("session-B");
	fireEvent.click(view.getByRole("button", { name: "Go to repo" }));
	expect(repositoriesStore.state.activeRepoPath).toBe("/chat-y");
	await sendFromVisibleChat(view.container);
});

// Catches: the closed-origin fallback keeps the previous repository's Chat active.
it("closed_progress_origin_does_not_keep_foreign_chat", async () => {
	const view = setup(true);
	const warn = vi.spyOn(appLogger, "warn");
	report("closed-session");
	fireEvent.click(view.getByRole("button", { name: "Go to repo" }));
	expect(warn).toHaveBeenCalledWith(
		"app",
		expect.stringContaining("terminal"),
		expect.objectContaining({ sessionId: "closed-session" }),
	);
	expect(terminalsStore.state.activeId).toBe(view.b);
	await sendFromVisibleChat(view.container);
});

// Catches: an empty destination retains a writable foreign Chat, including a split.
it("empty_progress_destination_hides_foreign_chat_and_split", () => {
	const view = setup(false);
	const group = paneLayoutStore.createGroup();
	paneLayoutStore.addTab(group, { id: view.a, type: "terminal" });
	paneLayoutStore.split(group, "horizontal");
	vi.spyOn(appLogger, "warn");
	report("closed-session");
	fireEvent.click(view.getByRole("button", { name: "Go to repo" }));
	expect(repositoriesStore.state.activeRepoPath).toBe("/chat-y");
	expect(terminalsStore.state.activeId).toBeNull();
	expect(paneLayoutStore.isSplit()).toBe(false);
	expect(savedPaneLayouts.get(paneLayoutKey("/chat-x", "main"))?.groups[group].tabs).toEqual([
		{ id: view.a, type: "terminal" },
	]);
	expect(view.container.querySelector(".terminal-pane.active")).toBeNull();
});

// Catches: native notification navigation activates a tab but leaves Chat on A.
it("native_notification_shows_and_sends_to_the_named_chat", async () => {
	const view = setup(true);
	navigateFromNativeNotice({ kind: "terminal", id: view.b! });
	await sendFromVisibleChat(view.container);
});

// Catches: repo-only notices ignore the remembered tab or select a closed tab.
it("repo_only_notice_uses_the_last_live_destination_tab", async () => {
	const view = setup(true);
	terminal("/chat-y", "session-C");
	repositoriesStore.setWorkspace("/chat-y", "main", { lastActiveTerminal: view.b });
	toastsStore.add("Repository notice", "Review", "info", false, undefined, undefined, "/chat-y");
	fireEvent.click(view.getByRole("button", { name: "Go to repo" }));
	await sendFromVisibleChat(view.container);
});
