import { Component, Show, createSignal } from "solid-js";
import { keybindingsStore } from "../../stores/keybindings";
import { normalizeCombo } from "../../keybindingDefaults";
import s from "./KeyComboCapture.module.css";

/** Bare modifier keys — ignored when pressed alone */
const BARE_MODIFIERS = new Set(["Meta", "Control", "Alt", "Shift"]);

export interface KeyComboCaptureProps {
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

  const conflictingAction = () => {
    if (!props.value) return null;
    const normalized = normalizeCombo(props.value);
    const action = keybindingsStore.getActionForCombo(normalized);
    if (!action) return null;
    const exclude = props.exclude ?? [];
    return exclude.includes(action) ? null : action;
  };

  const startCapture = () => {
    setCapturing(true);
    props.onCapturingChange?.(true);
  };

  const stopCapture = () => {
    setCapturing(false);
    props.onCapturingChange?.(false);
  };

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

  const displayLabel = () =>
    props.value || props.placeholder || "Click to set shortcut";

  return (
    <div class={s.root}>
      <Show
        when={capturing()}
        fallback={
          <button
            class={s.display}
            onClick={startCapture}
            title="Click to change shortcut"
          >
            {displayLabel()}
          </button>
        }
      >
        <input
          class={s.input}
          placeholder={props.placeholder ?? "Press a key combination..."}
          onKeyDown={handleKeyDown}
          onBlur={stopCapture}
          ref={(el) => requestAnimationFrame(() => el.focus())}
          readonly
        />
      </Show>
      <Show when={conflictingAction()}>
        {(action) => (
          <span class={s.collision}>Conflicts with: {action()}</span>
        )}
      </Show>
    </div>
  );
};
