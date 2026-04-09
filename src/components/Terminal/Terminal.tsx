import { Component, Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { TerminalSearch } from "./TerminalSearch";
import { isTauri, subscribePty, type Unsubscribe } from "../../transport";
import { handleOpenUrl } from "../../utils/openUrl";
import { browserCreatedSessions } from "../../hooks/useAppInit";
import { usePty } from "../../hooks/usePty";
import { settingsStore, FONT_FAMILIES } from "../../stores/settings";
import { getTerminalTheme } from "../../themes";
import { terminalsStore, type AwaitingInputType, isShellState } from "../../stores/terminals";
import { paneLayoutStore } from "../../stores/paneLayout";
import { rateLimitStore } from "../../stores/ratelimit";
import { appLogger } from "../../stores/appLogger";
import { notificationsStore } from "../../stores/notifications";
import { invoke } from "../../invoke";
import { isMacOS } from "../../platform";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { agentConfigsStore } from "../../stores/agentConfigs";
import { parseOsc7Url } from "../../utils/osc7";
import { kittySequenceForKey } from "./kittyKeyboard";
import { getAwaitingInputSound } from "./awaitingInputSound";
import { searchTerminalBuffer } from "../../utils/terminalSearch";
import { ScrollTracker, ViewportLock } from "./scrollTracker";
import { detectAgentForTerminal } from "../../hooks/useAgentPolling";
import s from "./Terminal.module.css";


/** Structured events parsed by Rust OutputParser, received via pty-parsed-{sessionId} */
type ParsedEvent =
  | { type: "rate-limit"; pattern_name: string; matched_text: string; retry_after_ms: number | null }
  | { type: "status-line"; task_name: string; full_line: string; time_info: string | null; token_info: string | null }
  | { type: "progress"; state: number; value: number }
  | { type: "question"; prompt_text: string; confident: boolean }
  | { type: "usage-limit"; percentage: number; limit_type: string }
  | { type: "usage-exhausted"; reset_time: string | null }
  | { type: "plan-file"; path: string }
  | { type: "user-input"; content: string }
  | { type: "api-error"; pattern_name: string; matched_text: string; error_kind: string }
  | { type: "intent"; text: string; title?: string }
  | { type: "suggest"; items: string[] }
  | { type: "active-subtasks"; count: number; task_type: string }
  | { type: "shell-state"; state: "busy" | "idle" };

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
  /** When true, terminal initializes immediately without requiring activeId match */
  alwaysVisible?: boolean;
  /** Called when the shell reports a working directory change via OSC 7 */
  onCwdChange?: (id: string, cwd: string) => void;
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

  // Strip leading spinner/symbol noise: *, middle dots, bullets, braille patterns,
  // dingbats, geometric shapes, and other non-alphanumeric decorators agents prepend.
  let cleaned = title.replace(/^[\s*\u00B7\u2022\u2219\u22C5\u2027\u25A0-\u25FF\u2800-\u28FF\u2720-\u273F\u2580-\u259F]+/, "");
  // Strip "user@host:" or bare "user@host" prefix
  cleaned = cleaned.replace(/^[^@\s]+@[^:\s]+(:\s*)?/, "");
  // Strip leading env var assignments (KEY=value pairs, including empty values)
  cleaned = cleaned.replace(/^(\s*\w+=\S*\s+)+/, "");
  cleaned = cleaned.trim();
  // Paths: shell is just reporting CWD (idle prompt) — not useful as a tab title
  // since the status bar already shows the full path. Return empty to keep original name.
  if (cleaned.startsWith("/") || cleaned.startsWith("~")) {
    return "";
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

/** Regex to identify suggest rows in xterm buffer text */
const SUGGEST_RE = /suggest:\s+.+\|/;
/** Regex to identify intent rows in xterm buffer text */
const INTENT_RE = /^[\s●⏺]*intent:\s+/;

// Max bytes to buffer before terminal is opened (prevents unbounded growth)
const OUTPUT_BUFFER_MAX_BYTES = 100 * 1024; // 100KB

// Flow control watermarks (bytes pending in xterm.js write queue)
const HIGH_WATERMARK = 512 * 1024;  // 512KB — pause reader when exceeded
const LOW_WATERMARK = 128 * 1024;   // 128KB — resume reader when drained below

// Target line height — snapped to integer device pixels at runtime to prevent
// sub-pixel seams between WebGL cell quads (see snapLineHeight).
const TARGET_LINE_HEIGHT = 1.2;

/** Snap lineHeight so cellHeight × devicePixelRatio is an integer.
 *  Fractional device-pixel cell heights cause visible 1px seams between rows
 *  in the WebGL renderer because adjacent quads round differently. */
function snapLineHeight(fontSize: number, target: number = TARGET_LINE_HEIGHT): number {
  const dpr = window.devicePixelRatio || 1;
  const rawDevicePx = fontSize * target * dpr;
  // Pick the closest integer device-pixel height to the target
  const lo = Math.floor(rawDevicePx);
  const hi = Math.ceil(rawDevicePx);
  const best = (Math.abs(rawDevicePx - lo) <= Math.abs(rawDevicePx - hi)) ? lo : hi;
  // Avoid degenerate values (too tight or too loose)
  const snapped = best / (fontSize * dpr);
  return Math.max(1.0, Math.min(snapped, 1.5));
}

// Minimum container dimensions before fit() is allowed — prevents WebGL rendering
// artifacts when xterm gets squeezed into impossibly small panes (e.g. narrow split)
const MIN_FIT_WIDTH = 80;   // px (~5 columns at 14px)
const MIN_FIT_HEIGHT = 40;  // px (~2 rows)

export const Terminal: Component<TerminalProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let terminal: XTerm | undefined;
  let fitAddon: FitAddon | undefined;
  const [searchAddon, setSearchAddon] = createSignal<SearchAddon | undefined>();
  let sessionId: string | null = null;

  // Search overlay state
  const [searchVisible, setSearchVisible] = createSignal(false);
  // WebSocket reconnect state (browser/PWA mode only)
  const [reconnecting, setReconnecting] = createSignal<{ attempt: number; max: number } | null>(null);
  let sessionInitialized = false;
  let unsubscribePty: Unsubscribe | undefined;
  let unlistenParsed: (() => void) | undefined;
  let unlistenKitty: (() => void) | undefined;

  // Kitty keyboard protocol: current flags for this session (0 = disabled)
  let kittyFlags = 0;

  // WebGL addon — retained so we can clear its atlas on demand and recreate
  // it after context loss. The addon exposes events that let us react to
  // actual atlas stress instead of rebuilding on a fixed timer.
  let webglAddon: WebglAddon | undefined;

  // Decoration-based token hiding: suggest rows get an opaque overlay via
  // xterm's registerDecoration API (works with WebGL renderer unlike DOM-based
  // MutationObserver). Each decoration is disposed when new suggest arrives or
  // when the agent cycle resets.
  let suggestDecorations: import("@xterm/xterm").IDecoration[] = [];
  // Intent rows get a colored overlay (dim yellow) via decoration
  let intentDecorations: import("@xterm/xterm").IDecoration[] = [];
  // Throttle counter for atlas cleanup triggered by onAddTextureAtlasCanvas.
  // Pages are added when the packer runs out of room for new glyphs, so a
  // burst of additions is a real signal of stress (e.g. large diverse output).
  let atlasPagesSinceCleanup = 0;
  let atlasLastCleanupMs = 0;
  const ATLAS_CLEANUP_MIN_PAGES = 3;
  const ATLAS_CLEANUP_MIN_INTERVAL_MS = 30_000;

  // Buffer for PTY output arriving before terminal.open()
  let outputBuffer: string[] = [];
  let outputBufferBytes = 0;

  // Flow control state
  let pendingWriteBytes = 0;
  let isPaused = false;

  // Auto-retry on API server errors — per-agent setting
  const RETRY_DELAYS = [5_000, 15_000, 30_000]; // exponential backoff
  let retryCount = 0;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let agentDetectTimer: ReturnType<typeof setTimeout> | undefined;

  // Resize debounce (150ms trailing edge)
  let resizeTimer: ReturnType<typeof setTimeout> | undefined;
  // ResizeObserver debounce — coalesces rapid layout changes before fitting
  let resizeObserverTimer: ReturnType<typeof setTimeout> | undefined;
  // rAF handle for the visibility effect — cancellable on cleanup
  let rafHandle = 0;

  let activityFlagged = false; // Avoids redundant activity store updates per data chunk
  let lastDataAtTimestamp = 0; // Throttle lastDataAt store updates to 1s
  let planFileNotified = false; // Play info sound at most once per agent cycle
  // Hide terminal until first fit to prevent visible resize flicker (80x24 → actual)
  const [fitted, setFitted] = createSignal(false);

  // Scroll position tracker — handles visibility, alternate buffer, and
  // re-entrancy without any DOM reads. See scrollTracker.ts for details.
  const scrollTracker = new ScrollTracker();
  const viewportLock = new ViewportLock();

  /** Fit terminal to container, preserving scroll position across reflows. */
  const doFit = () => {
    if (!containerRef || !fitAddon || !terminal) return;
    if (containerRef.offsetWidth < MIN_FIT_WIDTH || containerRef.offsetHeight < MIN_FIT_HEIGHT) return;

    // Snapshot BEFORE fit — fitAddon.fit() triggers reflow → onScroll events
    // that would corrupt the tracker's state before we can read it.
    const snapshot = scrollTracker.snapshotForFit();
    fitAddon.fit();
    const action = scrollTracker.computeFitRestore(terminal.buffer.active.baseY, snapshot);


    if (action.type === "scroll-to-bottom") {
      scrollTracker.suppressNextScroll();
      terminal.scrollToBottom();
    } else if (action.type === "scroll-to-line") {
      scrollTracker.suppressNextScroll();
      terminal.scrollToLine(action.line!);
    }
    viewportLock.update(scrollTracker.isAtBottom);
    if (!fitted()) setFitted(true);
  };

  // Reset activity flag when this terminal becomes active (store clears activity)
  createEffect(() => {
    if (terminalsStore.state.activeId === props.id) {
      activityFlagged = false;
    }
  });


  // Edge-detection for notification sounds: play once per awaitingInput transition.
  // Event handlers set state idempotently; this effect handles the one-shot sound.
  // Low-confidence questions are debounced to avoid phantom notifications when the
  // user is already typing (awaitingInput set then cleared within milliseconds).
  let prevAwaitingInput: AwaitingInputType = null;
  let questionDebounceTimer = 0;
  createEffect(() => {
    const term = terminalsStore.get(props.id);
    const current = term?.awaitingInput ?? null;
    const confident = term?.awaitingInputConfident ?? false;
    const sound = getAwaitingInputSound(prevAwaitingInput, current);
    prevAwaitingInput = current;

    // Clear any pending debounced question notification on state change
    if (questionDebounceTimer) {
      clearTimeout(questionDebounceTimer);
      questionDebounceTimer = 0;
    }

    if (sound === "error") {
      appLogger.info("terminal", `[Notify] ${props.id} error — awaitingInput transition`);
      notificationsStore.playError();
    } else if (sound === "question") {
      if (confident) {
        appLogger.info("terminal", `[Notify] ${props.id} question — awaitingInput transition`);
        notificationsStore.playQuestion();
      } else {
        // Debounce low-confidence questions: if cleared within 500ms, skip notification
        questionDebounceTimer = setTimeout(() => {
          questionDebounceTimer = 0;
          if (terminalsStore.get(props.id)?.awaitingInput === "question") {
            appLogger.info("terminal", `[Notify] ${props.id} question — awaitingInput transition (debounced)`);
            notificationsStore.playQuestion();
          }
        }, 500) as unknown as number;
      }
    }
  });

  // Original tab name before any OSC title overwrote it
  let originalName: string | null = null;

  const pty = usePty();

  const getFontFamily = () => {
    const font = settingsStore.state.font;
    return FONT_FAMILIES[font] || FONT_FAMILIES["JetBrains Mono"];
  };

  /** Preload a font via CSS Font Loading API so canvas/WebGL renderers can use it.
   *  @font-face fonts only load when referenced by DOM text; canvas-based xterm.js
   *  never triggers that, so we must load explicitly before applying. */
  const preloadFont = (fontName: string): Promise<FontFace[]> =>
    document.fonts.load(`16px "${fontName}"`);


  /** Process a chunk of PTY output — write to terminal or buffer if not ready */
  const handlePtyData = (rawData: string) => {
    if (terminal) {
      // Dispatch to plugins for all terminals — background tabs may have
      // plugin-relevant output (e.g. agent detection, error tracking)
      if (sessionId) {
        pluginRegistry.processRawOutput(rawData, sessionId);
      }

      const byteLen = rawData.length;
      pendingWriteBytes += byteLen;

      // Pause reader if we've accumulated too much unprocessed data.
      // Only pause when the terminal is visible — hidden terminals (inactive
      // pane-group tabs) may not fire write callbacks because the xterm render
      // loop is suspended on display:none containers.  Pausing a hidden
      // terminal's reader would block the PTY permanently since the resume
      // callback never fires.
      if (!isPaused && isVisible() && pendingWriteBytes > HIGH_WATERMARK && sessionId) {
        isPaused = true;
        pty.pause(sessionId).catch((err) => appLogger.warn("terminal", "PTY pause failed", { error: String(err) }));
      }

      viewportLock.writeStart();
      try {
        terminal.write(rawData, () => {
          viewportLock.writeEnd();
          pendingWriteBytes -= byteLen;
          if (isPaused && pendingWriteBytes < LOW_WATERMARK && sessionId) {
            isPaused = false;
            pty.resume(sessionId).catch((err) => appLogger.warn("terminal", "PTY resume failed", { error: String(err) }));
          }
        });
      } catch (e) {
        viewportLock.writeEnd();
        pendingWriteBytes -= byteLen;
        appLogger.warn("terminal", "terminal.write() threw", { error: String(e), sessionId });
      }
    } else {
      // Buffer output until terminal.open() is called
      outputBuffer.push(rawData);
      outputBufferBytes += rawData.length;
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

    // shellState is now derived in Rust (reader thread + silence timer).
    // handlePtyData no longer touches shellState — see ParsedEvent "shell-state" handler.
  };

  /** Replay buffered output into the now-open terminal */
  const replayBuffer = () => {
    if (terminal && outputBuffer.length > 0) {
      viewportLock.writeStart();
      for (const chunk of outputBuffer) {
        terminal.write(chunk);
      }
      outputBuffer = [];
      outputBufferBytes = 0;
      // writeEnd before state check so the scroll classification is correct
      viewportLock.writeEnd();
      scrollTracker.onScroll(terminal.buffer.active);
      viewportLock.update(scrollTracker.isAtBottom);
    }
  };

  /** Set up event listeners for a known session ID */
  const attachSessionListeners = async (targetSessionId: string) => {

    // Shared handler for structured events from Rust OutputParser.
    // Used by both Tauri (listen) and browser (WebSocket onParsed) modes.
    const handleParsedEvent = (parsed: ParsedEvent) => {
      switch (parsed.type) {
        case "progress": {
          const awProg = terminalsStore.get(props.id)?.awaitingInput;
          if (awProg && awProg !== "error" && awProg !== "question") {
            terminalsStore.clearAwaitingInput(props.id);
          }
          if (parsed.state === 0) {
            terminalsStore.update(props.id, { progress: null });
          } else if (parsed.state === 1 || parsed.state === 2 || parsed.state === 3) {
            terminalsStore.update(props.id, { progress: Math.min(100, Math.max(0, parsed.value)) });
          }
          break;
        }
        case "status-line": {
          retryCount = 0;
          const awState = terminalsStore.get(props.id)?.awaitingInput;
          if (awState && awState !== "error" && awState !== "question") {
            terminalsStore.clearAwaitingInput(props.id);
          }
          terminalsStore.update(props.id, { currentTask: parsed.task_name });
          break;
        }
        case "active-subtasks": {
          terminalsStore.update(props.id, { activeSubTasks: parsed.count });
          break;
        }
        case "rate-limit": {
          const terminal = terminalsStore.get(props.id);
          const detectedAgent = terminal?.agentType;
          appLogger.debug("terminal", `[RateLimit] pattern=${parsed.pattern_name} matched="${parsed.matched_text}" agent=${detectedAgent ?? "none"} shellState=${terminal?.shellState} sessionId=${targetSessionId}`);
          if (terminal?.shellState === "busy") {
            appLogger.debug("terminal", `[RateLimit] IGNORED (shellState=busy, likely false positive) pattern=${parsed.pattern_name}`);
            break;
          }
          const existing = rateLimitStore.getRateLimitInfo(targetSessionId);
          const recentlyDetected = existing && (Date.now() - existing.detectedAt) < 5000;
          if (detectedAgent && !recentlyDetected) {
            const info = {
              agentType: detectedAgent,
              sessionId: targetSessionId,
              retryAfterMs: parsed.retry_after_ms,
              message: `Rate limit detected (${parsed.pattern_name}): ${parsed.matched_text}`,
              detectedAt: Date.now(),
            };
            rateLimitStore.addRateLimit(info);
            props.onRateLimit?.(props.id, targetSessionId, parsed.retry_after_ms);
            appLogger.info("terminal", `[Notify] ${props.id} warning — rate-limit pattern=${parsed.pattern_name} matched="${parsed.matched_text}"`);
            notificationsStore.playWarning();
          }
          break;
        }
        case "question": {
          const qTerminal = terminalsStore.get(props.id);
          if (!parsed.confident && (qTerminal?.shellState === "busy" || (qTerminal?.activeSubTasks ?? 0) > 0)) {
            appLogger.debug("terminal", `[ParsedEvent] ${props.id} question IGNORED (busy=${qTerminal?.shellState === "busy"} subTasks=${qTerminal?.activeSubTasks} low-confidence) prompt="${parsed.prompt_text}"`);
            break;
          }
          terminalsStore.setAwaitingInput(props.id, "question", !!parsed.confident);
          break;
        }
        case "usage-limit": {
          const current = terminalsStore.get(props.id)?.usageLimit;
          if (current?.percentage !== parsed.percentage || current?.limitType !== parsed.limit_type) {
            terminalsStore.update(props.id, {
              usageLimit: { percentage: parsed.percentage, limitType: parsed.limit_type },
            });
          }
          break;
        }
        case "usage-exhausted": {
          appLogger.warn("terminal", `[UsageExhausted] ${props.id} reset_time=${parsed.reset_time ?? "unknown"}`);
          terminalsStore.setAwaitingInput(props.id, "error");
          break;
        }
        case "plan-file":
          if (terminalsStore.state.activeId !== props.id && !planFileNotified) {
            planFileNotified = true;
            appLogger.info("terminal", `[Notify] ${props.id} info — plan-file path="${parsed.path}" (background tab)`);
            notificationsStore.playInfo();
          }
          break;
        case "user-input":
          planFileNotified = false;
          // Clear suggest bar, pending buffer, and mark dismissed
          terminalsStore.update(props.id, { suggestDismissed: true, suggestedActions: null, pendingSuggest: null, activeSubTasks: 0 });
          // Remove suggest decoration overlays
          for (const d of suggestDecorations) d.dispose();
          suggestDecorations = [];
          invoke<string | null>("get_last_prompt", { sessionId: targetSessionId }).then((prompt) => {
            if (prompt !== null) terminalsStore.setLastPrompt(props.id, prompt);
          }).catch(() => {});
          break;
        case "api-error": {
          const agent = terminalsStore.get(props.id)?.agentType;
          const { error_kind: kind, pattern_name: patternName, matched_text: matchedText } = parsed;
          appLogger.debug("terminal", `[ApiError] ${props.id} pattern=${patternName} kind=${kind} agent=${agent ?? "none"} matched="${matchedText}"`);
          const label = agent ?? "Agent";
          const kindLabel = kind === "server" ? "server error" : kind === "auth" ? "auth failure" : "API error";
          appLogger.error("terminal", `${label}: ${kindLabel} (${patternName})`);
          terminalsStore.setAwaitingInput(props.id, "error");

          if (kind === "server" && agent && agentConfigsStore.isAutoRetryEnabled(agent) && !retryTimer) {
            if (retryCount < RETRY_DELAYS.length) {
              const delay = RETRY_DELAYS[retryCount];
              const attempt = retryCount + 1;
              appLogger.info("terminal", `[AutoRetry] ${label}: attempt ${attempt}/${RETRY_DELAYS.length} in ${delay / 1000}s`);
              retryTimer = setTimeout(() => {
                retryTimer = undefined;
                const current = terminalsStore.get(props.id);
                if (sessionId && current?.awaitingInput === "error") {
                  appLogger.info("terminal", `[AutoRetry] ${label}: injecting "continue" (attempt ${attempt})`);
                  pty.write(sessionId, "continue\r").catch((err) =>
                    appLogger.error("terminal", "[AutoRetry] Failed to write", { error: String(err) }),
                  );
                }
              }, delay);
              retryCount++;
            } else {
              appLogger.warn("terminal", `[AutoRetry] ${label}: exhausted ${RETRY_DELAYS.length} retries, manual intervention needed`);
            }
          }
          break;
        }
        case "intent":
          retryCount = 0;
          terminalsStore.setAgentIntent(props.id, parsed.text);
          if (parsed.title && settingsStore.state.intentTabTitle) {
            terminalsStore.update(props.id, { name: parsed.title });
          }
          // Color intent row with a subtle tint via decoration overlay
          if (terminal) {
            const term = terminal;
            requestAnimationFrame(() => {
              const buf = term.buffer.active;
              const cursorLine = buf.baseY + buf.cursorY;
              for (let offset = 0; offset < 5; offset++) {
                const lineIdx = cursorLine - offset;
                if (lineIdx < 0) break;
                const line = buf.getLine(lineIdx);
                if (!line) continue;
                const text = line.translateToString(true);
                if (INTENT_RE.test(text)) {
                  const markerOffset = -(buf.cursorY - (lineIdx - buf.baseY));
                  const marker = term.registerMarker(markerOffset);
                  if (marker) {
                    const deco = term.registerDecoration({ marker, width: term.cols });
                    if (deco) {
                      deco.onRender((el) => {
                        el.style.width = "100%";
                        el.style.height = "100%";
                        el.style.background = "rgba(181, 147, 90, 0.12)";
                        el.style.pointerEvents = "none";
                      });
                      intentDecorations.push(deco);
                      if (intentDecorations.length > 50) {
                        intentDecorations.shift()?.dispose();
                      }
                    }
                  }
                  break;
                }
              }
            });
          }
          break;
        case "suggest":
          if (settingsStore.state.suggestFollowups && parsed.items?.length) {
            const t = terminalsStore.get(props.id);
            if (t?.shellState === "idle") {
              if (!t.suggestDismissed) {
                terminalsStore.setSuggestedActions(props.id, parsed.items);
              }
            } else {
              terminalsStore.update(props.id, { pendingSuggest: parsed.items });
            }
            // Hide suggest rows via xterm decoration overlay (opaque background).
            // Deferred to next frame: the parsed event can arrive before
            // terminal.write() flushes the chunk into the buffer.
            if (terminal) {
              const term = terminal;
              requestAnimationFrame(() => {
                for (const d of suggestDecorations) d.dispose();
                suggestDecorations = [];
                const buf = term.buffer.active;
                const cursorLine = buf.baseY + buf.cursorY;
                const bg = term.options.theme?.background ?? "#1e1e1e";
                for (let offset = 0; offset < 5; offset++) {
                  const lineIdx = cursorLine - offset;
                  if (lineIdx < 0) break;
                  const line = buf.getLine(lineIdx);
                  if (!line) continue;
                  const text = line.translateToString(true);
                  if (SUGGEST_RE.test(text)) {
                    const markerOffset = -(buf.cursorY - (lineIdx - buf.baseY));
                    const marker = term.registerMarker(markerOffset);
                    if (marker) {
                      const deco = term.registerDecoration({ marker, width: term.cols });
                      if (deco) {
                        deco.onRender((el) => {
                          el.style.width = "100%";
                          el.style.height = "100%";
                          el.style.background = bg;
                        });
                        suggestDecorations.push(deco);
                      }
                    }
                    // Cover wrapped continuation line if present
                    const nextLine = buf.getLine(lineIdx + 1);
                    if (nextLine && (nextLine.isWrapped || nextLine.translateToString(true).includes("|"))) {
                      const marker2 = term.registerMarker(markerOffset + 1);
                      if (marker2) {
                        const deco2 = term.registerDecoration({ marker: marker2, width: term.cols });
                        if (deco2) {
                          deco2.onRender((el) => {
                            el.style.width = "100%";
                            el.style.height = "100%";
                            el.style.background = bg;
                          });
                          suggestDecorations.push(deco2);
                        }
                      }
                    }
                    break;
                  }
                }
              });
            }
          }
          break;
        case "shell-state": {
          terminalsStore.update(props.id, { shellState: parsed.state });
          if (parsed.state !== "idle") {
            // Shell goes busy: clear stale suggest bar and pending from previous cycle.
            // Reset dismissed so the suggest emitted at the END of this cycle can show.
            terminalsStore.update(props.id, { suggestedActions: null, suggestDismissed: false, pendingSuggest: null });
            // Remove suggest decoration overlays from previous cycle
            for (const d of suggestDecorations) d.dispose();
            suggestDecorations = [];
          }
          if (parsed.state === "idle") {
            // Show pending suggest buffered during the just-completed busy cycle
            const pendingT = terminalsStore.get(props.id);
            if (pendingT?.pendingSuggest?.length) {
              terminalsStore.setSuggestedActions(props.id, pendingT.pendingSuggest);
              terminalsStore.update(props.id, { pendingSuggest: null });
            }
            const initCmd = terminalsStore.get(props.id)?.pendingInitCommand;
            if (initCmd && targetSessionId) {
              terminalsStore.update(props.id, { pendingInitCommand: null });
              pty.write(targetSessionId, initCmd + "\r").catch((e) =>
                appLogger.error("terminal", "Failed to write init command", { error: String(e) }),
              );
            }
            // Idle: detect agent immediately — only idle can clear a detected agent
            detectAgentForTerminal(props.id, "idle").catch((err) =>
              appLogger.warn("terminal", "[AgentDetect] unexpected error", { error: String(err), termId: props.id }),
            );
          } else {
            // Busy: detect agent after 500ms debounce (can discover, never clear)
            clearTimeout(agentDetectTimer);
            agentDetectTimer = setTimeout(() => {
              detectAgentForTerminal(props.id, "busy").catch((err) =>
              appLogger.warn("terminal", "[AgentDetect] unexpected error", { error: String(err), termId: props.id }),
            );
            }, 500);
          }
          break;
        }
      }

      pluginRegistry.dispatchStructuredEvent(parsed.type, parsed, targetSessionId);
    };

    // PTY output + exit via transport abstraction (Tauri listen or WebSocket)
    unsubscribePty = await subscribePty(
      targetSessionId,
      (data: string) => handlePtyData(data),
      () => {
        if (terminal) {
          terminal.writeln("\r\n\x1b[33m[Process exited]\x1b[0m");
        }
        // Guard: terminal may have been removed from the store already
        // (e.g. pane closed). Updating a removed entry would recreate it as a ghost.
        const stillExists = terminalsStore.get(props.id);
        if (stillExists) {
          // Restore original tab name if it was overwritten by OSC title
          if (originalName && !stillExists.nameIsCustom) {
            terminalsStore.update(props.id, { name: originalName });
          }
          const hadAgent = stillExists.agentType !== null;
          terminalsStore.update(props.id, { sessionId: null, currentTask: null, agentType: null, agentSessionId: null });
          terminalsStore.clearAwaitingInput(props.id);
          if (hadAgent) {
            pluginRegistry.notifyStateChange({ type: "agent-stopped", sessionId: targetSessionId, terminalId: props.id });
          }
        }
        sessionId = null;
        props.onSessionExit?.(props.id);
        if (terminalsStore.state.activeId !== props.id) {
          appLogger.info("terminal", `[Notify] ${props.id} completion — session exited (background tab)`);
          notificationsStore.playCompletion();
        }
      },
      {
        onReconnecting: (attempt, max) => setReconnecting({ attempt, max }),
        onReconnected: () => setReconnecting(null),
        // Browser mode: receive parsed events via WebSocket JSON frames
        onParsed: (frame) => {
          if (frame.type === "parsed" && frame.event) {
            handleParsedEvent(frame.event as ParsedEvent);
          }
        },
      },
    );

    // Tauri-only listeners (kitty keyboard, shell state sync)
    if (isTauri()) {
      const { listen } = await import("@tauri-apps/api/event");
      unlistenParsed = await listen<ParsedEvent>(`pty-parsed-${targetSessionId}`, (event) => {
        handleParsedEvent(event.payload);
      });

      // Listen for kitty keyboard protocol flag changes from Rust
      unlistenKitty = await listen<number>(`kitty-keyboard-${targetSessionId}`, (event) => {
        kittyFlags = event.payload;
      });

      // Sync initial kitty flags — the push event may have fired before listener attached.
      // Only apply if the listener hasn't already updated kittyFlags (race guard).
      const preListenFlags = kittyFlags;
      const flags = await pty.getKittyFlags(targetSessionId);
      if (flags > 0 && kittyFlags === preListenFlags) {
        kittyFlags = flags;
      }

      // Sync shell state from Rust — covers events missed while unsubscribed
      // (e.g. tab switch, branch switch, component remount).
      invoke<string | null>("get_shell_state", { sessionId: targetSessionId }).then((rustState) => {
        if (rustState) {
          const current = terminalsStore.get(props.id)?.shellState;
          if (current !== rustState) {
            appLogger.debug("terminal", `[ShellState] ${props.id} sync from Rust: "${current}" → "${rustState}"`);
            if (isShellState(rustState)) {
              terminalsStore.update(props.id, { shellState: rustState });
            }
          }
        }
      }).catch(() => {});
    }
  };

  // Eagerly attach listeners for existing sessions (before terminal.open())
  // This prevents data loss when PTY output arrives while terminal is on a background tab
  const existingSessionId = terminalsStore.get(props.id)?.sessionId;
  if (existingSessionId) {
    sessionId = existingSessionId;
    // attachSessionListeners syncs shell state from Rust via get_shell_state
    attachSessionListeners(existingSessionId).catch((err) =>
      appLogger.error("terminal", "Failed to attach session listeners", err),
    );
  }

  /** Initialize PTY session and event listeners */
  const initSession = async () => {
    if (sessionInitialized || !terminal) return;
    sessionInitialized = true;
    appLogger.info("terminal", `initSession(${props.id}) — existing sessionId=${sessionId ?? "null"}`);

    try {
      let reconnected = false;
      if (sessionId) {
        // Already have a session (eagerly attached above) — just resize to current dimensions
        try {

          await pty.resize(sessionId, terminal.rows, terminal.cols);
          reconnected = true;
          appLogger.info("terminal", `initSession(${props.id}) — reconnected to ${sessionId}`);
        } catch {
          // Session no longer exists (app restarted) - create fresh
          appLogger.warn("terminal", `initSession(${props.id}) — resize failed for ${sessionId}, creating FRESH session`);
          sessionId = null;
          unsubscribePty?.();
          unlistenParsed?.();
          unlistenKitty?.();
          unlistenKitty = undefined;
        }
      }
      if (!reconnected) {
        appLogger.debug("terminal", `initSession(${props.id}) — creating FRESH PTY session (no prior sessionId)`);
        const termData = terminalsStore.get(props.id);
        sessionId = await pty.createSession({
          rows: terminal.rows,
          cols: terminal.cols,
          shell: settingsStore.state.shell ?? null,
          cwd: props.cwd || null,
          tuic_session: termData?.tuicSession ?? null,
          env: agentConfigsStore.getEnvFlags("claude"),
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

        // Flush a resize to the PTY with authoritative dimensions. The PTY
        // may have been created with preliminary rows/cols if the WebView
        // layout hadn't fully stabilized yet. Re-fit xterm to get the current
        // container size, then send that to the PTY so the shell (and any
        // agent launched from it) sees the correct viewport from the start.
        // Critical for the first terminal after app launch.
        if (terminal) {
          doFit();
          if (terminal.rows > 0 && terminal.cols > 0) {
            pty.resize(sessionId, terminal.rows, terminal.cols).catch(() => {});
          }
        }
      }
    } catch (err) {
      terminal.writeln(`\x1b[31mFailed to create PTY: ${err}\x1b[0m`);
    }
  };

  /** Fully rebuild the WebGL renderer by disposing the current addon and
   *  instantiating a new one. Deferred via queueMicrotask so it is safe to
   *  call from inside an addon callback (e.g. onAddTextureAtlasCanvas) —
   *  the current event handler finishes before dispose runs.
   *
   *  clearTextureAtlas() alone only wipes the glyph cache; it does not
   *  reset the atlas packer's internal layout or the underlying WebGL
   *  textures, so structural corruption (post-sleep texture loss,
   *  packer state drift after diverse-unicode bursts) survives a clear.
   *  A full addon recreate rebuilds every renderer resource while leaving
   *  the xterm core buffer, scroll position, and selection intact. */
  const rebuildAtlas = () => {
    if (!terminal || !webglAddon) return;
    const old = webglAddon;
    webglAddon = undefined;
    queueMicrotask(() => {
      try {
        old.dispose();
      } catch {
        // Addon may already be disposed (e.g. context loss race) — ignore.
      }
      if (terminal && !webglAddon) {
        webglAddon = createWebglAddon();
      }
    });
  };

  /** Instantiate WebglAddon and wire its lifecycle events.
   *  - onContextLoss: recreate the addon so WebGL rendering survives sleep/resume.
   *    Without recreation the terminal silently falls back to the DOM renderer.
   *  - onAddTextureAtlasCanvas: pages are added when the packer runs out of room
   *    for new glyphs. A burst of additions signals real atlas stress — we
   *    respond with a full renderer rebuild (clearTextureAtlas alone does not
   *    recover from structural packer corruption). */
  const createWebglAddon = (): WebglAddon | undefined => {
    if (!terminal) return undefined;
    try {
      const addon = new WebglAddon();
      addon.onContextLoss(() => {
        addon.dispose();
        if (webglAddon === addon) webglAddon = undefined;
        // Recreate on next microtask so dispose finishes first
        queueMicrotask(() => {
          if (terminal && !webglAddon) {
            webglAddon = createWebglAddon();
          }
        });
      });
      addon.onAddTextureAtlasCanvas(() => {
        atlasPagesSinceCleanup++;
        const now = performance.now();
        if (
          atlasPagesSinceCleanup >= ATLAS_CLEANUP_MIN_PAGES &&
          now - atlasLastCleanupMs > ATLAS_CLEANUP_MIN_INTERVAL_MS
        ) {
          atlasPagesSinceCleanup = 0;
          atlasLastCleanupMs = now;
          rebuildAtlas();
        }
      });
      terminal.loadAddon(addon);
      return addon;
    } catch {
      return undefined;
    }
  };

  let terminalOpened = false;
  let resizeObserver: ResizeObserver | undefined;

  /** Open xterm in the container and wire up event handlers (deferred until visible) */
  const openTerminal = () => {
    if (terminalOpened || !containerRef) return;
    terminalOpened = true;
    // DIAG: expose xterm instances for console debugging
    const w = window as any;
    if (!w.__terms) w.__terms = {};
    w.__terms[props.id] = () => terminal;

    terminal = new XTerm({
      scrollback: 10000,
      fontSize: settingsStore.state.defaultFontSize,
      fontFamily: getFontFamily(),
      fontWeight: String(settingsStore.state.fontWeight) as any,
      fontWeightBold: "bold",
      lineHeight: snapLineHeight(settingsStore.state.defaultFontSize),
      theme: currentTheme(),
      cursorBlink: true,
      allowProposedApi: true,
      rescaleOverlappingGlyphs: true,
      macOptionIsMeta: false, // Right Option keeps macOS composition (π, ∑, @…)
      // Override xterm's default OSC 8 link handler — the built-in one calls
      // window.confirm("WARNING: potentially dangerous") + window.open(), which
      // shows a scary dialog and then fails in Tauri (window.open is a no-op).
      linkHandler: {
        activate: (_event, uri) => {
          // Route file:// URIs to the file opener (e.g. OSC 8 links from Claude Code)
          if (uri.startsWith("file://") && props.onOpenFilePath) {
            try {
              const parsed = new URL(uri);
              const filePath = decodeURIComponent(parsed.pathname);
              if (filePath) {
                props.onOpenFilePath(filePath);
                return;
              }
            } catch { /* fall through to handleOpenUrl */ }
          }
          handleOpenUrl(uri);
        },
      },
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
    // Tracks the ESC key cycle (keydown→keypress→keyup) when dismissing the
    // resume banner so ALL event types are blocked, preventing a stray \x1b
    // keypress from reaching xterm (which would eat the next typed character).
    let blockEscForResumeDismiss = false;
    terminal.attachCustomKeyEventHandler((event: KeyboardEvent) => {
      // Arrow Down with no modifiers: snap to bottom when viewport is scrolled up
      if (event.type === "keydown" && event.key === "ArrowDown" && !event.shiftKey && !event.ctrlKey && !event.metaKey && !event.altKey && !scrollTracker.isAtBottom) {
        terminal!.scrollToBottom();
        return false;
      }

      // Cmd+Up/Down (macOS) or Ctrl+Up/Down (Win/Linux): navigate between command blocks (OSC 133)
      if (event.type === "keydown" && (event.metaKey || event.ctrlKey) && !event.altKey && !event.shiftKey
        && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
        const term = terminalsStore.get(props.id);
        if (term) {
          const blocks = term.commandBlocks;
          const active = term.activeBlock;
          const allPromptLines = blocks.map((b) => b.promptLine).concat(active ? [active.promptLine] : []);
          if (allPromptLines.length > 0) {
            const buf = terminal!.buffer.active;
            const currentViewLine = buf.viewportY;
            let targetLine: number | undefined;
            if (event.key === "ArrowUp") {
              // Find the nearest block prompt ABOVE the current viewport
              for (let i = allPromptLines.length - 1; i >= 0; i--) {
                if (allPromptLines[i] < currentViewLine) { targetLine = allPromptLines[i]; break; }
              }
            } else {
              // Find the nearest block prompt BELOW the current viewport
              for (let i = 0; i < allPromptLines.length; i++) {
                if (allPromptLines[i] > currentViewLine) { targetLine = allPromptLines[i]; break; }
              }
            }
            if (targetLine !== undefined) {
              terminal!.scrollToLine(targetLine);
              scrollTracker.onScroll(buf);
              viewportLock.update(scrollTracker.isAtBottom);
            }
            event.preventDefault();
            return false;
          }
        }
      }

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

      // Resume banner keyboard handling: Space/Enter accept (resume),
      // Escape or any other key dismisses without resuming.
      // Block the full key cycle to prevent stray keypresses reaching xterm.
      if (terminalsStore.get(props.id)?.pendingResumeCommand || blockEscForResumeDismiss) {
        if (event.type === "keydown" && !blockEscForResumeDismiss) {
          if (event.key === " " || event.key === "Enter") {
            // Accept: execute the resume command
            blockEscForResumeDismiss = true;
            handleResume();
          } else if (event.key.length === 1) {
            // Dismiss on printable key: clear banner and let the keystroke
            // pass through to xterm so the typed character is not swallowed.
            terminalsStore.update(props.id, { pendingResumeCommand: null });
            return true;
          } else if (event.key === "Escape" || event.key === "Backspace" || event.key === "Delete" || event.key === "Tab") {
            // Dismiss on editing/control keys: block the full key cycle
            // to prevent stray sequences (e.g. \x1b from Escape) reaching xterm.
            blockEscForResumeDismiss = true;
            terminalsStore.update(props.id, { pendingResumeCommand: null });
            terminal?.focus();
          } else {
            // Modifier-only keys (Shift, Ctrl, Alt, Meta) — ignore, don't dismiss
            return false;
          }
        } else if (event.type === "keyup") {
          blockEscForResumeDismiss = false;
        }
        return false;
      }

      // Shift+Enter → ESC CR (\x1b\r): standard multi-line newline for CLI apps
      // (e.g. Claude Code, Ink). Native terminals like ghostty/kitty/WezTerm send
      // this sequence natively; we replicate the behavior for our embedded terminal.
      // NOTE: this intentionally runs BEFORE the kitty keyboard block below.
      // CC expects \x1b\r from the terminal, not kitty CSI u (\x1b[13;2u).
      // Block ALL event types (keydown + keypress + keyup) — xterm fires keypress
      // for Enter which would send a bare \r and override our \x1b\r.
      if (event.key === "Enter" && event.shiftKey && !event.ctrlKey && !event.metaKey && !event.altKey) {
        if (event.type === "keydown") {
          terminal!.input("\x1b\r", true);
        }
        return false;
      }

      // Shift+Tab: prevent browser focus navigation while letting xterm send CSI Z
      if (event.key === "Tab" && event.shiftKey && !event.ctrlKey && !event.metaKey && !event.altKey) {
        event.preventDefault();
        return true;
      }

      // macOS WebKit Emacs keybindings: Ctrl+A/D/E/K etc. are intercepted by the
      // native text system on the hidden textarea before xterm sees them. We
      // explicitly send the correct control codes and block WebKit's handling.
      if (isMacOS() && event.type === "keydown" && event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey) {
        // event.key is "a"–"z" when Ctrl is held (most browsers), or the
        // control char itself ("\x01" etc.) in some WebKit builds.  Use
        // event.code (always "KeyA"–"KeyZ") for reliable mapping.
        const m = event.code.match(/^Key([A-Z])$/);
        if (m) {
          const ctrl = String.fromCharCode(m[1].charCodeAt(0) - 0x40); // A→\x01 … Z→\x1a
          event.preventDefault();
          terminal!.input(ctrl, true);
          return false;
        }
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
    terminal.loadAddon(new WebLinksAddon((event, uri) => {
      if (event.button !== 0) return; // only activate on left-click
      handleOpenUrl(uri);
    }));

    // Copy on select: auto-copy selection to clipboard when enabled.
    // Debounced to avoid IPC flood during drag-selection.
    let copyOnSelectTimer: ReturnType<typeof setTimeout> | undefined;
    terminal.onSelectionChange(() => {
      if (!settingsStore.state.copyOnSelect) return;
      clearTimeout(copyOnSelectTimer);
      copyOnSelectTimer = setTimeout(() => {
        const sel = terminal?.getSelection();
        if (sel) {
          const setStatus = (window as unknown as Record<string, unknown>).__tuic_setStatusInfo as ((msg: string) => void) | undefined;
          navigator.clipboard.writeText(sel).then(() => {
            setStatus?.("Copied to clipboard");
          }).catch((err) => {
            appLogger.warn("terminal", "Copy-on-select clipboard write failed", err);
            setStatus?.("Copy failed — clipboard unavailable");
          });
        }
      }, 50);
    });

    // Bell handler: flash and/or beep on BEL character
    terminal.onBell(() => {
      const style = settingsStore.state.bellStyle;
      if (style === "none") return;
      if (style === "visual" || style === "both") {
        containerRef?.classList.add("bell-flash");
        setTimeout(() => containerRef?.classList.remove("bell-flash"), 150);
      }
      if (style === "sound" || style === "both") {
        notificationsStore.play("info").catch((err) => {
          appLogger.warn("terminal", "Bell audio playback failed", err);
        });
      }
    });

    const search = new SearchAddon();
    terminal.loadAddon(search);
    setSearchAddon(search);

    // Register link provider for file paths (clickable to open in IDE or MD viewer)
    if (props.onOpenFilePath) {
      // Matches paths starting with /, ./, ../, or relative paths containing / with known extensions.
      // Optional :line or :line:col suffix.
      const CODING_EXT = "rs|ts|tsx|js|jsx|mjs|cjs|py|go|java|kt|kts|swift|c|h|cpp|hpp|cc|cs|rb|php|lua|zig|nim|ex|exs|erl|hs|ml|mli|fs|fsx|scala|clj|cljs|r|R|jl|dart|v|sv|vhdl|sol|move|css|scss|sass|less|html|htm|vue|svelte|astro|json|jsonc|json5|yaml|yml|toml|ini|cfg|conf|env|xml|plist|csv|tsv|sql|graphql|gql|proto|thrift|avsc|md|mdx|txt|rst|tex|adoc|org|sh|bash|zsh|fish|ps1|psm1|bat|cmd|dockerfile|containerfile|tf|tfvars|hcl|nix|cmake|make|mk|gradle|sbt|cabal|gemspec|podspec|lock|sum|mod|workspace|editorconfig|gitignore|gitattributes|dockerignore|eslintrc|prettierrc|babelrc|nvmrc|tool-versions";
      const filePathRegex = new RegExp(
        `(?:^|[\\s"'\`(\\[{])` +                                    // boundary
        `((?:~/|/|\\.\\.?/|[\\w@.-]+/)` +                            // path start: ~/, /, ./, ../, or word/
        `[\\w./@-]*` +                                               // middle segments
        `\\.(?:${CODING_EXT})` +                                     // .ext
        `(?::\\d+(?::\\d+)?)?)` +                                    // optional :line:col
        `(?=[\\s"'\`),;.!?:\\]}>]|$)`,                                // boundary (incl. sentence-ending punctuation)
        "g",
      );
      // file:// URLs — capture group 1 is the absolute path (without the `file://` prefix).
      // Accepts `file:///abs/path` (standard) and bare `file://abs/path`.
      const fileUrlRegex = /\bfile:\/\/(\/[^\s"'`<>()[\]{}]+)/g;

      const onOpenFilePath = props.onOpenFilePath; // capture for closure

      // Cache resolved links per line to avoid flicker from async IPC on every mouse move.
      // Key: "lineNumber:lineText", value: resolved ILink[] or undefined.
      // Capped at 200 entries; cleared wholesale when full (lines rarely re-hover after scroll).
      const linkCache = new Map<string, import("@xterm/xterm").ILink[] | undefined>();
      const cacheSet = (key: string, val: import("@xterm/xterm").ILink[] | undefined) => {
        if (linkCache.size >= 200) linkCache.clear();
        linkCache.set(key, val);
      };

      terminal.registerLinkProvider({
        provideLinks(bufferLineNumber: number, callback: (links: import("@xterm/xterm").ILink[] | undefined) => void) {
          const bufLine = terminal!.buffer.active.getLine(bufferLineNumber - 1);
          if (!bufLine) { callback(undefined); return; }
          const lineText = bufLine.translateToString();

          const cacheKey = `${bufferLineNumber}:${lineText}`;
          if (linkCache.has(cacheKey)) {
            callback(linkCache.get(cacheKey));
            return;
          }

          // Matches store the full span to highlight (`text`) and the path to resolve
          // via IPC (`candidate`). For plain paths these coincide; for `file://` URLs
          // the span covers the whole URL while the candidate is the stripped path.
          const matches: { text: string; candidate: string; index: number }[] = [];
          let match: RegExpExecArray | null;
          filePathRegex.lastIndex = 0;
          while ((match = filePathRegex.exec(lineText)) !== null) {
            const idx = lineText.indexOf(match[1], match.index);
            matches.push({ text: match[1], candidate: match[1], index: idx });
          }
          // Also match `file://` URLs — the default WebLinksAddon only handles http(s)/ws/ftp,
          // and the plain-path regex above won't match because `file://` supplies a `/` as the
          // boundary char, which is not in its boundary class.
          fileUrlRegex.lastIndex = 0;
          while ((match = fileUrlRegex.exec(lineText)) !== null) {
            matches.push({ text: match[0], candidate: match[1], index: match.index });
          }
          if (matches.length === 0) {
            cacheSet(cacheKey, undefined);
            callback(undefined);
            return;
          }

          // Get cwd from the terminal's PTY session
          const termData = terminalsStore.get(props.id);
          const cwd = termData?.cwd || "";

          // Validate all candidates via Rust IPC
          Promise.all(
            matches.map(async (m) => {
              try {
                const resolved = await invoke<{ absolute_path: string; is_directory: boolean } | null>(
                  "resolve_terminal_path",
                  { cwd, candidate: m.candidate },
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
              const lineColMatch = r.candidate.match(/:(\d+)(?::(\d+))?$/);
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
                activate: (event: MouseEvent) => {
                  if (event.button !== 0) return; // only activate on left-click
                  onOpenFilePath(r.resolved.absolute_path, line, col);
                },
              });
            }
            const result = links.length > 0 ? links : undefined;
            cacheSet(cacheKey, result);
            callback(result);
          }).catch(() => {
            // Ensure xterm always gets its callback even on failure
            cacheSet(cacheKey, undefined);
            callback(undefined);
          });
        },
      });
    }

    terminal.open(containerRef);
    viewportLock.attach(
      containerRef,
      (line) => terminal!.scrollToLine(line),
      () => ({
        viewportY: terminal!.buffer.active.viewportY,
        baseY: terminal!.buffer.active.baseY,
        type: terminal!.buffer.active.type,
      }),
    );
    viewportLock.setLogger((event, details) => {
      appLogger.warn("ViewportLock", `${event} ${JSON.stringify(details)}`);
    });
    // Preload the configured font so the canvas/WebGL renderer can measure
    // and render it correctly from the start (see preloadFont comment above).
    preloadFont(settingsStore.state.font).then(() => doFit());

    // Unicode 11 width tables — fixes width estimation for emoji, box-drawing,
    // and progress bar characters (█▓▒░) that cause progressive rendering corruption.
    const unicode11 = new Unicode11Addon();
    terminal.loadAddon(unicode11);
    terminal.unicode.activeVersion = "11";

    // Load WebGL renderer for 3-5x rendering performance over canvas.
    // DOM renderer remains as fallback if WebGL init fails.
    webglAddon = createWebglAddon();

    // Update tab title from shell OSC 0/2 escape sequences (e.g. user@host:~/path)
    // OSC titles take priority over status-line parsing
    terminal.onTitleChange((title) => {
      const term = terminalsStore.get(props.id);
      // Skip OSC title updates when tab name was set by an intent title
      if (title && !term?.nameIsCustom && !(term?.agentIntent && settingsStore.state.intentTabTitle)) {
        const cleaned = cleanOscTitle(title);
        if (cleaned) {
          if (!originalName) {
            originalName = terminalsStore.get(props.id)?.name || null;
          }
          terminalsStore.update(props.id, { name: cleaned });
        } else if (originalName) {
          // Bare user@host means shell prompt returned — restore original tab name
          terminalsStore.update(props.id, { name: originalName });
        }
      }
    });

    // Track command blocks via OSC 133 shell integration (FinalTerm/iTerm2/VS Code protocol).
    // A=prompt start, B=command start, C=pre-execution, D;exitcode=command finished.
    terminal.parser.registerOscHandler(133, (data: string) => {
      const parts = data.split(";");
      const type = parts[0]; // "A", "B", "C", or "D"
      if (!type || !"ABCD".includes(type)) return true;
      const ec = type === "D" && parts.length > 1 ? parseInt(parts[1], 10) : undefined;
      const buf = terminal!.buffer.active;
      const line = buf.baseY + buf.cursorY;
      terminalsStore.handleOsc133(props.id, type, line, Number.isNaN(ec) ? undefined : ec);

      // Gutter exit code marker on block completion
      if (type === "D" && terminal) {
        const term = terminalsStore.get(props.id);
        const blocks = term?.commandBlocks;
        const lastBlock = blocks?.[blocks.length - 1];
        if (lastBlock) {
          const offset = lastBlock.promptLine - buf.baseY - buf.cursorY;
          const marker = terminal.registerMarker(offset);
          if (marker) {
            const ok = lastBlock.exitCode === 0;
            const deco = terminal.registerDecoration({ marker, anchor: "left", x: 0, width: 1, height: 1 });
            deco?.onRender((el) => {
              el.classList.add(s.osc133Gutter, ok ? s.osc133GutterOk : s.osc133GutterErr);
            });
          }
        }
      }
      return true;
    });

    // Track working directory changes via OSC 7 (file://hostname/path).
    // Updates the terminal's cwd in the store and persists to Rust for restart recovery.
    terminal.parser.registerOscHandler(7, (data: string) => {
      const cwd = parseOsc7Url(data);
      if (!cwd) return true;
      terminalsStore.update(props.id, { cwd });
      if (sessionId) {
        invoke("update_session_cwd", { sessionId, cwd }).catch((err) =>
          appLogger.debug("terminal", "Failed to persist cwd to Rust session", err),
        );
      }
      props.onCwdChange?.(props.id, cwd);
      return true;
    });

    // Replay any PTY output buffered while terminal was not yet open
    replayBuffer();

    terminal.onScroll(() => {
      scrollTracker.onScroll(terminal!.buffer.active);
      viewportLock.update(scrollTracker.isAtBottom);
    });

    // xterm's scrollOnUserInput snaps the viewport to bottom on any user
    // keystroke. When ViewportLock is engaged and a write is in progress,
    // the usual disengage path is blocked — flag this as user intent so
    // the next update() can unlock through the guard.
    terminal.onKey(() => {
      if (!scrollTracker.isAtBottom) {
        viewportLock.userScrollIntent();
      }
    });


    resizeObserver = new ResizeObserver(() => {
      // Debounce: panels opening/closing can cause multiple rapid layout changes
      // (container oscillates between full-width and panel-reduced width).
      // Wait for layout to settle before fitting + resizing PTY.
      clearTimeout(resizeObserverTimer);
      resizeObserverTimer = setTimeout(() => {
        requestAnimationFrame(() => {
          if (!containerRef || containerRef.offsetWidth <= 0 || containerRef.offsetHeight <= 0) return;
          doFit();

          // First ResizeObserver event with valid dimensions: create PTY now
          // with authoritative rows/cols (layout has settled).
          if (deferInitToResize) {
            deferInitToResize = false;
            pendingOnReady = null; // cancel any safeFit-deferred callback
            initSession();
          } else if (pendingOnReady) {
            // safeFit exhausted retries and deferred to us
            const cb = pendingOnReady;
            pendingOnReady = null;
            cb();
          }

          // Cancel any pending stale onResize debounce — we're about to send
          // the authoritative dimensions ourselves.
          clearTimeout(resizeTimer);
          if (sessionId && terminal && terminal.rows > 0 && terminal.cols > 0) {

            pty.resize(sessionId, terminal.rows, terminal.cols).catch((err) => {
              appLogger.error("terminal", "ResizeObserver resize failed", err);
            });
          }
        });
      }, 100);
    });

    terminal.onData(async (data) => {
      if (sessionId) {
        // Focus report sequences (CSI I / CSI O) from DECSET 1004 fire on
        // tab/window focus changes — not user input. Still forward to PTY
        // (CC needs them) but don't clear awaitingInput.
        const isFocusReport = data === "\x1b[I" || data === "\x1b[O";
        if (!isFocusReport && terminalsStore.get(props.id)?.awaitingInput) {
          terminalsStore.clearAwaitingInput(props.id);
        }
        try {
          await pty.write(sessionId, data);
        } catch (err) {
          appLogger.error("terminal", "Failed to write to PTY", err);
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
              appLogger.error("terminal", "Failed to resize PTY", err);
            }
          }
        }, 150);
      }
    });

    terminal.textarea?.addEventListener("focus", () => {
      props.onFocus?.(props.id);
    });

    // Image paste support: intercept paste events in the capture phase (before
    // xterm's handler) to check for image content.  When the clipboard contains
    // an image, we send \x16 (Ctrl+V control code) to the PTY so CLI apps like
    // Claude Code can read the image from the OS clipboard directly.  Without
    // this, xterm's built-in paste handler calls getData("text/plain") and
    // silently discards image data.
    // When no image is present, the event propagates normally and xterm handles
    // text paste as usual.
    const handleImagePaste = (e: ClipboardEvent) => {
      if (!e.clipboardData) return;
      const items = e.clipboardData.items;
      for (let i = 0; i < items.length; i++) {
        if (items[i].type.startsWith("image/")) {
          e.stopPropagation();
          e.preventDefault();
          terminal!.input("\x16", true);
          return;
        }
      }
      // No image — let xterm handle text paste normally
    };
    containerRef.addEventListener("paste", handleImagePaste, true);
  };

  // Deferred onReady callback: when safeFit exhausts retries, the ResizeObserver
  // picks up the fit + onReady when the container finally gets real dimensions.
  let pendingOnReady: (() => void) | null = null;

  // Whether initSession should wait for the first ResizeObserver event.
  // On first mount, safeFit may succeed with preliminary dimensions (container
  // has *some* size but flex layout hasn't settled). The ResizeObserver debounce
  // fires ~100ms later with authoritative dimensions. Deferring PTY creation to
  // that event ensures Ink sees the correct terminal size on its first render.
  let deferInitToResize = false;

  /** Fit terminal only when container has valid dimensions, retrying if needed.
   *  If all retries are exhausted, defer to the ResizeObserver rather than
   *  proceeding with zero dimensions. */
  const safeFit = (onReady?: () => void, retries = 10) => {
    const tryFit = (remaining: number) => {
      requestAnimationFrame(() => {
        if (containerRef && containerRef.offsetWidth > 0 && containerRef.offsetHeight > 0) {
          doFit();
          onReady?.();
        } else if (remaining > 0) {
          tryFit(remaining - 1);
        } else {
          appLogger.debug("terminal", "Container has zero dimensions after retries — deferring to ResizeObserver");
          pendingOnReady = onReady ?? null;
        }
      });
    };
    tryFit(retries);
  };

  // Check if this terminal is visible. A terminal in a pane-tree group is only
  // visible when it's the active tab of its group — otherwise every tab in the
  // group runs xterm/ResizeObserver/ScrollTracker/PTY-subscribe concurrently,
  // which multiplies background work by the number of tabs per pane.
  const isActiveInPaneGroup = () => {
    if (!paneLayoutStore.isSplit()) return false;
    const groupId = paneLayoutStore.getGroupForTab(props.id);
    if (!groupId) return false;
    return paneLayoutStore.state.groups[groupId]?.activeTabId === props.id;
  };
  const isVisible = () =>
    props.alwaysVisible ||
    terminalsStore.state.activeId === props.id ||
    isActiveInPaneGroup();

  // Track hidden→visible transitions to rebuild the WebGL glyph atlas only once
  // after the terminal was actually hidden (branch/tab switch), not on every
  // isVisible() re-evaluation.
  let wasHidden = false;
  let hiddenSince = 0; // timestamp when terminal was last hidden

  // When this terminal becomes visible: open xterm, fit, and init PTY session
  createEffect(() => {
    if (isVisible()) {
      // Defer terminal.open() to the next animation frame so the browser has
      // completed the display:none → block reflow. Without this, xterm and its
      // WebGL renderer can capture stale container dimensions when a side panel
      // is already open (flex layout hasn't settled yet at the synchronous call site).
      rafHandle = requestAnimationFrame(() => {
        rafHandle = 0;
        const wasActuallyHidden = wasHidden;
        scrollTracker.setVisible(true);
        openTerminal();
        // Rebuild the WebGL addon on hidden→visible transition to recover
        // from GPU context corruption (layer re-composition, context drop
        // without an event fire). Only needed after long hides (sleep/resume,
        // display reconnect) — short hides (repo/tab switch, < 10s) don't
        // cause GPU context loss. Skipping on short hides eliminates N
        // expensive WebGL recreations during repo switch.
        if (wasActuallyHidden && terminal) {
          const hidDuration = performance.now() - hiddenSince;
          if (hidDuration > 10_000) {
            rebuildAtlas();
          }
          wasHidden = false;
        }
        // Only fit if the terminal was actually hidden or never fitted.
        // When the visibility effect re-triggers without a real hide (e.g.,
        // activeId flips null→id), skip the fit to avoid resetting scroll.
        if (!terminal) {
          // First mount — open xterm and fit, but defer PTY creation to the
          // first ResizeObserver event. safeFit may succeed with preliminary
          // dimensions before flex layout settles; the ResizeObserver debounce
          // fires with authoritative dimensions ~100ms later. Creating the PTY
          // there ensures Ink (Claude Code) sees correct rows/cols on first render.
          deferInitToResize = true;
          safeFit(() => {
            // If ResizeObserver already fired while safeFit was retrying,
            // deferInitToResize will be false — init was already called.
            if (!deferInitToResize) return;
            // safeFit succeeded but ResizeObserver hasn't fired yet.
            // Fall back after 200ms in case ResizeObserver never fires
            // (container already at final size, no resize event).
            setTimeout(() => {
              if (!deferInitToResize) return;
              deferInitToResize = false;
              pendingOnReady = null;
              initSession();
            }, 200);
          });
        } else if (wasActuallyHidden) {
          // Already mounted, was hidden → container reflow complete within
          // this RAF. Direct fit avoids the 10-retry RAF storm during repo
          // switch (N terminals × 10 retries saturates the rAF queue on
          // CPU-loaded machines). Fall back to safeFit only if container
          // dimensions aren't ready (edge case).
          if (containerRef && containerRef.offsetWidth > 0 && containerRef.offsetHeight > 0) {
            doFit();
            initSession();
          } else {
            safeFit(() => initSession());
          }
        } else {
          initSession();
        }

        // Resume PTY reader if it was paused — hidden terminals skip the pause
        // guard but a prior visible instance may have paused before hiding.
        if (isPaused && sessionId) {
          isPaused = false;
          pendingWriteBytes = 0; // Reset counter — stale value from hidden state
          pty.resume(sessionId).catch((err) => appLogger.warn("terminal", "PTY resume on show failed", { error: String(err) }));
        }

        // For reconnected terminals (existing sessionId), explicitly sync PTY dimensions.
        // When a terminal was in the background while a panel opened/close, terminal.onResize
        // may not fire (debounced 150ms + xterm may already report the fitted dimensions).
        // Force a resize to ensure PTY is in sync with the newly-fitted container.
        if (sessionId && terminal && terminal.rows > 0 && terminal.cols > 0) {

          pty.resize(sessionId, terminal.rows, terminal.cols).catch(() => {
            // Silently ignore resize errors (PTY may have exited)
          });
        }

        // Start observing resize while active (disconnect when inactive to avoid fit on display:none).
        // The ResizeObserver (100ms debounce) is the authoritative resize path — it handles
        // panel open/close, window resize, and tab-switch layout changes.
        if (resizeObserver && containerRef) {
          resizeObserver.observe(containerRef);
        }

        // Auto-focus: when this terminal just became visible because it was selected,
        // focus it now that the DOM is ready (the synchronous focus() in handleTerminalSelect
        // fires before this rAF, so it fails on a still-hidden element).
        if (terminalsStore.state.activeId === props.id) {
          terminal?.focus();
        }

        // The ResizeObserver (100ms debounce) is the authoritative resize path
        // and handles first-launch layout stabilization. No additional safety
        // refit is needed — it was adding N extra doFit+IPC calls per terminal
        // on every repo switch, compounding the visibility thundering herd.
      });

      onCleanup(() => {
        if (rafHandle) cancelAnimationFrame(rafHandle);
        resizeObserver?.disconnect();
        // Only mark hidden if the terminal is actually becoming invisible.
        // SolidJS re-runs the effect (and its cleanup) when reactive deps
        // change even if isVisible() stays true. Setting visible=false here
        // would corrupt the tracker's wasAtBottom state.
        if (!isVisible()) {
          scrollTracker.setVisible(false);
          wasHidden = true;
          hiddenSince = performance.now();
        }
      });
    }
  });

  // Handle font size changes (per-terminal zoom OR global default)
  createEffect(() => {
    const perTerminalSize = terminalsStore.state.terminals[props.id]?.fontSize;
    const defaultSize = settingsStore.state.defaultFontSize;
    if (!terminal) return;
    const size = perTerminalSize ?? defaultSize;
    terminal.options.fontSize = size;
    terminal.options.lineHeight = snapLineHeight(size);
    doFit();
  });

  // Handle font family + theme changes (global settings)
  // Preload via CSS Font Loading API before applying — canvas/WebGL renderers
  // cannot trigger @font-face loading on their own.
  createEffect(() => {
    const font = settingsStore.state.font;
    const weight = settingsStore.state.fontWeight;
    void settingsStore.state.theme;
    if (!terminal) return;
    terminal.options.theme = currentTheme();
    terminal.options.fontWeight = String(weight) as any;
    preloadFont(font).then(() => {
      terminal!.options.fontFamily = getFontFamily();
      doFit();
    });
  });

  // Cleanup on unmount - detach UI but keep PTY session alive
  onCleanup(() => {
    clearTimeout(resizeTimer);
    clearTimeout(resizeObserverTimer);
    clearTimeout(retryTimer);
    clearTimeout(agentDetectTimer);
    resizeObserver?.disconnect();
    unsubscribePty?.();
    unlistenParsed?.();
    unlistenKitty?.();
    kittyFlags = 0;

    // Resume reader if paused (don't leave PTY blocked after unmount)
    if (isPaused && sessionId) {
      isPaused = false;
      pty.resume(sessionId).catch((err) => appLogger.warn("terminal", "PTY resume failed (cleanup)", { error: String(err) }));
    }

    // NOTE: We intentionally do NOT close the PTY session here.
    // Sessions persist across branch/repo switches and reconnect on remount.
    // PTY sessions are only closed via explicit closeTerminal() action.

    // Clean up plugin line buffer for this session
    if (sessionId) pluginRegistry.removeSession(sessionId);

    for (const d of suggestDecorations) d.dispose();
    suggestDecorations = [];
    for (const d of intentDecorations) d.dispose();
    intentDecorations = [];
    viewportLock.dispose();
    terminal?.dispose();
  });

  // Public methods exposed via ref pattern
  const refMethods = {
    fit: () => doFit(),
    write: (data: string) => {
      // Write to PTY stdin (sends as user input to the shell)
      if (sessionId) {
        pty.write(sessionId, data).catch((err) => {
          appLogger.error("terminal", "Failed to write to PTY", err);
        });
      }
    },
    writeln: (data: string) => terminal?.writeln(data),
    input: (data: string) => terminal?.input(data, true),
    clear: () => terminal?.clear(),
    focus: () => terminal?.focus(),
    getSessionId: () => sessionId,
    openSearch: () => setSearchVisible(true),
    closeSearch: () => setSearchVisible(false),
    searchBuffer: (query: string) => {
      if (!terminal) return [];
      const buf = terminal.buffer.active;
      const lines: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        lines.push(buf.getLine(i)?.translateToString(true) ?? "");
      }
      const name = terminalsStore.get(props.id)?.name ?? props.id;
      return searchTerminalBuffer(lines, query, props.id, name);
    },
    scrollToLine: (lineIndex: number) => {
      if (!terminal) return;
      const viewportY = terminal.buffer.active.viewportY;
      const delta = lineIndex - viewportY - Math.floor(terminal.rows / 2);
      terminal.scrollLines(delta);
    },
    scrollToTop: () => terminal?.scrollToTop(),
    scrollToBottom: () => terminal?.scrollToBottom(),
    scrollPages: (pages: number) => terminal?.scrollPages(pages),
    getBufferLines: (startLine: number, endLine: number) => {
      if (!terminal) return [];
      const buf = terminal.buffer.active;
      const lines: string[] = [];
      for (let i = startLine; i < endLine; i++) {
        const line = buf.getLine(i);
        lines.push(line ? line.translateToString(true) : "");
      }
      return lines;
    },
  };

  onMount(() => {
    terminalsStore.update(props.id, { ref: refMethods });
  });

  const handleResume = () => {
    const cmd = terminalsStore.get(props.id)?.pendingResumeCommand;
    if (cmd && sessionId) {
      terminalsStore.update(props.id, { pendingResumeCommand: null });
      pty.write(sessionId, cmd + "\r").catch((e) => appLogger.error("terminal", "Failed to write resume command", { error: String(e) }));
    }
  };

  const handleDismissResume = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    terminalsStore.update(props.id, { pendingResumeCommand: null });
    terminal?.focus();
  };

  return (
    <div class={s.wrapper} data-terminal-id={props.id}>
      <TerminalSearch
        visible={searchVisible()}
        searchAddon={searchAddon()}
        onClose={() => {
          setSearchVisible(false);
          terminal?.focus();
        }}
      />
      <Show when={reconnecting()}>
        {(info) => (
          <div class={s.reconnectBanner}>
            Reconnecting ({info().attempt}/{info().max})...
          </div>
        )}
      </Show>
      <Show when={terminalsStore.get(props.id)?.pendingResumeCommand}>
        <div class={s.resumeBanner} onClick={handleResume}>
          <span>Agent session was active — click to resume</span>
          <button class={s.resumeDismiss} onClick={handleDismissResume} title="Dismiss">&times;</button>
        </div>
      </Show>
      <div
        ref={containerRef}
        class={s.content}
        style={{ width: "100%", height: "100%", opacity: fitted() ? 1 : 0 }}
      />
    </div>
  );
};

export default Terminal;
