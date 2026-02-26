import { isTauri } from "../transport";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import { appLogger } from "../stores/appLogger";

const ALLOWED_SCHEMES = new Set(["http:", "https:", "mailto:"]);

/** Open a URL in the system browser, using Tauri shell in native mode or window.open in browser mode.
 *  Only allows http/https/mailto schemes — terminal output is untrusted and arbitrary
 *  URI schemes (file://, smb://, custom protocols) could invoke OS handlers. */
export function handleOpenUrl(url: string): void {
  try {
    const parsed = new URL(url);
    if (!ALLOWED_SCHEMES.has(parsed.protocol)) {
      appLogger.warn("app", `Blocked URL with disallowed scheme: ${parsed.protocol}${url.slice(0, 80)}`);
      return;
    }
  } catch {
    appLogger.warn("app", `Blocked malformed URL: ${url.slice(0, 80)}`);
    return;
  }
  if (isTauri()) {
    tauriOpenUrl(url).catch((err) => appLogger.error("app", "Failed to open URL", err));
  } else {
    window.open(url, "_blank");
  }
}
