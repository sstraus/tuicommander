import type { Component } from "solid-js";
import { Show } from "solid-js";
import type { AgentType } from "../../agents";
import { AgentIcon } from "../ui/AgentIcon";
import { ContextMenu, type ContextMenuItem, type ContextMenuProps } from "./ContextMenu";
import s from "./ContextMenu.module.css";

export interface AgentLaunchMenuItem extends ContextMenuItem {
	agentType?: AgentType;
	agentName?: string;
	configName?: string;
	isDefault?: boolean;
}

/** Rich presentation is restricted to the new-tab agent picker. */
export const AgentLaunchMenu: Component<Omit<ContextMenuProps, "items" | "size"> & { items: AgentLaunchMenuItem[] }> = (
	props,
) => (
	<ContextMenu
		{...props}
		size="large"
		items={props.items.map((item) => ({
			...item,
			icon: item.agentType ? (
				<AgentIcon agent={item.agentType} size={20} />
			) : (
				<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
					<path d="M3 4h18v16H3V4zm2 2v12h14V6H5zm2 2 4 4-4 4-1.4-1.4L8.2 12 5.6 9.4 7 8zm5 6h5v2h-5v-2z" />
				</svg>
			),
			labelContent: (
				<span class={s.launchLabel}>
					<span>{item.agentName ?? item.label}</span>
					<Show when={item.configName}>
						<span class={s.configName}>{item.configName}</span>
					</Show>
					<Show when={item.isDefault}>
						<span class={s.defaultMark} title="Default configuration" aria-hidden="true">
							✓
						</span>
					</Show>
				</span>
			),
		}))}
	/>
);
