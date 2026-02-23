import { Component, createEffect, createSignal, onCleanup } from "solid-js";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { TerminalSearch } from "./TerminalSearch";
import { isTauri, subscribePty, type Unsubscribe } from "../../transport";
import { browserCreatedSessions } from "../../hooks/useAppInit";
import { usePty } from "../../hooks/usePty";
import { settingsStore, FONT_FAMILIES } from "../../stores/settings";
import { getTerminalTheme } from "../../themes";
import { terminalsStore } from "../../stores/terminals";
import { rateLimitStore } from "../../stores/ratelimit";
import { notificationsStore } from "../../stores/notifications";
import { invoke } from "../../invoke";
import { isMacOS } from "../../platform";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { kittySequenceForKey } from "./kittyKeyboard";
import s from "./Terminal.module.css";


/** Structured events parsed by Rust OutputParser, received via pty-parsed-{sessionId} */
type ParsedEvent =
  | { type: "rate-limit"; pattern_name: string; matched_text: string; retry_after_ms: number | null }
  | { type: "status-line"; task_name: string; full_line: string; time_info: string | null; token_info: string | null }
  | { type: "progress"; state: number; value: number }
  | { type: "question"; prompt_text: string }
  | { type: "usage-limit"; percentage: number; limit_type: string }
  | { type: "plan-file"; path: string };

export interface TerminalProps {
  id: string;
  cwd?: string | null;
  onFocus?: (id: string) => void;
  onSessionCreated?: (id: string, sessionId: string) => void;
  onSessionExit?: (id: string) => void;
  onRateLimit?: (id: string, sessionId: string, retryAfterMs: number | null) => void;
  /** Called when a file path is clicked in terminal output */
  onOpenFilePath?: (absolutePath: string, line?: number, col?: number) => void;
  /** When false, disables left-Option-as-Meta key sequences (macOS only). Default: true */
  metaHotkeys?: boolean;
  /** When true, terminal initializes immediately without requiring activeId match (e.g. lazygit pane) */
  alwaysVisible?: boolean;
}

/** Get current theme from settings, with scrollbar defaults */
function currentTheme() {
  return {
    ...getTerminalTheme(settingsStore.state.theme),
    scrollbarSliderBackground: "rgba(121, 121, 121, 0.4)",
    scrollbarSliderHoverBackground: "rgba(100, 100, 100, 0.7)",
    scrollbarSliderActiveBackground: "rgba(191, 191, 191, 0.4)",
  };
}

// Font families mapping
/** Shell control flow pattern — titles containing these are cryptic scripts, not useful names */
const SHELL_SCRIPT_RE = /;|&&|\|\||\$\(|\bif\b|\bthen\b|\belse\b|\belif\b|\bfi\b|\bfor\b|\bwhile\b|\bdo\b|\bdone\b|\bcase\b|\besac\b/;

/** Clean an OSC 0/2 title: strip user@host prefix, env var assignments, and command args.
 *  Returns empty string if the title is only a user@host pattern (no useful info),
 *  or if it looks like a shell script (compound commands, control flow). */
export function cleanOscTitle(title: string): string {
  // Reject titles that look like shell scripts before any processing
  if (SHELL_SCRIPT_RE.test(title)) return "";

  // Strip "user@host:" or bare "user@host" prefix
  let cleaned = title.replace(/^[^@\s]+@[^:\s]+(:\s*)?/, "");
  // Strip leading env var assignments (KEY=value pairs, including empty values)
  cleaned = cleaned.replace(/^(\s*\w+=\S*\s+)+/, "");
  cleaned = cleaned.trim();
  // Paths: extract last segment (status bar shows the full path)
  if (cleaned.startsWith("/") || cleaned.startsWith("~")) {
    const basename = cleaned.replace(/\/+$/, "").split("/").pop() || "";
    // Bare "~" (home dir) is not useful as a tab title — return empty to keep original name
    return basename === "~" ? "" : basename;
  }
  // Strip flags and their values, keep command + subcommands (bare words before first flag)
  if (cleaned) {
    const words = cleaned.split(/\s+/);
    const kept: string[] = [];
    for (const w of words) {
      if (w.startsWith("-")) break;
      kept.push(w);
    }
    cleaned = kept.join(" ");
  }
  return cleaned;
}

// Max bytes to buffer before terminal is opened (prevents unbounded growth)
const OUTPUT_BUFFER_MAX_BYTES = 100 * 1024; // 100KB

// Flow control watermarks (bytes pending in xterm.js write queue)
const HIGH_WATERMARK = 512 * 1024;  // 512KB — pause reader when exceeded
const LOW_WATERMARK = 128 * 1024;   // 128KB — resume reader when drained below

// Minimum container dimensions before fit() is allowed — prevents WebGL rendering
// artifacts when xterm gets squeezed into impossibly small panes (e.g. narrow lazygit split)
const MIN_FIT_WIDTH = 80;   // px (~5 columns at 14px)
const MIN_FIT_HEIGHT = 40;  // px (~2 rows)

export const Terminal: Component<TerminalProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let terminal: XTerm | undefined;
  let fitAddon: FitAddon | undefined;
  let searchAddon: SearchAddon | undefined;
  let sessionId: string | null = null;

  // Search overlay state
  const [searchVisible, setSearchVisible] = createSignal(false);
  let sessionInitialized = false;
  let unsubscribePty: Unsubscribe | undefined;
  let unlistenParsed: (() => void) | undefined;
  let unlistenKitty: (() => void) | undefined;

  // Kitty keyboard protocol: current flags for this session (0 = disabled)
  let kittyFlags = 0;

  // Buffer for PTY output arriving before terminal.open()
  let outputBuffer: string[] = [];
  let outputBufferBytes = 0;

  // Flow control state
  let pendingWriteBytes = 0;
  let isPaused = false;

  // Resize debounce (150ms trailing edge)
  let resizeTimer: ReturnType<typeof setTimeout> | undefined;

  // Shell idle detection: after 500ms of no PTY output, shell is idle
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let activityFlagged = false; // Avoids redundant store updates per data chunk
  let busyFlagged = false;
  let hasResumedAgent = false; // Ensures agent resume command fires only once
  let lastDataAtTimestamp = 0; // Throttle lastDataAt store updates to 1s

  /** Fit terminal to container, guarded against undersized containers.
   *  Skips fit when the container is too small to avoid WebGL rendering artifacts. */
  const doFit = () => {
    if (!containerRef || !fitAddon || !terminal) return;
    if (containerRef.offsetWidth < MIN_FIT_WIDTH || containerRef.offsetHeight < MIN_FIT_HEIGHT) return;
    fitAddon.fit();
  };

  // Reset activity flag when this terminal becomes active (store clears activity)
  createEffect(() => {
    if (terminalsStore.state.activeId === props.id) {
      activityFlagged = false;
    }
  });

  // Track when OSC title last updated the tab name (OSC titles take priority over status-line)
  let lastOscTitleUpdate = 0;
  // Original tab name before any command/agent overwrote it
  let originalName: string | null = null;

  const pty = usePty();

  const getFontFamily = () => {
    const font = settingsStore.state.font;
    return FONT_FAMILIES[font] || FONT_FAMILIES["JetBrains Mono"];
  };

  /** Process a chunk of PTY output — write to terminal or buffer if not ready */
  const handlePtyData = (data: string) => {
    if (terminal) {
      // Dispatch to plugins BEFORE writing to xterm so they observe the same byte order
      if (sessionId) pluginRegistry.processRawOutput(data, sessionId);

      const byteLen = data.length;
      pendingWriteBytes += byteLen;

      // Pause reader if we've accumulated too much unprocessed data
      if (!isPaused && pendingWriteBytes > HIGH_WATERMARK && sessionId) {
        isPaused = true;
        pty.pause(sessionId).catch(() => {});
      }

      terminal.write(data, () => {
        pendingWriteBytes -= byteLen;

        // Resume reader once xterm has drained enough
        if (isPaused && pendingWriteBytes < LOW_WATERMARK && sessionId) {
          isPaused = false;
          pty.resume(sessionId).catch(() => {});
        }
      });
    } else {
      // Buffer output until terminal.open() is called
      outputBuffer.push(data);
      outputBufferBytes += data.length;
      // Cap buffer: drop oldest chunks when over limit
      while (outputBufferBytes > OUTPUT_BUFFER_MAX_BYTES && outputBuffer.length > 1) {
        const dropped = outputBuffer.shift()!;
        outputBufferBytes -= dropped.length;
      }
    }

    // Track last PTY output timestamp for activity dashboard (throttled to 1s)
    const now = Date.now();
    if (!lastDataAtTimestamp || now - lastDataAtTimestamp > 1000) {
      lastDataAtTimestamp = now;
      terminalsStore.update(props.id, { lastDataAt: now });
    }

    if (terminalsStore.state.activeId !== props.id && !activityFlagged) {
      activityFlagged = true;
      terminalsStore.update(props.id, { activity: true });
    }

    // Shell idle detection: mark busy on output, start 500ms idle timer
    if (!busyFlagged) {
      busyFlagged = true;
      terminalsStore.update(props.id, { shellState: "busy" });
      // New output after idle means the user answered any pending prompt
      if (terminalsStore.get(props.id)?.awaitingInput) {
        terminalsStore.clearAwaitingInput(props.id);
      }
    }
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      busyFlagged = false;
      terminalsStore.update(props.id, { shellState: "idle" });

      // Auto-resume agent on first shell idle after restore
      if (!hasResumedAgent) {
        const pending = terminalsStore.get(props.id)?.pendingResumeCommand;
        if (pending && sessionId) {
          hasResumedAgent = true;
          terminalsStore.update(props.id, { pendingResumeCommand: null });
          pty.write(sessionId, pending + "\r").catch(() => {});
        }
      }
    }, 500);
  };

  /** Replay buffered output into the now-open terminal */
  const replayBuffer = () => {
    if (terminal && outputBuffer.length > 0) {
      for (const chunk of outputBuffer) {
        terminal.write(chunk);
      }
      outputBuffer = [];
      outputBufferBytes = 0;
    }
  };

  /** Truncate task name to fit in tab title */
  const truncateTaskName = (name: string, maxLength = 25): string => {
    if (name.length <= maxLength) return name;
    return name.slice(0, maxLength - 1) + "\u2026";
  };

  /** Set up event listeners for a known session ID */
  const attachSessionListeners = async (targetSessionId: string) => {

    // PTY output + exit via transport abstraction (Tauri listen or WebSocket)
    unsubscribePty = await subscribePty(
      targetSessionId,
      (data: string) => handlePtyData(data),
      () => {
        if (terminal) {
          terminal.writeln("\r\n\x1b[33m[Process exited]\x1b[0m");
        }
        // Guard: terminal may have been removed from the store already
        // (e.g. lazygit pane closed). Updating a removed entry would recreate it as a ghost.
        const stillExists = terminalsStore.get(props.id);
        if (stillExists) {
          // Restore original tab name if it was overwritten by status-line
          if (originalName && !stillExists.nameIsCustom) {
            terminalsStore.update(props.id, { name: originalName });
          }
          terminalsStore.update(props.id, { sessionId: null });
        }
        lastOscTitleUpdate = 0;
        sessionId = null;
        props.onSessionExit?.(props.id);
        if (terminalsStore.state.activeId !== props.id) {
          notificationsStore.playCompletion();
        }
      },
    );

    // Structured events from Rust OutputParser (Tauri-only, not available via HTTP)
    if (isTauri()) {
      const { listen } = await import("@tauri-apps/api/event");
      unlistenParsed = await listen<ParsedEvent>(`pty-parsed-${targetSessionId}`, (event) => {
        const parsed = event.payload;

        switch (parsed.type) {
          case "progress": {
            terminalsStore.clearAwaitingInput(props.id);
            if (parsed.state === 0) {
              terminalsStore.update(props.id, { progress: null });
            } else if (parsed.state === 1 || parsed.state === 2 || parsed.state === 3) {
              terminalsStore.update(props.id, { progress: Math.min(100, Math.max(0, parsed.value)) });
            }
            break;
          }
          case "status-line": {
            // Agent is working again — clear any question state
            terminalsStore.clearAwaitingInput(props.id);
            const now = Date.now();
            const currentTerm = terminalsStore.get(props.id);
            // Only use status-line title if OSC hasn't set a title recently (2s)
            if (now - lastOscTitleUpdate > 2000 && !currentTerm?.nameIsCustom) {
              if (!originalName) {
                originalName = currentTerm?.name || null;
              }
              const taskTitle = truncateTaskName(parsed.task_name);
              terminalsStore.update(props.id, { name: taskTitle });
            }
            break;
          }
          case "rate-limit": {
            const detectedAgent = terminalsStore.get(props.id)?.agentType;
            console.debug(`[RateLimit DEBUG] pattern=${parsed.pattern_name} matched="${parsed.matched_text}" agent=${detectedAgent ?? "none"} sessionId=${targetSessionId}`);
            if (detectedAgent) {
              const info = {
                agentType: detectedAgent,
                sessionId: targetSessionId,
                retryAfterMs: parsed.retry_after_ms,
                message: `Rate limit detected (${parsed.pattern_name}): ${parsed.matched_text}`,
                detectedAt: Date.now(),
              };
              rateLimitStore.addRateLimit(info);
              props.onRateLimit?.(props.id, targetSessionId, parsed.retry_after_ms);
              notificationsStore.playWarning();
            }
            break;
          }
          case "question":
            terminalsStore.setAwaitingInput(props.id, "question");
            if (terminalsStore.state.activeId !== props.id) {
              notificationsStore.playQuestion();
            }
            break;
          case "usage-limit":
            terminalsStore.update(props.id, {
              usageLimit: { percentage: parsed.percentage, limitType: parsed.limit_type },
            });
            break;
          case "plan-file":
            // Mark terminal as awaiting input — the plan needs user approval
            terminalsStore.setAwaitingInput(props.id, "question");
            if (terminalsStore.state.activeId !== props.id) {
              notificationsStore.playQuestion();
            }
            // Also handled by planPlugin via dispatchStructuredEvent below
            break;
        }

        // Also dispatch to plugin structured event handlers
        pluginRegistry.dispatchStructuredEvent(parsed.type, parsed, targetSessionId);
      });

      // Listen for kitty keyboard protocol flag changes from Rust
      unlistenKitty = await listen<number>(`kitty-keyboard-${targetSessionId}`, (event) => {
        kittyFlags = event.payload;
      });
    }
  };

  // Eagerly attach listeners for existing sessions (before terminal.open())
  // This prevents data loss when PTY output arrives while terminal is on a background tab
  const existingSessionId = terminalsStore.get(props.id)?.sessionId;
  if (existingSessionId) {
    sessionId = existingSessionId;
    attachSessionListeners(existingSessionId).catch((err) =>
      console.error("[Terminal] Failed to attach session listeners:", err),
    );
  }

  /** Initialize PTY session and event listeners */
  const initSession = async () => {
    if (sessionInitialized || !terminal) return;
    sessionInitialized = true;

    try {
      let reconnected = false;
      if (sessionId) {
        // Already have a session (eagerly attached above) — just resize to current dimensions
        try {
          await pty.resize(sessionId, terminal.rows, terminal.cols);
          reconnected = true;
        } catch {
          // Session no longer exists (app restarted) - create fresh
          sessionId = null;
          unsubscribePty?.();
          unlistenParsed?.();
        }
      }
      if (!reconnected) {
        sessionId = await pty.createSession({
          rows: terminal.rows,
          cols: terminal.cols,
          shell: null,
          cwd: props.cwd || null,
        });
        if (sessionId) {
          // Track browser-created sessions so beforeunload only closes our own
          if (!isTauri()) {
            browserCreatedSessions.add(sessionId);
          }
          await attachSessionListeners(sessionId);
        }
      }

      if (sessionId) {
        terminalsStore.update(props.id, { sessionId });
        props.onSessionCreated?.(props.id, sessionId);
      }
    } catch (err) {
      terminal.writeln(`\x1b[31mFailed to create PTY: ${err}\x1b[0m`);
    }
  };

  let terminalOpened = false;
  let resizeObserver: ResizeObserver | undefined;

  /** Open xterm in the container and wire up event handlers (deferred until visible) */
  const openTerminal = () => {
    if (terminalOpened || !containerRef) return;
    terminalOpened = true;

    terminal = new XTerm({
      fontSize: settingsStore.state.defaultFontSize,
      fontFamily: getFontFamily(),
      fontWeight: "normal",
      fontWeightBold: "bold",
      lineHeight: 1.2,
      theme: currentTheme(),
      cursorBlink: true,
      allowProposedApi: true,
      macOptionIsMeta: false, // Right Option keeps macOS composition (π, ∑, @…)
    });

    // iTerm2-style Option key split (macOS only):
    // Left Option → Meta (sends ESC + char for readline/emacs/vi keybindings)
    // Right Option → macOS composition (π, ∑, @ etc.) — handled by xterm natively
    //
    // On Windows/Linux Alt already sends escape sequences natively; no-op there.
    //
    // Important: event.location on a regular key (e.g. P) is always 0 (standard),
    // never 1 (left). We track left-Option state via keydown/keyup on AltLeft itself.
    //
    // We intercept both keydown (to inject ESC+char) and keypress (to suppress the
    // macOS-composed character that xterm's third-level-shift pass-through would send).
    let leftOptionHeld = false;
    terminal.attachCustomKeyEventHandler((event: KeyboardEvent) => {
      // Intercept Cmd+F (macOS) / Ctrl+F (Win/Linux) to open search overlay
      if (event.type === "keydown" && (event.metaKey || event.ctrlKey) && event.key === "f" && !event.altKey && !event.shiftKey) {
        event.preventDefault();
        setSearchVisible(true);
        return false;
      }

      // Escape closes search overlay when visible
      if (event.type === "keydown" && event.key === "Escape" && searchVisible()) {
        event.preventDefault();
        setSearchVisible(false);
        return false;
      }

      // Kitty keyboard protocol: encode special keys when flag 1 (disambiguate) is active
      if (event.type === "keydown" && (kittyFlags & 1)) {
        const seq = kittySequenceForKey(event.key, event.shiftKey, event.altKey, event.ctrlKey, event.metaKey);
        if (seq !== null) {
          terminal!.input(seq, true);
          return false;
        }
      }

      if (!isMacOS()) return true; // Windows/Linux: xterm handles Alt natively
      if (props.metaHotkeys === false) return true; // disabled for this repo
      if (event.metaKey || event.ctrlKey) return true; // Cmd+Alt or AltGr: pass through
      // AltLeft keyup fires with altKey=false — reset state here before early return
      if (!event.altKey) { leftOptionHeld = false; return true; }

      // Track left vs right Option state via the modifier key itself (location=1 for left)
      if (event.code === "AltLeft") {
        if (event.type === "keydown") leftOptionHeld = true;
        else if (event.type === "keyup") leftOptionHeld = false;
        return true; // let xterm see the modifier keydown/keyup
      }

      // Regular key: only intercept when left Option is held
      if (!leftOptionHeld) return true;

      // Only handle keydown and keypress (both can carry the composed char to xterm)
      if (event.type !== "keydown" && event.type !== "keypress") return true;

      const code = event.code;
      let seq: string | null = null;

      if (code.startsWith("Key")) {
        const ch = code.slice(3).toLowerCase();
        seq = "\x1b" + (event.shiftKey ? ch.toUpperCase() : ch);
      } else if (code.startsWith("Digit")) {
        seq = "\x1b" + code.slice(5);
      } else {
        switch (code) {
          case "Backspace":    seq = "\x1b\x7f"; break; // Alt+Backspace = backward-kill-word
          case "Space":        seq = "\x1b ";    break;
          case "Period":       seq = "\x1b.";    break; // Alt+. = insert-last-argument
          case "Comma":        seq = "\x1b,";    break;
          case "Slash":        seq = "\x1b/";    break;
          case "Minus":        seq = "\x1b-";    break;
          case "Equal":        seq = "\x1b=";    break;
          case "Semicolon":    seq = "\x1b;";    break;
          case "Quote":        seq = "\x1b'";    break;
          case "BracketLeft":  seq = "\x1b[";    break;
          case "BracketRight": seq = "\x1b]";    break;
          case "Backslash":    seq = "\x1b\\";   break;
          case "Backquote":    seq = "\x1b`";    break;
          case "ArrowLeft":    seq = "\x1b[1;3D"; break; // word backward
          case "ArrowRight":   seq = "\x1b[1;3C"; break; // word forward
          case "ArrowUp":      seq = "\x1b[1;3A"; break;
          case "ArrowDown":    seq = "\x1b[1;3B"; break;
        }
      }

      if (seq === null) return true; // unknown key: let xterm handle
      // On keydown: inject ESC sequence into PTY; on keypress: just suppress
      if (event.type === "keydown") terminal!.input(seq, true);
      return false; // suppress xterm's default alt-key processing (both keydown + keypress)
    });

    fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(new WebLinksAddon());

    searchAddon = new SearchAddon();
    terminal.loadAddon(searchAddon);

    // Register link provider for file paths (clickable to open in IDE or MD viewer)
    if (props.onOpenFilePath) {
      // Matches paths starting with /, ./, ../, or relative paths containing / with known extensions.
      // Optional :line or :line:col suffix.
      const CODING_EXT = "rs|ts|tsx|js|jsx|mjs|cjs|py|go|java|kt|kts|swift|c|h|cpp|hpp|cc|cs|rb|php|lua|zig|nim|ex|exs|erl|hs|ml|mli|fs|fsx|scala|clj|cljs|r|R|jl|dart|v|sv|vhdl|sol|move|css|scss|sass|less|html|htm|vue|svelte|astro|json|jsonc|json5|yaml|yml|toml|ini|cfg|conf|env|xml|plist|csv|tsv|sql|graphql|gql|proto|thrift|avsc|md|mdx|txt|rst|tex|adoc|org|sh|bash|zsh|fish|ps1|psm1|bat|cmd|dockerfile|containerfile|tf|tfvars|hcl|nix|cmake|make|mk|gradle|sbt|cabal|gemspec|podspec|lock|sum|mod|workspace|editorconfig|gitignore|gitattributes|dockerignore|eslintrc|prettierrc|babelrc|nvmrc|tool-versions";
      const filePathRegex = new RegExp(
        `(?:^|[\\s"'\`(\\[{])` +                                    // boundary
        `((?:/|\\.\\.?/|[\\w@.-]+/)` +                              // path start: /, ./, ../, or word/
        `[\\w./@-]*` +                                               // middle segments
        `\\.(?:${CODING_EXT})` +                                     // .ext
        `(?::\\d+(?::\\d+)?)?)` +                                    // optional :line:col
        `(?=[\\s"'\`),;\\]}>]|$)`,                                   // boundary
        "g",
      );

      const onOpenFilePath = props.onOpenFilePath; // capture for closure

      terminal.registerLinkProvider({
        provideLinks(bufferLineNumber: number, callback: (links: import("@xterm/xterm").ILink[] | undefined) => void) {
          const bufLine = terminal!.buffer.active.getLine(bufferLineNumber - 1);
          if (!bufLine) { callback(undefined); return; }
          const lineText = bufLine.translateToString();
          const matches: { text: string; index: number }[] = [];
          let match: RegExpExecArray | null;
          filePathRegex.lastIndex = 0;
          while ((match = filePathRegex.exec(lineText)) !== null) {
            matches.push({ text: match[1], index: lineText.indexOf(match[1], match.index) });
          }
          if (matches.length === 0) { callback(undefined); return; }

          // Get cwd from the terminal's PTY session
          const termData = terminalsStore.get(props.id);
          const cwd = termData?.cwd || "";

          // Validate all candidates via Rust IPC
          Promise.all(
            matches.map(async (m) => {
              try {
                const resolved = await invoke<{ absolute_path: string; is_directory: boolean } | null>(
                  "resolve_terminal_path",
                  { cwd, candidate: m.text },
                );
                return resolved ? { ...m, resolved } : null;
              } catch {
                return null;
              }
            }),
          ).then((results) => {
            const links: import("@xterm/xterm").ILink[] = [];
            for (const r of results) {
              if (!r) continue;
              const startCol = r.index + 1; // 1-based
              // Parse line:col from the candidate text
              let line: number | undefined;
              let col: number | undefined;
              const lineColMatch = r.text.match(/:(\d+)(?::(\d+))?$/);
              if (lineColMatch) {
                line = parseInt(lineColMatch[1], 10);
                if (lineColMatch[2]) col = parseInt(lineColMatch[2], 10);
              }
              links.push({
                range: {
                  start: { x: startCol, y: bufferLineNumber },
                  end: { x: startCol + r.text.length - 1, y: bufferLineNumber },
                },
                text: r.text,
                activate: () => onOpenFilePath(r.resolved.absolute_path, line, col),
              });
            }
            callback(links.length > 0 ? links : undefined);
          });
        },
      });
    }

    terminal.open(containerRef);

    // Load WebGL renderer for 3-5x rendering performance over canvas.
    // CanvasAddon fallback deferred to Story 158 (@xterm/addon-canvas beta has broken exports).
    // On context loss, DOM renderer remains as fallback.
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => {
        webgl.dispose();
      });
      terminal.loadAddon(webgl);
    } catch {
      // DOM renderer as fallback
    }

    // Update tab title from shell OSC 0/2 escape sequences (e.g. user@host:~/path)
    // OSC titles take priority over status-line parsing
    terminal.onTitleChange((title) => {
      if (title && !terminalsStore.get(props.id)?.nameIsCustom) {
        const cleaned = cleanOscTitle(title);
        if (cleaned) {
          if (!originalName) {
            originalName = terminalsStore.get(props.id)?.name || null;
          }
          terminalsStore.update(props.id, { name: cleaned });
          lastOscTitleUpdate = Date.now();
        } else if (originalName) {
          // Bare user@host means shell prompt returned — restore original tab name
          terminalsStore.update(props.id, { name: originalName });
          lastOscTitleUpdate = 0;
        }
      }
    });

    // Replay any PTY output buffered while terminal was not yet open
    replayBuffer();

    resizeObserver = new ResizeObserver(() => {
      requestAnimationFrame(() => {
        if (containerRef && containerRef.offsetWidth > 0 && containerRef.offsetHeight > 0) {
          doFit();
        }
      });
    });

    terminal.onData(async (data) => {
      if (sessionId) {
        try {
          await pty.write(sessionId, data);
        } catch (err) {
          console.error("Failed to write to PTY:", err);
        }
      }
    });

    terminal.onResize(({ rows, cols }) => {
      if (sessionId && rows > 0 && cols > 0) {
        // Debounce resize calls to avoid SIGWINCH storms during window drag
        clearTimeout(resizeTimer);
        resizeTimer = setTimeout(async () => {
          if (sessionId) {
            try {
              await pty.resize(sessionId, rows, cols);
            } catch (err) {
              console.error("Failed to resize PTY:", err);
            }
          }
        }, 150);
      }
    });

    terminal.textarea?.addEventListener("focus", () => {
      props.onFocus?.(props.id);
    });
  };

  /** Fit terminal only when container has valid dimensions, retrying if needed.
   *  If all retries are exhausted, proceed anyway so the PTY still initializes
   *  (Rust backend clamps rows/cols to sane minimums). */
  const safeFit = (onReady?: () => void, retries = 10) => {
    const tryFit = (remaining: number) => {
      requestAnimationFrame(() => {
        if (containerRef && containerRef.offsetWidth > 0 && containerRef.offsetHeight > 0) {
          doFit();
          onReady?.();
        } else if (remaining > 0) {
          tryFit(remaining - 1);
        } else {
          console.warn('[Terminal] Container has zero dimensions after retries, proceeding with defaults');
          onReady?.();
        }
      });
    };
    tryFit(retries);
  };

  // Check if this terminal is visible (active, in a split layout pane, or always-visible)
  const isVisible = () =>
    props.alwaysVisible ||
    terminalsStore.state.activeId === props.id ||
    terminalsStore.state.layout.panes.includes(props.id);

  // When this terminal becomes visible: open xterm, fit, and init PTY session
  createEffect(() => {
    if (isVisible()) {
      openTerminal();
      safeFit(() => initSession());

      // Start observing resize while active (disconnect when inactive to avoid fit on display:none)
      if (resizeObserver && containerRef) {
        resizeObserver.observe(containerRef);
      }

      // Tauri window resize listener - DOM resize event may not fire in webview
      let unlistenResize: (() => void) | undefined;
      if (isTauri()) {
        import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
          getCurrentWindow().onResized(() => {
            requestAnimationFrame(() => {
              if (containerRef && containerRef.offsetWidth > 0 && containerRef.offsetHeight > 0) {
                doFit();
              }
            });
          }).then((unlisten) => {
            unlistenResize = unlisten;
          });
        });
      }

      onCleanup(() => {
        resizeObserver?.disconnect();
        unlistenResize?.();
      });
    }
  });

  // Handle font size changes (per-terminal zoom from store)
  // Access store path directly for reliable SolidJS reactivity tracking
  createEffect(() => {
    const fontSize = terminalsStore.state.terminals[props.id]?.fontSize;
    if (terminal && fontSize !== undefined) {
      terminal.options.fontSize = fontSize;
      doFit();
    }
  });

  // Handle default font size changes (global setting from Appearance)
  createEffect(() => {
    const defaultSize = settingsStore.state.defaultFontSize;
    const perTerminalSize = terminalsStore.state.terminals[props.id]?.fontSize;
    // Only apply default if terminal has no per-terminal zoom override
    if (terminal && perTerminalSize === undefined) {
      terminal.options.fontSize = defaultSize;
      doFit();
    }
  });

  // Handle font family changes (global setting)
  createEffect(() => {
    settingsStore.state.font;
    if (terminal) {
      terminal.options.fontFamily = getFontFamily();
      doFit();
    }
  });

  // Handle theme changes (global setting)
  createEffect(() => {
    settingsStore.state.theme;
    if (terminal) {
      terminal.options.theme = currentTheme();
    }
  });

  // Cleanup on unmount - detach UI but keep PTY session alive
  onCleanup(() => {
    clearTimeout(resizeTimer);
    clearTimeout(idleTimer);
    resizeObserver?.disconnect();
    unsubscribePty?.();
    unlistenParsed?.();
    unlistenKitty?.();
    kittyFlags = 0;

    // Resume reader if paused (don't leave PTY blocked after unmount)
    if (isPaused && sessionId) {
      isPaused = false;
      pty.resume(sessionId).catch(() => {});
    }

    // NOTE: We intentionally do NOT close the PTY session here.
    // Sessions persist across branch/repo switches and reconnect on remount.
    // PTY sessions are only closed via explicit closeTerminal() action.

    // Clean up plugin line buffer for this session
    if (sessionId) pluginRegistry.removeSession(sessionId);

    terminal?.dispose();
  });

  // Public methods exposed via ref pattern
  const refMethods = {
    fit: () => doFit(),
    write: (data: string) => {
      // Write to PTY stdin (sends as user input to the shell)
      if (sessionId) {
        pty.write(sessionId, data).catch((err) => {
          console.error("Failed to write to PTY:", err);
        });
      }
    },
    writeln: (data: string) => terminal?.writeln(data),
    clear: () => terminal?.clear(),
    focus: () => terminal?.focus(),
    getSessionId: () => sessionId,
    openSearch: () => setSearchVisible(true),
    closeSearch: () => setSearchVisible(false),
  };

  createEffect(() => {
    terminalsStore.update(props.id, { ref: refMethods });
  });

  return (
    <div class={s.wrapper} data-terminal-id={props.id}>
      <TerminalSearch
        visible={searchVisible()}
        searchAddon={searchAddon}
        onClose={() => {
          setSearchVisible(false);
          terminal?.focus();
        }}
      />
      <div
        ref={containerRef}
        class={s.content}
        style={{ width: "100%", height: "100%" }}
      />
    </div>
  );
};

export default Terminal;
