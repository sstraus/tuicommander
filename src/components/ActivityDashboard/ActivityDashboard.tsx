import { Component, For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { activityDashboardStore } from "../../stores/activityDashboard";
import { terminalsStore } from "../../stores/terminals";
import { rateLimitStore } from "../../stores/ratelimit";
import { formatRelativeTime } from "../../utils/time";
import s from "./ActivityDashboard.module.css";

/** Derive status label and CSS class from terminal state */
function getTerminalStatus(
  shellState: string | null,
  awaitingInput: string | null,
  sessionId: string | null,
): { label: string; className: string } {
  if (sessionId && rateLimitStore.isRateLimited(sessionId)) {
    return { label: "Rate limited", className: s.statusRateLimited };
  } else if (awaitingInput) {
    return { label: "Waiting for input", className: s.statusWaiting };
  } else if (shellState === "busy") {
    return { label: "Working", className: s.statusWorking };
  } else if (shellState === "idle") {
    return { label: "Idle", className: s.statusIdle };
  }
  return { label: "—", className: s.statusIdle };
}

/** Extract project name (last path segment) from cwd */
function projectName(cwd: string | null): string | null {
  if (!cwd) return null;
  const segments = cwd.replace(/\/+$/, "").split("/");
  return segments[segments.length - 1] || null;
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

  const terminals = createMemo(() => {
    const ids = terminalsStore.getAttachedIds();
    return ids.map((id) => {
      const term = terminalsStore.get(id);
      if (!term) return null;
      const status = getTerminalStatus(term.shellState, term.awaitingInput, term.sessionId);
      return {
        id,
        name: term.name,
        project: projectName(term.cwd),
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
      project: string | null;
      agent: string;
      status: { label: string; className: string };
      lastDataAt: number | null;
      lastPrompt: string | null;
      agentIntent: string | null;
      isActive: boolean;
    }>;
  });

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
                    <Show when={term.project}>
                      <span class={s.project}>{term.project}</span>
                    </Show>
                    <span class={s.agent}>{term.agent}</span>
                    <span class={`${s.status} ${term.status.className}`}>{term.status.label}</span>
                    <span class={s.lastActivity}>{formatRelativeTime(term.lastDataAt)}</span>
                  </div>
                  <Show when={term.agentIntent} keyed>
                    {(intent) => (
                      <div class={s.promptRow} title={intent}>
                        {/* Target/crosshair icon for agent-declared intent */}
                        <svg class={s.promptIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
                          <path d="M8 1a.75.75 0 0 1 .75.75v1.82a4.505 4.505 0 0 1 3.68 3.68h1.82a.75.75 0 0 1 0 1.5h-1.82a4.505 4.505 0 0 1-3.68 3.68v1.82a.75.75 0 0 1-1.5 0v-1.82a4.505 4.505 0 0 1-3.68-3.68H1.75a.75.75 0 0 1 0-1.5h1.82A4.505 4.505 0 0 1 7.25 3.57V1.75A.75.75 0 0 1 8 1ZM5.5 8a2.5 2.5 0 1 0 5 0 2.5 2.5 0 0 0-5 0Z"/>
                        </svg>
                        <span class={s.promptText}>{truncatePrompt(intent)}</span>
                      </div>
                    )}
                  </Show>
                  {(() => {
                    const prompt = term.lastPrompt;
                    if (!prompt || term.agentIntent) return null;
                    return (
                      <div class={s.promptRow} title={prompt}>
                        <svg class={s.promptIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
                          <path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h11A1.5 1.5 0 0 1 15 3.5v7A1.5 1.5 0 0 1 13.5 12H9.373l-2.62 1.81A.75.75 0 0 1 5.6 13.2V12H2.5A1.5 1.5 0 0 1 1 10.5v-7Zm1.5-.5a.5.5 0 0 0-.5.5v7a.5.5 0 0 0 .5.5H6.35a.75.75 0 0 1 .75.75v.83l1.81-1.25a.75.75 0 0 1 .427-.133H13.5a.5.5 0 0 0 .5-.5v-7a.5.5 0 0 0-.5-.5h-11Z"/>
                        </svg>
                        <span class={s.promptText}>{truncatePrompt(prompt)}</span>
                      </div>
                    );
                  })()}
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
