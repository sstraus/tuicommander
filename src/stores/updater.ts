import { createStore } from "solid-js/store";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

interface UpdaterState {
  available: boolean;
  checking: boolean;
  downloading: boolean;
  progress: number;
  version: string | null;
  body: string | null;
  error: string | null;
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
  });

  let pendingUpdate: Update | null = null;

  const actions = {
    async checkForUpdate(): Promise<void> {
      if (state.checking || state.downloading) return;
      setState({ checking: true, error: null });
      try {
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
          });
        } else {
          pendingUpdate = null;
          setState({ available: false, version: null, body: null });
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        console.error("Update check failed:", message);
        setState({ error: message });
      } finally {
        setState({ checking: false });
      }
    },

    async downloadAndInstall(): Promise<void> {
      if (!pendingUpdate || state.downloading) return;
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
          // "Finished" event handled implicitly when downloadAndInstall resolves
        });
        await relaunch();
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        console.error("Update install failed:", message);
        setState({ error: message, downloading: false });
      }
    },

    dismiss(): void {
      pendingUpdate = null;
      setState({ available: false, version: null, body: null, error: null });
    },
  };

  return { state, ...actions };
}

export const updaterStore = createUpdaterStore();
