import { Component, createEffect, onCleanup } from "solid-js";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { isTauri, subscribePty, type Unsubscribe } from "../../transport";
import { usePty } from "../../hooks/usePty";
import { settingsStore } from "../../stores/settings";
import { getTerminalTheme } from "../../themes";
import { terminalsStore } from "../../stores/terminals";
import { rateLimitStore } from "../../stores/ratelimit";
import { notificationsStore } from "../../stores/notifications";


/** Structured events parsed by Rust OutputParser, received via pty-parsed-{sessionId} */
type ParsedEvent =
  | { type: "rate-limit"; pattern_name: string; matched_text: string; retry_after_ms: number | null }
  | { type: "status-line"; task_name: string; full_line: string; time_info: string | null; token_info: string | null }
  | { type: "pr-url"; number: number; url: string; platform: string }
  | { type: "progress"; state: number; value: number }
  | { type: "question"; prompt_text: string };

export interface TerminalProps {
  id: string;
  cwd?: string | null;
  onFocus?: (id: string) => void;
  onSessionCreated?: (id: string, sessionId: string) => void;
  onSessionExit?: (id: string) => void;
  onRateLimit?: (id: string, sessionId: string, retryAfterMs: number | null) => void;
  /** Called when a .md file path is clicked in terminal output */
  onOpenMdFile?: (filePath: string) => void;
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
export const FONT_FAMILIES: Record<string, string> = {
  "JetBrains Mono": '"JetBrains Mono", monospace',
  "Fira Code": '"Fira Code", monospace',
  Hack: '"Hack", monospace',
};

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
    return basename === "~" ? "~" : basename;
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

export const Terminal: Component<TerminalProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let terminal: XTerm | undefined;
  let fitAddon: FitAddon | undefined;
  let sessionId: string | null = null;
  let sessionInitialized = false;
  let unsubscribePty: Unsubscribe | undefined;
  let unlistenParsed: (() => void) | undefined;

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

    if (terminalsStore.state.activeId !== props.id && !activityFlagged) {
      activityFlagged = true;
      terminalsStore.update(props.id, { activity: true });
    }

    // Shell idle detection: mark busy on output, start 500ms idle timer
    if (!busyFlagged) {
      busyFlagged = true;
      terminalsStore.update(props.id, { shellState: "busy" });
    }
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      busyFlagged = false;
      terminalsStore.update(props.id, { shellState: "idle" });
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
        // Restore original tab name if it was overwritten by status-line
        if (originalName && !terminalsStore.get(props.id)?.nameIsCustom) {
          terminalsStore.update(props.id, { name: originalName });
        }
        lastOscTitleUpdate = 0;
        sessionId = null;
        terminalsStore.update(props.id, { sessionId: null });
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
          case "pr-url":
            break;
        }
      });
    }
  };

  // Eagerly attach listeners for existing sessions (before terminal.open())
  // This prevents data loss when PTY output arrives while terminal is on a background tab
  const existingSessionId = terminalsStore.get(props.id)?.sessionId;
  if (existingSessionId) {
    sessionId = existingSessionId;
    attachSessionListeners(existingSessionId);
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
      fontWeight: "300",
      fontWeightBold: "500",
      theme: currentTheme(),
      cursorBlink: true,
      allowProposedApi: true,
      macOptionIsMeta: true,
    });

    fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(new WebLinksAddon());

    // Register link provider for .md file paths (clickable to open in MD viewer)
    if (props.onOpenMdFile) {
      const mdPathRegex = /(?:^|\s)((?:\.\/|\.\.\/|[\w-]+\/)*[\w.-]+\.md)(?:\s|$|[):,])/g;
      terminal.registerLinkProvider({
        provideLinks(bufferLineNumber: number, callback: (links: import("@xterm/xterm").ILink[] | undefined) => void) {
          const line = terminal!.buffer.active.getLine(bufferLineNumber - 1);
          if (!line) { callback(undefined); return; }
          const lineText = line.translateToString();
          const links: import("@xterm/xterm").ILink[] = [];
          let match: RegExpExecArray | null;
          mdPathRegex.lastIndex = 0;
          while ((match = mdPathRegex.exec(lineText)) !== null) {
            const filePath = match[1];
            const startCol = lineText.indexOf(filePath, match.index) + 1; // 1-based
            links.push({
              range: {
                start: { x: startCol, y: bufferLineNumber },
                end: { x: startCol + filePath.length - 1, y: bufferLineNumber },
              },
              text: filePath,
              activate: () => props.onOpenMdFile!(filePath),
            });
          }
          callback(links.length > 0 ? links : undefined);
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
          fitAddon?.fit();
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

  /** Fit terminal only when container has valid dimensions, retrying if needed */
  const safeFit = (onReady?: () => void, retries = 10) => {
    const tryFit = (remaining: number) => {
      requestAnimationFrame(() => {
        if (containerRef && containerRef.offsetWidth > 0 && containerRef.offsetHeight > 0) {
          fitAddon?.fit();
          onReady?.();
        } else if (remaining > 0) {
          tryFit(remaining - 1);
        }
      });
    };
    tryFit(retries);
  };

  // Check if this terminal is visible (active or in a split layout pane)
  const isVisible = () =>
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
                fitAddon?.fit();
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
      fitAddon?.fit();
    }
  });

  // Handle font family changes (global setting)
  createEffect(() => {
    settingsStore.state.font;
    if (terminal) {
      terminal.options.fontFamily = getFontFamily();
      fitAddon?.fit();
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

    // Resume reader if paused (don't leave PTY blocked after unmount)
    if (isPaused && sessionId) {
      isPaused = false;
      pty.resume(sessionId).catch(() => {});
    }

    // NOTE: We intentionally do NOT close the PTY session here.
    // Sessions persist across branch/repo switches and reconnect on remount.
    // PTY sessions are only closed via explicit closeTerminal() action.

    terminal?.dispose();
  });

  // Public methods exposed via ref pattern
  const refMethods = {
    fit: () => fitAddon?.fit(),
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
  };

  createEffect(() => {
    terminalsStore.update(props.id, { ref: refMethods });
  });

  return (
    <div
      ref={containerRef}
      class="terminal-content"
      data-terminal-id={props.id}
      style={{ width: "100%", height: "100%" }}
    />
  );
};

export default Terminal;
