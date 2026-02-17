import { Component, For, Show, createEffect, onCleanup } from "solid-js";
import type { JSX } from "solid-js";

export interface DropdownItem {
  id: string;
  label: string;
  icon?: JSX.Element;
  divider?: boolean;
  disabled?: boolean;
}

export interface DropdownProps {
  items: DropdownItem[];
  selected?: string;
  visible: boolean;
  onSelect: (id: string) => void;
  onClose: () => void;
  position?: "top" | "bottom";
  class?: string;
}

export const Dropdown: Component<DropdownProps> = (props) => {
  let dropdownRef: HTMLDivElement | undefined;

  // Close on click outside
  createEffect(() => {
    if (!props.visible) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (dropdownRef && !dropdownRef.contains(e.target as Node)) {
        props.onClose();
      }
    };

    // Delay to avoid immediate close
    setTimeout(() => {
      document.addEventListener("click", handleClickOutside);
    }, 0);

    onCleanup(() => {
      document.removeEventListener("click", handleClickOutside);
    });
  });

  // Close on Escape
  createEffect(() => {
    if (!props.visible) return;

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        props.onClose();
      }
    };

    document.addEventListener("keydown", handleEscape);

    onCleanup(() => {
      document.removeEventListener("keydown", handleEscape);
    });
  });

  return (
    <Show when={props.visible}>
      <div
        ref={dropdownRef}
        class={`dropdown ${props.position === "top" ? "dropdown-top" : ""} ${props.class || ""}`}
      >
        <For each={props.items}>
          {(item) => (
            <Show
              when={!item.divider}
              fallback={<div class="dropdown-divider" />}
            >
              <div
                class={`dropdown-item ${item.id === props.selected ? "selected" : ""} ${item.disabled ? "disabled" : ""}`}
                onClick={() => !item.disabled && props.onSelect(item.id)}
              >
                <Show when={item.icon}>
                  <span class="dropdown-item-icon">{item.icon}</span>
                </Show>
                <span class="dropdown-item-label">{item.label}</span>
              </div>
            </Show>
          )}
        </For>
      </div>
    </Show>
  );
};
