import { Component, createSignal, onCleanup, onMount } from "solid-js";
import { Terminal as XTerm, type ITerminalOptions } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebglAddon } from "@xterm/addon-webgl";
import { WebglLifecycle } from "./webglLifecycle";
import { TerminalSearch } from "./TerminalSearch";
import { logLinesToAnsi } from "./logLineToAnsi";
import type { LogLine } from "../../mobile/utils/logLine";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import s from "./AltScreenHistory.module.css";

const POLL_INTERVAL = 500;

interface VtLogChunk {
  lines: LogLine[];
  screen: LogLine[];
  total_lines: number;
  oldest: number;
}

interface Props {
  sessionId: string;
  onClose: () => void;
  terminalOptions: ITerminalOptions;
  searchVisible: boolean;
  onSearchClose: () => void;
}

export const AltScreenHistory: Component<Props> = (props) => {
  let containerEl: HTMLDivElement | undefined;
  let terminal: XTerm | undefined;
  let fitAddon: FitAddon | undefined;
  const [searchAddon, setSearchAddon] = createSignal<SearchAddon | undefined>();
  const webglLife = new WebglLifecycle(() => new WebglAddon());
  let newestTotal = 0;
  let ignoreScrollUntil = 0;

  async function fetchAll() {
    try {
      const chunk = await invoke<VtLogChunk>("read_vt_log", {
        sessionId: props.sessionId,
        offset: 0,
        limit: 100000,
      });
      const allLines = [...chunk.lines, ...chunk.screen];
      newestTotal = chunk.total_lines;
      const ansi = logLinesToAnsi(allLines);
      terminal?.write("\x1b[?25l"); // hide cursor
      if (ansi) terminal?.write(ansi);
      // Scroll to bottom after write
      requestAnimationFrame(() => {
        ignoreScrollUntil = performance.now() + 150;
        terminal?.scrollToBottom();
      });
    } catch (err) {
      appLogger.error("terminal", "read_vt_log failed in overlay", { error: String(err) });
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
        newestTotal = chunk.total_lines;
        const ansi = logLinesToAnsi(chunk.lines);
        if (ansi) terminal?.write("\r\n" + ansi);
      }
    } catch (err) {
      appLogger.debug("terminal", "read_vt_log poll failed in overlay", { sessionId: props.sessionId, error: String(err) });
    }
  }

  onMount(() => {
    if (!containerEl) return;

    terminal = new XTerm({
      ...props.terminalOptions,
      disableStdin: true,
      cursorBlink: false,
      scrollback: 100000,
      allowProposedApi: true,
    });

    fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);

    const sa = new SearchAddon();
    terminal.loadAddon(sa);
    setSearchAddon(sa);

    terminal.open(containerEl);
    fitAddon.fit();

    const unicode11 = new Unicode11Addon();
    terminal.loadAddon(unicode11);
    terminal.unicode.activeVersion = "11";

    webglLife.attach(terminal);

    fetchAll();

    const pollId = setInterval(fetchNewer, POLL_INTERVAL);
    onCleanup(() => clearInterval(pollId));

    const handleResize = () => fitAddon?.fit();
    window.addEventListener("resize", handleResize);
    onCleanup(() => window.removeEventListener("resize", handleResize));
  });

  onCleanup(() => {
    webglLife.dispose();
    searchAddon()?.dispose();
    fitAddon?.dispose();
    terminal?.dispose();
  });

  const handleScroll = () => {
    if (!terminal || performance.now() < ignoreScrollUntil) return;
    const buf = terminal.buffer.active;
    const atBottom = buf.viewportY >= buf.baseY;
    if (atBottom) props.onClose();
  };

  return (
    <div class={s.overlay}>
      <TerminalSearch
        visible={props.searchVisible}
        searchAddon={searchAddon()}
        onClose={props.onSearchClose}
      />
      <div class={s.header} style={{ background: props.terminalOptions.theme?.background ?? "#1e1e1e" }}>
        <span class={s.label}>Scroll history</span>
        <button class={s.closeBtn} onClick={props.onClose}>
          Return to live ↓
        </button>
      </div>
      <div
        ref={containerEl}
        class={s.xtermContainer}
        onScroll={handleScroll}
      />
    </div>
  );
};
