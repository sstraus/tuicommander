import { createStore } from "solid-js/store";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { isTauri, rpc } from "../transport";
import { settingsStore } from "./settings";
import type { UpdateChannel } from "./settings";
import { appLogger } from "./appLogger";

/** GitHub release manifest URLs per update channel */
const CHANNEL_ENDPOINTS: Record<UpdateChannel, string> = {
  stable: "https://github.com/sstraus/tuicommander/releases/latest/download/latest.json",
  beta: "https://github.com/sstraus/tuicommander/releases/download/beta/latest.json",
  nightly: "https://github.com/sstraus/tuicommander/releases/download/nightly/latest.json",
};

/** GitHub release page URLs per channel (for manual download) */
const CHANNEL_RELEASE_PAGES: Record<UpdateChannel, string> = {
  stable: "https://github.com/sstraus/tuicommander/releases/latest",
  beta: "https://github.com/sstraus/tuicommander/releases/tag/beta",
  nightly: "https://github.com/sstraus/tuicommander/releases/tag/nightly",
};

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
            new Promise<null>((resolve) => setTimeout(() => resolve(null), 10_000)),
          ]);
          if (update) {
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
          // Beta/Nightly: fetch manifest via Rust (bypasses WebView CSP)
          pendingUpdate = null;
          const endpoint = CHANNEL_ENDPOINTS[channel];
          const manifest = await rpc<{ version?: string; notes?: string }>(
            "fetch_update_manifest",
            { url: endpoint },
          );
          if (manifest.version) {
            setState({
              available: true,
              version: manifest.version,
              body: manifest.notes ?? null,
              downloadUrl: CHANNEL_RELEASE_PAGES[channel],
            });
          } else {
            setState({ available: false, version: null, body: null, downloadUrl: null });
          }
        }
      } catch (err) {
        const raw = err instanceof Error ? err.message : String(err);
        if (/fetch|load failed|valid release|404|not found/i.test(raw)) {
          // No release published for this channel — informational, not an error
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
