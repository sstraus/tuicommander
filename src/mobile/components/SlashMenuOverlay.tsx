import { For } from "solid-js";
import { rpc } from "../../transport";
import type { SlashMenuItem } from "../useSessions";
import styles from "./SlashMenuOverlay.module.css";

interface SlashMenuOverlayProps {
  sessionId: string;
  items: SlashMenuItem[];
  onSelect: (command: string) => void;
  onDismiss: () => void;
}

const ARROW_UP = "\x1b[A";
const ARROW_DOWN = "\x1b[B";

export function SlashMenuOverlay(props: SlashMenuOverlayProps) {
  const select = (command: string) => {
    props.onSelect(command);
    props.onDismiss();
  };

  const sendArrows = (arrow: string, count: number, e: MouseEvent) => {
    e.stopPropagation();
    const batch = arrow.repeat(count);
    rpc("write_pty", { sessionId: props.sessionId, data: batch }).catch(() => {});
  };

  const pageSize = () => Math.max(props.items.length, 1);

  const handleBackdropClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) {
      props.onDismiss();
    }
  };

  return (
    <div class={styles.backdrop} onClick={handleBackdropClick}>
      <div class={styles.sheet}>
        <For each={props.items}>
          {(item) => (
            <button
              class={styles.item}
              onClick={() => select(item.command)}
            >
              <span class={styles.command}>{item.command}</span>
              <span class={styles.description}>{item.description}</span>
            </button>
          )}
        </For>
        <div class={styles.arrowBar}>
          <button class={styles.arrowBtn} onClick={(e) => sendArrows(ARROW_UP, pageSize(), e)} title="Page up">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <path d="M18 11l-6-6-6 6" />
              <path d="M18 18l-6-6-6 6" />
            </svg>
          </button>
          <button class={styles.arrowBtn} onClick={(e) => sendArrows(ARROW_UP, 1, e)} title="Up">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <path d="M18 15l-6-6-6 6" />
            </svg>
          </button>
          <button class={styles.arrowBtn} onClick={(e) => sendArrows(ARROW_DOWN, 1, e)} title="Down">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <path d="M6 9l6 6 6-6" />
            </svg>
          </button>
          <button class={styles.arrowBtn} onClick={(e) => sendArrows(ARROW_DOWN, pageSize(), e)} title="Page down">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <path d="M6 6l6 6 6-6" />
              <path d="M6 13l6 6 6-6" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  );
}
