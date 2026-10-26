import { createStore, reconcile } from "solid-js/store";
import { invoke } from "../invoke";
import {
	DEFAULT_NOTIFICATION_CONFIG,
	type NotificationConfig,
	type NotificationSound,
	notificationManager,
} from "../notifications";
import { showNativeNotice } from "../services/nativeNotifications";
import { reportSettingsConflicts, settingsRefresh } from "../services/settingsRefresh";
import { isTauri } from "../transport";
import { createConfigDeltaWriter } from "../utils/configDeltaWriter";
import { appLogger } from "./appLogger";
import { terminalsStore } from "./terminals";
import { setToastBellMirrorResolver } from "./toasts";

interface PlayOptions {
	terminalId?: string;
	/** MCP notice origin: apply session policy without adding a terminal OS notice. */
	originSessionId?: string;
	/** A repeat of an earlier notice: the OS notice also goes out in a focused window unless the user is looking at this terminal, and is off with notifications or this sound disabled. */
	reminder?: boolean;
}

const OS_NOTIFICATION_TITLES: Record<NotificationSound, string> = {
	question: "Agent needs input",
	error: "Error detected",
	completion: "Task completed",
	warning: "Warning",
	info: "Info",
	attention: "Agent needs you",
};

// Card routing is a per-client view preference, like editor wrapping.
const AUTO_TOAST_CARDS_KEY = "tui-commander-auto-toast-cards";

function loadAutoToastCards(): boolean {
	try {
		return localStorage.getItem(AUTO_TOAST_CARDS_KEY) !== "false";
	} catch {
		return true;
	}
}

const LEGACY_STORAGE_KEY = "tui-commander-notifications";
const notificationWriter = createConfigDeltaWriter<NotificationConfig>("save_notification_config");

/** Create a fresh copy of the default config */
function copyDefaults(): NotificationConfig {
	return {
		...DEFAULT_NOTIFICATION_CONFIG,
		sounds: { ...DEFAULT_NOTIFICATION_CONFIG.sounds },
		audio_device: DEFAULT_NOTIFICATION_CONFIG.audio_device,
	};
}

/** Persist config to Rust backend (fire-and-forget) */
function saveConfig(config: NotificationConfig): void {
	notificationWriter.save(config).catch((err) => appLogger.debug("config", "Failed to save notification config", err));
}

/** Notifications store state */
interface NotificationsState {
	autoToastCards: boolean;
	config: NotificationConfig;
	isAvailable: boolean;
	badgeCount: number;
}

/** Create notifications store */
function createNotificationsStore() {
	const defaults = copyDefaults();
	notificationManager.updateConfig(defaults);
	const notifiedAcpInteractions = new Set<string>();

	const [state, setState] = createStore<NotificationsState>({
		config: defaults,
		autoToastCards: loadAutoToastCards(),
		isAvailable: notificationManager.isAvailable(),
		badgeCount: 0,
	});

	const actions = {
		startRemoteSync(): () => void {
			return settingsRefresh.register({
				section: "notifications",
				read: () => invoke<NotificationConfig>("load_notification_config"),
				apply: (loaded) => {
					const merged = notificationWriter.adopt({ ...copyDefaults(), ...loaded }, state.config);
					setState("config", reconcile(merged.value));
					notificationManager.updateConfig(merged.value);
					reportSettingsConflicts(merged.conflicts);
				},
			});
		},
		/** Show a card when an MCP notice arrives from a terminal the user is not viewing. */
		setAutoToastCards(enabled: boolean): void {
			setState("autoToastCards", enabled);
			try {
				localStorage.setItem(AUTO_TOAST_CARDS_KEY, String(enabled));
			} catch (err) {
				appLogger.warn("config", "Failed to persist automatic toast cards", err);
			}
		},
		/** Send one native notice per pending ACP question; a notice still in flight is dropped once the question settles. */
		syncAcpAttention(interactions: { id: string; kind: "permission" | "elicitation" }[]): void {
			const pending = new Set(interactions.map((interaction) => interaction.id));
			for (const id of notifiedAcpInteractions) if (!pending.has(id)) notifiedAcpInteractions.delete(id);
			for (const interaction of interactions) {
				if (notifiedAcpInteractions.has(interaction.id)) continue;
				notifiedAcpInteractions.add(interaction.id);
				void showNativeNotice({
					title: "AI Chat needs input",
					body: interaction.kind === "permission" ? "Permission requested" : "Form requested",
					key: `acp:${interaction.id}`,
					target: { kind: "aichat", id: interaction.id },
					isCurrent: () => notifiedAcpInteractions.has(interaction.id),
				}).catch((error: unknown) => appLogger.warn("ai-chat", "Could not show ACP notification", error));
			}
		},
		/** Load config from Rust backend; migrate from localStorage on first run */
		async hydrate(): Promise<void> {
			try {
				const loaded = await invoke<NotificationConfig>("load_notification_config");
				notificationWriter.loaded(loaded ?? copyDefaults());
				// One-time migration from localStorage
				const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
				if (legacy) {
					try {
						const parsed = { ...copyDefaults(), ...JSON.parse(legacy) };
						await notificationWriter.save(parsed);
					} catch {
						/* ignore corrupt legacy data */
					}
					localStorage.removeItem(LEGACY_STORAGE_KEY);
				}

				const config = { ...copyDefaults(), ...loaded };
				setState("config", config);
				notificationManager.updateConfig(config);
			} catch (err) {
				appLogger.debug("config", "Failed to hydrate notification config", err);
			}
		},

		/** Enable or disable notifications */
		setEnabled(enabled: boolean): void {
			setState("config", "enabled", enabled);
			notificationManager.setEnabled(enabled);
			saveConfig(state.config);
		},

		/** Set volume (0.0 to 1.0) */
		setVolume(volume: number): void {
			const clampedVolume = Math.max(0, Math.min(1, volume));
			setState("config", "volume", clampedVolume);
			notificationManager.setVolume(clampedVolume);
			saveConfig(state.config);
		},

		/** Set the audio output device (null = system default) */
		setAudioDevice(device: string | null): void {
			setState("config", "audio_device", device);
			notificationManager.updateConfig({ audio_device: device });
			saveConfig(state.config);
		},

		/** One parentage source for every session-scoped sound and native notice.
		 * Backend events can name the tab, PTY, or stable TUIC peer identity. */
		isSessionSilenced(id?: string): boolean {
			if (!id || !state.config.silence_remote_completions) return false;
			const terminal =
				terminalsStore.get(id) ??
				Object.values(terminalsStore.state.terminals).find((term) => term.sessionId === id || term.tuicSession === id);
			return !!terminal?.parentSession;
		},

		/** Keep the existing persisted key and default for the expanded subagent policy. */
		setSilenceRemoteCompletions(silence: boolean): void {
			setState("config", "silence_remote_completions", silence);
			saveConfig(state.config);
		},

		/** Mirror toasts into the toolbar bell, or leave them transient.
		 *  Not forwarded to notificationManager: this is about the visual list,
		 *  not about audio. */
		setToastsInBell(mirror: boolean): void {
			setState("config", "toasts_in_bell", mirror);
			saveConfig(state.config);
		},

		/** OS notifications for PR transitions (ready, CI failed, changes requested, merged). */
		setPrNativeNotifications(enabled: boolean): void {
			setState("config", "pr_native_notifications", enabled);
			saveConfig(state.config);
		},

		/** Enable/disable a specific sound */
		setSoundEnabled(sound: NotificationSound, enabled: boolean): void {
			setState("config", "sounds", sound, enabled);
			notificationManager.setSoundEnabled(sound, enabled);
			saveConfig(state.config);
		},

		/** Play a notification sound; also increments dock badge and sends OS notification when window is not focused */
		async play(sound: NotificationSound, opts?: PlayOptions): Promise<void> {
			const caller =
				new Error().stack
					?.split("\n")
					.slice(1, 4)
					.map((l) => l.trim())
					.join(" <- ") ?? "unknown";
			appLogger.debug("app", `[Notification.Play] sound=${sound} focused=${document.hasFocus()} caller=${caller}`);
			if (actions.isSessionSilenced(opts?.terminalId ?? opts?.originSessionId)) return;
			await notificationManager.play(sound);
			const unfocused = !document.hasFocus();
			if (unfocused) actions.incrementBadge();
			if (opts?.terminalId && (unfocused || (opts.reminder && actions.isSoundEnabled(sound)))) {
				const terminalId = opts.terminalId;
				void import("./terminals")
					.then(({ terminalsStore }) => {
						const isViewed = () =>
							document.hasFocus() &&
							terminalsStore.state.activeId === terminalId &&
							!terminalsStore.isDetached(terminalId);
						if (actions.isSessionSilenced(terminalId) || (opts.reminder && isViewed())) return;
						return showNativeNotice({
							title: OS_NOTIFICATION_TITLES[sound],
							body: terminalsStore.get(terminalId)?.name ?? terminalId,
							key: `${opts.reminder ? "reminder:" : ""}${sound}:${terminalId}`,
							target: { kind: "terminal", id: terminalId },
							isCurrent: () => !actions.isSessionSilenced(terminalId) && (!opts.reminder || !isViewed()),
							...(opts.reminder ? { ignoreFocus: true } : {}),
						});
					})
					.catch((error: unknown) => appLogger.warn("app", "Could not show terminal notification", error));
			}
		},

		/** Play question notification */
		async playQuestion(terminalId?: string): Promise<void> {
			await actions.play("question", { terminalId });
		},

		/** Repeat the question notification for a question left unanswered */
		async playQuestionReminder(terminalId: string): Promise<void> {
			await actions.play("question", { terminalId, reminder: true });
		},

		/** Play error notification */
		async playError(terminalId?: string): Promise<void> {
			await actions.play("error", { terminalId });
		},

		/** Play completion notification */
		async playCompletion(terminalId?: string): Promise<void> {
			await actions.play("completion", { terminalId });
		},

		/** Play warning notification */
		async playWarning(terminalId?: string): Promise<void> {
			await actions.play("warning", { terminalId });
		},

		/** Play info notification */
		async playInfo(terminalId?: string): Promise<void> {
			await actions.play("info", { terminalId });
		},

		/** Test a notification sound — explicit user action, so it bypasses the
		 *  enabled / per-sound / rate-limit gates and always plays at the current volume */
		async testSound(sound: NotificationSound): Promise<void> {
			await notificationManager.play(sound, { force: true });
		},

		/** Increment badge count on the app dock icon */
		async incrementBadge(): Promise<void> {
			const newCount = state.badgeCount + 1;
			setState("badgeCount", newCount);
			try {
				if (isTauri()) {
					const { getCurrentWindow } = await import("@tauri-apps/api/window");
					await getCurrentWindow().setBadgeCount(newCount);
				} else if ("setAppBadge" in navigator) {
					await (navigator as Navigator & { setAppBadge: (n: number) => Promise<void> }).setAppBadge(newCount);
				}
			} catch (err) {
				appLogger.debug("app", "Badge API unavailable or failed", err);
			}
		},

		/** Clear badge count from the app dock icon */
		async clearBadge(): Promise<void> {
			if (state.badgeCount === 0) return;
			setState("badgeCount", 0);
			try {
				if (isTauri()) {
					const { getCurrentWindow } = await import("@tauri-apps/api/window");
					await getCurrentWindow().setBadgeCount();
				} else if ("clearAppBadge" in navigator) {
					await (navigator as Navigator & { clearAppBadge: () => Promise<void> }).clearAppBadge();
				}
			} catch (err) {
				appLogger.debug("app", "Badge API unavailable or failed", err);
			}
		},

		/** Reset to defaults */
		reset(): void {
			actions.setAutoToastCards(true);
			const defaults = copyDefaults();
			setState("config", defaults);
			notificationManager.updateConfig(defaults);
			saveConfig(defaults);
		},

		/** Check if notifications are enabled */
		isEnabled(): boolean {
			return state.config.enabled;
		},

		/** Check if a specific sound is enabled */
		isSoundEnabled(sound: NotificationSound): boolean {
			return state.config.enabled && state.config.sounds[sound];
		},
	};

	return { state, ...actions };
}

export const notificationsStore = createNotificationsStore();

setToastBellMirrorResolver(() => notificationsStore.state.config.toasts_in_bell);
