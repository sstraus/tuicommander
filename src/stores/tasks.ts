import { createStore, reconcile } from "solid-js/store";
import type { AgentType } from "../agents";

/** Task status */
export type TaskStatus = "pending" | "running" | "completed" | "failed" | "cancelled";

/** Task data */
export interface TaskData {
  id: string;
  name: string;
  description?: string;
  status: TaskStatus;
  agentType: AgentType;
  sessionId: string | null;
  createdAt: number;
  startedAt: number | null;
  completedAt: number | null;
  exitCode: number | null;
  error: string | null;
}

/** Task state */
interface TasksState {
  tasks: Record<string, TaskData>;
  activeTaskId: string | null;
  taskQueue: string[];
  nextId: number;
}

/** Task completion callback */
export type TaskCompletionCallback = (task: TaskData) => void;

/** Create tasks store */
function createTasksStore() {
  const [state, setState] = createStore<TasksState>({
    tasks: {},
    activeTaskId: null,
    taskQueue: [],
    nextId: 1,
  });

  const completionCallbacks: TaskCompletionCallback[] = [];

  const actions = {
    /** Create a new task */
    create(data: {
      name: string;
      description?: string;
      agentType: AgentType;
    }): string {
      const id = `task-${state.nextId}`;

      setState("tasks", id, {
        id,
        name: data.name,
        description: data.description,
        status: "pending",
        agentType: data.agentType,
        sessionId: null,
        createdAt: Date.now(),
        startedAt: null,
        completedAt: null,
        exitCode: null,
        error: null,
      });

      setState("nextId", state.nextId + 1);
      setState("taskQueue", [...state.taskQueue, id]);

      return id;
    },

    /** Start a task */
    start(taskId: string, sessionId: string): void {
      if (!state.tasks[taskId]) return;

      setState("tasks", taskId, {
        status: "running",
        sessionId,
        startedAt: Date.now(),
      });

      setState("activeTaskId", taskId);

      // Remove from queue
      setState(
        "taskQueue",
        state.taskQueue.filter((id) => id !== taskId)
      );
    },

    /** Mark task as completed */
    complete(taskId: string, exitCode: number = 0): void {
      const task = state.tasks[taskId];
      if (!task) return;

      const status: TaskStatus = exitCode === 0 ? "completed" : "failed";

      setState("tasks", taskId, {
        status,
        completedAt: Date.now(),
        exitCode,
      });

      // Clear active if this was active
      if (state.activeTaskId === taskId) {
        setState("activeTaskId", null);
      }

      // Notify callbacks
      const updatedTask = state.tasks[taskId];
      completionCallbacks.forEach((cb) => cb(updatedTask));
    },

    /** Mark task as failed with error */
    fail(taskId: string, error: string): void {
      const task = state.tasks[taskId];
      if (!task) return;

      setState("tasks", taskId, {
        status: "failed",
        completedAt: Date.now(),
        error,
      });

      if (state.activeTaskId === taskId) {
        setState("activeTaskId", null);
      }

      const updatedTask = state.tasks[taskId];
      completionCallbacks.forEach((cb) => cb(updatedTask));
    },

    /** Cancel a task */
    cancel(taskId: string): void {
      const task = state.tasks[taskId];
      if (!task) return;

      setState("tasks", taskId, {
        status: "cancelled",
        completedAt: Date.now(),
      });

      // Remove from queue if pending
      setState(
        "taskQueue",
        state.taskQueue.filter((id) => id !== taskId)
      );

      if (state.activeTaskId === taskId) {
        setState("activeTaskId", null);
      }

      const updatedTask = state.tasks[taskId];
      completionCallbacks.forEach((cb) => cb(updatedTask));
    },

    /** Get a task by ID */
    get(taskId: string): TaskData | undefined {
      return state.tasks[taskId];
    },

    /** Get active task */
    getActive(): TaskData | undefined {
      return state.activeTaskId ? state.tasks[state.activeTaskId] : undefined;
    },

    /** Get next task in queue */
    getNextInQueue(): TaskData | undefined {
      const nextId = state.taskQueue[0];
      return nextId ? state.tasks[nextId] : undefined;
    },

    /** Get all tasks */
    getAll(): TaskData[] {
      return Object.values(state.tasks);
    },

    /** Get tasks by status */
    getByStatus(status: TaskStatus): TaskData[] {
      return Object.values(state.tasks).filter((t) => t.status === status);
    },

    /** Get task by session ID */
    getBySessionId(sessionId: string): TaskData | undefined {
      return Object.values(state.tasks).find((t) => t.sessionId === sessionId);
    },

    /** Remove a task */
    remove(taskId: string): void {
      const { [taskId]: _, ...rest } = state.tasks;
      setState("tasks", reconcile(rest));
      setState(
        "taskQueue",
        state.taskQueue.filter((id) => id !== taskId)
      );

      if (state.activeTaskId === taskId) {
        setState("activeTaskId", null);
      }
    },

    /** Clear completed tasks */
    clearCompleted(): void {
      const completedIds = Object.values(state.tasks)
        .filter((t) => t.status === "completed" || t.status === "cancelled" || t.status === "failed")
        .map((t) => t.id);

      completedIds.forEach((id) => actions.remove(id));
    },

    /** Register completion callback */
    onCompletion(callback: TaskCompletionCallback): () => void {
      completionCallbacks.push(callback);
      return () => {
        const index = completionCallbacks.indexOf(callback);
        if (index !== -1) {
          completionCallbacks.splice(index, 1);
        }
      };
    },

    /** Get queue length */
    getQueueLength(): number {
      return state.taskQueue.length;
    },

    /** Reorder task in queue */
    reorderQueue(taskId: string, newIndex: number): void {
      const queue = [...state.taskQueue];
      const currentIndex = queue.indexOf(taskId);
      if (currentIndex === -1) return;

      queue.splice(currentIndex, 1);
      queue.splice(newIndex, 0, taskId);
      setState("taskQueue", queue);
    },
  };

  return { state, ...actions };
}

export const tasksStore = createTasksStore();
