import { type Component, createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import { isMacOS } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import type { CustomLauncher, IdeType } from "../../stores/settings";
import { IDE_CATEGORIES, IDE_ICON_PATHS, IDE_NAMES, settingsStore } from "../../stores/settings";
import { isTauri } from "../../transport";

/** Code editors that can open individual files (as opposed to terminals, git clients, etc.) */
const FILE_CAPABLE_IDES = new Set<string>([...IDE_CATEGORIES.editors, ...IDE_CATEGORIES.jetbrains]);

import { useRepository } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { keyFor } from "../../utils/hotkey";
import s from "./IdeLauncher.module.css";

export interface IdeLauncherProps {
	repoPath?: string;
	/** Absolute path of the focused file (editor/MD tab). Code editors open this file; other apps open repoPath. */
	focusedFilePath?: string;
	runCommand?: string;
	onOpenInIde?: (ide: IdeType) => void;
	onRun?: (shiftKey: boolean) => void;
}

/** IDE icon component - renders the SVG icon at specified size */
const IdeIcon: Component<{ ide: IdeType; size?: number }> = (props) => {
	const size = () => props.size ?? 14;
	return (
		<img class={s.icon} src={IDE_ICON_PATHS[props.ide]} width={size()} height={size()} alt={IDE_NAMES[props.ide]} />
	);
};

/** Generic monochrome icon shared by all user-defined custom launchers. */
const CustomToolIcon: Component<{ size?: number }> = (props) => (
	<svg
		class={s.icon}
		width={props.size ?? 14}
		height={props.size ?? 14}
		viewBox="0 0 16 16"
		fill="currentColor"
		aria-hidden="true"
	>
		<path d="M10.5 1a3.5 3.5 0 0 0-3.3 4.66L1.5 11.3a1.7 1.7 0 0 0 2.4 2.4l5.64-5.7A3.5 3.5 0 1 0 10.5 1zm0 1.5a2 2 0 0 1 .9 3.79l-.5.25.06.56a2 2 0 0 1-2.6 2.1l-.5-.16-4.7 4.74a.2.2 0 1 1-.28-.28l4.74-4.7-.16-.5a2 2 0 0 1 2.1-2.6l.56.06.25-.5A2 2 0 0 1 10.5 2.5z" />
	</svg>
);

/** Current OS as a custom-launcher platform tag. */
const osPlatform = (): NonNullable<CustomLauncher["platform"]> =>
	isMacOS() ? "macos" : navigator.userAgent.includes("Win") ? "windows" : "linux";

export const IdeLauncher: Component<IdeLauncherProps> = (props) => {
	const [isOpen, setIsOpen] = createSignal(false);
	const [installedIdes, setInstalledIdes] = createSignal<string[]>([]);
	const repo = useRepository();

	// Detect installed IDEs on mount
	onMount(async () => {
		try {
			const installed = await invoke<string[]>("detect_installed_ides");
			setInstalledIdes(installed);
		} catch (err) {
			appLogger.error("app", "Failed to detect installed IDEs", err);
			setInstalledIdes(["terminal", "finder"]);
		}
	});

	const categoryOrder = [
		{ key: "editors", label: t("ideLauncher.codeEditors", "Code Editors") },
		{ key: "jetbrains", label: t("ideLauncher.jetbrains", "JetBrains") },
		{ key: "terminals", label: t("ideLauncher.terminals", "Terminals") },
		{ key: "git", label: t("ideLauncher.gitTools", "Git Tools") },
		{ key: "utilities", label: t("ideLauncher.system", "System") },
	];

	// Filter IDE list to only installed ones
	const filterInstalled = (ides: IdeType[]): IdeType[] => {
		return ides.filter((ide) => installedIdes().includes(ide));
	};

	let dropdownRef: HTMLDivElement | undefined;

	// Close dropdown on outside click
	createEffect(() => {
		if (!isOpen()) return;

		const handleClickOutside = (e: MouseEvent) => {
			if (dropdownRef && !dropdownRef.contains(e.target as Node)) {
				setIsOpen(false);
			}
		};

		document.addEventListener("mousedown", handleClickOutside);
		onCleanup(() => document.removeEventListener("mousedown", handleClickOutside));
	});

	// Handle keyboard
	createEffect(() => {
		if (!isOpen()) return;

		const handleKeydown = (e: KeyboardEvent) => {
			if (e.key === "Escape") {
				setIsOpen(false);
			}
		};

		document.addEventListener("keydown", handleKeydown);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown));
	});

	/** Resolve the path to open: focused file for code editors, repo root for everything else */
	const launchPathFor = (ide: IdeType): string | undefined => {
		if (props.focusedFilePath && FILE_CAPABLE_IDES.has(ide)) {
			return props.focusedFilePath;
		}
		return props.repoPath;
	};

	const handleOpenIn = async (ide: IdeType) => {
		const target = launchPathFor(ide);
		if (!target) return;

		settingsStore.setIde(ide);
		setIsOpen(false);

		try {
			await repo.openInApp(target, ide);
			props.onOpenInIde?.(ide);
		} catch (err) {
			appLogger.error("app", "Failed to open in IDE", err);
		}
	};

	const handleLaunchCurrent = async () => {
		const target = launchPathFor(currentIde());
		if (!target) return;

		try {
			await repo.openInApp(target, currentIde());
			props.onOpenInIde?.(currentIde());
		} catch (err) {
			appLogger.error("app", "Failed to open in IDE", err);
		}
	};

	/** Enabled custom launchers applicable to the current OS. */
	const customLaunchers = (): CustomLauncher[] =>
		settingsStore.state.customLaunchers.filter((l) => l.enabled && (!l.platform || l.platform === osPlatform()));

	const handleOpenCustom = async (launcher: CustomLauncher) => {
		const target = props.focusedFilePath ?? props.repoPath;
		if (!target) return;
		setIsOpen(false);
		try {
			await invoke("open_in_custom", {
				path: target,
				executable: launcher.executable,
				args: launcher.args,
				line: null,
				col: null,
			});
		} catch (err) {
			appLogger.error("app", "Failed to open in custom launcher", err);
		}
	};

	const handleRun = (e: MouseEvent) => {
		setIsOpen(false);
		props.onRun?.(e.shiftKey);
	};

	const runLabel = () => {
		const cmd = props.runCommand;
		if (!cmd) return t("ideLauncher.run", "Run...");
		const maxLen = 20;
		return cmd.length > maxLen ? `Run: ${cmd.slice(0, maxLen)}...` : `Run: ${cmd}`;
	};

	const currentIde = () => settingsStore.state.ide;

	// IDE launcher requires native Tauri APIs — hide in browser mode
	if (!isTauri()) return null;

	return (
		<div class={s.launcher} ref={dropdownRef}>
			<div class={s.split}>
				{/* Main button - launches current IDE */}
				<button
					class={cx(s.btn, s.main)}
					onClick={handleLaunchCurrent}
					disabled={!props.repoPath}
					title={`Open in ${IDE_NAMES[currentIde()]}`}
				>
					<IdeIcon ide={currentIde()} />
					<span class={s.name}>{IDE_NAMES[currentIde()]}</span>
				</button>
				{/* Arrow button - opens dropdown */}
				<button
					class={cx(s.btn, s.arrowBtn)}
					onClick={() => setIsOpen(!isOpen())}
					title={t("ideLauncher.chooseEditor", "Choose editor")}
				>
					<span class={s.arrow}>{isOpen() ? "▲" : "▼"}</span>
				</button>
			</div>

			<Show when={isOpen()}>
				<div class={s.dropdown}>
					<For each={categoryOrder}>
						{(cat) => {
							const items = () => filterInstalled(IDE_CATEGORIES[cat.key]);
							return (
								<Show when={items().length > 0}>
									<Show when={cat.key !== "editors"}>
										<div class={s.divider} />
									</Show>
									<div class={s.section}>
										<div class={s.sectionTitle}>{cat.label}</div>
										<For each={items()}>
											{(ide) => (
												<button
													class={cx(s.item, currentIde() === ide && s.selected)}
													onClick={() => handleOpenIn(ide)}
													disabled={!props.repoPath}
												>
													<IdeIcon ide={ide} />
													<span class={s.itemName}>{IDE_NAMES[ide]}</span>
													<Show when={currentIde() === ide}>
														<span class={s.itemCheck}>✓</span>
													</Show>
												</button>
											)}
										</For>
									</div>
								</Show>
							);
						}}
					</For>

					{/* User-defined custom launchers */}
					<Show when={customLaunchers().length > 0}>
						<div class={s.divider} />
						<div class={s.section}>
							<div class={s.sectionTitle}>{t("ideLauncher.custom", "Custom")}</div>
							<For each={customLaunchers()}>
								{(launcher) => (
									<button class={s.item} onClick={() => handleOpenCustom(launcher)} disabled={!props.repoPath}>
										<CustomToolIcon />
										<span class={s.itemName}>{launcher.name}</span>
									</button>
								)}
							</For>
						</div>
					</Show>

					{/* Actions */}
					<div class={s.divider} />
					<div class={s.section}>
						<button class={cx(s.item, s.action)} onClick={handleRun} disabled={!props.repoPath}>
							<span class={cx(s.icon, s.iconEmoji)}>▶</span>
							<span class={s.itemName}>{runLabel()}</span>
							<span class={s.shortcut}>{keyFor("run-command")}</span>
						</button>
					</div>
				</div>
			</Show>
		</div>
	);
};

export default IdeLauncher;
