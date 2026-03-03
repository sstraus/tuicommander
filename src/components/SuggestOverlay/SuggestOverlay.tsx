import { Component, For, onCleanup, onMount } from "solid-js";
import styles from "./SuggestOverlay.module.css";

interface SuggestOverlayProps {
  items: string[];
  onSelect: (text: string) => void;
  onDismiss: () => void;
}

const DISMISS_TIMEOUT_MS = 30_000;

const SuggestOverlay: Component<SuggestOverlayProps> = (props) => {
  let timer: ReturnType<typeof setTimeout> | undefined;

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      props.onDismiss();
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
    timer = setTimeout(() => props.onDismiss(), DISMISS_TIMEOUT_MS);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
    if (timer) clearTimeout(timer);
  });

  return (
    <div class={styles.overlay}>
      <For each={props.items}>
        {(item) => (
          <button class={styles.chip} onClick={() => props.onSelect(item)}>
            {item}
          </button>
        )}
      </For>
    </div>
  );
};

export default SuggestOverlay;
