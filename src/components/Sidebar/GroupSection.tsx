import { Component, Show, type JSX } from "solid-js";
import type { RepoGroup, RepositoryState } from "../../stores/repositories";
import { repositoriesStore } from "../../stores/repositories";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import type { ContextMenuItem } from "../ContextMenu";
import { cx } from "../../utils";
import { t } from "../../i18n";
import s from "./Sidebar.module.css";

/** Group section component — accordion header with collapsible repo list */
export const GroupSection: Component<{
  group: RepoGroup;
  repos: RepositoryState[];
  onRename: (groupId: string) => void;
  onColorChange: (groupId: string) => void;
  onMouseDrag?: (e: MouseEvent) => void;
  dragOverClass?: string;
  children: JSX.Element;
}> = (props) => {
  const groupMenu = createContextMenu();

  const groupMenuItems = (): ContextMenuItem[] => [
    { label: "Rename Group", action: () => props.onRename(props.group.id) },
    { label: "Change Color", action: () => props.onColorChange(props.group.id) },
    { label: "Delete Group", action: () => repositoriesStore.deleteGroup(props.group.id) },
  ];

  return (
    <div
      class={cx(s.groupSection, props.dragOverClass)}
      data-sidebar-group={props.group.id}
    >
      <div
        class={s.groupHeader}
        onClick={() => repositoriesStore.toggleGroupCollapsed(props.group.id)}
        onContextMenu={groupMenu.open}
        onMouseDown={(e) => props.onMouseDrag?.(e)}
      >
        <Show when={props.group.color}>
          <span class={s.groupColorDot} style={{ background: props.group.color }} />
        </Show>
        <span class={s.groupName}>{props.group.name}</span>
        <span class={s.groupCount}>{props.repos.length}</span>
        <span class={cx(s.groupChevron, !props.group.collapsed && s.expanded)}>{"\u203A"}</span>
      </div>
      <Show when={!props.group.collapsed}>
        <div class={s.groupRepos}>
          <Show when={props.repos.length === 0}>
            <div class={s.groupEmptyHint}>{t("sidebar.dragReposHere", "Drag repos here")}</div>
          </Show>
          {props.children}
        </div>
      </Show>
      <ContextMenu
        items={groupMenuItems()}
        x={groupMenu.position().x}
        y={groupMenu.position().y}
        visible={groupMenu.visible()}
        onClose={groupMenu.close}
      />
    </div>
  );
};
