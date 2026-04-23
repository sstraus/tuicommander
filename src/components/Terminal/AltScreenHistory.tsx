import { Component, For, Index, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { type LogLine, spanStyle } from "../../mobile/utils/logLine";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import s from "./AltScreenHistory.module.css";

const POLL_INTERVAL = 500;

function lineText(line: LogLine): string {
  return line.spans.map((sp) => sp.text).join("");
}

function deduplicatedScreen(log: LogLine[], screen: LogLine[]): LogLine[] {
  if (screen.length === 0 || log.length === 0) return screen;
  const lastLogTexts = log.slice(-screen.length).map(lineText);
  let overlap = 0;
  for (let start = 0; start <= lastLogTexts.length - screen.length + overlap; start++) {
    let match = true;
    for (let j = 0; j < screen.length && start + j < lastLogTexts.length; j++) {
      if (lastLogTexts[start + j] !== lineText(screen[j])) {
        match = false;
        break;
      }
    }
    if (match) {
      overlap = Math.min(screen.length, lastLogTexts.length - start);
      break;
    }
  }
  return overlap > 0 ? screen.slice(overlap) : screen;
}

interface VtLogChunk {
  lines: LogLine[];
  screen: LogLine[];
  total_lines: number;
  oldest: number;
}

interface Props {
  sessionId: string;
  onClose: () => void;
  terminalBg: string;
  fontFamily: string;
  fontSize: number;
  fontWeight: string;
  cellHeight: number;
}

export const AltScreenHistory: Component<Props> = (props) => {
  let containerEl: HTMLDivElement | undefined;
  let ignoreScrollUntil = 0;

  const [logLines, setLogLines] = createSignal<LogLine[]>([]);
  const [screenRows, setScreenRows] = createSignal<LogLine[]>([]);
  let newestTotal = 0;

  async function fetchAll() {
    try {
      const chunk = await invoke<VtLogChunk>("read_vt_log", {
        sessionId: props.sessionId,
        offset: 0,
        limit: 100000,
      });
      setLogLines(chunk.lines);
      setScreenRows(chunk.screen);
      newestTotal = chunk.total_lines;
    } catch (err) {
      appLogger.error("terminal", "read_vt_log failed", { error: String(err) });
    }
  }

  async function fetchNewer() {
    try {
      const chunk = await invoke<VtLogChunk>("read_vt_log", {
        sessionId: props.sessionId,
        offset: newestTotal,
        limit: 500,
      });
      if (chunk.lines.length > 0) {
        setLogLines((prev) => [...prev, ...chunk.lines]);
      }
      setScreenRows(chunk.screen);
      newestTotal = chunk.total_lines;
    } catch {
      // silently ignore polling errors
    }
  }

  onMount(async () => {
    await fetchAll();
    requestAnimationFrame(() => {
      if (containerEl) {
        ignoreScrollUntil = performance.now() + 150;
        containerEl.scrollTop = containerEl.scrollHeight;
      }
    });

    const pollId = setInterval(fetchNewer, POLL_INTERVAL);

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        props.onClose();
      }
    };
    document.addEventListener("keydown", onKey, { capture: true });
    onCleanup(() => {
      document.removeEventListener("keydown", onKey, { capture: true });
      clearInterval(pollId);
    });
  });

  const handleScroll = () => {
    if (!containerEl || performance.now() < ignoreScrollUntil) return;
    const canScroll = containerEl.scrollHeight > containerEl.clientHeight + 16;
    if (!canScroll) return;
    const atBottom =
      containerEl.scrollTop + containerEl.clientHeight >= containerEl.scrollHeight - 8;
    if (atBottom) props.onClose();
  };

  const allLines = createMemo(() => [
    ...logLines(),
    ...deduplicatedScreen(logLines(), screenRows()),
  ]);

  const renderLine = (line: LogLine) => (
    <div class={s.row}>
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

  return (
    <div
      ref={containerEl}
      class={s.overlay}
      style={{ background: props.terminalBg }}
      onScroll={handleScroll}
    >
      <div class={s.header} style={{ background: props.terminalBg }}>
        <span class={s.label}>Scroll history — {logLines().length} lines</span>
        <button class={s.closeBtn} onClick={props.onClose}>
          Return to live ↓
        </button>
      </div>
      <div
        class={s.content}
        style={{
          "--cell-height": `${props.cellHeight}px`,
          "font-family": props.fontFamily,
          "font-size": `${props.fontSize}px`,
          "font-weight": props.fontWeight,
        }}
      >
        <For each={allLines()}>{renderLine}</For>
      </div>
    </div>
  );
};
