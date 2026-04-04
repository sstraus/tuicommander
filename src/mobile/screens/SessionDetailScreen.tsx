import { Show, createSignal, createEffect, onCleanup } from "solid-js";
import { StatusBadge } from "../components/StatusBadge";
import { OutputView } from "../components/OutputView";
import { SuggestChips } from "../components/SuggestChips";
import { SlashMenuOverlay } from "../components/SlashMenuOverlay";
import { CommandWidget } from "../components/CommandWidget";
import { CommandInput } from "../components/CommandInput";
import { TerminalKeybar } from "../components/TerminalKeybar";
import { QuestionContext } from "../components/QuestionContext";
import type { SessionInfo } from "../useSessions";
import { deriveStatus } from "../utils/deriveStatus";
import { formatRetryCountdown } from "../utils/formatRetryCountdown";
import styles from "./SessionDetailScreen.module.css";

interface SessionDetailScreenProps {
  session: SessionInfo;
  sessionExists: boolean;
  onBack: () => void;
}

function projectName(cwd: string | null): string {
  if (!cwd) return "unknown";
  const parts = cwd.split("/");
  return parts[parts.length - 1] || "unknown";
}

export function SessionDetailScreen(props: SessionDetailScreenProps) {
  // Merge polled state with real-time WebSocket state pushes.
  // WS state arrives instantly; poll state arrives every 3s as fallback.
  const [wsState, setWsState] = createSignal<Record<string, unknown> | null>(null);
  const sessionState = () => {
    const ws = wsState();
    const poll = props.session.state;
    if (!ws) return poll;
    // WS state is authoritative when present; poll state fills gaps on reconnect.
    return { ...poll, ...ws } as typeof poll;
  };
  const status = () => deriveStatus({ ...props.session, state: sessionState() });

  // Search filter
  const [searchOpen, setSearchOpen] = createSignal(false);
  const [searchQuery, setSearchQuery] = createSignal("");

  // Command widget overlay toggle
  const [commandWidgetOpen, setCommandWidgetOpen] = createSignal(false);

  // Prefill value for CommandInput (set by slash menu selection).
  // Counter ensures the effect re-fires even when the same command is selected twice.
  const [inputPrefill, setInputPrefill] = createSignal<{ text: string; seq: number }>({ text: "", seq: 0 });
  let prefillSeq = 0;

  // PTY input line synced from WebSocket (what's on the terminal prompt)
  const [ptyInputLine, setPtyInputLine] = createSignal<string | null>(null);

  // Raw screen text for question context overlay
  const [screenText, setScreenText] = createSignal<string[]>([]);

  // Local dismiss flag for the slash menu overlay (resets when new items arrive)
  const [slashMenuDismissed, setSlashMenuDismissed] = createSignal(false);
  let lastSlashMenuItems: unknown = null;
  const showSlashMenu = () => {
    const items = sessionState()?.slash_menu_items;
    // Reset dismiss flag when items change
    if (items !== lastSlashMenuItems) {
      lastSlashMenuItems = items;
      setSlashMenuDismissed(false);
    }
    return !slashMenuDismissed() && items != null && items.length > 0;
  };

  // Live countdown for rate limit retry_after_ms
  const [retryRemaining, setRetryRemaining] = createSignal(0);

  createEffect(() => {
    const ms = sessionState()?.retry_after_ms;
    if (!ms || !sessionState()?.rate_limited) {
      setRetryRemaining(0);
      return;
    }
    setRetryRemaining(ms);
    const interval = setInterval(() => {
      setRetryRemaining((prev) => {
        const next = prev - 1000;
        if (next <= 0) {
          clearInterval(interval);
          return 0;
        }
        return next;
      });
    }, 1000);
    onCleanup(() => clearInterval(interval));
  });

  return (
    <div class={styles.screen}>
      <header class={styles.header}>
        <button class={styles.backBtn} onClick={props.onBack}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M15 18l-6-6 6-6" />
          </svg>
        </button>
        <div class={styles.headerInfo}>
          <span class={styles.agentName}>
            {sessionState()?.agent_type ?? "Terminal"}
          </span>
          <span class={styles.project}>{projectName(props.session.cwd)}</span>
        </div>
        <Show when={sessionState()?.usage_limit_pct != null}>
          <span
            class={styles.usageLabel}
            classList={{ [styles.danger]: (sessionState()!.usage_limit_pct ?? 0) > 80 }}
          >
            {sessionState()!.usage_limit_pct}%
          </span>
        </Show>
        <button
          class={styles.searchToggle}
          classList={{ [styles.searchToggleActive]: searchOpen() }}
          onClick={() => {
            if (searchOpen()) {
              setSearchOpen(false);
              setSearchQuery("");
            } else {
              setSearchOpen(true);
            }
          }}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
        </button>
        <StatusBadge status={status()} />
      </header>

      <Show when={searchOpen()}>
        <div class={styles.searchBar}>
          <input
            class={styles.searchInput}
            type="text"
            placeholder="Filter output..."
            value={searchQuery()}
            onInput={(e) => setSearchQuery(e.currentTarget.value)}
            autofocus
          />
          <Show when={searchQuery()}>
            <button class={styles.searchClear} onClick={() => setSearchQuery("")}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </Show>
        </div>
      </Show>

      <Show when={sessionState()?.agent_intent}>
        <div class={styles.intentLine}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10" />
            <line x1="22" y1="12" x2="18" y2="12" />
            <line x1="6" y1="12" x2="2" y2="12" />
            <line x1="12" y1="6" x2="12" y2="2" />
            <line x1="12" y1="22" x2="12" y2="18" />
          </svg>
          <span class={styles.subText}>{sessionState()!.agent_intent}</span>
        </div>
      </Show>

      <Show when={sessionState()?.current_task}>
        <div class={styles.taskLine}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
          <span class={styles.subText}>{sessionState()!.current_task}</span>
        </div>
      </Show>

      <Show when={(sessionState()?.active_sub_tasks ?? 0) > 0}>
        <div class={styles.taskLine}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
            <circle cx="9" cy="7" r="4" />
            <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
            <path d="M16 3.13a4 4 0 0 1 0 7.75" />
          </svg>
          <span class={styles.subText}>{sessionState()!.active_sub_tasks} sub-tasks running</span>
        </div>
      </Show>

      <Show when={sessionState()?.progress != null}>
        <div class={styles.headerProgressBar}>
          <div class={styles.headerProgressFill} style={{ width: `${sessionState()!.progress}%` }} />
        </div>
      </Show>

      <Show when={sessionState()?.question_text}>
        <QuestionContext
          questionText={sessionState()!.question_text!}
          screenText={screenText()}
        />
      </Show>

      <Show when={sessionState()?.last_error}>
        <div class={styles.errorBar}>
          {sessionState()!.last_error}
        </div>
      </Show>

      <Show when={sessionState()?.rate_limited}>
        <div class={styles.rateLimitBar}>
          <span>Rate limited</span>
          <Show when={retryRemaining() > 0}>
            <span class={styles.rateLimitCountdown}>
              {formatRetryCountdown(retryRemaining())}
            </span>
          </Show>
        </div>
      </Show>

      <div class={styles.outputArea}>
        <OutputView sessionId={props.session.session_id} onStateChange={setWsState} onInputLine={setPtyInputLine} onScreenText={setScreenText} searchQuery={searchQuery()} />
        <Show when={!props.sessionExists}>
          <div class={styles.endedOverlay}>
            <span class={styles.endedText}>Session ended</span>
            <button class={styles.endedBackBtn} onClick={props.onBack}>Back</button>
          </div>
        </Show>
      </div>
      <Show when={sessionState()?.suggested_actions?.length}>
        <SuggestChips sessionId={props.session.session_id} items={sessionState()!.suggested_actions!} agentType={sessionState()?.agent_type as string | null | undefined} />
      </Show>
      <TerminalKeybar
        sessionId={props.session.session_id}
        agentType={sessionState()?.agent_type as string | null | undefined}
        awaitingInput={sessionState()?.awaiting_input}
        questionConfident={sessionState()?.question_confident}
        onCommandWidgetOpen={() => setCommandWidgetOpen(true)}
      />
      <CommandInput sessionId={props.session.session_id} prefillValue={inputPrefill()} ptyInputLine={ptyInputLine()} agentType={sessionState()?.agent_type as string | null | undefined} />
      <Show when={showSlashMenu()}>
        <SlashMenuOverlay
          sessionId={props.session.session_id}
          items={sessionState()!.slash_menu_items!}
          onSelect={(cmd) => setInputPrefill({ text: cmd, seq: ++prefillSeq })}
          onDismiss={() => setSlashMenuDismissed(true)}
        />
      </Show>
      <Show when={commandWidgetOpen()}>
        <CommandWidget
          sessionId={props.session.session_id}
          agentType={sessionState()?.agent_type as string | null | undefined}
          onDismiss={() => setCommandWidgetOpen(false)}
        />
      </Show>
    </div>
  );
}
