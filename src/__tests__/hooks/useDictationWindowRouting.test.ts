import { createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({
	nextId: 0,
	callbacks: new Map<number, (event: { payload: unknown }) => void>(),
	listeners: new Map<number, { event: string; target: { kind: string; label?: string }; handler: number }>(),
	state: { enabled: true, hotkey: "Fn", capturingHotkey: false, longPressMs: 0 },
}));

// Keep installed Tauri event and WebviewWindow implementations: in particular,
// global listen defaults to Any, whereas WebviewWindow.listen addresses itself.
vi.mock("@tauri-apps/api/event", async (importOriginal) => importOriginal());
vi.mock("@tauri-apps/api/window", async (importOriginal) => importOriginal());
vi.mock("@tauri-apps/api/webviewWindow", async (importOriginal) => importOriginal());
vi.mock("@tauri-apps/api/core", async (importOriginal) => importOriginal());
vi.mock("../../stores/dictation", () => ({ dictationStore: { state: native.state } }));
vi.mock("../../transport", () => ({ isTauri: () => true }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { error: vi.fn() } }));

import { useDictationHotkey } from "../../hooks/useDictationHotkey";

// Mirrors pinned Tauri listener.rs match_any_or_filter: an Any registration
// receives even a labeled emit. This is the regression the old Map mock hid.
function emitTo(label: string, event: string) {
	for (const listener of native.listeners.values()) {
		if (listener.event === event && (listener.target.kind === "Any" || listener.target.label === label)) {
			native.callbacks.get(listener.handler)?.({ payload: undefined });
		}
	}
}

const disposers: Array<() => void> = [];
function mount(label: string) {
	Object.assign((window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }).__TAURI_INTERNALS__, {
		metadata: { currentWindow: { label }, currentWebview: { label } },
	});
	const onStart = vi.fn();
	const onStop = vi.fn();
	let dispose = () => {};
	createRoot((cleanup) => {
		dispose = cleanup;
		useDictationHotkey({ onStart, onStop });
	});
	disposers.push(dispose);
	return { onStart, onStop, dispose };
}

async function registered() {
	// Tauri listen and the guarded listener setup each resolve asynchronously.
	await vi.advanceTimersByTimeAsync(0);
}

beforeEach(() => {
	vi.useFakeTimers();
	Object.assign((window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }).__TAURI_INTERNALS__, {
		transformCallback: (callback: (event: { payload: unknown }) => void) => {
			const id = ++native.nextId;
			native.callbacks.set(id, callback);
			return id;
		},
		invoke: async (
			command: string,
			args: { event: string; target: { kind: string; label?: string }; handler: number; eventId: number },
		) => {
			if (command === "plugin:event|listen") {
				const id = ++native.nextId;
				native.listeners.set(id, args);
				return id;
			}
			if (command === "plugin:event|unlisten") native.listeners.delete(args.eventId);
		},
	});
	native.state.longPressMs = 0;
	vi.stubGlobal("__TAURI_EVENT_PLUGIN_INTERNALS__", { unregisterListener: vi.fn() });
});
afterEach(() => {
	for (const dispose of disposers.splice(0)) dispose();
	native.listeners.clear();
	native.callbacks.clear();
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

describe("native Fn multi-WebView ownership", () => {
	it.each(["main", "panel-ai-chat"])(
		"does not start or deliver in the other window when %s receives Fn",
		async (owner) => {
			const main = mount("main");
			const detached = mount("panel-ai-chat");
			await registered();
			emitTo(owner, "fn-key-down");
			emitTo("main", "fn-key-up");
			emitTo("panel-ai-chat", "fn-key-up");
			const active = owner === "main" ? main : detached;
			const inactive = owner === "main" ? detached : main;
			expect(active.onStart).toHaveBeenCalledOnce();
			expect(active.onStop).toHaveBeenCalledOnce();
			expect(inactive.onStart).not.toHaveBeenCalled();
			expect(inactive.onStop).not.toHaveBeenCalled();
		},
	);

	it("does not arm the detached long-press timer from a main-window Fn press", async () => {
		native.state.longPressMs = 300;
		const main = mount("main");
		const detached = mount("panel-ai-chat");
		await registered();
		emitTo("main", "fn-key-down");
		await vi.advanceTimersByTimeAsync(300);
		emitTo("panel-ai-chat", "fn-key-up");
		emitTo("main", "fn-key-up");
		expect(main.onStart).toHaveBeenCalledOnce();
		expect(main.onStop).toHaveBeenCalledOnce();
		expect(detached.onStart).not.toHaveBeenCalled();
		expect(detached.onStop).not.toHaveBeenCalled();
	});

	it("disposes subscriptions that resolve after the detached hook closes", async () => {
		const detached = mount("panel-ai-chat");
		detached.dispose();
		await registered();
		emitTo("panel-ai-chat", "fn-key-down");
		emitTo("panel-ai-chat", "fn-key-up");
		expect(detached.onStart).not.toHaveBeenCalled();
		expect(detached.onStop).not.toHaveBeenCalled();
		expect(native.listeners.size).toBe(0);
	});

	it("removes detached listeners during a take without handing its release to main", async () => {
		const main = mount("main");
		const detached = mount("panel-ai-chat");
		await registered();
		emitTo("panel-ai-chat", "fn-key-down");
		detached.dispose();
		await registered();
		emitTo("main", "fn-key-up");
		emitTo("panel-ai-chat", "fn-key-up");
		expect(main.onStart).not.toHaveBeenCalled();
		expect(main.onStop).not.toHaveBeenCalled();
		expect(detached.onStop).not.toHaveBeenCalled();
		emitTo("main", "fn-key-down");
		emitTo("main", "fn-key-up");
		expect(main.onStart).toHaveBeenCalledOnce();
		expect(main.onStop).toHaveBeenCalledOnce();
		expect(detached.onStart).toHaveBeenCalledOnce();
	});
});
