import { isTauri } from "../transport";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import { appLogger } from "../stores/appLogger";

/** Open a URL in the system browser, using Tauri shell in native mode or window.open in browser mode. */
export function handleOpenUrl(url: string): void {
  if (isTauri()) {
    tauriOpenUrl(url).catch((err) => appLogger.error("app", "Failed to open URL", err));
  } else {
    window.open(url, "_blank");
  }
}
