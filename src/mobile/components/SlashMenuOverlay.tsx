import { createSignal, createMemo, For } from "solid-js";
import type { SlashMenuItem } from "../useSessions";
import styles from "./SlashMenuOverlay.module.css";

interface SlashMenuOverlayProps {
  sessionId: string;
  items: SlashMenuItem[];
  onSelect: (command: string) => void;
  onDismiss: () => void;
}

export function SlashMenuOverlay(props: SlashMenuOverlayProps) {
  const [filter, setFilter] = createSignal("");
  let inputEl: HTMLInputElement | undefined;

  const filteredItems = createMemo(() => {
    const q = filter().toLowerCase();
    if (!q) return props.items;
    return props.items.filter(
      (item) =>
        item.command.toLowerCase().includes(q) ||
        item.description.toLowerCase().includes(q),
    );
  });

  const select = (command: string) => {
    props.onSelect(command);
    props.onDismiss();
  };

  const handleBackdropClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) {
      props.onDismiss();
    }
  };

  return (
    <div class={styles.backdrop} onClick={handleBackdropClick}>
      <div class={styles.sheet}>
        <div class={styles.filterBar}>
          <input
            ref={inputEl}
            class={styles.filterInput}
            type="text"
            placeholder="Filter commands..."
            value={filter()}
            onInput={(e) => setFilter(e.currentTarget.value)}
            autocomplete="off"
            autocorrect="off"
            spellcheck={false}
          />
        </div>
        <div class={styles.list}>
          <For each={filteredItems()}>
            {(item) => (
              <button
                class={styles.item}
                classList={{ [styles.itemHighlighted]: item.highlighted }}
                onClick={() => select(item.command)}
              >
                <span class={styles.command}>{item.command}</span>
                <span class={styles.description}>{item.description}</span>
              </button>
            )}
          </For>
        </div>
      </div>
    </div>
  );
}
