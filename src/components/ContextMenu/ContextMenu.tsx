import { Component, For, Show, createSignal, onCleanup, createEffect } from "solid-js";
import { cx } from "../../utils";
import s from "./ContextMenu.module.css";

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

/** Clamp a submenu position so it stays within the viewport (8px margin). */
const clampSubmenu = (wrapEl: HTMLDivElement, submenuEl: HTMLDivElement) => {
  const parentRect = wrapEl.getBoundingClientRect();
  const subRect = submenuEl.getBoundingClientRect();
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const margin = 8;

  // Horizontal: prefer right of parent, flip left if needed, clamp to viewport
  let left = parentRect.right;
  if (left + subRect.width > vw - margin) {
    left = parentRect.left - subRect.width;
  }
  left = Math.max(margin, Math.min(left, vw - subRect.width - margin));

  // Vertical: align top with parent item, clamp to viewport
  let top = parentRect.top;
  if (top + subRect.height > vh - margin) {
    top = vh - subRect.height - margin;
  }
  top = Math.max(margin, top);

  submenuEl.style.left = `${left}px`;
  submenuEl.style.top = `${top}px`;
};

/** Single menu item — handles both leaf items and items with submenus */
const MenuItem: Component<{
  item: ContextMenuItem;
  onClose: () => void;
}> = (props) => {
  let wrapRef: HTMLDivElement | undefined;
  let submenuRef: HTMLDivElement | undefined;
  const [submenuOpen, setSubmenuOpen] = createSignal(false);
  const hasChildren = () => !!(props.item.children && props.item.children.length > 0);

  const openSubmenu = () => {
    if (props.item.disabled || !hasChildren()) return;
    setSubmenuOpen(true);
    // Position after render
    requestAnimationFrame(() => {
      if (wrapRef && submenuRef) clampSubmenu(wrapRef, submenuRef);
    });
  };

  return (
    <>
      <Show when={props.item.separator}>
        <div class={s.separator} />
      </Show>
      <div
        ref={wrapRef}
        class={s.itemWrap}
        onMouseEnter={openSubmenu}
        onMouseLeave={() => setSubmenuOpen(false)}
      >
        <button
          class={cx(s.item, props.item.disabled && s.disabled)}
          onClick={() => {
            if (props.item.disabled) return;
            if (hasChildren()) {
              if (submenuOpen()) { setSubmenuOpen(false); } else { openSubmenu(); }
              return;
            }
            props.item.action();
            props.onClose();
          }}
          disabled={props.item.disabled}
        >
          <span class={s.label}>{props.item.label}</span>
          <Show when={props.item.shortcut}>
            <span class={s.shortcut}>{props.item.shortcut}</span>
          </Show>
          <Show when={hasChildren()}>
            <span class={s.arrow}>{"\u203A"}</span>
          </Show>
        </button>
        <Show when={submenuOpen() && props.item.children}>
          <div ref={submenuRef} class={s.submenu}>
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
      // Close on any keyboard input — not just Escape.
      // Modifier-only keys (Shift, Control, etc.) are ignored.
      if (e.key === "Shift" || e.key === "Control" || e.key === "Alt" || e.key === "Meta") return;
      if (e.key === "Escape") e.preventDefault();
      props.onClose();
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
        class={s.menu}
        onClick={(e) => e.stopPropagation()}
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
