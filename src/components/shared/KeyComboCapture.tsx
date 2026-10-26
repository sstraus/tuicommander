import { type Component, createSignal, onCleanup, Show } from "solid-js";
import { useNativeKeyCombo } from "../../hooks/useNativeKeyCombo";
import { listen } from "../../invoke";
import { normalizeCombo } from "../../keybindingDefaults";
import type { SettingId } from "../../services/settingsCatalogue";
import { keybindingsStore } from "../../stores/keybindings";
import { isTauri } from "../../transport";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import s from "./KeyComboCapture.module.css";

/** Bare modifier keys — ignored when pressed alone */
const BARE_MODIFIERS = new Set(["Meta", "Control", "Alt", "Shift"]);

export interface KeyComboCaptureProps {
	settingId?: SettingId;
	/** Current key combo value (e.g. "Cmd+Shift+D") */
	value: string;
	/** Called with newly captured combo */
	onChange: (combo: string) => void;
	/** Placeholder shown when value is empty or in capture mode */
	placeholder?: string;
	/** Action names excluded from collision checks (use for self-editing) */
	exclude?: string[];
	/** Called when capture mode starts (true) or ends (false) */
	onCapturingChange?: (capturing: boolean) => void;
}

/**
 * Button that enters capture mode on click, records the next key combo,
 * and emits it via onChange. Shows a collision warning when the current
 * value conflicts with a registered keybinding.
 */
export const KeyComboCapture: Component<KeyComboCaptureProps> = (props) => {
	const [capturing, setCapturing] = createSignal(false);

	// F13-F20 never reach the WebView on macOS, exactly like Fn below. Rust catches them
	// with an NSEvent monitor and re-emits them; without this they look unbindable even
	// though `validateGlobalHotkeyCombo` and `global-hotkey` both accept F13-F24.
	useNativeKeyCombo(capturing, (combo) => {
		props.onChange(combo);
		stopCapture();
	});

	const conflictingAction = () => {
		if (!props.value) return null;
		const normalized = normalizeCombo(props.value);
		const action = keybindingsStore.getActionForCombo(normalized);
		if (!action) return null;
		const exclude = props.exclude ?? [];
		return exclude.includes(action) ? null : action;
	};

	let unlistenFn: (() => void) | undefined;
	let fnCancelled = false;

	const startCapture = () => {
		setCapturing(true);
		props.onCapturingChange?.(true);
		fnCancelled = false;

		// Listen for Fn/Globe key via native macOS monitor (not in DOM)
		if (isTauri()) {
			listen("fn-key-down", () => {
				if (capturing()) {
					props.onChange("Fn");
					stopCapture();
				}
			})
				.then((fn) => {
					fnCancelled ? fn() : (unlistenFn = fn);
				})
				.catch(() => {
					/* Fn key not available on this platform */
				});
		}
	};

	const stopCapture = () => {
		setCapturing(false);
		props.onCapturingChange?.(false);
		fnCancelled = true;
		unlistenFn?.();
		unlistenFn = undefined;
	};

	onCleanup(() => {
		fnCancelled = true;
		unlistenFn?.();
	});

	const handleKeyDown = (e: KeyboardEvent) => {
		e.preventDefault();
		e.stopPropagation();

		const key = e.key;

		// Escape cancels without recording
		if (key === "Escape") {
			stopCapture();
			return;
		}

		// Ignore bare modifier presses — wait for a real key
		if (BARE_MODIFIERS.has(key)) return;

		const parts: string[] = [];
		if (e.metaKey) parts.push("Cmd");
		if (e.ctrlKey) parts.push("Ctrl");
		if (e.altKey) parts.push("Alt");
		if (e.shiftKey) parts.push("Shift");

		if (key === " ") {
			parts.push("Space");
		} else if (key.length === 1) {
			parts.push(key.toUpperCase());
		} else {
			parts.push(key);
		}

		props.onChange(parts.join("+"));
		stopCapture();
	};

	const displayLabel = () => props.value || props.placeholder || "Click to set shortcut";

	return (
		<div class={s.root}>
			<Show
				when={capturing()}
				fallback={
					<Button
						data-setting-id={props.settingId}
						class={s.display}
						onClick={startCapture}
						title="Click to change shortcut"
					>
						{displayLabel()}
					</Button>
				}
			>
				<Input
					data-setting-id={props.settingId}
					class={s.input}
					placeholder={props.placeholder ?? "Press a key combination..."}
					onKeyDown={handleKeyDown}
					onBlur={stopCapture}
					ref={(el) => requestAnimationFrame(() => el.focus())}
					readonly
				/>
			</Show>
			<Show when={conflictingAction()}>{(action) => <span class={s.collision}>Conflicts with: {action()}</span>}</Show>
		</div>
	);
};
