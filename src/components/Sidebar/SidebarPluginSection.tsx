import { Component, For, Show, createSignal } from "solid-js";
import type { SidebarPanelState } from "../../stores/sidebarPluginStore";
import { sidebarPluginStore } from "../../stores/sidebarPluginStore";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import type { ContextMenuItem } from "../ContextMenu";
import s from "./Sidebar.module.css";

interface SidebarPluginSectionProps {
  panel: SidebarPanelState;
}

export const SidebarPluginSection: Component<SidebarPluginSectionProps> = (props) => {
  const ctxMenu = createContextMenu();
  const [activeItemIdx, setActiveItemIdx] = createSignal(-1);

  const contextMenuItems = (): ContextMenuItem[] => {
    const idx = activeItemIdx();
    const item = idx >= 0 ? props.panel.items[idx] : undefined;
    if (!item?.contextMenu?.length) return [];
    return item.contextMenu.map((action) => ({
      label: action.label,
      action: () => action.action(),
      disabled: action.disabled,
    }));
  };

  return (
    <div class={s.pluginSection}>
      <div
        class={s.pluginSectionHeader}
        onClick={() => sidebarPluginStore.toggleCollapsed(props.panel.pluginId, props.panel.id)}
      >
        <Show when={props.panel.icon}>
          <span class={s.pluginSectionIcon} innerHTML={props.panel.icon!} />
        </Show>
        <span class={s.pluginSectionLabel}>{props.panel.label}</span>
        <Show when={props.panel.badge}>
          <span class={s.pluginSectionBadge}>{props.panel.badge}</span>
        </Show>
        <span class={s.pluginSectionChevron}>{props.panel.collapsed ? "\u25b6" : "\u25bc"}</span>
      </div>
      <Show when={!props.panel.collapsed}>
        <div class={s.pluginSectionItems}>
          <For each={props.panel.items}>
            {(item, index) => (
              <div
                class={s.pluginItem}
                onClick={() => item.onClick?.()}
                onContextMenu={(e) => {
                  if (item.contextMenu?.length) {
                    e.preventDefault();
                    e.stopPropagation();
                    setActiveItemIdx(index());
                    ctxMenu.open(e);
                  }
                }}
                style={{ cursor: item.onClick ? "pointer" : "default" }}
              >
                <Show when={item.icon}>
                  <span
                    class={s.pluginItemIcon}
                    innerHTML={item.icon!}
                    style={{ color: item.iconColor }}
                  />
                </Show>
                <div class={s.pluginItemText}>
                  <span class={s.pluginItemLabel}>{item.label}</span>
                  <Show when={item.subtitle}>
                    <span class={s.pluginItemSubtitle}>{item.subtitle}</span>
                  </Show>
                </div>
              </div>
            )}
          </For>
        </div>
      </Show>
      <ContextMenu
        items={contextMenuItems()}
        x={ctxMenu.position().x}
        y={ctxMenu.position().y}
        visible={ctxMenu.visible()}
        onClose={ctxMenu.close}
      />
    </div>
  );
};
