import { Component, For, Show, createSignal } from "solid-js";
import { tasksStore, type TaskData, type TaskStatus } from "../../stores/tasks";
import { t } from "../../i18n";
import { cx } from "../../utils";
import s from "./TaskQueuePanel.module.css";

export interface TaskQueuePanelProps {
  visible: boolean;
  onClose: () => void;
  onTaskSelect?: (taskId: string) => void;
}

const STATUS_ICONS: Record<TaskStatus, string> = {
  pending: "○",
  running: "◉",
  completed: "✓",
  failed: "✗",
  cancelled: "⊘",
};

const STATUS_COLORS: Record<TaskStatus, string> = {
  pending: "var(--fg-muted)",
  running: "var(--accent)",
  completed: "var(--success)",
  failed: "var(--error)",
  cancelled: "var(--fg-muted)",
};

export const TaskQueuePanel: Component<TaskQueuePanelProps> = (props) => {
  const [draggedId, setDraggedId] = createSignal<string | null>(null);

  const tasks = () => tasksStore.getAll();
  const pendingTasks = () => tasks().filter((t) => t.status === "pending");
  const runningTasks = () => tasks().filter((t) => t.status === "running");
  const completedTasks = () =>
    tasks().filter((t) => t.status === "completed" || t.status === "failed" || t.status === "cancelled");

  const handleDragStart = (e: DragEvent, taskId: string) => {
    setDraggedId(taskId);
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
    }
  };

  const handleDragOver = (e: DragEvent, targetId: string) => {
    e.preventDefault();
    if (draggedId() && draggedId() !== targetId) {
      const queue = tasksStore.state.taskQueue;
      const targetIndex = queue.indexOf(targetId);
      if (targetIndex !== -1) {
        tasksStore.reorderQueue(draggedId()!, targetIndex);
      }
    }
  };

  const handleDragEnd = () => {
    setDraggedId(null);
  };

  return (
    <Show when={props.visible}>
      <div class={s.panel}>
        <div class={s.header}>
          <h3>{t("taskQueue.title", "Task Queue")}</h3>
          <div class={s.actions}>
            <button
              class={s.clearBtn}
              onClick={() => tasksStore.clearCompleted()}
              title={t("taskQueue.clearCompleted", "Clear completed")}
            >
              {t("taskQueue.clear", "Clear")}
            </button>
            <button class={s.closeBtn} onClick={props.onClose}>
              &times;
            </button>
          </div>
        </div>

        <div class={s.content}>
          {/* Running Tasks */}
          <Show when={runningTasks().length > 0}>
            <div class={s.section}>
              <div class={s.sectionTitle}>{t("taskQueue.running", "Running")}</div>
              <For each={runningTasks()}>
                {(task) => (
                  <TaskItem
                    task={task}
                    onSelect={() => props.onTaskSelect?.(task.id)}
                    onCancel={() => tasksStore.cancel(task.id)}
                  />
                )}
              </For>
            </div>
          </Show>

          {/* Pending Tasks */}
          <Show when={pendingTasks().length > 0}>
            <div class={s.section}>
              <div class={s.sectionTitle}>
                {t("taskQueue.pending", "Pending")} ({pendingTasks().length})
              </div>
              <For each={pendingTasks()}>
                {(task) => (
                  <div
                    class={cx(s.itemWrapper, draggedId() === task.id && s.dragging)}
                    draggable={true}
                    onDragStart={(e) => handleDragStart(e, task.id)}
                    onDragOver={(e) => handleDragOver(e, task.id)}
                    onDragEnd={handleDragEnd}
                  >
                    <TaskItem
                      task={task}
                      onSelect={() => props.onTaskSelect?.(task.id)}
                      onCancel={() => tasksStore.cancel(task.id)}
                      draggable
                    />
                  </div>
                )}
              </For>
            </div>
          </Show>

          {/* Completed Tasks */}
          <Show when={completedTasks().length > 0}>
            <div class={s.section}>
              <div class={s.sectionTitle}>
                {t("taskQueue.completed", "Completed")} ({completedTasks().length})
              </div>
              <For each={completedTasks().slice(0, 10)}>
                {(task) => (
                  <TaskItem
                    task={task}
                    onSelect={() => props.onTaskSelect?.(task.id)}
                    compact
                  />
                )}
              </For>
            </div>
          </Show>

          {/* Empty State */}
          <Show when={tasks().length === 0}>
            <div class={s.empty}>
              <p>{t("taskQueue.noTasks", "No tasks in queue")}</p>
              <p class={s.hint}>{t("taskQueue.hint", "Tasks will appear here when agents are running")}</p>
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
};

/** Individual task item */
interface TaskItemProps {
  task: TaskData;
  onSelect?: () => void;
  onCancel?: () => void;
  compact?: boolean;
  draggable?: boolean;
}

const TaskItem: Component<TaskItemProps> = (props) => {
  const statusIcon = () => STATUS_ICONS[props.task.status];
  const statusColor = () => STATUS_COLORS[props.task.status];

  const getDuration = () => {
    if (!props.task.startedAt) return null;
    const end = props.task.completedAt || Date.now();
    const duration = Math.round((end - props.task.startedAt) / 1000);
    if (duration < 60) return `${duration}s`;
    const mins = Math.floor(duration / 60);
    const secs = duration % 60;
    return `${mins}m ${secs}s`;
  };

  return (
    <div
      class={cx(s.item, s[props.task.status], props.compact && s.compact)}
      onClick={props.onSelect}
    >
      <span class={s.itemStatus} style={{ color: statusColor() }}>
        {statusIcon()}
      </span>

      <div class={s.itemContent}>
        <div class={s.itemName}>{props.task.name}</div>
        <Show when={!props.compact && props.task.description}>
          <div class={s.itemDescription}>{props.task.description}</div>
        </Show>
      </div>

      <div class={s.itemMeta}>
        <Show when={getDuration()}>
          <span class={s.itemDuration}>{getDuration()}</span>
        </Show>
        <Show when={props.draggable}>
          <span class={s.itemDrag}>⋮⋮</span>
        </Show>
        <Show when={props.onCancel && (props.task.status === "pending" || props.task.status === "running")}>
          <button
            class={s.itemCancel}
            onClick={(e) => {
              e.stopPropagation();
              props.onCancel?.();
            }}
            title={t("taskQueue.cancelTask", "Cancel task")}
          >
            ✕
          </button>
        </Show>
      </div>
    </div>
  );
};

export default TaskQueuePanel;
