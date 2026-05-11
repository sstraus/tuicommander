import { appLogger } from "../stores/appLogger";
import { settingsStore } from "../stores/settings";
import { applyAppTheme, applyFontFamily, listenForThemeChanges, loadThemes } from "../themes";

export async function initPanelWindow(): Promise<void> {
	document.getElementById("splash")?.remove();
	await settingsStore.hydrate().catch((e) => {
		appLogger.warn("panel", "Failed to hydrate settings in panel window — using defaults", e);
	});
	await loadThemes();
	void listenForThemeChanges();
	applyAppTheme(settingsStore.state.theme);
	applyFontFamily(settingsStore.state.font);
}
