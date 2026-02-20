import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { createRoot } from "solid-js";
import { ContextMenu, createContextMenu } from "../../components/ContextMenu/ContextMenu";
import type { ContextMenuItem } from "../../components/ContextMenu/ContextMenu";

const sampleItems: ContextMenuItem[] = [
  { label: "Copy", shortcut: "\u2318C", action: vi.fn() },
  { label: "Paste", shortcut: "\u2318V", action: vi.fn() },
  { label: "Delete", action: vi.fn(), disabled: true },
];

describe("ContextMenu", () => {
  it("renders nothing when not visible", () => {
    const { container } = render(() => (
      <ContextMenu
        items={sampleItems}
        x={100}
        y={200}
        visible={false}
        onClose={() => {}}
      />
    ));
    const menu = container.querySelector(".context-menu");
    expect(menu).toBeNull();
  });

  it("renders menu items when visible", () => {
    const { container } = render(() => (
      <ContextMenu
        items={sampleItems}
        x={100}
        y={200}
        visible={true}
        onClose={() => {}}
      />
    ));
    const items = container.querySelectorAll(".context-menu-item");
    expect(items.length).toBe(3);
  });

  it("renders labels correctly", () => {
    const { container } = render(() => (
      <ContextMenu
        items={sampleItems}
        x={0}
        y={0}
        visible={true}
        onClose={() => {}}
      />
    ));
    const labels = container.querySelectorAll(".context-menu-label");
    expect(labels[0].textContent).toBe("Copy");
    expect(labels[1].textContent).toBe("Paste");
    expect(labels[2].textContent).toBe("Delete");
  });

  it("renders shortcuts when provided", () => {
    const { container } = render(() => (
      <ContextMenu
        items={sampleItems}
        x={0}
        y={0}
        visible={true}
        onClose={() => {}}
      />
    ));
    const shortcuts = container.querySelectorAll(".context-menu-shortcut");
    expect(shortcuts.length).toBe(2); // Copy and Paste have shortcuts
    expect(shortcuts[0].textContent).toBe("\u2318C");
  });

  it("fires action and closes on item click", () => {
    const action = vi.fn();
    const handleClose = vi.fn();
    const items: ContextMenuItem[] = [
      { label: "Run", action },
    ];
    const { container } = render(() => (
      <ContextMenu
        items={items}
        x={0}
        y={0}
        visible={true}
        onClose={handleClose}
      />
    ));
    const btn = container.querySelector(".context-menu-item")!;
    fireEvent.click(btn);
    expect(action).toHaveBeenCalledOnce();
    expect(handleClose).toHaveBeenCalledOnce();
  });

  it("does not fire action on disabled item click", () => {
    const action = vi.fn();
    const items: ContextMenuItem[] = [
      { label: "Disabled", action, disabled: true },
    ];
    const { container } = render(() => (
      <ContextMenu
        items={items}
        x={0}
        y={0}
        visible={true}
        onClose={() => {}}
      />
    ));
    const btn = container.querySelector(".context-menu-item")!;
    fireEvent.click(btn);
    expect(action).not.toHaveBeenCalled();
  });

  it("renders separator when item has separator flag", () => {
    const items: ContextMenuItem[] = [
      { label: "Above", action: vi.fn(), separator: true },
      { label: "Below", action: vi.fn() },
    ];
    const { container } = render(() => (
      <ContextMenu
        items={items}
        x={0}
        y={0}
        visible={true}
        onClose={() => {}}
      />
    ));
    const separators = container.querySelectorAll(".context-menu-separator");
    expect(separators.length).toBe(1);
  });

  it("closes on Escape key", () => {
    const handleClose = vi.fn();
    render(() => (
      <ContextMenu
        items={sampleItems}
        x={0}
        y={0}
        visible={true}
        onClose={handleClose}
      />
    ));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(handleClose).toHaveBeenCalled();
  });

  it("closes on click outside menu", () => {
    const handleClose = vi.fn();
    render(() => (
      <div>
        <div data-testid="outside">Outside</div>
        <ContextMenu
          items={sampleItems}
          x={0}
          y={0}
          visible={true}
          onClose={handleClose}
        />
      </div>
    ));
    fireEvent.mouseDown(document.body);
    expect(handleClose).toHaveBeenCalled();
  });

  it("clamps position to viewport bounds", () => {
    // Set a small viewport
    Object.defineProperty(window, "innerWidth", { value: 200, writable: true, configurable: true });
    Object.defineProperty(window, "innerHeight", { value: 100, writable: true, configurable: true });

    const { container } = render(() => (
      <ContextMenu
        items={[{ label: "Test", action: vi.fn() }]}
        x={190}
        y={90}
        visible={true}
        onClose={() => {}}
      />
    ));
    const menu = container.querySelector(".context-menu") as HTMLElement;
    // x should be clamped: 200 - 180 - 8 = 12
    expect(parseInt(menu.style.left)).toBeLessThan(190);
    // y should be clamped
    expect(parseInt(menu.style.top)).toBeLessThan(90);

    // Restore
    Object.defineProperty(window, "innerWidth", { value: 1024, writable: true, configurable: true });
    Object.defineProperty(window, "innerHeight", { value: 768, writable: true, configurable: true });
  });

  it("positions menu at x,y coordinates", () => {
    const { container } = render(() => (
      <ContextMenu
        items={[{ label: "Test", action: vi.fn() }]}
        x={150}
        y={250}
        visible={true}
        onClose={() => {}}
      />
    ));
    const menu = container.querySelector(".context-menu") as HTMLElement;
    expect(menu.style.left).toBe("150px");
    expect(menu.style.top).toBe("250px");
  });
});

describe("ContextMenu submenus", () => {
  it("items with children render submenu arrow indicator", () => {
    const items: ContextMenuItem[] = [
      { label: "Normal", action: vi.fn() },
      {
        label: "Move to Group",
        action: vi.fn(),
        children: [
          { label: "Work", action: vi.fn() },
          { label: "Personal", action: vi.fn() },
        ],
      },
    ];
    const { container } = render(() => (
      <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />
    ));
    const arrows = container.querySelectorAll(".context-menu-arrow");
    expect(arrows.length).toBe(1);
  });

  it("hovering parent shows submenu", async () => {
    const items: ContextMenuItem[] = [
      {
        label: "Move to Group",
        action: vi.fn(),
        children: [
          { label: "Work", action: vi.fn() },
        ],
      },
    ];
    const { container } = render(() => (
      <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />
    ));
    const parentWrap = container.querySelector(".context-menu-item-wrap")!;
    fireEvent.mouseEnter(parentWrap);
    const submenu = container.querySelector(".context-submenu");
    expect(submenu).not.toBeNull();
  });

  it("clicking submenu item fires action and closes all menus", () => {
    const childAction = vi.fn();
    const handleClose = vi.fn();
    const items: ContextMenuItem[] = [
      {
        label: "Move to Group",
        action: vi.fn(),
        children: [
          { label: "Work", action: childAction },
        ],
      },
    ];
    const { container } = render(() => (
      <ContextMenu items={items} x={0} y={0} visible={true} onClose={handleClose} />
    ));
    // Show submenu
    const parentWrap = container.querySelector(".context-menu-item-wrap")!;
    fireEvent.mouseEnter(parentWrap);
    // Click submenu item
    const submenuItem = container.querySelector(".context-submenu .context-menu-item")!;
    fireEvent.click(submenuItem);
    expect(childAction).toHaveBeenCalledOnce();
    expect(handleClose).toHaveBeenCalledOnce();
  });

  it("clicking parent item with children does not fire parent action", () => {
    const parentAction = vi.fn();
    const items: ContextMenuItem[] = [
      {
        label: "Move to Group",
        action: parentAction,
        children: [
          { label: "Work", action: vi.fn() },
        ],
      },
    ];
    const { container } = render(() => (
      <ContextMenu items={items} x={0} y={0} visible={true} onClose={() => {}} />
    ));
    const parentItem = container.querySelector(".context-menu-item")!;
    fireEvent.click(parentItem);
    expect(parentAction).not.toHaveBeenCalled();
  });
});

describe("createContextMenu", () => {
  it("initializes with visible=false", () => {
    createRoot((dispose) => {
      const menu = createContextMenu();
      expect(menu.visible()).toBe(false);
      expect(menu.position().x).toBe(0);
      expect(menu.position().y).toBe(0);
      dispose();
    });
  });

  it("open sets visible and position from mouse event", () => {
    createRoot((dispose) => {
      const menu = createContextMenu();
      const mockEvent = {
        preventDefault: vi.fn(),
        clientX: 300,
        clientY: 400,
      } as unknown as MouseEvent;

      menu.open(mockEvent);

      expect(menu.visible()).toBe(true);
      expect(menu.position().x).toBe(300);
      expect(menu.position().y).toBe(400);
      expect(mockEvent.preventDefault).toHaveBeenCalled();
      dispose();
    });
  });

  it("openAt sets visible and position from coordinates", () => {
    createRoot((dispose) => {
      const menu = createContextMenu();
      menu.openAt(250, 350);

      expect(menu.visible()).toBe(true);
      expect(menu.position().x).toBe(250);
      expect(menu.position().y).toBe(350);
      dispose();
    });
  });

  it("close sets visible to false", () => {
    createRoot((dispose) => {
      const menu = createContextMenu();
      const mockEvent = {
        preventDefault: vi.fn(),
        clientX: 100,
        clientY: 100,
      } as unknown as MouseEvent;

      menu.open(mockEvent);
      expect(menu.visible()).toBe(true);

      menu.close();
      expect(menu.visible()).toBe(false);
      dispose();
    });
  });
});
