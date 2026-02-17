import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

describe("tasksStore", () => {
  let store: typeof import("../../stores/tasks").tasksStore;

  beforeEach(async () => {
    vi.resetModules();
    localStorage.clear();
    store = (await import("../../stores/tasks")).tasksStore;
  });

  describe("create()", () => {
    it("creates a task with generated ID", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test task", agentType: "claude" });
        expect(id).toBe("task-1");
        const task = store.get(id);
        expect(task).toBeDefined();
        expect(task!.name).toBe("Test task");
        expect(task!.status).toBe("pending");
        expect(task!.agentType).toBe("claude");
        dispose();
      });
    });

    it("adds to task queue", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        expect(store.state.taskQueue).toContain(id);
        dispose();
      });
    });

    it("increments IDs", () => {
      createRoot((dispose) => {
        const id1 = store.create({ name: "T1", agentType: "claude" });
        const id2 = store.create({ name: "T2", agentType: "gemini" });
        expect(id1).toBe("task-1");
        expect(id2).toBe("task-2");
        dispose();
      });
    });
  });

  describe("start()", () => {
    it("marks task as running", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.get(id)!.status).toBe("running");
        expect(store.get(id)!.sessionId).toBe("sess-1");
        expect(store.get(id)!.startedAt).toBeDefined();
        dispose();
      });
    });

    it("sets as active task", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.state.activeTaskId).toBe(id);
        dispose();
      });
    });

    it("removes from queue", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.state.taskQueue).not.toContain(id);
        dispose();
      });
    });
  });

  describe("complete()", () => {
    it("marks task as completed with exit code 0", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.complete(id, 0);
        expect(store.get(id)!.status).toBe("completed");
        expect(store.get(id)!.exitCode).toBe(0);
        dispose();
      });
    });

    it("marks task as failed with non-zero exit code", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.complete(id, 1);
        expect(store.get(id)!.status).toBe("failed");
        dispose();
      });
    });

    it("clears active task", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.complete(id);
        expect(store.state.activeTaskId).toBeNull();
        dispose();
      });
    });

    it("calls completion callbacks", () => {
      createRoot((dispose) => {
        const callback = vi.fn();
        store.onCompletion(callback);
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.complete(id);
        expect(callback).toHaveBeenCalledTimes(1);
        dispose();
      });
    });
  });

  describe("fail()", () => {
    it("marks task as failed with error", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.fail(id, "Something broke");
        expect(store.get(id)!.status).toBe("failed");
        expect(store.get(id)!.error).toBe("Something broke");
        dispose();
      });
    });
  });

  describe("cancel()", () => {
    it("marks task as cancelled", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.cancel(id);
        expect(store.get(id)!.status).toBe("cancelled");
        dispose();
      });
    });

    it("removes from queue", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.cancel(id);
        expect(store.state.taskQueue).not.toContain(id);
        dispose();
      });
    });
  });

  describe("getActive()", () => {
    it("returns active task", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.getActive()?.id).toBe(id);
        dispose();
      });
    });
  });

  describe("getNextInQueue()", () => {
    it("returns first task in queue", () => {
      createRoot((dispose) => {
        const id1 = store.create({ name: "T1", agentType: "claude" });
        store.create({ name: "T2", agentType: "claude" });
        expect(store.getNextInQueue()?.id).toBe(id1);
        dispose();
      });
    });
  });

  describe("getAll()", () => {
    it("returns all tasks", () => {
      createRoot((dispose) => {
        store.create({ name: "T1", agentType: "claude" });
        store.create({ name: "T2", agentType: "gemini" });
        expect(store.getAll()).toHaveLength(2);
        dispose();
      });
    });
  });

  describe("getByStatus()", () => {
    it("filters by status", () => {
      createRoot((dispose) => {
        store.create({ name: "T1", agentType: "claude" });
        const id2 = store.create({ name: "T2", agentType: "claude" });
        store.start(id2, "sess-1");
        expect(store.getByStatus("pending")).toHaveLength(1);
        expect(store.getByStatus("running")).toHaveLength(1);
        dispose();
      });
    });
  });

  describe("getBySessionId()", () => {
    it("finds task by session ID", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.getBySessionId("sess-1")?.id).toBe(id);
        dispose();
      });
    });
  });

  describe("remove()", () => {
    it("removes a task", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.remove(id);
        expect(store.get(id)).toBeUndefined();
        dispose();
      });
    });
  });

  describe("clearCompleted()", () => {
    it("removes completed, cancelled, and failed tasks", () => {
      createRoot((dispose) => {
        const id1 = store.create({ name: "T1", agentType: "claude" });
        const id2 = store.create({ name: "T2", agentType: "claude" });
        const id3 = store.create({ name: "T3", agentType: "claude" });
        store.create({ name: "T4", agentType: "claude" }); // pending

        store.start(id1, "s1");
        store.complete(id1);
        store.cancel(id2);
        store.start(id3, "s3");
        store.fail(id3, "err");

        store.clearCompleted();
        expect(store.getAll()).toHaveLength(1);
        expect(store.getAll()[0].name).toBe("T4");
        dispose();
      });
    });
  });

  describe("onCompletion()", () => {
    it("returns unsubscribe function", () => {
      createRoot((dispose) => {
        const callback = vi.fn();
        const unsubscribe = store.onCompletion(callback);
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");

        unsubscribe();
        store.complete(id);
        expect(callback).not.toHaveBeenCalled();
        dispose();
      });
    });
  });

  describe("reorderQueue()", () => {
    it("reorders tasks in queue", () => {
      createRoot((dispose) => {
        const id1 = store.create({ name: "T1", agentType: "claude" });
        const id2 = store.create({ name: "T2", agentType: "claude" });
        const id3 = store.create({ name: "T3", agentType: "claude" });
        store.reorderQueue(id1, 2);
        expect(store.state.taskQueue).toEqual([id2, id3, id1]);
        dispose();
      });
    });

    it("does nothing for non-existent task", () => {
      createRoot((dispose) => {
        store.create({ name: "T1", agentType: "claude" });
        store.reorderQueue("non-existent", 0);
        expect(store.getQueueLength()).toBe(1);
        dispose();
      });
    });
  });

  describe("getQueueLength()", () => {
    it("returns queue length", () => {
      createRoot((dispose) => {
        store.create({ name: "T1", agentType: "claude" });
        store.create({ name: "T2", agentType: "claude" });
        expect(store.getQueueLength()).toBe(2);
        dispose();
      });
    });
  });

  describe("guard clauses", () => {
    it("start() ignores non-existent task", () => {
      createRoot((dispose) => {
        store.start("non-existent", "sess-1");
        expect(store.state.activeTaskId).toBeNull();
        dispose();
      });
    });

    it("complete() ignores non-existent task", () => {
      createRoot((dispose) => {
        store.complete("non-existent");
        expect(store.state.activeTaskId).toBeNull();
        dispose();
      });
    });

    it("fail() ignores non-existent task", () => {
      createRoot((dispose) => {
        store.fail("non-existent", "error");
        expect(store.state.activeTaskId).toBeNull();
        dispose();
      });
    });

    it("cancel() ignores non-existent task", () => {
      createRoot((dispose) => {
        store.cancel("non-existent");
        expect(store.state.activeTaskId).toBeNull();
        dispose();
      });
    });
  });

  describe("cancel() active task", () => {
    it("clears activeTaskId when cancelling active task", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.state.activeTaskId).toBe(id);
        store.cancel(id);
        expect(store.state.activeTaskId).toBeNull();
        expect(store.get(id)!.status).toBe("cancelled");
        dispose();
      });
    });

    it("calls completion callbacks on cancel", () => {
      createRoot((dispose) => {
        const callback = vi.fn();
        store.onCompletion(callback);
        const id = store.create({ name: "Test", agentType: "claude" });
        store.cancel(id);
        expect(callback).toHaveBeenCalledTimes(1);
        dispose();
      });
    });
  });

  describe("fail() active task", () => {
    it("clears activeTaskId when failing active task", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.fail(id, "boom");
        expect(store.state.activeTaskId).toBeNull();
        dispose();
      });
    });

    it("calls completion callbacks on fail", () => {
      createRoot((dispose) => {
        const callback = vi.fn();
        store.onCompletion(callback);
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.fail(id, "boom");
        expect(callback).toHaveBeenCalledTimes(1);
        dispose();
      });
    });
  });

  describe("remove() active task", () => {
    it("clears activeTaskId when removing active task", () => {
      createRoot((dispose) => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.remove(id);
        expect(store.state.activeTaskId).toBeNull();
        expect(store.get(id)).toBeUndefined();
        dispose();
      });
    });
  });

  describe("getActive()", () => {
    it("returns undefined when no active task", () => {
      createRoot((dispose) => {
        expect(store.getActive()).toBeUndefined();
        dispose();
      });
    });
  });

  describe("getNextInQueue()", () => {
    it("returns undefined when queue is empty", () => {
      createRoot((dispose) => {
        expect(store.getNextInQueue()).toBeUndefined();
        dispose();
      });
    });
  });

  describe("getBySessionId()", () => {
    it("returns undefined when no match", () => {
      createRoot((dispose) => {
        expect(store.getBySessionId("non-existent")).toBeUndefined();
        dispose();
      });
    });
  });

  describe("complete() non-active task", () => {
    it("does not clear activeTaskId when completing a different task", () => {
      createRoot((dispose) => {
        const id1 = store.create({ name: "T1", agentType: "claude" });
        const id2 = store.create({ name: "T2", agentType: "claude" });
        store.start(id1, "sess-1");
        store.start(id2, "sess-2");
        // id2 is now active; complete id1
        store.complete(id1, 0);
        expect(store.state.activeTaskId).toBe(id2);
        dispose();
      });
    });
  });
});
