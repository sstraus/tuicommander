import { describe, it, expect, beforeEach } from "vitest";
import { createRoot } from "solid-js";
import { sidebarPluginStore } from "../../stores/sidebarPluginStore";
import type { SidebarItem } from "../../plugins/types";

describe("sidebarPluginStore", () => {
  beforeEach(() => {
    sidebarPluginStore.clear();
  });

  describe("registerPanel", () => {
    it("registers a panel and returns a handle", () => {
      createRoot((dispose) => {
        const handle = sidebarPluginStore.registerPanel("my-plugin", {
          id: "panel1",
          label: "My Panel",
        });
        expect(handle).toBeDefined();
        expect(typeof handle.setItems).toBe("function");
        expect(typeof handle.setBadge).toBe("function");
        expect(typeof handle.dispose).toBe("function");
        dispose();
      });
    });

    it("panel appears in getPanels", () => {
      createRoot((dispose) => {
        sidebarPluginStore.registerPanel("my-plugin", {
          id: "panel1",
          label: "My Panel",
        });
        const panels = sidebarPluginStore.getPanels();
        expect(panels).toHaveLength(1);
        expect(panels[0].label).toBe("My Panel");
        expect(panels[0].pluginId).toBe("my-plugin");
        dispose();
      });
    });

    it("registers multiple panels sorted by priority", () => {
      createRoot((dispose) => {
        sidebarPluginStore.registerPanel("p1", { id: "low", label: "Low", priority: 200 });
        sidebarPluginStore.registerPanel("p2", { id: "high", label: "High", priority: 10 });
        sidebarPluginStore.registerPanel("p3", { id: "default", label: "Default" });
        const panels = sidebarPluginStore.getPanels();
        expect(panels.map((p) => p.label)).toEqual(["High", "Default", "Default"].slice(0, 1).concat(["Default", "Low"]));
        // priority: 10, 100 (default), 200
        expect(panels[0].label).toBe("High");
        expect(panels[2].label).toBe("Low");
        dispose();
      });
    });

    it("replaces panel with same id from same plugin", () => {
      createRoot((dispose) => {
        sidebarPluginStore.registerPanel("p1", { id: "panel1", label: "Original" });
        sidebarPluginStore.registerPanel("p1", { id: "panel1", label: "Updated" });
        const panels = sidebarPluginStore.getPanels();
        expect(panels).toHaveLength(1);
        expect(panels[0].label).toBe("Updated");
        dispose();
      });
    });
  });

  describe("handle.setItems", () => {
    it("sets items on a panel", () => {
      createRoot((dispose) => {
        const handle = sidebarPluginStore.registerPanel("p1", { id: "panel1", label: "Test" });
        const items: SidebarItem[] = [
          { id: "item1", label: "Item 1" },
          { id: "item2", label: "Item 2", subtitle: "description" },
        ];
        handle.setItems(items);
        const panels = sidebarPluginStore.getPanels();
        expect(panels[0].items).toHaveLength(2);
        expect(panels[0].items[0].label).toBe("Item 1");
        expect(panels[0].items[1].subtitle).toBe("description");
        dispose();
      });
    });

    it("replaces previous items", () => {
      createRoot((dispose) => {
        const handle = sidebarPluginStore.registerPanel("p1", { id: "panel1", label: "Test" });
        handle.setItems([{ id: "a", label: "A" }]);
        handle.setItems([{ id: "b", label: "B" }, { id: "c", label: "C" }]);
        const panels = sidebarPluginStore.getPanels();
        expect(panels[0].items).toHaveLength(2);
        expect(panels[0].items[0].label).toBe("B");
        dispose();
      });
    });
  });

  describe("handle.setBadge", () => {
    it("sets badge text on panel header", () => {
      createRoot((dispose) => {
        const handle = sidebarPluginStore.registerPanel("p1", { id: "panel1", label: "Test" });
        handle.setBadge("3");
        expect(sidebarPluginStore.getPanels()[0].badge).toBe("3");
        dispose();
      });
    });

    it("clears badge with null", () => {
      createRoot((dispose) => {
        const handle = sidebarPluginStore.registerPanel("p1", { id: "panel1", label: "Test" });
        handle.setBadge("5");
        handle.setBadge(null);
        expect(sidebarPluginStore.getPanels()[0].badge).toBeNull();
        dispose();
      });
    });
  });

  describe("handle.dispose", () => {
    it("removes panel from store", () => {
      createRoot((dispose) => {
        const handle = sidebarPluginStore.registerPanel("p1", { id: "panel1", label: "Test" });
        expect(sidebarPluginStore.getPanels()).toHaveLength(1);
        handle.dispose();
        expect(sidebarPluginStore.getPanels()).toHaveLength(0);
        dispose();
      });
    });
  });

  describe("clearPlugin", () => {
    it("removes all panels for a plugin", () => {
      createRoot((dispose) => {
        sidebarPluginStore.registerPanel("p1", { id: "a", label: "A" });
        sidebarPluginStore.registerPanel("p1", { id: "b", label: "B" });
        sidebarPluginStore.registerPanel("p2", { id: "c", label: "C" });
        expect(sidebarPluginStore.getPanels()).toHaveLength(3);
        sidebarPluginStore.clearPlugin("p1");
        expect(sidebarPluginStore.getPanels()).toHaveLength(1);
        expect(sidebarPluginStore.getPanels()[0].pluginId).toBe("p2");
        dispose();
      });
    });
  });

  describe("collapsed state", () => {
    it("defaults to collapsed=true", () => {
      createRoot((dispose) => {
        sidebarPluginStore.registerPanel("p1", { id: "panel1", label: "Test" });
        expect(sidebarPluginStore.getPanels()[0].collapsed).toBe(true);
        dispose();
      });
    });

    it("respects initial collapsed=false", () => {
      createRoot((dispose) => {
        sidebarPluginStore.registerPanel("p1", { id: "panel1", label: "Test", collapsed: false });
        expect(sidebarPluginStore.getPanels()[0].collapsed).toBe(false);
        dispose();
      });
    });

    it("toggleCollapsed flips state", () => {
      createRoot((dispose) => {
        sidebarPluginStore.registerPanel("p1", { id: "panel1", label: "Test", collapsed: false });
        sidebarPluginStore.toggleCollapsed("p1", "panel1");
        expect(sidebarPluginStore.getPanels()[0].collapsed).toBe(true);
        sidebarPluginStore.toggleCollapsed("p1", "panel1");
        expect(sidebarPluginStore.getPanels()[0].collapsed).toBe(false);
        dispose();
      });
    });
  });
});
