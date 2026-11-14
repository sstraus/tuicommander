import type { JSX } from "solid-js";
import { type Component, createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { cx } from "../../utils";
import s from "./Dropdown.module.css";
import { Input } from "./Input";

export interface DropdownItem {
	id: string;
	label: string;
	icon?: JSX.Element;
	divider?: boolean;
	disabled?: boolean;
}

export interface DropdownProps {
	items: DropdownItem[];
	/** Enables an in-menu text filter with this accessible label. */
	searchLabel?: string;
	selected?: string;
	visible: boolean;
	onSelect: (id: string) => void;
	onClose: () => void;
	position?: "top" | "bottom";
	class?: string;
	/** Portals a viewport-clamped menu beside this trigger. */
	anchor?: () => DOMRect | undefined;
	/** Names the menu for assistive technology. */
	"aria-label"?: string;
}

export const Dropdown: Component<DropdownProps> = (props) => {
	let dropdownRef: HTMLDivElement | undefined;
	let searchRef: HTMLInputElement | undefined;
	const [query, setQuery] = createSignal("");
	const filteredItems = () =>
		props.items.filter((item) => !props.searchLabel || item.label.toLowerCase().includes(query().toLowerCase()));
	const [placement, setPlacement] = createSignal<JSX.CSSProperties>();
	createEffect(() => {
		if (!props.visible || !props.anchor) return;
		const place = () => {
			const anchor = props.anchor?.();
			if (!anchor || !dropdownRef) return;
			const viewport = window.visualViewport;
			const left = viewport?.offsetLeft ?? 0;
			const top = viewport?.offsetTop ?? 0;
			const width = viewport?.width ?? window.innerWidth;
			const height = viewport?.height ?? window.innerHeight;
			const menu = dropdownRef.getBoundingClientRect();
			const maxHeight = Math.max(0, Math.min(height - 16, props.searchLabel ? 360 : height - 16));
			const menuHeight = Math.min(menu.height, maxHeight);
			const desiredTop = props.position === "top" ? anchor.top - menuHeight - 4 : anchor.bottom + 4;
			setPlacement({
				position: "fixed",
				"min-width": "0",
				right: "auto",
				bottom: "auto",
				left: `${Math.max(left + 8, Math.min(anchor.right - menu.width, left + width - menu.width - 8))}px`,
				top: `${Math.max(top + 8, Math.min(desiredTop, top + height - menuHeight - 8))}px`,
				"max-width": `${Math.max(0, width - 16)}px`,
				"max-height": `${maxHeight}px`,
				"overflow-y": "auto",
			});
		};
		queueMicrotask(place);
		window.addEventListener("resize", place);
		window.addEventListener("scroll", place, true);
		window.visualViewport?.addEventListener("resize", place);
		onCleanup(() => {
			window.removeEventListener("resize", place);
			window.removeEventListener("scroll", place, true);
			window.visualViewport?.removeEventListener("resize", place);
		});
	});

	const enabledItems = () =>
		dropdownRef ? [...dropdownRef.querySelectorAll<HTMLElement>('[role="menuitem"]:not([aria-disabled="true"])')] : [];

	// Focus moves into the menu when it opens, so keys act on it.
	createEffect(() => {
		if (props.visible) {
			setQuery("");
			queueMicrotask(() => (searchRef ?? enabledItems()[0])?.focus());
		}
	});

	const handleKeyDown = (e: KeyboardEvent) => {
		if (e.key === "Escape") {
			// Handled here so hosts that stop work on Escape never see it.
			e.preventDefault();
			e.stopPropagation();
			props.onClose();
			return;
		}
		const items = enabledItems();
		if (!items.length) return;
		const current = items.indexOf(document.activeElement as HTMLElement);
		const target =
			e.key === "ArrowDown"
				? items[(current + 1) % items.length]
				: e.key === "ArrowUp"
					? items[(current <= 0 ? items.length : current) - 1]
					: e.key === "Home"
						? items[0]
						: e.key === "End"
							? items.at(-1)
							: undefined;
		if (target) {
			e.preventDefault();
			target.focus();
		} else if ((e.key === "Enter" || e.key === " ") && current >= 0) {
			e.preventDefault();
			items[current].click();
		}
	};

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

	const menu = () => (
		<Show when={props.visible}>
			<div
				ref={dropdownRef}
				style={props.anchor ? placement() : undefined}
				class={cx(s.dropdown, props.position === "top" && s.top, props.class)}
				role="menu"
				aria-label={props["aria-label"]}
				onKeyDown={handleKeyDown}
				data-testid="dropdown"
				data-position={props.position || "bottom"}
			>
				<Show when={props.searchLabel}>
					<div class={s.search}>
						<Input
							ref={searchRef}
							aria-label={props.searchLabel}
							value={query()}
							onInput={(event) => setQuery(event.currentTarget.value)}
						/>
					</div>
				</Show>
				<For each={filteredItems()}>
					{(item) => (
						<Show when={!item.divider} fallback={<div class={s.divider} data-testid="dropdown-divider" />}>
							<div
								class={cx(s.item, item.id === props.selected && s.selected)}
								role="menuitem"
								tabindex="-1"
								aria-disabled={item.disabled || undefined}
								data-testid="dropdown-item"
								data-selected={item.id === props.selected || undefined}
								data-disabled={item.disabled || undefined}
								onClick={() => !item.disabled && props.onSelect(item.id)}
							>
								<Show when={item.icon}>
									<span class={s.itemIcon} data-testid="dropdown-item-icon">
										{item.icon}
									</span>
								</Show>
								<span class={s.itemLabel} data-testid="dropdown-item-label">
									{item.label}
								</span>
							</div>
						</Show>
					)}
				</For>
			</div>
		</Show>
	);
	return (
		<Show when={props.anchor} fallback={menu()}>
			<Portal>{menu()}</Portal>
		</Show>
	);
};
