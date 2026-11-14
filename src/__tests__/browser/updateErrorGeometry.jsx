import { render } from "solid-js/web";
import { UpdateProgressDialog } from "../../components/UpdateProgressDialog/UpdateProgressDialog";
import { updaterStore } from "../../stores/updater";
import "../../global.css";
// The updater store normally gets this string from the plugin's rejected IPC call.
const path = "/var/folders/h5/" + "a".repeat(120) + "/T/tauri_updated_app123/Contents/MacOS/tuicommander";
const errors = [
  "failed to unpack `TUICommander.app/Contents/MacOS/tuicommander` into `" + path + "`",
  "Update failed\nCaused by: " + path + "\nPermission denied (os error 13)",
  "Network unavailable",
];
const error = errors[Number(new URLSearchParams(location.search).get("case") ?? 0)];
window.__TAURI_INTERNALS__ = { invoke: async () => { throw new Error(error); } };
render(() => <UpdateProgressDialog />, document.getElementById("root"));
await updaterStore.checkForUpdate();
window.updateErrorExpected = error;
window.updateErrorReady = true;
