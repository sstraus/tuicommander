import { createStore } from "solid-js/store";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { isTauri, rpc } from "../transport";
import { settingsStore } from "./settings";
import { appLogger } from "./appLogger";

/** Compare two semver strings. Returns true if remote > current.
 *  Strips prerelease suffixes (e.g. "1.0.0-nightly.1" → "1.0.0") so that
 *  a prerelease tagged with the same base version is NOT considered newer. */
function isNewerVersion(remote: string, current: string): boolean {
  // Strip leading "v" and prerelease/build suffixes (everything after first "-" or "+")
  const parse = (v: string) =>
    v.replace(/^v/, "").replace(/[-+].*$/, "").split(".").map(Number);
  const r = parse(remote);
  const c = parse(current);
  for (let i = 0; i < Math.max(r.length, c.length); i++) {
    const rv = r[i] ?? 0;
    const cv = c[i] ?? 0;
    if (rv > cv) return true;
    if (rv < cv) return false;
  }
  return false;
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
  /** For non-stable channels, a URL to the release page for manual download */
  downloadUrl: string | null;
}

/** Sentinel to distinguish "check() timed out" from "no update available". */
const TIMEOUT_SENTINEL = Symbol("timeout");

/** Run Tauri's built-in stable update check with a 10-second timeout. */
function checkStableWithTimeout(): Promise<Update | null | typeof TIMEOUT_SENTINEL> {
  return Promise.race([
    check(),
    new Promise<typeof TIMEOUT_SENTINEL>((resolve) =>
      setTimeout(() => resolve(TIMEOUT_SENTINEL), 10_000),
    ),
  ]);
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
    downloadUrl: null,
  });

  let pendingUpdate: Update | null = null;

  const actions = {
    async checkForUpdate(): Promise<void> {
      if (!isTauri()) return;
      if (state.checking || state.downloading) return;
      setState({ checking: true, error: null, noRelease: false });

      const channel = settingsStore.state.updateChannel;

      try {
        if (channel === "stable") {
          // Stable: use Tauri's built-in updater (supports downloadAndInstall)
          const update = await checkStableWithTimeout();
          if (update === TIMEOUT_SENTINEL) {
            pendingUpdate = null;
            setState({ available: false, version: null, body: null, downloadUrl: null, error: "Update check timed out" });
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
          // Nightly: check BOTH stable (via Tauri built-in) and nightly (via Rust)
          // in parallel. Prefer stable if available — it supports in-app downloadAndInstall.
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
            const raw = nightlySettled.reason instanceof Error ? nightlySettled.reason.message : String(nightlySettled.reason);
            appLogger.warn("app", "All update sources failed", raw);
            setState({ error: raw, available: false, version: null, body: null, downloadUrl: null });
          } else if (stableUpdate) {
            // Prefer stable (in-app install) over nightly (browser download)
            pendingUpdate = stableUpdate;
            setState({
              available: true,
              version: stableUpdate.version,
              body: stableUpdate.body ?? null,
              downloadUrl: null,
              noRelease: false,
            });
          } else if (nightlyResult?.available && nightlyResult.version && isNewerVersion(nightlyResult.version, currentVersion)) {
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
        if (channel === "stable" && /fetch|load failed|valid release|404|not found/i.test(raw)) {
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
        await pendingUpdate.downloadAndInstall((event) => {
          if (event.event === "Started") {
            contentLength = (event.data as { contentLength?: number }).contentLength ?? 0;
          } else if (event.event === "Progress") {
            downloaded += (event.data as { chunkLength: number }).chunkLength;
            const pct = contentLength > 0 ? Math.round((downloaded / contentLength) * 100) : 0;
            setState({ progress: pct });
          }
        });
        await relaunch();
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        appLogger.error("app", "Update install failed", message);
        setState({ error: message, downloading: false });
      }
    },

    dismiss(): void {
      pendingUpdate = null;
      setState({ available: false, version: null, body: null, error: null, noRelease: false, downloadUrl: null });
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
