import { Show, createSignal, createEffect, onCleanup } from "solid-js";
import { dictationStore } from "../../stores/dictation";
import styles from "./DictationToast.module.css";

/**
 * Floating toast that shows partial transcription results during streaming
 * dictation. Positioned above the status bar, auto-shows when recording
 * starts and hides when recording stops.
 *
 * Reads partialText reactively from dictationStore (single event
 * subscription lives in the store, not here).
 */
export function DictationToast() {
  const [visible, setVisible] = createSignal(false);
  const [exiting, setExiting] = createSignal(false);

  // Show toast when recording starts and partialText arrives
  createEffect(() => {
    const text = dictationStore.state.partialText;
    if (text && dictationStore.state.recording && !visible()) {
      setExiting(false);
      setVisible(true);
    }
  });

  // Auto-hide when recording stops
  createEffect(() => {
    if (!dictationStore.state.recording && visible()) {
      setExiting(true);
      const timer = setTimeout(() => {
        setVisible(false);
        setExiting(false);
      }, 150); // match fadeOut duration
      onCleanup(() => clearTimeout(timer));
    }
  });

  return (
    <Show when={visible()}>
      <div class={styles.toast} data-exiting={exiting()}>
        <span class={styles.indicator} />
        <span class={styles.text}>
          {dictationStore.state.partialText || "Listening"}
          <span class={styles.dots} />
        </span>
      </div>
    </Show>
  );
}
