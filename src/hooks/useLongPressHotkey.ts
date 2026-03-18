/**
 * Long-press hotkey detection using tauri-plugin-user-input.
 *
 * Listens for global keyboard events and distinguishes short presses
 * (which pass through as normal input) from long presses (which trigger
 * a callback). Key repeat is filtered.
 */

import { parseHotkey, isPluginModifierKey, updateModifierState, modifiersMatch } from "../utils";
import type { ModifierState, ParsedHotkey } from "../utils";

/** Minimal event shape matching tauri-plugin-user-input InputEvent */
export interface KeyEvent {
  eventType: string;
  key?: string;
}

/** Callbacks for long-press lifecycle */
export interface LongPressCallbacks {
  onStart: () => void;
  onStop: () => void;
}

/**
 * Pure state machine for long-press hotkey detection.
 * Decoupled from Tauri plugin — can be fed events directly for testing.
 */
export function createLongPressHandler(
  parsed: ParsedHotkey,
  longPressMs: number,
  callbacks: LongPressCallbacks,
) {
  let longPressTimer: ReturnType<typeof setTimeout> | null = null;
  let hotkeyDown = false;
  let dictationStarted = false;
  const mods: ModifierState = { cmd: false, shift: false, alt: false, ctrl: false };

  const handleEvent = (event: KeyEvent) => {
    const key = event.key;
    if (!key) return;

    const isPress = event.eventType === "KeyPress";
    const isRelease = event.eventType === "KeyRelease";
    if (!isPress && !isRelease) return;

    // Track modifier state
    if (isPluginModifierKey(key)) {
      updateModifierState(mods, key, isPress);
      return;
    }

    // Only act on the configured hotkey's primary key
    if (key !== parsed.key) return;

    if (isPress) {
      // Ignore key repeat (KeyPress without prior KeyRelease)
      if (hotkeyDown) return;

      // Check modifier requirements
      if (!modifiersMatch(parsed, mods)) return;

      hotkeyDown = true;
      dictationStarted = false;

      // Start long-press timer
      longPressTimer = setTimeout(() => {
        longPressTimer = null;
        dictationStarted = true;
        callbacks.onStart();
      }, longPressMs);
    } else if (isRelease && hotkeyDown) {
      hotkeyDown = false;

      if (longPressTimer !== null) {
        // Released before threshold — short press, passes through
        clearTimeout(longPressTimer);
        longPressTimer = null;
      } else if (dictationStarted) {
        // Released after dictation started — stop recording
        dictationStarted = false;
        callbacks.onStop();
      }
    }
  };

  const cleanup = () => {
    if (longPressTimer !== null) {
      clearTimeout(longPressTimer);
      longPressTimer = null;
    }
  };

  /** Expose internal state for testing */
  const getState = () => ({ hotkeyDown, dictationStarted, mods: { ...mods } });

  return { handleEvent, cleanup, getState };
}

/**
 * Parse a hotkey string and create a long-press handler.
 * Returns null if the hotkey is invalid.
 */
export function createLongPressHandlerFromHotkey(
  hotkey: string,
  longPressMs: number,
  callbacks: LongPressCallbacks,
) {
  const parsed = parseHotkey(hotkey);
  if (!parsed) return null;
  return createLongPressHandler(parsed, longPressMs, callbacks);
}
