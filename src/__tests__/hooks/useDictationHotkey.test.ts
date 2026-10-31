import { createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from "vitest";

const { state, listeners } = vi.hoisted(() => ({
	state: { enabled: true, hotkey: "F8", capturingHotkey: false, longPressMs: 0 },
	listeners: new Map<string, () => void>(),
}));

vi.mock("../../stores/dictation", () => ({ dictationStore: { state } }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { error: vi.fn() } }));
vi.mock("../../transport", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
	getCurrentWebviewWindow: () => ({
		listen: vi.fn((name: string, callback: () => void) => {
			listeners.set(name, callback);
			return Promise.resolve(() => listeners.delete(name));
		}),
	}),
}));

import { useDictationHotkey } from "../../hooks/useDictationHotkey";

describe("push-to-talk release recovery", () => {
	let dispose: () => void;
	let onStart: Mock<() => void>;
	let onStop: Mock<() => void>;

	beforeEach(() => {
		state.hotkey = "F8";
		onStart = vi.fn();
		onStop = vi.fn();
	});

	afterEach(() => {
		dispose?.();
		listeners.clear();
	});

	it("releases a held hotkey when focus leaves and accepts the next press", () => {
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useDictationHotkey({ onStart, onStop });
		});
		window.dispatchEvent(new KeyboardEvent("keydown", { code: "F8" }));
		window.dispatchEvent(new Event("blur"));
		expect(onStart).toHaveBeenCalledOnce();
		expect(onStop).toHaveBeenCalledOnce();

		window.dispatchEvent(new KeyboardEvent("keydown", { code: "F8" }));
		window.dispatchEvent(new KeyboardEvent("keyup", { code: "F8" }));
		expect(onStart).toHaveBeenCalledTimes(2);
		expect(onStop).toHaveBeenCalledTimes(2);
	});

	// Fn is a global hotkey: its key-up arrives natively whatever window has focus,
	// and the backend stops capture on it. A focus change while Fn is held (another
	// app, a notification) must not cut the recording short.
	it("keeps recording native Fn across focus loss until its key-up", async () => {
		state.hotkey = "Fn";
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useDictationHotkey({ onStart, onStop });
		});
		await Promise.resolve();
		listeners.get("fn-key-down")?.();
		window.dispatchEvent(new Event("blur"));
		expect(onStart).toHaveBeenCalledOnce();
		expect(onStop).not.toHaveBeenCalled();

		listeners.get("fn-key-up")?.();
		expect(onStop).toHaveBeenCalledOnce();
	});

	// Catches: broadcasting native release stops main when only detached chat saw key-down.
	it("ignores native Fn release in a window that never received key-down", async () => {
		state.hotkey = "Fn";
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useDictationHotkey({ onStart, onStop });
		});
		await Promise.resolve();
		listeners.get("fn-key-up")?.();
		expect(onStart).not.toHaveBeenCalled();
		expect(onStop).not.toHaveBeenCalled();
	});

	it("forgets held modifiers after focus loss", () => {
		state.hotkey = "Cmd+F8";
		createRoot((rootDispose) => {
			dispose = rootDispose;
			useDictationHotkey({ onStart, onStop });
		});
		window.dispatchEvent(new KeyboardEvent("keydown", { code: "MetaLeft" }));
		window.dispatchEvent(new KeyboardEvent("keydown", { code: "F8" }));
		window.dispatchEvent(new Event("blur"));
		window.dispatchEvent(new KeyboardEvent("keydown", { code: "F8" }));
		expect(onStart).toHaveBeenCalledOnce();
		expect(onStop).toHaveBeenCalledOnce();
	});
});
