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
      e.preventDefault();
      e.stopPropagation();
      props.onDismiss();
      return;
    }
    // Number keys 1-4 select the corresponding suggestion
    const num = parseInt(e.key, 10);
    if (num >= 1 && num <= 4 && num <= props.items.length) {
      e.preventDefault();
      props.onSelect(props.items[num - 1]);
      return;
    }
    // Any printable key (typing) dismisses the overlay
    if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      props.onDismiss();
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown, true);
    timer = setTimeout(() => props.onDismiss(), DISMISS_TIMEOUT_MS);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown, true);
    if (timer) clearTimeout(timer);
  });

  return (
    <div class={styles.overlay}>
      <For each={props.items}>
        {(item, index) => (
          <button class={styles.chip} onClick={() => props.onSelect(item)}>
            <span class={styles.shortcut} data-shortcut>
              {index() + 1}
            </span>
            {item}
          </button>
        )}
      </For>
    </div>
  );
};

export default SuggestOverlay;
