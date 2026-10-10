/* @refresh reload */
import { prefersDesktopUI, tabletAppDestination } from "./utils/tabletRouting";

const tabletDestination = tabletAppDestination(
	navigator.userAgent,
	navigator.maxTouchPoints,
	window.location.pathname,
	window.location.search,
	window.location.hash,
	"__TAURI_INTERNALS__" in window && !("__TAURI_SHIM__" in window),
	prefersDesktopUI(),
);

const root = document.getElementById("app");
if (!root) throw new Error("Root element #app not found");

if (tabletDestination) {
	window.location.replace(tabletDestination);
} else if (/^#\/secret-form(?:\?|$)/.test(window.location.hash)) {
	document.getElementById("splash")?.remove();
	root.id = "secret-form-root";
	void import("./secretForm").then(({ startSecretForm }) => startSecretForm(root));
} else {
	void import("./appEntry");
}
