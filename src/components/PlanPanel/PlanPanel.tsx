import { Component, createMemo, For, Show } from "solid-js";
import { activityStore } from "../../stores/activityStore";
import { mdTabsStore } from "../../stores/mdTabs";
import { PanelResizeHandle } from "../ui/PanelResizeHandle";
import { cx } from "../../utils";
import p from "../shared/panel.module.css";
import s from "./PlanPanel.module.css";

export interface PlanPanelProps {
  visible: boolean;
  repoPath: string | null;
  onClose: () => void;
}

/** Normalize status strings to a CSS class key. */
function statusClass(status: string): string {
  const lower = status.toLowerCase();
  if (lower === "completed" || lower === "done") return s.statusCompleted;
  if (lower === "in progress" || lower === "in-progress" || lower === "active") return s.statusInProgress;
  return s.statusDraft;
}

export const PlanPanel: Component<PlanPanelProps> = (props) => {
  const planItems = createMemo(() => {
    if (!props.repoPath) return [];
    return activityStore.getForSection("plan", props.repoPath);
  });

  const handleClick = (title: string, contentUri: string) => {
    mdTabsStore.addVirtual(title, contentUri);
  };

  return (
    <div id="plan-panel" class={cx(s.panel, !props.visible && s.hidden)}>
      <PanelResizeHandle panelId="plan-panel" />
      <div class={p.header}>
        <div class={p.headerLeft}>
          <span class={p.title}>Plans</span>
          <Show when={planItems().length > 0}>
            <span class={p.fileCountBadge} data-testid="plan-count-badge">{planItems().length}</span>
          </Show>
        </div>
        <button class={p.close} data-testid="plan-panel-close" onClick={props.onClose}>
          &times;
        </button>
      </div>

      <div class={p.content}>
        <Show when={!props.repoPath}>
          <div class={s.empty}>No repository selected</div>
        </Show>

        <Show when={props.repoPath && planItems().length === 0}>
          <div class={s.empty}>No plans found</div>
        </Show>

        <Show when={planItems().length > 0}>
          <For each={planItems()}>
            {(item) => (
              <div
                class={s.planItem}
                data-testid="plan-item"
                onClick={() => item.contentUri && handleClick(item.title, item.contentUri)}
              >
                <div class={s.planIcon} innerHTML={item.icon} />
                <div class={s.planInfo}>
                  <div class={s.planTitle}>{item.title}</div>
                  <Show when={item.metadata}>
                    <div class={s.planMeta}>
                      <Show when={item.metadata?.status}>
                        <span
                          class={cx(s.badge, statusClass(item.metadata!.status!))}
                          data-testid="plan-status-badge"
                        >
                          {item.metadata!.status}
                        </span>
                      </Show>
                      <Show when={item.metadata?.effort}>
                        <span class={cx(s.badge, s.badgeEffort)} data-testid="plan-effort-badge">
                          {item.metadata!.effort}
                        </span>
                      </Show>
                      <Show when={item.metadata?.priority}>
                        <span class={cx(s.badge, s.badgePriority)} data-testid="plan-priority-badge">
                          {item.metadata!.priority}
                        </span>
                      </Show>
                      <Show when={item.metadata?.story}>
                        <span class={cx(s.badge, s.badgeStory)} data-testid="plan-story-badge">
                          #{item.metadata!.story}
                        </span>
                      </Show>
                    </div>
                  </Show>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
};

export default PlanPanel;
