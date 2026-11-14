import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { createStore } from "solid-js/store";
import { isTauri, rpc } from "../transport";
import { installUpdate } from "../utils/installUpdate";
import { compareAppVersion } from "../utils/semver";
import { clearUpdateRestartMark, markUpdateRestart } from "../utils/updateRestart";
import { appLogger } from "./appLogger";
import { settingsStore } from "./settings";
import { terminalsStore } from "./terminals";

/** How often a deferred relaunch re-checks whether every agent is idle. */
const IDLE_POLL_MS = 2_000;

/** Sessions an immediate relaunch would interrupt: busy, or waiting on the user. */
function countBusySessions(): number {
	return terminalsStore.getIds().filter((id) => terminalsStore.isBusy(id) || !!terminalsStore.get(id)?.awaitingInput)
		.length;
}

/** True if remote > current by app-version precedence: 2.0.0 is newer than
 *  2.0.0-beta.1, and a nightly is newer than the release it was built after. */
function isNewerVersion(remote: string, current: string): boolean {
	return compareAppVersion(remote, current) > 0;
}

/** Typed result from the Rust `check_update_channel` command. */
interface UpdateChannelResult {
	available: boolean;
	version: string | null;
	notes: string | null;
	release_page: string | null;
	not_found: boolean;
}

interface UpdaterState {
	available: boolean;
	checking: boolean;
	downloading: boolean;
	progress: number;
	version: string | null;
	body: string | null;
	error: string | null;
	/** Informational message when no release exists for the channel (not an error) */
	noRelease: boolean;
	/** Why the built-in updater cannot run from this binary at all (not an error) */
	unsupported: string | null;
	/** For non-stable channels, a URL to the release page for manual download */
	downloadUrl: string | null;
	/** Update installed; the relaunch waits for busy sessions to finish */
	restartPending: boolean;
}

/** Sentinel to distinguish "check() timed out" from "no update available". */
const TIMEOUT_SENTINEL = Symbol("timeout");

/** Tauri resolves the running binary through `current_exe()` and refuses a path
 *  containing a symlink, because it cannot prove what it would be replacing.
 *  That is not a failure the user can act on: it is a property of where the
 *  binary sits. A `make dev` build under an mbx target view
 *  (`src-tauri/target` -> `~/Gits/.mbx/targets/...`) hits it every time, and so
 *  would a release build installed under a symlinked path. Report it as a
 *  condition, never as a red "Update failed". */
function unsupportedBinaryPathReason(message: string): string | null {
	if (!/current_exe\(\)/i.test(message) || !/symlink/i.test(message)) return null;
	return "In-app updates are unavailable because this binary runs from a path containing a symlink. Download the update manually.";
}

/** Run Tauri's built-in stable update check with a 10-second timeout. */
function checkStableWithTimeout(): Promise<Update | null | typeof TIMEOUT_SENTINEL> {
	return new Promise((resolve, reject) => {
		const timer = setTimeout(() => resolve(TIMEOUT_SENTINEL), 10_000);
		check().then(
			(v) => {
				clearTimeout(timer);
				resolve(v);
			},
			(e) => {
				clearTimeout(timer);
				reject(e);
			},
		);
	});
}

function createUpdaterStore() {
	const [state, setState] = createStore<UpdaterState>({
		available: false,
		checking: false,
		downloading: false,
		progress: 0,
		version: null,
		body: null,
		error: null,
		noRelease: false,
		unsupported: null,
		downloadUrl: null,
		restartPending: false,
	});

	let pendingUpdate: Update | null = null;
	let idleTimer: ReturnType<typeof setInterval> | null = null;

	function stopIdleWatch(): void {
		if (idleTimer !== null) clearInterval(idleTimer);
		idleTimer = null;
	}

	async function relaunchNow(): Promise<void> {
		stopIdleWatch();
		markUpdateRestart();
		try {
			await relaunch();
		} catch (err) {
			clearUpdateRestartMark();
			const message = err instanceof Error ? err.message : String(err);
			appLogger.error("app", "Update relaunch failed", message);
			setState({ error: message, downloading: false, restartPending: false });
		}
	}

	const actions = {
		async checkForUpdate(): Promise<void> {
			if (!isTauri()) return;
			if (state.checking || state.downloading) return;
			setState({ checking: true, error: null, noRelease: false, unsupported: null });

			const channel = settingsStore.state.updateChannel;

			try {
				if (channel === "stable") {
					// Stable: use Tauri's built-in updater (supports downloadAndInstall)
					const update = await checkStableWithTimeout();
					if (update === TIMEOUT_SENTINEL) {
						pendingUpdate = null;
						setState({
							available: false,
							version: null,
							body: null,
							downloadUrl: null,
							error: "Update check timed out",
						});
					} else if (update) {
						const currentVersion = await getVersion();
						if (!isNewerVersion(update.version, currentVersion)) {
							appLogger.debug("app", `Ignoring update ${update.version} — not newer than ${currentVersion}`);
							pendingUpdate = null;
							setState({ available: false, version: null, body: null, downloadUrl: null });
						} else {
							pendingUpdate = update;
							setState({
								available: true,
								version: update.version,
								body: update.body ?? null,
								downloadUrl: null,
							});
						}
					} else {
						pendingUpdate = null;
						setState({ available: false, version: null, body: null, downloadUrl: null });
					}
				} else {
					// Prerelease channels check stable (Tauri) and their feed (Rust)
					// in parallel. Beta picks the newer version; nightly keeps stable-first behavior.
					const currentVersion = await getVersion();

					const [stableSettled, nightlySettled] = await Promise.allSettled([
						checkStableWithTimeout(),
						rpc<UpdateChannelResult>("check_update_channel", { channel }),
					]);

					// Extract results, logging individual failures
					let stableUpdate: Update | null = null;
					if (stableSettled.status === "fulfilled") {
						const maybe = stableSettled.value;
						if (maybe && maybe !== TIMEOUT_SENTINEL && isNewerVersion(maybe.version, currentVersion)) {
							stableUpdate = maybe;
						}
					} else {
						appLogger.debug("app", "Stable check from nightly channel failed (non-fatal)", stableSettled.reason);
					}

					let nightlyResult: UpdateChannelResult | null = null;
					if (nightlySettled.status === "fulfilled") {
						nightlyResult = nightlySettled.value;
					} else {
						appLogger.debug("app", "Nightly check failed (non-fatal)", nightlySettled.reason);
					}

					// Both failed → surface error to user
					if (stableSettled.status === "rejected" && nightlySettled.status === "rejected") {
						const raw =
							nightlySettled.reason instanceof Error ? nightlySettled.reason.message : String(nightlySettled.reason);
						appLogger.warn("app", "All update sources failed", raw);
						setState({ error: raw, available: false, version: null, body: null, downloadUrl: null });
					} else if (
						stableUpdate &&
						(channel !== "beta" ||
							!nightlyResult?.available ||
							!nightlyResult.version ||
							compareAppVersion(stableUpdate.version, nightlyResult.version) >= 0)
					) {
						// Stable supports in-app install; prerelease feeds open their release page.
						pendingUpdate = stableUpdate;
						setState({
							available: true,
							version: stableUpdate.version,
							body: stableUpdate.body ?? null,
							downloadUrl: null,
							noRelease: false,
						});
					} else if (
						nightlyResult?.available &&
						nightlyResult.version &&
						isNewerVersion(nightlyResult.version, currentVersion)
					) {
						pendingUpdate = null;
						setState({
							available: true,
							version: nightlyResult.version,
							body: nightlyResult.notes ?? null,
							downloadUrl: nightlyResult.release_page ?? null,
							noRelease: false,
						});
					} else if (nightlyResult?.not_found) {
						appLogger.debug("app", `No ${channel} release found`);
						setState({ noRelease: true, available: false, version: null, body: null, downloadUrl: null });
					} else {
						setState({ available: false, version: null, body: null, downloadUrl: null });
					}
				}
			} catch (err) {
				const raw = err instanceof Error ? err.message : String(err);
				const unsupported = unsupportedBinaryPathReason(raw);
				if (unsupported) {
					appLogger.debug("app", "Built-in updater unavailable for this binary path", raw);
					setState({ unsupported, available: false, version: null, body: null, downloadUrl: null });
				} else if (channel === "stable" && /fetch|load failed|valid release|404|not found/i.test(raw)) {
					// No release published for stable — informational, not an error
					appLogger.debug("app", `No ${channel} release found`, raw);
					setState({ noRelease: true });
				} else {
					appLogger.error("app", "Update check failed", raw);
					setState({ error: raw });
				}
			} finally {
				setState({ checking: false });
			}
		},

		async downloadAndInstall(): Promise<void> {
			// For non-stable channels, open the release page in the browser
			if (state.downloadUrl) {
				window.open(state.downloadUrl, "_blank");
				return;
			}
			if (!isTauri() || !pendingUpdate || state.downloading) return;
			setState({ downloading: true, progress: 0, error: null });
			try {
				let contentLength = 0;
				let downloaded = 0;
				await installUpdate(pendingUpdate, (event) => {
					if (event.event === "Started") {
						contentLength = (event.data as { contentLength?: number }).contentLength ?? 0;
					} else if (event.event === "Progress") {
						downloaded += (event.data as { chunkLength: number }).chunkLength;
						const pct = contentLength > 0 ? Math.round((downloaded / contentLength) * 100) : 0;
						setState({ progress: pct });
					}
				});
			} catch (err) {
				const message = err instanceof Error ? err.message : String(err);
				appLogger.error("app", "Update install failed", message);
				setState({ error: message, downloading: false });
				return;
			}
			if (settingsStore.state.deferUpdateRelaunch && countBusySessions() > 0) {
				appLogger.info("app", "Update installed; restart deferred until agents are idle");
				setState({ downloading: false, restartPending: true });
				idleTimer = setInterval(() => {
					if (countBusySessions() === 0) void relaunchNow();
				}, IDLE_POLL_MS);
				return;
			}
			await relaunchNow();
		},

		/** Number of sessions a restart right now would interrupt. */
		busyCount(): number {
			return countBusySessions();
		},

		/** Restart immediately, even with busy sessions (the user confirmed). */
		restartNow(): Promise<void> {
			return relaunchNow();
		},

		dismiss(): void {
			pendingUpdate = null;
			setState({
				available: false,
				version: null,
				body: null,
				error: null,
				noRelease: false,
				unsupported: null,
				downloadUrl: null,
			});
		},

		/** Simulate an available update (dev/testing only) */
		simulateAvailable(version: string): void {
			pendingUpdate = null;
			setState({
				available: true,
				version,
				body: `Simulated update to v${version}`,
				downloading: false,
				progress: 0,
				error: null,
				downloadUrl: null,
			});
		},
	};

	return { state, ...actions };
}

export const updaterStore = createUpdaterStore();
