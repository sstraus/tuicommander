import { Component, For, Show, createSignal } from "solid-js";
import { tasksStore, type TaskData, type TaskStatus } from "../../stores/tasks";

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
      <div class="task-queue-panel">
        <div class="task-queue-header">
          <h3>Task Queue</h3>
          <div class="task-queue-actions">
            <button
              class="task-queue-clear"
              onClick={() => tasksStore.clearCompleted()}
              title="Clear completed"
            >
              Clear
            </button>
            <button class="task-queue-close" onClick={props.onClose}>
              &times;
            </button>
          </div>
        </div>

        <div class="task-queue-content">
          {/* Running Tasks */}
          <Show when={runningTasks().length > 0}>
            <div class="task-queue-section">
              <div class="task-queue-section-title">Running</div>
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
            <div class="task-queue-section">
              <div class="task-queue-section-title">
                Pending ({pendingTasks().length})
              </div>
              <For each={pendingTasks()}>
                {(task) => (
                  <div
                    class={`task-item-wrapper ${draggedId() === task.id ? "dragging" : ""}`}
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
            <div class="task-queue-section">
              <div class="task-queue-section-title">
                Completed ({completedTasks().length})
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
            <div class="task-queue-empty">
              <p>No tasks in queue</p>
              <p class="task-queue-hint">Tasks will appear here when agents are running</p>
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
      class={`task-item ${props.task.status} ${props.compact ? "compact" : ""}`}
      onClick={props.onSelect}
    >
      <span class="task-item-status" style={{ color: statusColor() }}>
        {statusIcon()}
      </span>

      <div class="task-item-content">
        <div class="task-item-name">{props.task.name}</div>
        <Show when={!props.compact && props.task.description}>
          <div class="task-item-description">{props.task.description}</div>
        </Show>
      </div>

      <div class="task-item-meta">
        <Show when={getDuration()}>
          <span class="task-item-duration">{getDuration()}</span>
        </Show>
        <Show when={props.draggable}>
          <span class="task-item-drag">⋮⋮</span>
        </Show>
        <Show when={props.onCancel && (props.task.status === "pending" || props.task.status === "running")}>
          <button
            class="task-item-cancel"
            onClick={(e) => {
              e.stopPropagation();
              props.onCancel?.();
            }}
            title="Cancel task"
          >
            ✕
          </button>
        </Show>
      </div>
    </div>
  );
};

export default TaskQueuePanel;
