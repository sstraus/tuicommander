import { Component, For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { activityDashboardStore } from "../../stores/activityDashboard";
import { terminalsStore } from "../../stores/terminals";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { rateLimitStore } from "../../stores/ratelimit";
import { formatRelativeTime } from "../../utils/time";
import { GlobeIcon } from "../GlobeIcon";
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
  return { label: "\u2014", className: s.statusIdle };
}

/** Extract project name (last path segment) from cwd */
function projectName(cwd: string | null): string | null {
  if (!cwd) return null;
  const segments = cwd.replace(/\/+$/, "").split("/");
  return segments[segments.length - 1] || null;
}

/** Truncate a string to a single line for display */
function truncate(text: string, maxLen = 80): string {
  const oneLine = text.replace(/\n/g, " ").trim();
  if (oneLine.length <= maxLen) return oneLine;
  return oneLine.slice(0, maxLen - 1) + "\u2026";
}

/** Speech bubble icon (last prompt) */
const PromptIcon: Component = () => (
  <svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
    <path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h11A1.5 1.5 0 0 1 15 3.5v7A1.5 1.5 0 0 1 13.5 12H9.373l-2.62 1.81A.75.75 0 0 1 5.6 13.2V12H2.5A1.5 1.5 0 0 1 1 10.5v-7Zm1.5-.5a.5.5 0 0 0-.5.5v7a.5.5 0 0 0 .5.5H6.35a.75.75 0 0 1 .75.75v.83l1.81-1.25a.75.75 0 0 1 .427-.133H13.5a.5.5 0 0 0 .5-.5v-7a.5.5 0 0 0-.5-.5h-11Z"/>
  </svg>
);

/** Crosshair icon (agent intent) */
const IntentIcon: Component = () => (
  <svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
    <path d="M8 1a.75.75 0 0 1 .75.75v1.82a4.505 4.505 0 0 1 3.68 3.68h1.82a.75.75 0 0 1 0 1.5h-1.82a4.505 4.505 0 0 1-3.68 3.68v1.82a.75.75 0 0 1-1.5 0v-1.82a4.505 4.505 0 0 1-3.68-3.68H1.75a.75.75 0 0 1 0-1.5h1.82A4.505 4.505 0 0 1 7.25 3.57V1.75A.75.75 0 0 1 8 1ZM5.5 8a2.5 2.5 0 1 0 5 0 2.5 2.5 0 0 0-5 0Z"/>
  </svg>
);

/** Gear/spinner icon (current task) */
const TaskIcon: Component = () => (
  <svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
    <path d="M7.068.727c.243-.97 1.62-.97 1.864 0l.3 1.2a.957.957 0 0 0 1.18.633l1.18-.39c.93-.31 1.753.789 1.13 1.593l-.76.98a.957.957 0 0 0 .166 1.34l1.01.76c.78.59.39 1.82-.55 1.84l-1.22.03a.957.957 0 0 0-.905.905l-.03 1.22c-.02.94-1.25 1.33-1.84.55l-.76-1.01a.957.957 0 0 0-1.34-.166l-.98.76c-.804.623-1.903-.2-1.593-1.13l.39-1.18a.957.957 0 0 0-.633-1.18l-1.2-.3c-.97-.243-.97-1.62 0-1.864l1.2-.3a.957.957 0 0 0 .633-1.18l-.39-1.18c-.31-.93.789-1.753 1.593-1.13l.98.76a.957.957 0 0 0 1.34-.166l.76-1.01ZM8 11a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z"/>
  </svg>
);


/** People icon (active sub-tasks / agents) */
const SubTaskIcon: Component = () => (
  <svg class={s.subIcon} viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
    <path d="M2 5.5a3.5 3.5 0 1 1 5.898 2.549 5.508 5.508 0 0 1 3.034 4.084.75.75 0 1 1-1.482.235 4.001 4.001 0 0 0-7.9 0 .75.75 0 0 1-1.482-.236A5.507 5.507 0 0 1 3.102 8.05 3.493 3.493 0 0 1 2 5.5ZM11 4a.75.75 0 1 0 0 1.5 2.5 2.5 0 0 1 2.45 2.993.75.75 0 1 0 1.472.29A4.001 4.001 0 0 0 11 4Zm-5.5.5a2 2 0 1 0 0 4 2 2 0 0 0 0-4Z"/>
  </svg>
);

interface ActivityDashboardProps {
  onSelect?: (id: string) => void;
}

export const ActivityDashboard: Component<ActivityDashboardProps> = (props) => {
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
    if (props.onSelect) {
      props.onSelect(termId);
    } else {
      terminalsStore.setActive(termId);
      requestAnimationFrame(() => terminalsStore.get(termId)?.ref?.focus());
    }
    activityDashboardStore.close();
  };

  type TerminalRow = {
    id: string;
    name: string;
    project: string | null;
    agent: string;
    status: { label: string; className: string };
    lastDataAt: number | null;
    lastPrompt: string | null;
    agentIntent: string | null;
    currentTask: string | null;
    activeSubTasks: number;
    isActive: boolean;
  };

  /** Build a fresh row from the live store. Called at render time so every
   *  store mutation (intent, status, last-prompt, …) flows through immediately
   *  instead of waiting for the 10s order snapshot. */
  const buildRow = (id: string): TerminalRow | null => {
    const term = terminalsStore.get(id);
    if (!term) return null;
    const status = getTerminalStatus(term.shellState, term.awaitingInput, term.sessionId);
    return {
      id,
      name: term.name,
      project: projectName(term.cwd),
      agent: term.agentType || "shell",
      status,
      lastDataAt: terminalsStore.getLastDataAt(id),
      lastPrompt: term.lastPrompt,
      agentIntent: term.agentIntent,
      // Claude Code spinner verbs are decorative garbage — suppress them
      currentTask: term.agentType === "claude" ? null : term.currentTask,
      activeSubTasks: term.activeSubTasks,
      isActive: terminalsStore.state.activeId === id,
    };
  };

  /** Order-only snapshot: list of ids sorted by lastDataAt. Row *contents*
   *  remain live — only the sort order is stabilised. */
  const liveOrder = createMemo(() => {
    const ids = terminalsStore.getAttachedIds();
    return ids
      .map((id) => ({ id, t: terminalsStore.getLastDataAt(id) ?? 0 }))
      .sort((a, b) => b.t - a.t)
      .map((x) => x.id);
  });

  // Snapshot sort order every 10s so rows don't reshuffle on every mutation.
  // New items/removals trigger an immediate snapshot via count change.
  const [orderSnapshot, setOrderSnapshot] = createSignal<string[]>(liveOrder());
  createEffect(() => {
    if (!isOpen()) return;
    const interval = setInterval(() => setOrderSnapshot(liveOrder()), 10_000);
    onCleanup(() => clearInterval(interval));
  });

  let prevCount = liveOrder().length;
  const orderedIds = createMemo(() => {
    const current = liveOrder();
    if (current.length !== prevCount) {
      prevCount = current.length;
      setOrderSnapshot(current);
    }
    return orderSnapshot();
  });

  const terminals = createMemo(() =>
    orderedIds().map(buildRow).filter(Boolean) as TerminalRow[],
  );

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
                    <div class={s.nameCell}>
                      <span class={s.termName}>{term.name}</span>
                      <Show when={term.project}>
                        <span class={s.project}>{term.project}</span>
                      </Show>
                    </div>
                    <span class={s.agent}>{term.agent}</span>
                    <span class={`${s.status} ${term.status.className}`}>{term.status.label}</span>
                    <span class={s.lastActivity}>{formatRelativeTime(term.lastDataAt)}</span>
                    {(() => {
                      const isGlobal = globalWorkspaceStore.isPromoted(term.id);
                      return (
                    <button
                      class={`${s.promoteBtn} ${isGlobal ? s.promoted : ""}`}
                      title={isGlobal ? "Remove from Global Workspace" : "Promote to Global Workspace"}
                      onClick={(e) => {
                        e.stopPropagation();
                        globalWorkspaceStore.togglePromote(term.id);
                      }}
                    >
                      <GlobeIcon />
                    </button>
                      );
                    })()}
                  </div>
                  <Show when={term.currentTask}>
                    {(task) => (
                      <div class={s.subRow} title={task()}>
                        <TaskIcon />
                        <span class={s.subText}>{truncate(task())}</span>
                      </div>
                    )}
                  </Show>
                  <Show when={term.activeSubTasks > 0}>
                    <div class={s.subRow} title={`${term.activeSubTasks} sub-tasks running`}>
                      <SubTaskIcon />
                      <span class={s.subText}>{term.activeSubTasks} sub-tasks running</span>
                    </div>
                  </Show>
                  <Show when={term.agentIntent} keyed>
                    {(intent) => (
                      <div class={s.subRow} title={intent}>
                        <IntentIcon />
                        <span class={s.subText}>{truncate(intent)}</span>
                      </div>
                    )}
                  </Show>
                  {(() => {
                    const prompt = term.lastPrompt;
                    if (!prompt || term.agentIntent) return null;
                    return (
                      <div class={s.subRow} title={prompt}>
                        <PromptIcon />
                        <span class={s.subText}>{truncate(prompt)}</span>
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
