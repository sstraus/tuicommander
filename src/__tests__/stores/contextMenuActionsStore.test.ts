import { describe, it, expect, beforeEach } from "vitest";
import "../mocks/tauri";
import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";
import type { TerminalAction } from "../../plugins/types";

function makeAction(id: string, label?: string): TerminalAction {
  return {
    id,
    label: label ?? `Action ${id}`,
    action: () => {},
  };
}

describe("contextMenuActionsStore", () => {
  beforeEach(() => {
    contextMenuActionsStore.clear();
  });

  it("starts empty", () => {
    expect(contextMenuActionsStore.getActions()).toEqual([]);
  });

  it("registers an action and returns it via getActions()", () => {
    contextMenuActionsStore.registerAction("pluginA", makeAction("a1"));
    const actions = contextMenuActionsStore.getActions();
    expect(actions).toHaveLength(1);
    expect(actions[0].id).toBe("a1");
    expect(actions[0].label).toBe("Action a1");
  });

  it("dispose removes the action", () => {
    const disposable = contextMenuActionsStore.registerAction("pluginA", makeAction("a1"));
    expect(contextMenuActionsStore.getActions()).toHaveLength(1);
    disposable.dispose();
    expect(contextMenuActionsStore.getActions()).toHaveLength(0);
  });

  it("registers actions from two plugins as flat list", () => {
    contextMenuActionsStore.registerAction("pluginA", makeAction("a1"));
    contextMenuActionsStore.registerAction("pluginB", makeAction("b1"));
    const actions = contextMenuActionsStore.getActions();
    expect(actions).toHaveLength(2);
    expect(actions.map((a) => a.id)).toEqual(["a1", "b1"]);
  });

  it("replaces action with same id from same plugin", () => {
    contextMenuActionsStore.registerAction("pluginA", makeAction("a1", "First"));
    contextMenuActionsStore.registerAction("pluginA", makeAction("a1", "Second"));
    const actions = contextMenuActionsStore.getActions();
    expect(actions).toHaveLength(1);
    expect(actions[0].label).toBe("Second");
  });

  it("does not replace action with same id from different plugin", () => {
    contextMenuActionsStore.registerAction("pluginA", makeAction("x1", "From A"));
    contextMenuActionsStore.registerAction("pluginB", makeAction("x1", "From B"));
    expect(contextMenuActionsStore.getActions()).toHaveLength(2);
  });

  it("clearPlugin removes only that plugin's actions", () => {
    contextMenuActionsStore.registerAction("pluginA", makeAction("a1"));
    contextMenuActionsStore.registerAction("pluginB", makeAction("b1"));
    contextMenuActionsStore.clearPlugin("pluginA");
    const actions = contextMenuActionsStore.getActions();
    expect(actions).toHaveLength(1);
    expect(actions[0].id).toBe("b1");
  });

  it("clear() removes all actions", () => {
    contextMenuActionsStore.registerAction("pluginA", makeAction("a1"));
    contextMenuActionsStore.registerAction("pluginB", makeAction("b1"));
    contextMenuActionsStore.clear();
    expect(contextMenuActionsStore.getActions()).toHaveLength(0);
  });
});
