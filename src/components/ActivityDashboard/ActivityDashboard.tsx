import { Component, For, Show, createEffect, createSignal, onCleanup } from "solid-js";
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
  }
  if (awaitingInput) {
    return { label: "Waiting for input", className: s.statusWaiting };
  }
  if (shellState === "busy") {
    return { label: "Working", className: s.statusWorking };
  }
  if (shellState === "idle") {
    return { label: "Idle", className: s.statusIdle };
  }
  return { label: "—", className: s.statusIdle };
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
      const status = getTerminalStatus(term.shellState, term.awaitingInput, term.sessionId);
      return {
        id,
        name: term.name,
        agent: term.agentType || "shell",
        status,
        lastDataAt: term.lastDataAt,
        isActive: terminalsStore.state.activeId === id,
      };
    }).filter(Boolean) as Array<{
      id: string;
      name: string;
      agent: string;
      status: { label: string; className: string };
      lastDataAt: number | null;
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
                  <span class={s.termName}>{term.name}</span>
                  <span class={s.agent}>{term.agent}</span>
                  <span class={`${s.status} ${term.status.className}`}>{term.status.label}</span>
                  <span class={s.lastActivity}>{formatRelativeTime(term.lastDataAt)}</span>
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
