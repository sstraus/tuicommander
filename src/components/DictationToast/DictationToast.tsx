import { Show, createSignal, createEffect, onCleanup } from "solid-js";
import { dictationStore } from "../../stores/dictation";
import styles from "./DictationToast.module.css";

/**
 * Floating toast that shows partial transcription results during streaming
 * dictation. Positioned above the status bar, auto-shows when partials arrive
 * and hides when recording stops.
 */
export function DictationToast() {
  const [visible, setVisible] = createSignal(false);
  const [exiting, setExiting] = createSignal(false);

  // Show toast when partialText becomes non-empty
  createEffect(() => {
    if (dictationStore.state.partialText) {
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
