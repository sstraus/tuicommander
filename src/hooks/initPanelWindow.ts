import { settingsStore } from "../stores/settings";
import { appLogger } from "../stores/appLogger";
import { applyAppTheme, applyFontFamily } from "../themes";

export async function initPanelWindow(): Promise<void> {
  document.getElementById("splash")?.remove();
  await settingsStore.hydrate().catch((e) => {
    appLogger.warn("panel", "Failed to hydrate settings in panel window — using defaults", e);
  });
  applyAppTheme(settingsStore.state.theme);
  applyFontFamily(settingsStore.state.font);
}
