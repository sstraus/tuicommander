import { Component, For, Show, createSignal, onCleanup, createEffect } from "solid-js";

export interface ContextMenuItem {
  label: string;
  shortcut?: string;
  action: () => void;
  separator?: boolean;
  disabled?: boolean;
  children?: ContextMenuItem[];
}

export interface ContextMenuProps {
  items: ContextMenuItem[];
  x: number;
  y: number;
  visible: boolean;
  onClose: () => void;
}

/** Single menu item — handles both leaf items and items with submenus */
const MenuItem: Component<{
  item: ContextMenuItem;
  onClose: () => void;
}> = (props) => {
  const [submenuOpen, setSubmenuOpen] = createSignal(false);
  const hasChildren = () => !!(props.item.children && props.item.children.length > 0);

  return (
    <>
      <Show when={props.item.separator}>
        <div class="context-menu-separator" />
      </Show>
      <div
        class="context-menu-item-wrap"
        onMouseEnter={() => hasChildren() && setSubmenuOpen(true)}
        onMouseLeave={() => setSubmenuOpen(false)}
      >
        <button
          class={`context-menu-item ${props.item.disabled ? "disabled" : ""} ${hasChildren() ? "has-children" : ""}`}
          onClick={() => {
            if (props.item.disabled || hasChildren()) return;
            props.item.action();
            props.onClose();
          }}
          disabled={props.item.disabled}
        >
          <span class="context-menu-label">{props.item.label}</span>
          <Show when={props.item.shortcut}>
            <span class="context-menu-shortcut">{props.item.shortcut}</span>
          </Show>
          <Show when={hasChildren()}>
            <span class="context-menu-arrow">{"\u203A"}</span>
          </Show>
        </button>
        <Show when={submenuOpen() && props.item.children}>
          <div class="context-submenu">
            <For each={props.item.children}>
              {(child) => (
                <MenuItem item={child} onClose={props.onClose} />
              )}
            </For>
          </div>
        </Show>
      </div>
    </>
  );
};

export const ContextMenu: Component<ContextMenuProps> = (props) => {
  let menuRef: HTMLDivElement | undefined;

  // Close on escape key
  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        props.onClose();
      }
    };

    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef && !menuRef.contains(e.target as Node)) {
        props.onClose();
      }
    };

    document.addEventListener("keydown", handleKeydown);
    document.addEventListener("mousedown", handleClickOutside);

    onCleanup(() => {
      document.removeEventListener("keydown", handleKeydown);
      document.removeEventListener("mousedown", handleClickOutside);
    });
  });

  // Calculate position to keep menu in viewport
  const getPosition = () => {
    const menuWidth = 180;
    const menuHeight = props.items.length * 32;
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;

    let x = props.x;
    let y = props.y;

    if (x + menuWidth > viewportWidth) {
      x = viewportWidth - menuWidth - 8;
    }
    if (y + menuHeight > viewportHeight) {
      y = viewportHeight - menuHeight - 8;
    }

    return { x, y };
  };

  return (
    <Show when={props.visible}>
      <div
        ref={menuRef}
        class="context-menu"
        style={{
          left: `${getPosition().x}px`,
          top: `${getPosition().y}px`,
        }}
      >
        <For each={props.items}>
          {(item) => <MenuItem item={item} onClose={props.onClose} />}
        </For>
      </div>
    </Show>
  );
};

/** Hook to manage context menu state */
export function createContextMenu() {
  const [visible, setVisible] = createSignal(false);
  const [position, setPosition] = createSignal({ x: 0, y: 0 });

  const open = (e: MouseEvent) => {
    e.preventDefault();
    setPosition({ x: e.clientX, y: e.clientY });
    setVisible(true);
  };

  /** Open the menu at specific coordinates (for programmatic positioning) */
  const openAt = (x: number, y: number) => {
    setPosition({ x, y });
    setVisible(true);
  };

  const close = () => {
    setVisible(false);
  };

  return {
    visible,
    position,
    open,
    openAt,
    close,
  };
}

export default ContextMenu;
