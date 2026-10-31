import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { createEffect, onCleanup } from "solid-js";
import { guardTauriUnlisten } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { dictationStore } from "../stores/dictation";
import { isTauri } from "../transport";
import { parseHotkey } from "../utils";
import { createLongPressHandlerFromHotkey } from "./useLongPressHotkey";

interface DictationHotkeyOptions {
	onStart: () => void;
	onStop: () => void;
}

/** Owns native Fn and DOM key lifecycles for push-to-talk dictation. */
export function useDictationHotkey(options: DictationHotkeyOptions): void {
	createEffect(() => {
		const hotkey = dictationStore.state.hotkey;
		const capturing = dictationStore.state.capturingHotkey;
		const longPressMs = dictationStore.state.longPressMs;
		if (!dictationStore.state.enabled || !hotkey || capturing) return;

		const handler = createLongPressHandlerFromHotkey(hotkey, longPressMs, {
			onStart: options.onStart,
			onStop: options.onStop,
		});
		if (!handler) return;

		let cleanupListeners: () => void;
		if (hotkey === "Fn") {
			let cancelled = false;
			let unlistenDown: (() => void) | undefined;
			let unlistenUp: (() => void) | undefined;
			if (isTauri()) {
				const currentWindow = getCurrentWebviewWindow();
				currentWindow
					.listen("fn-key-down", () => handler.handleEvent({ eventType: "KeyPress", key: "Fn" }))
					.then((dispose) => {
						const guarded = guardTauriUnlisten(dispose);
						cancelled ? guarded() : (unlistenDown = guarded);
					})
					.catch((error) => appLogger.error("dictation", "Failed to listen for fn-key-down", error));
				currentWindow
					.listen("fn-key-up", () => handler.handleEvent({ eventType: "KeyRelease", key: "Fn" }))
					.then((dispose) => {
						const guarded = guardTauriUnlisten(dispose);
						cancelled ? guarded() : (unlistenUp = guarded);
					})
					.catch((error) => appLogger.error("dictation", "Failed to listen for fn-key-up", error));
			}
			cleanupListeners = () => {
				cancelled = true;
				unlistenDown?.();
				unlistenUp?.();
			};
		} else {
			const hasModifiers = hotkey.includes("+");
			const onKeyDown = (event: KeyboardEvent) => {
				const consumed = handler.handleEvent({ eventType: "KeyPress", key: event.code });
				if (consumed && (hasModifiers || event.repeat)) event.preventDefault();
			};
			const onKeyUp = (event: KeyboardEvent) => {
				handler.handleEvent({ eventType: "KeyRelease", key: event.code });
			};
			// Window key events stop at focus loss, so a key released in another app
			// never reaches us: treat the blur as that release. Fn is global and
			// reports its own key-up, so it keeps recording across focus changes.
			const primaryKey = parseHotkey(hotkey)?.key;
			const onBlur = () => {
				if (primaryKey) handler.handleEvent({ eventType: "KeyRelease", key: primaryKey });
				for (const key of ["MetaLeft", "ShiftLeft", "AltLeft", "ControlLeft"]) {
					handler.handleEvent({ eventType: "KeyRelease", key });
				}
			};
			window.addEventListener("keydown", onKeyDown);
			window.addEventListener("keyup", onKeyUp);
			window.addEventListener("blur", onBlur);
			cleanupListeners = () => {
				window.removeEventListener("keydown", onKeyDown);
				window.removeEventListener("keyup", onKeyUp);
				window.removeEventListener("blur", onBlur);
			};
		}

		onCleanup(() => {
			handler.cleanup();
			cleanupListeners();
		});
	});
}
