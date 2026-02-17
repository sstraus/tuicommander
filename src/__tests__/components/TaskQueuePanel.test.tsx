import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";

// Mock Tauri APIs (tasks store may import from agents which could need these)
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
  emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    listen: vi.fn().mockResolvedValue(vi.fn()),
    setTitle: vi.fn().mockResolvedValue(undefined),
  })),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

import { TaskQueuePanel } from "../../components/TaskQueuePanel/TaskQueuePanel";
import { tasksStore } from "../../stores/tasks";

/** Helper to clean all tasks from the store */
function clearAllTasks(): void {
  tasksStore.clearCompleted();
  const remaining = tasksStore.getAll();
  remaining.forEach((t) => {
    if (t.status === "pending" || t.status === "running") {
      tasksStore.cancel(t.id);
    }
  });
  tasksStore.clearCompleted();
}

describe("TaskQueuePanel", () => {
  beforeEach(() => {
    clearAllTasks();
  });

  describe("visibility", () => {
    it("renders nothing when not visible", () => {
      const { container } = render(() => (
        <TaskQueuePanel visible={false} onClose={() => {}} />
      ));
      const panel = container.querySelector(".task-queue-panel");
      expect(panel).toBeNull();
    });

    it("renders panel when visible", () => {
      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));
      const panel = container.querySelector(".task-queue-panel");
      expect(panel).not.toBeNull();
    });
  });

  describe("header", () => {
    it("renders header with title and close button", () => {
      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));
      const header = container.querySelector(".task-queue-header h3");
      expect(header).not.toBeNull();
      expect(header!.textContent).toBe("Task Queue");

      const closeBtn = container.querySelector(".task-queue-close");
      expect(closeBtn).not.toBeNull();
    });

    it("calls onClose when close button is clicked", () => {
      const onClose = vi.fn();
      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={onClose} />
      ));
      const closeBtn = container.querySelector(".task-queue-close")!;
      fireEvent.click(closeBtn);
      expect(onClose).toHaveBeenCalledOnce();
    });

    it("renders clear button", () => {
      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));
      const clearBtn = container.querySelector(".task-queue-clear");
      expect(clearBtn).not.toBeNull();
      expect(clearBtn!.textContent).toBe("Clear");
    });

    it("clicking clear button clears completed tasks", () => {
      // Create and complete a task
      const taskId = tasksStore.create({
        name: "Done Task",
        agentType: "claude",
      });
      tasksStore.start(taskId, "session-1");
      tasksStore.complete(taskId, 0);

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      // Verify task appears
      const tasksBefore = container.querySelectorAll(".task-item-name");
      expect(tasksBefore.length).toBeGreaterThan(0);

      // Click clear
      const clearBtn = container.querySelector(".task-queue-clear")!;
      fireEvent.click(clearBtn);

      // Verify task is removed
      const tasksAfter = container.querySelectorAll(".task-item-name");
      expect(tasksAfter.length).toBe(0);
    });
  });

  describe("empty state", () => {
    it("shows empty state when no tasks exist", () => {
      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));
      const empty = container.querySelector(".task-queue-empty");
      expect(empty).not.toBeNull();
      expect(empty!.textContent).toContain("No tasks in queue");
    });

    it("shows hint text in empty state", () => {
      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));
      const hint = container.querySelector(".task-queue-hint");
      expect(hint).not.toBeNull();
      expect(hint!.textContent).toContain("Tasks will appear here");
    });
  });

  describe("pending tasks", () => {
    it("renders pending tasks", () => {
      tasksStore.create({
        name: "Test Task",
        description: "A test task",
        agentType: "claude",
      });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const taskNames = container.querySelectorAll(".task-item-name");
      expect(taskNames.length).toBeGreaterThan(0);
      expect(taskNames[0].textContent).toBe("Test Task");
    });

    it("shows pending section title with count", () => {
      tasksStore.create({ name: "Task 1", agentType: "claude" });
      tasksStore.create({ name: "Task 2", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const sectionTitles = container.querySelectorAll(".task-queue-section-title");
      const pendingTitle = Array.from(sectionTitles).find(
        (t) => t.textContent?.includes("Pending")
      );
      expect(pendingTitle).not.toBeNull();
      expect(pendingTitle!.textContent).toContain("2");
    });

    it("pending tasks are draggable", () => {
      tasksStore.create({ name: "Draggable", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const wrapper = container.querySelector(".task-item-wrapper");
      expect(wrapper).not.toBeNull();
      expect(wrapper!.getAttribute("draggable")).toBe("true");
    });

    it("shows drag handle on pending tasks", () => {
      tasksStore.create({ name: "Draggable", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const dragHandle = container.querySelector(".task-item-drag");
      expect(dragHandle).not.toBeNull();
    });

    it("shows cancel button on pending tasks", () => {
      tasksStore.create({ name: "Cancellable", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const cancelBtn = container.querySelector(".task-item-cancel");
      expect(cancelBtn).not.toBeNull();
    });

    it("clicking cancel button cancels the task", () => {
      const taskId = tasksStore.create({ name: "To Cancel", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const cancelBtn = container.querySelector(".task-item-cancel")!;
      fireEvent.click(cancelBtn);

      const task = tasksStore.get(taskId);
      expect(task?.status).toBe("cancelled");
    });
  });

  describe("running tasks", () => {
    it("shows running section when tasks are running", () => {
      const taskId = tasksStore.create({ name: "Running Task", agentType: "claude" });
      tasksStore.start(taskId, "session-1");

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const sectionTitles = container.querySelectorAll(".task-queue-section-title");
      const runningTitle = Array.from(sectionTitles).find(
        (t) => t.textContent?.includes("Running")
      );
      expect(runningTitle).not.toBeNull();
    });

    it("running tasks have status icon and color", () => {
      const taskId = tasksStore.create({ name: "Running Task", agentType: "claude" });
      tasksStore.start(taskId, "session-1");

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      // Running status icon is a filled circle
      const statusIcon = container.querySelector(".task-item-status");
      expect(statusIcon).not.toBeNull();
    });

    it("shows cancel button on running tasks", () => {
      const taskId = tasksStore.create({ name: "Running", agentType: "claude" });
      tasksStore.start(taskId, "session-1");

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const cancelBtn = container.querySelector(".task-item-cancel");
      expect(cancelBtn).not.toBeNull();
    });

    it("shows duration for running tasks", () => {
      const taskId = tasksStore.create({ name: "Timed Task", agentType: "claude" });
      tasksStore.start(taskId, "session-1");

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const duration = container.querySelector(".task-item-duration");
      expect(duration).not.toBeNull();
      // Should show seconds since it just started
      expect(duration!.textContent).toMatch(/\d+s/);
    });
  });

  describe("completed tasks", () => {
    it("shows completed section when tasks are completed", () => {
      const taskId = tasksStore.create({ name: "Done Task", agentType: "claude" });
      tasksStore.start(taskId, "session-1");
      tasksStore.complete(taskId, 0);

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const sectionTitles = container.querySelectorAll(".task-queue-section-title");
      const completedTitle = Array.from(sectionTitles).find(
        (t) => t.textContent?.includes("Completed")
      );
      expect(completedTitle).not.toBeNull();
    });

    it("shows completed count in section title", () => {
      const t1 = tasksStore.create({ name: "Done 1", agentType: "claude" });
      tasksStore.start(t1, "s1");
      tasksStore.complete(t1, 0);

      const t2 = tasksStore.create({ name: "Done 2", agentType: "claude" });
      tasksStore.start(t2, "s2");
      tasksStore.complete(t2, 0);

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const sectionTitles = container.querySelectorAll(".task-queue-section-title");
      const completedTitle = Array.from(sectionTitles).find(
        (t) => t.textContent?.includes("Completed")
      );
      expect(completedTitle!.textContent).toContain("2");
    });

    it("completed tasks render in compact mode (no description shown)", () => {
      const taskId = tasksStore.create({
        name: "Compact Task",
        description: "Should not appear",
        agentType: "claude",
      });
      tasksStore.start(taskId, "s1");
      tasksStore.complete(taskId, 0);

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      // Completed tasks have compact class
      const compactItems = container.querySelectorAll(".task-item.compact");
      expect(compactItems.length).toBeGreaterThan(0);

      // Description should not be rendered in compact mode
      const descriptions = container.querySelectorAll(".task-item.compact .task-item-description");
      expect(descriptions.length).toBe(0);
    });

    it("does not show cancel button on completed tasks", () => {
      const taskId = tasksStore.create({ name: "Done", agentType: "claude" });
      tasksStore.start(taskId, "s1");
      tasksStore.complete(taskId, 0);

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      // Only completed tasks are present
      const cancelBtn = container.querySelector(".task-item-cancel");
      expect(cancelBtn).toBeNull();
    });

    it("shows failed tasks in completed section", () => {
      const taskId = tasksStore.create({ name: "Failed Task", agentType: "claude" });
      tasksStore.start(taskId, "s1");
      tasksStore.complete(taskId, 1); // non-zero exit code = failed

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const sectionTitles = container.querySelectorAll(".task-queue-section-title");
      const completedTitle = Array.from(sectionTitles).find(
        (t) => t.textContent?.includes("Completed")
      );
      expect(completedTitle).not.toBeNull();
    });

    it("shows cancelled tasks in completed section", () => {
      const taskId = tasksStore.create({ name: "Cancelled Task", agentType: "claude" });
      tasksStore.cancel(taskId);

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const sectionTitles = container.querySelectorAll(".task-queue-section-title");
      const completedTitle = Array.from(sectionTitles).find(
        (t) => t.textContent?.includes("Completed")
      );
      expect(completedTitle).not.toBeNull();
    });
  });

  describe("task item details", () => {
    it("shows task name", () => {
      tasksStore.create({ name: "My Task", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const name = container.querySelector(".task-item-name");
      expect(name).not.toBeNull();
      expect(name!.textContent).toBe("My Task");
    });

    it("shows task description for non-compact pending tasks", () => {
      tasksStore.create({
        name: "Described Task",
        description: "A longer description",
        agentType: "claude",
      });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const desc = container.querySelector(".task-item-description");
      expect(desc).not.toBeNull();
      expect(desc!.textContent).toBe("A longer description");
    });

    it("does not show description when task has none", () => {
      tasksStore.create({ name: "No Desc", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const desc = container.querySelector(".task-item-description");
      expect(desc).toBeNull();
    });

    it("shows status icon for each status", () => {
      // Pending task
      tasksStore.create({ name: "Pending", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const statusIcons = container.querySelectorAll(".task-item-status");
      expect(statusIcons.length).toBeGreaterThan(0);
      // Pending icon is an empty circle
      expect(statusIcons[0].textContent).toBe("○");
    });
  });

  describe("task selection", () => {
    it("calls onTaskSelect when a task item is clicked", () => {
      const onTaskSelect = vi.fn();
      const taskId = tasksStore.create({ name: "Selectable", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} onTaskSelect={onTaskSelect} />
      ));

      const taskItem = container.querySelector(".task-item")!;
      fireEvent.click(taskItem);
      expect(onTaskSelect).toHaveBeenCalledWith(taskId);
    });

    it("does not fail when onTaskSelect is not provided", () => {
      tasksStore.create({ name: "No Callback", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const taskItem = container.querySelector(".task-item")!;
      expect(() => fireEvent.click(taskItem)).not.toThrow();
    });
  });

  describe("mixed task states", () => {
    it("shows all sections when tasks exist in every state", () => {
      // Create pending task
      tasksStore.create({ name: "Pending Task", agentType: "claude" });

      // Create running task
      const runId = tasksStore.create({ name: "Running Task", agentType: "claude" });
      tasksStore.start(runId, "s1");

      // Create completed task
      const doneId = tasksStore.create({ name: "Done Task", agentType: "claude" });
      tasksStore.start(doneId, "s2");
      tasksStore.complete(doneId, 0);

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const sectionTitles = container.querySelectorAll(".task-queue-section-title");
      const titleTexts = Array.from(sectionTitles).map((t) => t.textContent);

      expect(titleTexts.some((t) => t?.includes("Running"))).toBe(true);
      expect(titleTexts.some((t) => t?.includes("Pending"))).toBe(true);
      expect(titleTexts.some((t) => t?.includes("Completed"))).toBe(true);

      // Empty state should NOT show
      const empty = container.querySelector(".task-queue-empty");
      expect(empty).toBeNull();
    });

    it("does not show running section when no tasks are running", () => {
      tasksStore.create({ name: "Pending Only", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const sectionTitles = container.querySelectorAll(".task-queue-section-title");
      const runningTitle = Array.from(sectionTitles).find(
        (t) => t.textContent?.includes("Running")
      );
      expect(runningTitle).toBeUndefined();
    });

    it("does not show pending section when no tasks are pending", () => {
      const taskId = tasksStore.create({ name: "Only Running", agentType: "claude" });
      tasksStore.start(taskId, "s1");

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const sectionTitles = container.querySelectorAll(".task-queue-section-title");
      const pendingTitle = Array.from(sectionTitles).find(
        (t) => t.textContent?.includes("Pending")
      );
      expect(pendingTitle).toBeUndefined();
    });

    it("does not show completed section when no tasks are completed", () => {
      tasksStore.create({ name: "Pending Only", agentType: "claude" });

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const sectionTitles = container.querySelectorAll(".task-queue-section-title");
      const completedTitle = Array.from(sectionTitles).find(
        (t) => t.textContent?.includes("Completed")
      );
      expect(completedTitle).toBeUndefined();
    });
  });

  describe("duration formatting", () => {
    it("shows duration in minutes and seconds for long-running tasks", () => {
      // Start the task at a known time, then advance Date.now for the render
      const startTime = Date.now();
      const taskId = tasksStore.create({ name: "Long Task", agentType: "claude" });
      tasksStore.start(taskId, "s1");

      // Advance Date.now by 90 seconds so getDuration() computes > 60s
      const realDateNow = Date.now;
      Date.now = () => startTime + 90000;

      const { container } = render(() => (
        <TaskQueuePanel visible={true} onClose={() => {}} />
      ));

      const duration = container.querySelector(".task-item-duration");
      expect(duration).not.toBeNull();
      // Should show minutes and seconds
      expect(duration!.textContent).toMatch(/\d+m \d+s/);

      // Restore
      Date.now = realDateNow;
    });
  });
});
