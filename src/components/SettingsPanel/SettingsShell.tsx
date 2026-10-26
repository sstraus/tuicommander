import { type Component, For, type JSX, Show } from "solid-js";
import { cx } from "../../utils";
import { Button } from "../ui/Button";
import { Select } from "../ui/Select";
import s from "./Settings.module.css";

const NAV_MIN_WIDTH = 140;
const NAV_MAX_WIDTH = 280;
const NAV_DEFAULT_WIDTH = 232;

export interface SettingsShellTab {
	key: string;
	label: string;
	/** Optional visual identity; accessible name remains the tab label. */
	icon?: JSX.Element;
	color?: string;
	/** Loaded plugin id, separate from the human-readable page label. */
	source?: string;
}

export interface SettingsShellProps {
	visible: boolean;
	onClose: () => void;
	title: string;
	subtitle?: string;
	icon?: string;
	tabs: SettingsShellTab[];
	activeTab: string;
	onTabChange: (key: string) => void;
	/** Rendered in the header, left of the close button — the Expert switch lives here */
	headerActions?: JSX.Element;
	/** Rendered above the nav items — the Settings search box lives here */
	navHeader?: JSX.Element;
	/** Width of the nav sidebar in px (persisted externally) */
	navWidth?: number;
	/** Called on every pixel during nav resize drag (update state only) */
	onNavWidthChange?: (width: number) => void;
	/** Called once on mouseup after nav resize drag (persist to disk) */
	onNavWidthPersist?: () => void;
	footer?: JSX.Element;
	children: JSX.Element;
}

/** Shared shell for settings panels: overlay → panel → header → (nav | content) */
export const SettingsShell: Component<SettingsShellProps> = (props) => {
	const hasRepoHeader = () => !!props.icon || !!props.subtitle;
	const navWidth = () => props.navWidth ?? NAV_DEFAULT_WIDTH;

	const handleNavResizeStart = (e: MouseEvent) => {
		e.preventDefault();
		const startX = e.clientX;
		const startWidth = navWidth();

		const onMove = (ev: MouseEvent) => {
			const delta = ev.clientX - startX;
			const clamped = Math.min(NAV_MAX_WIDTH, Math.max(NAV_MIN_WIDTH, startWidth + delta));
			props.onNavWidthChange?.(clamped);
		};

		const onUp = () => {
			document.removeEventListener("mousemove", onMove);
			document.removeEventListener("mouseup", onUp);
			document.body.style.cursor = "";
			document.body.style.userSelect = "";
			props.onNavWidthPersist?.();
		};

		document.addEventListener("mousemove", onMove);
		document.addEventListener("mouseup", onUp);
		document.body.style.cursor = "col-resize";
		document.body.style.userSelect = "none";
	};

	return (
		<Show when={props.visible}>
			<div class={s.overlay} onClick={props.onClose}>
				<div class={s.panel} onClick={(e) => e.stopPropagation()}>
					{/* Header */}
					<div class={cx(s.header, hasRepoHeader() && s.headerRepo)}>
						<Show when={hasRepoHeader()} fallback={<h2>{props.title}</h2>}>
							<div class={s.titleRepo}>
								<Show when={props.icon}>
									<span class={s.iconRepo}>{props.icon}</span>
								</Show>
								<div>
									<h2>{props.title}</h2>
									<Show when={props.subtitle}>
										<p class={s.pathRepo}>{props.subtitle}</p>
									</Show>
								</div>
							</div>
						</Show>
						<div class={s.headerActions}>
							{props.headerActions}
							<Button
								variant="ghost"
								data-non-setting="Navigate or close Settings without editing configuration"
								class={s.close}
								onClick={props.onClose}
							>
								&times;
							</Button>
						</div>
					</div>

					{/* Body: nav sidebar + scrollable content */}
					<div class={s.body}>
						<nav class={s.nav} style={{ width: `${navWidth()}px` }}>
							<div class={s.compactNav}>
								<Select
									data-non-setting="Navigate between Settings pages"
									aria-label="Settings page"
									value={props.activeTab}
									onChange={(event) => props.onTabChange(event.currentTarget.value)}
								>
									<For each={props.tabs.filter((tab) => tab.key !== "__sep__" && !tab.key.startsWith("__label__:"))}>
										{(tab) => (
											<option value={tab.key} selected={props.activeTab === tab.key}>
												{tab.label}
											</option>
										)}
									</For>
								</Select>
							</div>
							<div class={s.navSearch}>{props.navHeader}</div>
							<For each={props.tabs}>
								{(tab) =>
									tab.key === "__sep__" ? (
										<div class={s.navSeparator} />
									) : tab.key.startsWith("__label__:") ? (
										<div class={s.navLabel}>
											<Show when={tab.icon}>
												<span aria-hidden="true">{tab.icon}</span>{" "}
											</Show>
											{tab.label}
										</div>
									) : (
										<button
											data-control-layout="row"
											data-non-setting="Navigate or close Settings without editing configuration"
											class={cx(
												s.navItem,
												tab.source && s.navItemPlugin,
												tab.key.startsWith("repo:") && s.navItemRepo,
												props.activeTab === tab.key && s.active,
											)}
											title={tab.source ? `Plugin: ${tab.source}` : undefined}
											style={tab.color ? { color: tab.color } : undefined}
											onClick={() => props.onTabChange(tab.key)}
										>
											<Show when={tab.icon}>
												<span aria-hidden="true">{tab.icon}</span>{" "}
											</Show>
											<Show when={tab.source} fallback={tab.label}>
												<span class={s.pluginLabel}>{tab.label}</span>
												<span class={s.pluginIdentity}> Plugin</span>
											</Show>
										</button>
									)
								}
							</For>
							<div class={s.navResizeHandle} onMouseDown={handleNavResizeStart} />
						</nav>

						{/* Content — `data-settings-content` scopes the search's scroll-to-section lookup */}
						<div class={s.content} data-settings-content>
							{props.children}
						</div>
					</div>

					{/* Footer (optional) */}
					{props.footer}
				</div>
			</div>
		</Show>
	);
};
