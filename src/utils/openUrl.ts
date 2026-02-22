import { isTauri } from "../transport";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";

/** Open a URL in the system browser, using Tauri shell in native mode or window.open in browser mode. */
export function handleOpenUrl(url: string): void {
  if (isTauri()) {
    tauriOpenUrl(url).catch((err) => console.error("Failed to open URL:", err));
  } else {
    window.open(url, "_blank");
  }
}
