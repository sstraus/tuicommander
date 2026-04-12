import { For } from "solid-js";
import type { SlashMenuItem } from "../useSessions";
import styles from "./SlashMenuOverlay.module.css";

interface SlashMenuOverlayProps {
  items: SlashMenuItem[];
  onSelect: (command: string) => void;
}

/** Compact dropup that renders above the input area. Items come pre-filtered
 *  from the backend (Claude Code's own slash menu filtering). */
export function SlashMenuOverlay(props: SlashMenuOverlayProps) {
  return (
    <div class={styles.dropup}>
      <For each={props.items}>
        {(item) => (
          <button
            class={styles.item}
            classList={{ [styles.itemHighlighted]: item.highlighted }}
            onClick={() => props.onSelect(item.command)}
          >
            <span class={styles.command}>{item.command}</span>
            <span class={styles.description}>{item.description}</span>
          </button>
        )}
      </For>
    </div>
  );
}
