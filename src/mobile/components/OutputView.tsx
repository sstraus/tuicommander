import { createSignal, createMemo, onMount, onCleanup, For, Index } from "solid-js";
import { subscribePty } from "../../transport";
import { type LogLine, normalizeLogLine, spanStyle, lineMatchesQuery } from "../utils/logLine";
import styles from "./OutputView.module.css";

const MAX_LINES = 500;

interface OutputViewProps {
  sessionId: string;
  /** Real-time session state pushed via WebSocket (bypasses 3s polling). */
  onStateChange?: (state: Record<string, unknown>) => void;
  /** When set, only lines matching this query (case-insensitive) are shown. */
  searchQuery?: string;
}

export function OutputView(props: OutputViewProps) {
  const [lines, setLines] = createSignal<LogLine[]>([]);
  let containerEl: HTMLDivElement | undefined;
  let unsubscribe: (() => void) | null = null;

  /** Fetch initial log lines via HTTP; returns the total_lines offset for WS catch-up. */
  async function fetchInitialOutput(): Promise<number> {
    try {
      const resp = await fetch(`/sessions/${props.sessionId}/output?format=log`);
      if (resp.ok) {
        const json = await resp.json() as { lines: unknown[]; total_lines: number };
        if (json.lines && json.lines.length > 0) {
          setLines(json.lines.slice(-MAX_LINES).map(normalizeLogLine));
          scrollToBottom();
        }
        return json.total_lines ?? 0;
      }
    } catch {
      // Silently fail — live stream will populate
    }
    return 0;
  }

  function scrollToBottom() {
    requestAnimationFrame(() => {
      if (containerEl) {
        containerEl.scrollTop = containerEl.scrollHeight;
      }
    });
  }

  onMount(async () => {
    const offset = await fetchInitialOutput();

    unsubscribe = (await subscribePty(
      props.sessionId,
      () => {}, // unused — onLogLines handles log delivery
      () => {
        setLines((prev) => [...prev, { spans: [{ text: "--- session exited ---" }] }]);
      },
      {
        format: "log",
        logOffset: offset,
        onLogLines(rawLines) {
          setLines((prev) => {
            const incoming = rawLines.map(normalizeLogLine);
            return [...prev, ...incoming].slice(-MAX_LINES);
          });
          scrollToBottom();
        },
        onStateChange: props.onStateChange,
      },
    )) ?? null;
  });

  onCleanup(() => {
    unsubscribe?.();
  });

  const displayedLines = createMemo(() => {
    const q = props.searchQuery;
    if (!q) return lines();
    return lines().filter((line) => lineMatchesQuery(line, q));
  });

  return (
    <div ref={containerEl} class={styles.output}>
      <pre class={styles.text}>
        <For each={displayedLines()}>
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
