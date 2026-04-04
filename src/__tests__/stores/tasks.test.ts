import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScope } from "../helpers/store";

describe("tasksStore", () => {
  let store: typeof import("../../stores/tasks").tasksStore;

  beforeEach(async () => {
    vi.resetModules();
    localStorage.clear();
    store = (await import("../../stores/tasks")).tasksStore;
  });

  describe("create()", () => {
    it("creates a task with generated ID", () => {
      testInScope(() => {
        const id = store.create({ name: "Test task", agentType: "claude" });
        expect(id).toBe("task-1");
        const task = store.get(id);
        expect(task).toBeDefined();
        expect(task!.name).toBe("Test task");
        expect(task!.status).toBe("pending");
        expect(task!.agentType).toBe("claude");
      });
    });

    it("adds to task queue", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        expect(store.state.taskQueue).toContain(id);
      });
    });

    it("increments IDs", () => {
      testInScope(() => {
        const id1 = store.create({ name: "T1", agentType: "claude" });
        const id2 = store.create({ name: "T2", agentType: "gemini" });
        expect(id1).toBe("task-1");
        expect(id2).toBe("task-2");
      });
    });
  });

  describe("start()", () => {
    it("marks task as running", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.get(id)!.status).toBe("running");
        expect(store.get(id)!.sessionId).toBe("sess-1");
        expect(store.get(id)!.startedAt).toBeDefined();
      });
    });

    it("sets as active task", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.state.activeTaskId).toBe(id);
      });
    });

    it("removes from queue", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.state.taskQueue).not.toContain(id);
      });
    });
  });

  describe("complete()", () => {
    it("marks task as completed with exit code 0", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.complete(id, 0);
        expect(store.get(id)!.status).toBe("completed");
        expect(store.get(id)!.exitCode).toBe(0);
      });
    });

    it("marks task as failed with non-zero exit code", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.complete(id, 1);
        expect(store.get(id)!.status).toBe("failed");
      });
    });

    it("clears active task", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.complete(id);
        expect(store.state.activeTaskId).toBeNull();
      });
    });

    it("calls completion callbacks", () => {
      testInScope(() => {
        const callback = vi.fn();
        store.onCompletion(callback);
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.complete(id);
        expect(callback).toHaveBeenCalledTimes(1);
      });
    });
  });

  describe("fail()", () => {
    it("marks task as failed with error", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.fail(id, "Something broke");
        expect(store.get(id)!.status).toBe("failed");
        expect(store.get(id)!.error).toBe("Something broke");
      });
    });
  });

  describe("cancel()", () => {
    it("marks task as cancelled", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.cancel(id);
        expect(store.get(id)!.status).toBe("cancelled");
      });
    });

    it("removes from queue", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.cancel(id);
        expect(store.state.taskQueue).not.toContain(id);
      });
    });
  });

  describe("getActive()", () => {
    it("returns active task", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.getActive()?.id).toBe(id);
      });
    });
  });

  describe("getNextInQueue()", () => {
    it("returns first task in queue", () => {
      testInScope(() => {
        const id1 = store.create({ name: "T1", agentType: "claude" });
        store.create({ name: "T2", agentType: "claude" });
        expect(store.getNextInQueue()?.id).toBe(id1);
      });
    });
  });

  describe("getAll()", () => {
    it("returns all tasks", () => {
      testInScope(() => {
        store.create({ name: "T1", agentType: "claude" });
        store.create({ name: "T2", agentType: "gemini" });
        expect(store.getAll()).toHaveLength(2);
      });
    });
  });

  describe("getByStatus()", () => {
    it("filters by status", () => {
      testInScope(() => {
        store.create({ name: "T1", agentType: "claude" });
        const id2 = store.create({ name: "T2", agentType: "claude" });
        store.start(id2, "sess-1");
        expect(store.getByStatus("pending")).toHaveLength(1);
        expect(store.getByStatus("running")).toHaveLength(1);
      });
    });
  });

  describe("getBySessionId()", () => {
    it("finds task by session ID", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.getBySessionId("sess-1")?.id).toBe(id);
      });
    });
  });

  describe("remove()", () => {
    it("removes a task", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.remove(id);
        expect(store.get(id)).toBeUndefined();
      });
    });
  });

  describe("clearCompleted()", () => {
    it("removes completed, cancelled, and failed tasks", () => {
      testInScope(() => {
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
      });
    });
  });

  describe("onCompletion()", () => {
    it("returns unsubscribe function", () => {
      testInScope(() => {
        const callback = vi.fn();
        const unsubscribe = store.onCompletion(callback);
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");

        unsubscribe();
        store.complete(id);
        expect(callback).not.toHaveBeenCalled();
      });
    });
  });

  describe("reorderQueue()", () => {
    it("reorders tasks in queue", () => {
      testInScope(() => {
        const id1 = store.create({ name: "T1", agentType: "claude" });
        const id2 = store.create({ name: "T2", agentType: "claude" });
        const id3 = store.create({ name: "T3", agentType: "claude" });
        store.reorderQueue(id1, 2);
        expect(store.state.taskQueue).toEqual([id2, id3, id1]);
      });
    });

    it("does nothing for non-existent task", () => {
      testInScope(() => {
        store.create({ name: "T1", agentType: "claude" });
        store.reorderQueue("non-existent", 0);
        expect(store.getQueueLength()).toBe(1);
      });
    });
  });

  describe("getQueueLength()", () => {
    it("returns queue length", () => {
      testInScope(() => {
        store.create({ name: "T1", agentType: "claude" });
        store.create({ name: "T2", agentType: "claude" });
        expect(store.getQueueLength()).toBe(2);
      });
    });
  });

  describe("guard clauses", () => {
    it("start() ignores non-existent task", () => {
      testInScope(() => {
        store.start("non-existent", "sess-1");
        expect(store.state.activeTaskId).toBeNull();
      });
    });

    it("complete() ignores non-existent task", () => {
      testInScope(() => {
        store.complete("non-existent");
        expect(store.state.activeTaskId).toBeNull();
      });
    });

    it("fail() ignores non-existent task", () => {
      testInScope(() => {
        store.fail("non-existent", "error");
        expect(store.state.activeTaskId).toBeNull();
      });
    });

    it("cancel() ignores non-existent task", () => {
      testInScope(() => {
        store.cancel("non-existent");
        expect(store.state.activeTaskId).toBeNull();
      });
    });
  });

  describe("cancel() active task", () => {
    it("clears activeTaskId when cancelling active task", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        expect(store.state.activeTaskId).toBe(id);
        store.cancel(id);
        expect(store.state.activeTaskId).toBeNull();
        expect(store.get(id)!.status).toBe("cancelled");
      });
    });

    it("calls completion callbacks on cancel", () => {
      testInScope(() => {
        const callback = vi.fn();
        store.onCompletion(callback);
        const id = store.create({ name: "Test", agentType: "claude" });
        store.cancel(id);
        expect(callback).toHaveBeenCalledTimes(1);
      });
    });
  });

  describe("fail() active task", () => {
    it("clears activeTaskId when failing active task", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.fail(id, "boom");
        expect(store.state.activeTaskId).toBeNull();
      });
    });

    it("calls completion callbacks on fail", () => {
      testInScope(() => {
        const callback = vi.fn();
        store.onCompletion(callback);
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.fail(id, "boom");
        expect(callback).toHaveBeenCalledTimes(1);
      });
    });
  });

  describe("remove() active task", () => {
    it("clears activeTaskId when removing active task", () => {
      testInScope(() => {
        const id = store.create({ name: "Test", agentType: "claude" });
        store.start(id, "sess-1");
        store.remove(id);
        expect(store.state.activeTaskId).toBeNull();
        expect(store.get(id)).toBeUndefined();
      });
    });
  });

  describe("getActive()", () => {
    it("returns undefined when no active task", () => {
      testInScope(() => {
        expect(store.getActive()).toBeUndefined();
      });
    });
  });

  describe("getNextInQueue()", () => {
    it("returns undefined when queue is empty", () => {
      testInScope(() => {
        expect(store.getNextInQueue()).toBeUndefined();
      });
    });
  });

  describe("getBySessionId()", () => {
    it("returns undefined when no match", () => {
      testInScope(() => {
        expect(store.getBySessionId("non-existent")).toBeUndefined();
      });
    });
  });

  describe("complete() non-active task", () => {
    it("does not clear activeTaskId when completing a different task", () => {
      testInScope(() => {
        const id1 = store.create({ name: "T1", agentType: "claude" });
        const id2 = store.create({ name: "T2", agentType: "claude" });
        store.start(id1, "sess-1");
        store.start(id2, "sess-2");
        // id2 is now active; complete id1
        store.complete(id1, 0);
        expect(store.state.activeTaskId).toBe(id2);
      });
    });
  });
});
