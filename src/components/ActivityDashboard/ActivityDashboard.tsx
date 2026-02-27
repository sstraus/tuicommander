import { Component, For, Show, createEffect, createSignal, onCleanup } from "solid-js";
import { activityDashboardStore } from "../../stores/activityDashboard";
import { terminalsStore } from "../../stores/terminals";
import { rateLimitStore } from "../../stores/ratelimit";
import { appLogger } from "../../stores/appLogger";
import { formatRelativeTime } from "../../utils/time";
import s from "./ActivityDashboard.module.css";

/** Derive status label and CSS class from terminal state */
function getTerminalStatus(
  termId: string,
  shellState: string | null,
  awaitingInput: string | null,
  sessionId: string | null,
): { label: string; className: string } {
  let result: { label: string; className: string };
  let reason: string;
  if (sessionId && rateLimitStore.isRateLimited(sessionId)) {
    result = { label: "Rate limited", className: s.statusRateLimited };
    reason = `rateLimitStore.isRateLimited(${sessionId})=true`;
  } else if (awaitingInput) {
    result = { label: "Waiting for input", className: s.statusWaiting };
    reason = `awaitingInput="${awaitingInput}"`;
  } else if (shellState === "busy") {
    result = { label: "Working", className: s.statusWorking };
    reason = `shellState="busy"`;
  } else if (shellState === "idle") {
    result = { label: "Idle", className: s.statusIdle };
    reason = `shellState="idle"`;
  } else {
    result = { label: "—", className: s.statusIdle };
    reason = `shellState=${shellState === null ? "null" : `"${shellState}"`} (fallthrough)`;
  }
  appLogger.debug("app", `[ActivityDash] ${termId} → "${result.label}" because ${reason}`);
  return result;
}

/** Truncate a prompt to a single line for display */
function truncatePrompt(prompt: string, maxLen = 80): string {
  const oneLine = prompt.replace(/\n/g, " ").trim();
  if (oneLine.length <= maxLen) return oneLine;
  return oneLine.slice(0, maxLen - 1) + "\u2026";
}

export const ActivityDashboard: Component = () => {
  const [, setTick] = createSignal(0);
  const isOpen = () => activityDashboardStore.state.isOpen;

  // Tick every second to refresh relative timestamps
  createEffect(() => {
    if (!isOpen()) return;
    const interval = setInterval(() => setTick((n) => n + 1), 1000);
    onCleanup(() => clearInterval(interval));
  });

  // Keyboard navigation
  createEffect(() => {
    if (!isOpen()) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        activityDashboardStore.close();
      }
    };

    document.addEventListener("keydown", handleKeydown, true);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown, true));
  });

  const handleRowClick = (termId: string) => {
    terminalsStore.setActive(termId);
    activityDashboardStore.close();
    // Focus the terminal after switching
    requestAnimationFrame(() => terminalsStore.get(termId)?.ref?.focus());
  };

  const terminals = () => {
    // Force re-read on tick for relative timestamps
    void setTick;
    const ids = terminalsStore.getAttachedIds();
    return ids.map((id) => {
      const term = terminalsStore.get(id);
      if (!term) return null;
      const status = getTerminalStatus(id, term.shellState, term.awaitingInput, term.sessionId);
      return {
        id,
        name: term.name,
        agent: term.agentType || "shell",
        status,
        lastDataAt: term.lastDataAt,
        lastPrompt: term.lastPrompt,
        agentIntent: term.agentIntent,
        isActive: terminalsStore.state.activeId === id,
      };
    }).filter(Boolean) as Array<{
      id: string;
      name: string;
      agent: string;
      status: { label: string; className: string };
      lastDataAt: number | null;
      lastPrompt: string | null;
      agentIntent: string | null;
      isActive: boolean;
    }>;
  };

  return (
    <Show when={isOpen()}>
      <div class={s.overlay} onClick={() => activityDashboardStore.close()}>
        <div class={s.dashboard} onClick={(e) => e.stopPropagation()}>
          <div class={s.header}>
            <h3>Activity Dashboard</h3>
            <button class={s.close} onClick={() => activityDashboardStore.close()}>
              &times;
            </button>
          </div>

          <div class={s.list}>
            <Show when={terminals().length === 0}>
              <div class={s.empty}>No active terminals</div>
            </Show>

            <For each={terminals()}>
              {(term) => (
                <div
                  class={`${s.row} ${term.isActive ? s.activeRow : ""}`}
                  onClick={() => handleRowClick(term.id)}
                >
                  <div class={s.rowMain}>
                    <span class={s.termName}>{term.name}</span>
                    <span class={s.agent}>{term.agent}</span>
                    <span class={`${s.status} ${term.status.className}`}>{term.status.label}</span>
                    <span class={s.lastActivity}>{formatRelativeTime(term.lastDataAt)}</span>
                  </div>
                  <Show when={term.agentIntent}>
                    <div class={s.promptRow} title={term.agentIntent!}>
                      {/* Target/crosshair icon for agent-declared intent */}
                      <svg class={s.promptIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
                        <path d="M8 1a.75.75 0 0 1 .75.75v1.82a4.505 4.505 0 0 1 3.68 3.68h1.82a.75.75 0 0 1 0 1.5h-1.82a4.505 4.505 0 0 1-3.68 3.68v1.82a.75.75 0 0 1-1.5 0v-1.82a4.505 4.505 0 0 1-3.68-3.68H1.75a.75.75 0 0 1 0-1.5h1.82A4.505 4.505 0 0 1 7.25 3.57V1.75A.75.75 0 0 1 8 1ZM5.5 8a2.5 2.5 0 1 0 5 0 2.5 2.5 0 0 0-5 0Z"/>
                      </svg>
                      <span class={s.promptText}>{truncatePrompt(term.agentIntent!)}</span>
                    </div>
                  </Show>
                  <Show when={term.lastPrompt && !term.agentIntent}>
                    <div class={s.promptRow} title={term.lastPrompt!}>
                      <svg class={s.promptIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
                        <path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h11A1.5 1.5 0 0 1 15 3.5v7A1.5 1.5 0 0 1 13.5 12H9.373l-2.62 1.81A.75.75 0 0 1 5.6 13.2V12H2.5A1.5 1.5 0 0 1 1 10.5v-7Zm1.5-.5a.5.5 0 0 0-.5.5v7a.5.5 0 0 0 .5.5H6.35a.75.75 0 0 1 .75.75v.83l1.81-1.25a.75.75 0 0 1 .427-.133H13.5a.5.5 0 0 0 .5-.5v-7a.5.5 0 0 0-.5-.5h-11Z"/>
                      </svg>
                      <span class={s.promptText}>{truncatePrompt(term.lastPrompt!)}</span>
                    </div>
                  </Show>
                </div>
              )}
            </For>
          </div>

          <div class={s.footer}>
            <span>{terminals().length} terminal(s)</span>
            <span style={{ "margin-left": "auto" }}>Click to switch • Esc to close</span>
          </div>
        </div>
      </div>
    </Show>
  );
};
