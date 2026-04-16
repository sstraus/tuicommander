import { Component, For, Show, createSignal, createEffect, onCleanup } from "solid-js";
import { invoke, listen } from "../../invoke";
import { cx } from "../../utils";
import { appLogger } from "../../stores/appLogger";
import s from "./SessionKnowledgeBar.module.css";

interface OutcomeSummary {
  timestamp: number;
  command: string;
  exit_code: number | null;
  duration_ms: number;
  kind: string;
  error_type: string | null;
}

interface SessionKnowledgeSummary {
  session_id: string;
  commands_count: number;
  recent_outcomes: OutcomeSummary[];
  recent_errors: OutcomeSummary[];
  tui_mode: string | null;
  tui_apps_seen: string[];
}

const EMPTY: SessionKnowledgeSummary = {
  session_id: "",
  commands_count: 0,
  recent_outcomes: [],
  recent_errors: [],
  tui_mode: null,
  tui_apps_seen: [],
};

const REFRESH_DEBOUNCE_MS = 2000;

function kindBadgeClass(kind: string): string {
  switch (kind) {
    case "success":
      return s.badgeOk;
    case "error":
      return s.badgeErr;
    case "inferred":
      return s.badgeInferred;
    case "tui_launched":
      return s.badgeTui;
    case "timeout":
      return s.badgeErr;
    case "user_cancelled":
      return s.badgeInferred;
    default:
      return s.badgeInferred;
  }
}

function truncate(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

export const SessionKnowledgeBar: Component<{ sessionId: string | null }> = (props) => {
  const [summary, setSummary] = createSignal<SessionKnowledgeSummary>(EMPTY);
  const [expanded, setExpanded] = createSignal(false);
  let refreshTimer: ReturnType<typeof setTimeout> | null = null;

  const fetchSummary = async () => {
    const sid = props.sessionId;
    if (!sid) {
      setSummary(EMPTY);
      return;
    }
    try {
      const data = await invoke<SessionKnowledgeSummary>("get_session_knowledge", {
        sessionId: sid,
      });
      setSummary(data);
    } catch (e) {
      appLogger.warn("ai-agent", `get_session_knowledge failed: ${String(e)}`);
    }
  };

  const scheduleRefresh = () => {
    if (refreshTimer) return;
    refreshTimer = setTimeout(() => {
      refreshTimer = null;
      void fetchSummary();
    }, REFRESH_DEBOUNCE_MS);
  };

  createEffect(() => {
    const sid = props.sessionId;
    let active = true;
    void fetchSummary();
    if (!sid) return;

    const safeSchedule = () => { if (active) scheduleRefresh(); };

    const unlistenPromise = listen(`pty-parsed-${sid}`, () => {
      safeSchedule();
    });

    onCleanup(() => {
      active = false;
      if (refreshTimer) {
        clearTimeout(refreshTimer);
        refreshTimer = null;
      }
      void unlistenPromise.then((u) => u());
    });
  });

  return (
    <div class={cx(s.bar, !props.sessionId && s.hidden)}>
      <button
        type="button"
        class={s.summaryRow}
        onClick={() => setExpanded(!expanded())}
        title="Session knowledge"
      >
        <span class={s.caret}>{expanded() ? "▾" : "▸"}</span>
        <span class={s.label}>knowledge</span>
        <span class={s.count}>{summary().commands_count} cmds</span>
        <Show when={summary().recent_errors.length > 0}>
          <span class={cx(s.count, s.errCount)}>
            {summary().recent_errors.length} recent err
          </span>
        </Show>
        <Show when={summary().tui_mode}>
          <span class={cx(s.count, s.tuiMode)}>tui: {summary().tui_mode}</span>
        </Show>
      </button>

      <Show when={expanded() && summary().commands_count > 0}>
        <div class={s.details}>
          <Show when={summary().recent_outcomes.length > 0}>
            <div class={s.section}>
              <div class={s.sectionLabel}>Recent</div>
              <For each={summary().recent_outcomes}>
                {(o) => (
                  <div class={s.row}>
                    <span class={cx(s.badge, kindBadgeClass(o.kind))}>{o.kind}</span>
                    <span class={s.cmd} title={o.command}>
                      {truncate(o.command || "(inferred)", 48)}
                    </span>
                    <Show when={o.exit_code !== null}>
                      <span class={s.exit}>[{o.exit_code}]</span>
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </Show>

          <Show when={summary().recent_errors.length > 0}>
            <div class={s.section}>
              <div class={s.sectionLabel}>Errors</div>
              <For each={summary().recent_errors}>
                {(e) => (
                  <div class={s.row}>
                    <span class={cx(s.badge, s.badgeErr)}>{e.error_type ?? "error"}</span>
                    <span class={s.cmd} title={e.command}>
                      {truncate(e.command || "(inferred)", 48)}
                    </span>
                  </div>
                )}
              </For>
            </div>
          </Show>

          <Show when={summary().tui_apps_seen.length > 0}>
            <div class={s.section}>
              <div class={s.sectionLabel}>TUI apps</div>
              <div class={s.row}>
                <span class={s.cmd}>{summary().tui_apps_seen.join(", ")}</span>
              </div>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
};

export default SessionKnowledgeBar;
