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
      const resp = await fetch(`/sessions/${props.sessionId}/output?format=text&limit=8192`);
      if (resp.ok) {
        const json = await resp.json() as { data: string };
        if (json.data) {
          setLines(json.data.split("\n"));
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
          const newLines = data.split("\n");
          // Append to last line if no newline at start
          if (prev.length > 0 && newLines.length > 0) {
            const merged = [...prev];
            merged[merged.length - 1] += newLines[0];
            return [...merged, ...newLines.slice(1)].slice(-500);
          }
          return [...prev, ...newLines].slice(-500);
        });
        scrollToBottom();
      },
      () => {
        setLines((prev) => [...prev, "--- session exited ---"]);
      },
      { stripAnsi: true },
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
