import {
  Component,
  For,
  Show,
  createEffect,
  createMemo,
  createResource,
  createSignal,
  onCleanup,
} from "solid-js";
import { invoke } from "../../invoke";
import { uiStore } from "../../stores/ui";
import { appLogger } from "../../stores/appLogger";
import { cx } from "../../utils";
import s from "./KnowledgeHistoryOverlay.module.css";

interface SessionListEntry {
  session_id: string;
  last_activity: number;
  commands_count: number;
  errors_count: number;
  last_cwd: string | null;
  tui_apps_seen: string[];
}

interface HistoryCommand {
  id: number;
  timestamp: number;
  command: string;
  cwd: string;
  exit_code: number | null;
  output_snippet: string;
  duration_ms: number;
  kind: string;
  error_type: string | null;
  semantic_intent: string | null;
}

interface SessionDetail {
  session_id: string;
  commands: HistoryCommand[];
  tui_apps_seen: string[];
  cwd_history: [string, number][];
}

function formatRelativeTime(ts: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = now - ts;
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  const days = Math.floor(diff / 86400);
  if (days < 30) return `${days}d ago`;
  return new Date(ts * 1000).toLocaleDateString();
}

function kindBadgeClass(kind: string): string {
  switch (kind) {
    case "success":
      return s.badgeOk;
    case "error":
    case "timeout":
      return s.badgeErr;
    case "tui_launched":
      return s.badgeTui;
    default:
      return s.badgeInferred;
  }
}

async function copyToClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch (e) {
    appLogger.warn("ai-agent", `clipboard write failed: ${String(e)}`);
  }
}

export const KnowledgeHistoryOverlay: Component = () => {
  const visible = () => uiStore.state.knowledgeHistoryOverlayVisible;
  const [searchRaw, setSearchRaw] = createSignal("");
  const [search, setSearch] = createSignal("");
  const [errorsOnly, setErrorsOnly] = createSignal(false);
  const [sinceDays, setSinceDays] = createSignal<number | null>(null);
  const [selected, setSelected] = createSignal<string | null>(null);

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  const onSearchInput = (e: InputEvent) => {
    const value = (e.currentTarget as HTMLInputElement).value;
    setSearchRaw(value);
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      setSearch(value);
    }, 250);
  };

  const close = () => {
    uiStore.setKnowledgeHistoryOverlayVisible(false);
  };

  createEffect(() => {
    if (!visible()) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
  });

  const filterKey = createMemo(() => ({
    visible: visible(),
    text: search().trim(),
    hasErrors: errorsOnly(),
    sinceDays: sinceDays(),
  }));

  const [sessions] = createResource(filterKey, async (key) => {
    if (!key.visible) return [] as SessionListEntry[];
    const filter: Record<string, unknown> = {};
    if (key.text) filter.text = key.text;
    if (key.hasErrors) filter.hasErrors = true;
    if (key.sinceDays !== null) {
      filter.since = Math.floor(Date.now() / 1000) - key.sinceDays * 86400;
    }
    try {
      return await invoke<SessionListEntry[]>("list_knowledge_sessions", {
        filter: Object.keys(filter).length ? filter : null,
        limit: 200,
      });
    } catch (e) {
      appLogger.warn("ai-agent", `list_knowledge_sessions failed: ${String(e)}`);
      return [] as SessionListEntry[];
    }
  });

  // Auto-select first row once the list loads and no selection is set.
  createEffect(() => {
    const rows = sessions();
    if (!rows || rows.length === 0) return;
    if (!selected() || !rows.some((r) => r.session_id === selected())) {
      setSelected(rows[0].session_id);
    }
  });

  const [detail] = createResource(selected, async (sid) => {
    if (!sid) return null;
    try {
      return await invoke<SessionDetail | null>("get_knowledge_session_detail", {
        sessionId: sid,
      });
    } catch (e) {
      appLogger.warn("ai-agent", `get_knowledge_session_detail failed: ${String(e)}`);
      return null;
    }
  });

  const commandsReversed = createMemo(() => {
    const d = detail();
    if (!d) return [] as HistoryCommand[];
    return [...d.commands].reverse();
  });

  return (
    <Show when={visible()}>
      <div
        class={s.backdrop}
        onClick={(e) => {
          if (e.target === e.currentTarget) close();
        }}
      >
        <div class={s.dialog} role="dialog" aria-label="Knowledge history">
          <div class={s.header}>
            <div class={s.title}>Knowledge</div>
            <div class={s.searchRow}>
              <input
                class={s.searchInput}
                type="search"
                placeholder="Search commands, output, errors…"
                value={searchRaw()}
                onInput={onSearchInput}
                autofocus
              />
              <label class={s.filterCheck}>
                <input
                  type="checkbox"
                  checked={errorsOnly()}
                  onChange={(e) => setErrorsOnly(e.currentTarget.checked)}
                />
                errors only
              </label>
              <select
                class={s.filterSelect}
                value={sinceDays() === null ? "all" : String(sinceDays())}
                onChange={(e) => {
                  const v = e.currentTarget.value;
                  setSinceDays(v === "all" ? null : Number(v));
                }}
              >
                <option value="1">24h</option>
                <option value="7">7d</option>
                <option value="30">30d</option>
                <option value="all">all</option>
              </select>
            </div>
            <button type="button" class={s.closeBtn} onClick={close} title="Close (Esc)">
              ×
            </button>
          </div>

          <div class={s.body}>
            <div class={s.sessionList}>
              <Show
                when={(sessions() ?? []).length > 0}
                fallback={
                  <div class={s.empty}>
                    {sessions.loading ? "Loading…" : "No sessions match."}
                  </div>
                }
              >
                <For each={sessions()}>
                  {(row) => (
                    <button
                      type="button"
                      class={cx(
                        s.sessionRow,
                        selected() === row.session_id && s.sessionRowActive,
                      )}
                      onClick={() => setSelected(row.session_id)}
                    >
                      <div class={s.sessionId} title={row.session_id}>
                        {row.session_id}
                      </div>
                      <div class={s.sessionMeta}>
                        <span>{formatRelativeTime(row.last_activity)}</span>
                        <span>{row.commands_count} cmds</span>
                        <Show when={row.errors_count > 0}>
                          <span class={s.sessionMetaErr}>{row.errors_count} err</span>
                        </Show>
                      </div>
                      <Show when={row.last_cwd}>
                        <div class={s.sessionMeta}>
                          <span title={row.last_cwd ?? ""}>{row.last_cwd}</span>
                        </div>
                      </Show>
                    </button>
                  )}
                </For>
              </Show>
            </div>

            <div class={s.detailPane}>
              <Show
                when={detail() && commandsReversed().length > 0}
                fallback={
                  <div class={s.empty}>
                    {!selected()
                      ? "Select a session to inspect."
                      : detail.loading
                      ? "Loading…"
                      : "No commands recorded in this session."}
                  </div>
                }
              >
                <For each={commandsReversed()}>
                  {(c) => (
                    <div class={s.cmdCard}>
                      <div class={s.cmdHeader}>
                        <span class={cx(s.badge, kindBadgeClass(c.kind))}>
                          {c.error_type ?? c.kind}
                        </span>
                        <span class={s.cmdText} title={c.command}>
                          {c.command || "(inferred)"}
                        </span>
                        <Show when={c.command}>
                          <button
                            type="button"
                            class={s.copyBtn}
                            onClick={() => void copyToClipboard(c.command)}
                            title="Copy command"
                          >
                            copy
                          </button>
                        </Show>
                      </div>
                      <Show when={c.semantic_intent}>
                        <div class={s.intent}>{c.semantic_intent}</div>
                      </Show>
                      <div class={s.meta}>
                        <span>{formatRelativeTime(c.timestamp)}</span>
                        <Show when={c.exit_code !== null}>
                          <span>exit {c.exit_code}</span>
                        </Show>
                        <Show when={c.duration_ms > 0}>
                          <span>{c.duration_ms}ms</span>
                        </Show>
                        <Show when={c.cwd}>
                          <span title={c.cwd}>{c.cwd}</span>
                        </Show>
                      </div>
                      <Show when={c.output_snippet}>
                        <pre class={s.snippet}>{c.output_snippet}</pre>
                      </Show>
                    </div>
                  )}
                </For>
              </Show>
            </div>
          </div>

          <div class={s.footer}>
            <span>
              {(sessions() ?? []).length} sessions
              {detail() ? ` · ${detail()!.commands.length} commands` : ""}
            </span>
            <span>Esc to close</span>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default KnowledgeHistoryOverlay;
