import { createSignal, onMount, onCleanup } from "solid-js";
import { subscribePty } from "../../transport";
import styles from "./OutputView.module.css";

interface OutputViewProps {
  sessionId: string;
}

export function OutputView(props: OutputViewProps) {
  const [lines, setLines] = createSignal<string[]>([]);
  let containerEl: HTMLDivElement | undefined;
  let unsubscribe: (() => void) | null = null;

  async function fetchInitialOutput() {
    try {
      const resp = await fetch(`/sessions/${props.sessionId}/output?format=log`);
      if (resp.ok) {
        const json = await resp.json() as { lines: string[] };
        if (json.lines && json.lines.length > 0) {
          setLines(json.lines.slice(-500));
          scrollToBottom();
        }
      }
    } catch {
      // Silently fail — live stream will populate
    }
  }

  function scrollToBottom() {
    requestAnimationFrame(() => {
      if (containerEl) {
        containerEl.scrollTop = containerEl.scrollHeight;
      }
    });
  }

  onMount(async () => {
    await fetchInitialOutput();

    unsubscribe = (await subscribePty(
      props.sessionId,
      (data) => {
        setLines((prev) => {
          const newLines = data.split("\n").filter((l) => l.length > 0);
          return [...prev, ...newLines].slice(-500);
        });
        scrollToBottom();
      },
      () => {
        setLines((prev) => [...prev, "--- session exited ---"]);
      },
      { format: "log" },
    )) ?? null;
  });

  onCleanup(() => {
    unsubscribe?.();
  });

  return (
    <div ref={containerEl} class={styles.output}>
      <pre class={styles.text}>
        {lines().join("\n")}
      </pre>
    </div>
  );
}
