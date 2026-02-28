import { Show, createSignal, createEffect, onCleanup } from "solid-js";
import { listen } from "../../invoke";
import { dictationStore } from "../../stores/dictation";
import styles from "./DictationToast.module.css";

/**
 * Floating toast that shows partial transcription results during streaming
 * dictation. Positioned above the status bar, auto-shows when recording
 * starts and hides when recording stops.
 */
export function DictationToast() {
  const [partialText, setPartialText] = createSignal("");
  const [visible, setVisible] = createSignal(false);
  const [exiting, setExiting] = createSignal(false);

  // Listen for partial transcription events from the streaming thread
  let unlisten: (() => void) | undefined;

  createEffect(() => {
    listen<string>("dictation-partial", (event) => {
      setPartialText(event.payload);
      if (!visible()) {
        setExiting(false);
        setVisible(true);
      }
    }).then((fn) => {
      unlisten = fn;
    });
  });

  onCleanup(() => {
    unlisten?.();
  });

  // Auto-hide when recording stops
  createEffect(() => {
    if (!dictationStore.state.recording && visible()) {
      setExiting(true);
      const timer = setTimeout(() => {
        setVisible(false);
        setExiting(false);
        setPartialText("");
      }, 150); // match fadeOut duration
      onCleanup(() => clearTimeout(timer));
    }
  });

  return (
    <Show when={visible()}>
      <div class={styles.toast} data-exiting={exiting()}>
        <span class={styles.indicator} />
        <span class={styles.text}>
          {partialText() || "Listening"}
          <span class={styles.dots}>...</span>
        </span>
      </div>
    </Show>
  );
}
