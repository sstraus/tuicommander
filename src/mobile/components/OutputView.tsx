import { createSignal, createMemo, createEffect, onMount, onCleanup, For, Index, Show } from "solid-js";
import { subscribePty } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { type LogLine, normalizeLogLine, spanStyle, lineMatchesQuery, groupLineBlocks } from "../utils/logLine";
import styles from "./OutputView.module.css";

const MAX_LINES = 500;

interface OutputViewProps {
  sessionId: string;
  /** Real-time session state pushed via WebSocket (bypasses 3s polling). */
  onStateChange?: (state: Record<string, unknown>) => void;
  /** Receive current PTY input line text from the prompt row. */
  onInputLine?: (text: string | null) => void;
  /** When set, only lines matching this query (case-insensitive) are shown. */
  searchQuery?: string;
  /** Receive raw screen row text whenever screen content updates. */
  onScreenText?: (rows: string[]) => void;
}

export function OutputView(props: OutputViewProps) {
  const [logLines, setLogLines] = createSignal<LogLine[]>([]);
  const [screenRows, setScreenRows] = createSignal<LogLine[]>([]);
  const [subscribeError, setSubscribeError] = createSignal<string | null>(null);
  let containerEl: HTMLDivElement | undefined;
  let unsubscribe: (() => void) | null = null;
  // When the user scrolls up manually, stop auto-scrolling until they
  // return near the bottom.
  let userScrolledUp = false;

  // Propagate raw screen text to parent for question context overlay
  createEffect(() => {
    if (!props.onScreenText) return;
    const rows = screenRows();
    const texts = rows.map((r) =>
      typeof r === "string" ? r : (r.spans?.map((s: { text?: string }) => s.text ?? "").join("") ?? ""),
    );
    props.onScreenText(texts);
  });

  /** Fetch initial log lines + screen rows via HTTP; returns the total_lines offset for WS catch-up. */
  async function fetchInitialOutput(): Promise<number> {
    try {
      const resp = await fetch(`/sessions/${props.sessionId}/output?format=log`);
      if (resp.ok) {
        const json = await resp.json() as { lines: unknown[]; total_lines: number; screen?: string[] };
        if (json.lines && json.lines.length > 0) {
          setLogLines(json.lines.slice(-MAX_LINES).map(normalizeLogLine));
        }
        if (json.screen && json.screen.length > 0) {
          setScreenRows((json.screen as unknown[]).map(normalizeLogLine));
        }
        // Sync initial input_line from HTTP response
        if (props.onInputLine) {
          const il = (json as Record<string, unknown>).input_line;
          props.onInputLine(typeof il === "string" ? il : null);
        }
        scrollToBottom(true);
        return json.total_lines ?? 0;
      }
    } catch (err) {
      appLogger.warn("terminal", "fetchInitialOutput failed, will rely on WS catch-up", { error: err });
    }
    return 0;
  }

  function scrollToBottom(force = false) {
    if (!force && userScrolledUp) return;
    requestAnimationFrame(() => {
      if (containerEl) {
        containerEl.scrollTop = containerEl.scrollHeight;
      }
    });
  }

  function handleScroll() {
    if (!containerEl) return;
    // Consider "at bottom" when within 80px of the end
    const atBottom = containerEl.scrollHeight - containerEl.scrollTop - containerEl.clientHeight < 80;
    userScrolledUp = !atBottom;
  }

  onMount(async () => {
    containerEl?.addEventListener("scroll", handleScroll, { passive: true });
    const offset = await fetchInitialOutput();

    try {
      unsubscribe = (await subscribePty(
        props.sessionId,
        () => {}, // unused — onLogLines handles log delivery
        () => {
          setLogLines((prev) => [...prev, { spans: [{ text: "--- session exited ---" }] }]);
          setScreenRows([]);
        },
        {
          format: "log",
          logOffset: offset,
          onLogLines(rawLines) {
            setLogLines((prev) => {
              const incoming = rawLines.map(normalizeLogLine);
              return [...prev, ...incoming].slice(-MAX_LINES);
            });
            scrollToBottom();
          },
          onScreenRows(rows) {
            setScreenRows(rows.map(normalizeLogLine));
            scrollToBottom();
          },
          onStateChange: props.onStateChange,
          onInputLine: props.onInputLine,
        },
      )) ?? null;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.error("terminal", "Failed to subscribe to PTY output", { error: msg });
      setSubscribeError(msg);
    }
  });

  onCleanup(() => {
    unsubscribe?.();
    containerEl?.removeEventListener("scroll", handleScroll);
  });

  const allLines = createMemo(() => [...logLines(), ...screenRows()]);

  const displayedLines = createMemo(() => {
    const q = props.searchQuery;
    if (!q) return allLines();
    return allLines().filter((line) => lineMatchesQuery(line, q));
  });

  const lineBlocks = createMemo(() => groupLineBlocks(displayedLines()));

  return (
    <div ref={containerEl} class={styles.output}>
      <Show when={subscribeError()}>
        {(errMsg) => (
          <div class={styles.error}>Failed to connect to terminal: {errMsg()}</div>
        )}
      </Show>
      <pre class={styles.text}>
        <For each={lineBlocks()}>
          {(block) => {
            const renderLine = (line: LogLine) => (
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
            );
            return block.type === "table"
              ? <div class={styles.tableBlock}><For each={block.lines}>{renderLine}</For></div>
              : renderLine(block.line);
          }}
        </For>
      </pre>
    </div>
  );
}
