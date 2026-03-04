import { createSignal, onMount, onCleanup, For, Index } from "solid-js";
import { subscribePty } from "../../transport";
import { type LogLine, normalizeLogLine, spanStyle } from "../utils/logLine";
import styles from "./OutputView.module.css";

const MAX_LINES = 500;

interface OutputViewProps {
  sessionId: string;
}

export function OutputView(props: OutputViewProps) {
  const [lines, setLines] = createSignal<LogLine[]>([]);
  let containerEl: HTMLDivElement | undefined;
  let unsubscribe: (() => void) | null = null;

  async function fetchInitialOutput() {
    try {
      const resp = await fetch(`/sessions/${props.sessionId}/output?format=log`);
      if (resp.ok) {
        const json = await resp.json() as { lines: unknown[] };
        if (json.lines && json.lines.length > 0) {
          setLines(json.lines.slice(-MAX_LINES).map(normalizeLogLine));
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
      () => {}, // unused — onLogLines handles log delivery
      () => {
        setLines((prev) => [...prev, { spans: [{ text: "--- session exited ---" }] }]);
      },
      {
        format: "log",
        onLogLines(rawLines) {
          setLines((prev) => {
            const incoming = rawLines.map(normalizeLogLine);
            return [...prev, ...incoming].slice(-MAX_LINES);
          });
          scrollToBottom();
        },
      },
    )) ?? null;
  });

  onCleanup(() => {
    unsubscribe?.();
  });

  return (
    <div ref={containerEl} class={styles.output}>
      <pre class={styles.text}>
        <For each={lines()}>
          {(line) => (
            <div class={styles.line}>
              <Index each={line.spans}>
                {(span) => {
                  const st = spanStyle(span());
                  return st
                    ? <span style={st}>{span().text}</span>
                    : <>{span().text}</>;
                }}
              </Index>
            </div>
          )}
        </For>
      </pre>
    </div>
  );
}
