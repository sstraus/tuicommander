import { createSignal, onCleanup } from "solid-js";
import { appLogger } from "../stores/appLogger";

const CHECK_INTERVAL_MS = 60_000;

/**
 * Polls /api/version to detect when the server has been rebuilt.
 * Returns a signal that becomes true when an update is available,
 * and a function to apply the update (hard reload).
 */
export function useVersionCheck() {
  const [updateAvailable, setUpdateAvailable] = createSignal(false);

  const clientHash: string = typeof __BUILD_GIT_HASH__ !== "undefined" ? __BUILD_GIT_HASH__ : "";

  async function check() {
    try {
      const resp = await fetch("/api/version");
      if (!resp.ok) return;
      const data = await resp.json() as { version: string; git_hash: string };
      const serverHash = data.git_hash;
      if (!serverHash || !clientHash) return;
      if (serverHash !== clientHash) {
        appLogger.info("app", `Update available: ${clientHash} → ${serverHash}`);
        setUpdateAvailable(true);
      }
    } catch {
      // Network error — skip silently, will retry next interval
    }
  }

  check();
  const timer = setInterval(check, CHECK_INTERVAL_MS);
  onCleanup(() => clearInterval(timer));

  function applyUpdate() {
    // Unregister SW so the browser fetches fresh assets on reload
    if ("serviceWorker" in navigator) {
      navigator.serviceWorker.getRegistrations().then((regs) => {
        for (const reg of regs) reg.unregister();
      });
    }
    // Hard reload bypassing cache
    location.reload();
  }

  return { updateAvailable, applyUpdate };
}
