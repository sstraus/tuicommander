import type { JSX } from "solid-js";
import { type Component, createEffect, For, onCleanup, Show } from "solid-js";
import { cx } from "../../utils";
import s from "./Dropdown.module.css";

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

		// Delay to avoid immediate close — use rAF instead of setTimeout
		// so cleanup can cancel the frame if the component unmounts before it fires
		let attached = false;
		const rafId = requestAnimationFrame(() => {
			document.addEventListener("click", handleClickOutside);
			attached = true;
		});

		onCleanup(() => {
			cancelAnimationFrame(rafId);
			if (attached) document.removeEventListener("click", handleClickOutside);
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
			<div ref={dropdownRef} class={cx(s.dropdown, props.position === "top" && s.top, props.class)} data-testid="dropdown" data-position={props.position || "bottom"}>
				<For each={props.items}>
					{(item) => (
						<Show when={!item.divider} fallback={<div class={s.divider} data-testid="dropdown-divider" />}>
							<div
								class={cx(s.item, item.id === props.selected && s.selected)}
								data-testid="dropdown-item"
								data-selected={item.id === props.selected || undefined}
								data-disabled={item.disabled || undefined}
								onClick={() => !item.disabled && props.onSelect(item.id)}
							>
								<Show when={item.icon}>
									<span class={s.itemIcon} data-testid="dropdown-item-icon">{item.icon}</span>
								</Show>
								<span class={s.itemLabel} data-testid="dropdown-item-label">{item.label}</span>
							</div>
						</Show>
					)}
				</For>
			</div>
		</Show>
	);
};
