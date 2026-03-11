import { createStore } from "solid-js/store";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { isTauri, rpc } from "../transport";
import { settingsStore } from "./settings";
import { appLogger } from "./appLogger";

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
          const update = await Promise.race([
            check(),
            new Promise<typeof TIMEOUT_SENTINEL>((resolve) =>
              setTimeout(() => resolve(TIMEOUT_SENTINEL), 10_000),
            ),
          ]);
          if (update === TIMEOUT_SENTINEL) {
            pendingUpdate = null;
            setState({ available: false, version: null, body: null, downloadUrl: null, error: "Update check timed out" });
          } else if (update) {
            pendingUpdate = update;
            setState({
              available: true,
              version: update.version,
              body: update.body ?? null,
              downloadUrl: null,
            });
          } else {
            pendingUpdate = null;
            setState({ available: false, version: null, body: null, downloadUrl: null });
          }
        } else {
          // Beta/Nightly: Rust owns URL mapping, fetch, parsing, and error classification
          pendingUpdate = null;
          const result = await rpc<UpdateChannelResult>(
            "check_update_channel",
            { channel },
          );
          if (result.not_found) {
            appLogger.debug("app", `No ${channel} release found`);
            setState({ noRelease: true });
          } else if (result.available) {
            setState({
              available: true,
              version: result.version,
              body: result.notes ?? null,
              downloadUrl: result.release_page ?? null,
            });
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
