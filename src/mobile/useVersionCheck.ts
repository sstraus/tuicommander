import { createSignal, onCleanup } from "solid-js";
import { appLogger } from "../stores/appLogger";

const CHECK_INTERVAL_MS = 60_000;

/**
 * Polls /api/version to detect when the server has been rebuilt.
 * Returns a signal that becomes true when an update is available,
 * and a function to apply the update (hard reload).
 */
const CONSECUTIVE_FAILURES_THRESHOLD = 2;

export function useVersionCheck() {
  const [updateAvailable, setUpdateAvailable] = createSignal(false);
  const [serverDown, setServerDown] = createSignal(false);
  let consecutiveFailures = 0;

  const clientHash: string = typeof __BUILD_GIT_HASH__ !== "undefined" ? __BUILD_GIT_HASH__ : "";

  async function check() {
    try {
      const resp = await fetch("/api/version");
      if (!resp.ok) return;
      consecutiveFailures = 0;
      setServerDown(false);
      const data = await resp.json() as { version: string; git_hash: string };
      const serverHash = data.git_hash;
      if (!serverHash || !clientHash) return;
      if (serverHash !== clientHash) {
        appLogger.info("app", `Update available: ${clientHash} → ${serverHash}`);
        setUpdateAvailable(true);
      }
    } catch {
      consecutiveFailures++;
      if (consecutiveFailures >= CONSECUTIVE_FAILURES_THRESHOLD) {
        setServerDown(true);
      }
    }
  }

  // iOS SW freshness workaround: force SW update check on every page load
  if ("serviceWorker" in navigator) {
    navigator.serviceWorker.ready.then((reg) => {
      reg.update().catch(() => {});
    }).catch(() => {});
  }

  check();
  const timer = setInterval(check, CHECK_INTERVAL_MS);
  onCleanup(() => clearInterval(timer));

  function applyUpdate() {
    // Cache-bust navigation for iOS standalone mode (location.reload() may serve from RAM cache)
    location.replace(location.pathname + "?v=" + Date.now());
  }

  return { updateAvailable, serverDown, applyUpdate };
}
