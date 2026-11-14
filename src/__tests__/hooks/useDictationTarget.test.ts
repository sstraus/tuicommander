import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import claudeLongInput from "../../../src-tauri/src/fixtures/agent_prompts/claude-long-dictation-2502.json";
import type { AgentType } from "../../agents";
import { useDictation } from "../../hooks/useDictation";
import { appLogger } from "../../stores/appLogger";
import { dictationStore } from "../../stores/dictation";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { nextAnimationFrame } from "../helpers/animationFrame";
import { mockInvoke } from "../mocks/tauri";

function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((done) => {
		resolve = done;
	});
	return { promise, resolve };
}
const result = { text: "hello world", skip_reason: null, duration_s: 1, truncated_s: 0 };

describe("dictation delivery target", () => {
	const views: EditorView[] = [];
	const write = vi.fn().mockResolvedValue(undefined);
	const status = vi.fn();
	const recording = {
		state: {
			enabled: true,
			recording: false,
			processing: false,
			loading: false,
			modelStatus: "ready" as const,
			handsFree: null,
		},
		refreshStatus: vi.fn().mockResolvedValue(undefined),
		refreshHandsFree: vi.fn().mockResolvedValue(undefined),
		disarmHandsFree: vi.fn().mockResolvedValue(undefined),
		startRecording: vi.fn(async () => {
			recording.state.recording = true;
		}),
		stopRecording: vi.fn().mockResolvedValue(result),
	};
	let hook: ReturnType<typeof useDictation>;
	function terminal(sessionId: string, chat = false) {
		const id = terminalsStore.add({ sessionId, fontSize: 14, name: sessionId, cwd: null, awaitingInput: null });
		terminalsStore.setActive(id);
		if (chat) terminalsStore.setViewMode(id, "chat");
		return id;
	}
	function editor(id?: string) {
		const root = document.createElement("div");
		if (id) root.dataset.terminalId = id;
		const mount = document.createElement("div");
		mount.dataset.composeEditor = "";
		root.append(mount);
		document.body.append(root);
		const view = new EditorView({
			parent: mount,
			state: EditorState.create({ doc: "before THIS after", selection: { anchor: 7, head: 11 } }),
		});
		views.push(view);
		return view;
	}
	beforeEach(async () => {
		vi.clearAllMocks();
		vi.spyOn(appLogger, "info").mockImplementation(() => {});
		vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		vi.spyOn(appLogger, "debug").mockImplementation(() => {});
		// The Messages bell persists through a debounced timer that outlives the test.
		vi.spyOn(toastsStore, "addToBell").mockReturnValue(-1);
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		// happy-dom lacks execCommand. This adapter models its current-focused
		// editor insertion through CM, avoiding happy-dom's reentrant DOM observer.
		Object.defineProperty(document, "execCommand", {
			configurable: true,
			value: vi.fn((_command: string, _ui: boolean, text: string) => {
				const focused = document.activeElement;
				const view = focused instanceof HTMLElement ? EditorView.findFromDOM(focused) : null;
				if (!view) return false;
				view.dispatch(view.state.replaceSelection(text));
				return true;
			}),
		});
		recording.state.recording = false;
		recording.stopRecording.mockResolvedValue(result);
		recording.refreshHandsFree.mockResolvedValue(undefined);
		mockInvoke.mockResolvedValueOnce({}).mockResolvedValueOnce(undefined);
		await dictationStore.saveConfig({ auto_send: false });
		hook = useDictation({ pty: { write }, dictation: recording, setStatusInfo: status, openSettings: vi.fn() });
	});
	afterEach(async () => {
		// Delivery restores PTY focus in a frame after handleDictationStop resolves.
		await nextAnimationFrame();
		for (const view of views.splice(0)) view.destroy();
		document.body.replaceChildren();
		vi.restoreAllMocks();
	});

	// Catches: read-only Chat transcript focus falls through to the hidden CLI PTY.
	it("inserts into Chat Compose selection when the transcript is focused", async () => {
		const id = terminal("chat-session", true);
		const view = editor(id);
		const transcript = document.createElement("div");
		transcript.tabIndex = 0;
		document.body.append(transcript);
		transcript.focus();
		await hook.handleDictationStart();
		await hook.handleDictationStop();
		expect(view.state.doc.toString()).toBe("before hello world after");
		expect(write).not.toHaveBeenCalled();
	});

	// Catches: execCommand inserts into the current DOM selection rather than the captured editor.
	it("keeps the original editor selection when focus moves during the final pass", async () => {
		const first = editor();
		const second = editor();
		first.focus();
		await hook.handleDictationStart();
		const final = deferred<typeof result>();
		recording.stopRecording.mockReturnValueOnce(final.promise);
		const stopping = hook.handleDictationStop();
		second.focus();
		final.resolve(result);
		await stopping;
		expect(first.state.doc.toString()).toBe("before hello world after");
		expect(second.state.doc.toString()).toBe("before THIS after");
	});

	// Catches: getActive at delivery redirects speech into a newly selected terminal.
	it("writes to the captured CLI session when terminals switch during the final pass", async () => {
		terminal("original-session");
		await hook.handleDictationStart();
		const final = deferred<typeof result>();
		recording.stopRecording.mockReturnValueOnce(final.promise);
		const stopping = hook.handleDictationStop();
		terminal("other-session");
		final.resolve(result);
		await stopping;
		expect(write.mock.calls).toEqual([["original-session", "hello world"]]);
	});
	// Catches: hands-free refresh yields before the destination has been captured.
	it("captures BODY-focused Chat before hands-free refresh can switch terminals", async () => {
		const id = terminal("chat-start", true);
		const view = editor(id);
		const refreshed = deferred<void>();
		recording.refreshHandsFree.mockReturnValueOnce(refreshed.promise);
		const starting = hook.handleDictationStart();
		terminal("later-session");
		refreshed.resolve();
		await starting;
		await hook.handleDictationStop();
		expect(view.state.doc.toString()).toBe("before hello world after");
		expect(write).not.toHaveBeenCalled();
	});

	// Catches: a disconnected editor or replaced PTY silently receives or loses speech.
	it.each(["editor removed", "session removed", "session replaced", "draft edited"])(
		"retains text without redirecting when %s",
		async (change) => {
			const id = terminal("disappearing", change !== "session replaced");
			const view = change === "session replaced" ? null : editor(id);
			await hook.handleDictationStart();
			if (change === "editor removed") view?.dom.remove();
			if (change === "session removed") terminalsStore.remove(id);
			if (change === "session replaced") terminalsStore.update(id, { sessionId: "replacement" });
			if (change === "draft edited") view?.dispatch({ changes: { from: 0, insert: "new " } });
			terminal("fallback-session");
			await hook.handleDictationStop();
			expect(write).not.toHaveBeenCalled();
			expect(
				toastsStore.toasts.some(
					(toast) => toast.title === "Dictation was not delivered" && toast.message.includes("hello world"),
				),
			).toBe(true);
			expect(status).toHaveBeenLastCalledWith("Dictation: target unavailable — transcript kept in Messages");
			if (change === "draft edited") expect(view?.state.doc.toString()).toBe("new before THIS after");
		},
	);

	// Catches: review-only dictation bypasses framing and drops the beginning too.
	it.each([false, true])("frames long Claude dictation with auto-send=%s", async (autoSend) => {
		const id = terminal("long-dictation-session");
		terminalsStore.update(id, { agentType: "claude" });
		mockInvoke.mockResolvedValueOnce({}).mockResolvedValueOnce(undefined);
		await dictationStore.saveConfig({ auto_send: autoSend });
		recording.stopRecording.mockResolvedValueOnce({ ...result, text: claudeLongInput.sent, duration_s: 82 });
		await hook.handleDictationStart();
		await hook.handleDictationStop();
		const payload = `\x1b[200~${claudeLongInput.sent}\x1b[201~`;
		expect(write.mock.calls).toEqual(
			(autoSend ? [payload, "\r"] : [payload]).map((data) => ["long-dictation-session", data]),
		);
	});

	// Catches: dictation writes Ctrl-U and wipes the user's existing prompt (pasted image included).
	it.each<AgentType | null>(["claude", null])(
		"never writes Ctrl-U when auto-sending dictation (agent=%s)",
		async (agentType) => {
			const id = terminal("append-session");
			if (agentType) terminalsStore.update(id, { agentType });
			mockInvoke.mockResolvedValueOnce({}).mockResolvedValueOnce(undefined);
			await dictationStore.saveConfig({ auto_send: true });
			await hook.handleDictationStart();
			await hook.handleDictationStop();
			const sent = write.mock.calls.map((c) => c[1]);
			expect(sent.join("")).not.toContain("\x15");
			expect(sent).toEqual(["hello world", "\r"]);
		},
	);

	// Catches: metadata accidentally includes a dictated body or omits the correlation id.
	it("logs correlated delivery metadata without transcript text", async () => {
		terminal("metadata-session");
		await hook.handleDictationStart();
		await hook.handleDictationStop();
		const delivery = vi.mocked(appLogger.info).mock.calls.find((call) => call[1] === "Dictation delivered");
		expect(delivery?.[2]).toEqual({
			recordingId: expect.any(String),
			targetKind: "pty",
			terminalId: expect.any(String),
			sessionId: "metadata-session",
			characterCount: 11,
			outcome: "delivered",
		});
		const captured = vi.mocked(appLogger.info).mock.calls.find((call) => call[1] === "Dictation target captured");
		if (!delivery) throw new Error("Missing delivery metadata");
		expect(captured?.[2]).toMatchObject({ recordingId: (delivery[2] as { recordingId: string }).recordingId });
		expect(JSON.stringify(vi.mocked(appLogger.info).mock.calls)).not.toContain("hello world");
	});
});
