import { createSignal, For, type JSX, Show } from "solid-js";
import { Button } from "../ui/Button";
import { Dropdown } from "../ui/Dropdown";
import s from "./PanelToolbar.module.css";

export interface PanelToolbarAction {
	id: string;
	label: string;
	icon: JSX.Element;
	onClick: () => void;
	disabled?: boolean;
	tooltip?: string;
}

export interface PanelToolbarMenuItem {
	id: string;
	label: string;
	onSelect: () => void;
	disabled?: boolean;
	/** Draws a separator line before the item (the Dropdown renders a `divider` item as a bare line). */
	divider?: boolean;
}

/** UI v3 fixed toolbar: icon + label buttons that never change, then a `⋯` menu for rare actions. */
export function PanelToolbar(props: {
	actions: PanelToolbarAction[];
	menu: PanelToolbarMenuItem[];
	menuLabel: string;
}) {
	const [open, setOpen] = createSignal(false);
	let trigger: HTMLButtonElement | undefined;
	return (
		<div class={s.toolbar} data-testid="panel-toolbar">
			<For each={props.actions}>
				{(action) => (
					<Button
						size="compact"
						variant="ghost"
						class={s.action}
						disabled={action.disabled}
						onClick={action.onClick}
						data-tooltip={action.tooltip}
						data-tooltip-pos="bottom"
					>
						<span class={s.icon}>{action.icon}</span>
						{action.label}
					</Button>
				)}
			</For>
			<Show when={props.menu.length > 0}>
				<Button
					ref={trigger}
					size="compact"
					variant="ghost"
					shape="icon"
					class={s.more}
					aria-label={props.menuLabel}
					aria-haspopup="menu"
					aria-expanded={open()}
					onClick={() => setOpen((value) => !value)}
				>
					<MoreIcon />
				</Button>
				<Dropdown
					visible={open()}
					aria-label={props.menuLabel}
					anchor={() => trigger?.getBoundingClientRect()}
					items={props.menu.flatMap(({ id, label, disabled, divider }, index) => [
						...(divider && index > 0 ? [{ id: `${id}-divider`, label: "", divider: true }] : []),
						{ id, label, disabled },
					])}
					onSelect={(id) => {
						setOpen(false);
						props.menu.find((item) => item.id === id)?.onSelect();
					}}
					onClose={() => setOpen(false)}
				/>
			</Show>
		</div>
	);
}

const MoreIcon = () => (
	<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
		<path d="M8 9a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3ZM1.5 9a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Zm13 0a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z" />
	</svg>
);

/** Toolbar glyphs shared by the PR and issue panels (monochrome, currentColor). */
export const ToolbarIcons = {
	diff: () => (
		<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
			<path d="M8.75 1.75V5H12a.75.75 0 0 1 0 1.5H8.75v3.25a.75.75 0 0 1-1.5 0V6.5H4A.75.75 0 0 1 4 5h3.25V1.75a.75.75 0 0 1 1.5 0ZM4 13h8a.75.75 0 0 1 0 1.5H4A.75.75 0 0 1 4 13Z" />
		</svg>
	),
	checkout: () => (
		<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
			<path d="M2.75 14A1.75 1.75 0 0 1 1 12.25v-2.5a.75.75 0 0 1 1.5 0v2.5c0 .14.11.25.25.25h10.5a.25.25 0 0 0 .25-.25v-2.5a.75.75 0 0 1 1.5 0v2.5A1.75 1.75 0 0 1 13.25 14ZM7.25 7.69V1.75a.75.75 0 0 1 1.5 0v5.94l1.97-1.97a.75.75 0 1 1 1.06 1.06l-3.25 3.25a.75.75 0 0 1-1.06 0L4.22 6.78a.75.75 0 0 1 1.06-1.06Z" />
		</svg>
	),
	external: () => (
		<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
			<path d="M3.75 2h3.5a.75.75 0 0 1 0 1.5h-3.5a.25.25 0 0 0-.25.25v8.5c0 .14.11.25.25.25h8.5a.25.25 0 0 0 .25-.25v-3.5a.75.75 0 0 1 1.5 0v3.5A1.75 1.75 0 0 1 12.25 14h-8.5A1.75 1.75 0 0 1 2 12.25v-8.5C2 2.78 2.78 2 3.75 2Zm6.85-.5h3.65a.25.25 0 0 1 .25.25V5.4a.25.25 0 0 1-.43.18l-1.24-1.25-3.23 3.23a.75.75 0 1 1-1.06-1.06l3.23-3.23-1.25-1.24a.25.25 0 0 1 .18-.43Z" />
		</svg>
	),
	copy: () => (
		<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
			<path d="M0 6.75C0 5.78.78 5 1.75 5h1.5a.75.75 0 0 1 0 1.5h-1.5a.25.25 0 0 0-.25.25v7.5c0 .14.11.25.25.25h7.5a.25.25 0 0 0 .25-.25v-1.5a.75.75 0 0 1 1.5 0v1.5A1.75 1.75 0 0 1 9.25 16h-7.5A1.75 1.75 0 0 1 0 14.25ZM5 1.75C5 .78 5.78 0 6.75 0h7.5C15.22 0 16 .78 16 1.75v7.5A1.75 1.75 0 0 1 14.25 11h-7.5A1.75 1.75 0 0 1 5 9.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .14.11.25.25.25h7.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z" />
		</svg>
	),
	tasks: () => (
		<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
			<path d="M2.5 1.75v11.5c0 .14.11.25.25.25h10.5a.25.25 0 0 0 .25-.25V1.75a.25.25 0 0 0-.25-.25H2.75a.25.25 0 0 0-.25.25ZM2.75 0h10.5C14.22 0 15 .78 15 1.75v11.5A1.75 1.75 0 0 1 13.25 15H2.75A1.75 1.75 0 0 1 1 13.25V1.75C1 .78 1.78 0 2.75 0ZM5 4.75A.75.75 0 0 1 5.75 4h4.5a.75.75 0 0 1 0 1.5h-4.5A.75.75 0 0 1 5 4.75Zm0 3A.75.75 0 0 1 5.75 7h4.5a.75.75 0 0 1 0 1.5h-4.5A.75.75 0 0 1 5 7.75Z" />
		</svg>
	),
	comment: () => (
		<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
			<path d="M1 2.75C1 1.78 1.78 1 2.75 1h10.5c.97 0 1.75.78 1.75 1.75v7.5A1.75 1.75 0 0 1 13.25 12H9.06l-2.57 2.57A.75.75 0 0 1 5.25 14v-2h-2.5A1.75 1.75 0 0 1 1 10.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .14.11.25.25.25h3.25a.75.75 0 0 1 .75.75v1.19l1.72-1.72a.75.75 0 0 1 .53-.22h4.25a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z" />
		</svg>
	),
	person: () => (
		<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
			<path d="M10.56 8.83a4 4 0 1 0-5.12 0A6.01 6.01 0 0 0 2 14.25a.75.75 0 0 0 1.5 0 4.5 4.5 0 0 1 9 0 .75.75 0 0 0 1.5 0 6.01 6.01 0 0 0-3.44-5.42ZM5.5 5.75a2.5 2.5 0 1 1 5 0 2.5 2.5 0 0 1-5 0Z" />
		</svg>
	),
	reply: () => (
		<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
			<path d="M6.78 1.97a.75.75 0 0 1 0 1.06L3.81 6h6.44A4.75 4.75 0 0 1 15 10.75v2.5a.75.75 0 0 1-1.5 0v-2.5a3.25 3.25 0 0 0-3.25-3.25H3.81l2.97 2.97a.75.75 0 1 1-1.06 1.06L1.47 7.28a.75.75 0 0 1 0-1.06l4.25-4.25a.75.75 0 0 1 1.06 0Z" />
		</svg>
	),
	check: () => (
		<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
			<path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 0 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0Z" />
		</svg>
	),
};
