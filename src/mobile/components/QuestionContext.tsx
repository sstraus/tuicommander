import { Show, createSignal } from "solid-js";
import styles from "./QuestionContext.module.css";

/** Max screen rows to show in the expanded context panel. */
const MAX_CONTEXT_ROWS = 20;

interface QuestionContextProps {
  questionText: string;
  screenText: string[];
}

export function QuestionContext(props: QuestionContextProps) {
  const [expanded, setExpanded] = createSignal(false);

  const contextLines = () => {
    const rows = props.screenText;
    // Take the last N non-empty rows for context
    const trimmed = rows.filter((r) => r.trim().length > 0);
    return trimmed.slice(-MAX_CONTEXT_ROWS);
  };

  return (
    <div class={styles.container}>
      <button
        class={styles.header}
        onClick={() => setExpanded((v) => !v)}
      >
        <span class={styles.icon}>{expanded() ? "\u25BC" : "\u25B6"}</span>
        <span class={styles.text}>{props.questionText}</span>
      </button>
      <Show when={expanded()}>
        <pre class={styles.context}>{contextLines().join("\n")}</pre>
      </Show>
    </div>
  );
}
