import { Channel } from "@tauri-apps/api/core";
import type { DownloadEvent, Update } from "@tauri-apps/plugin-updater";
import { rpc } from "../transport";

/** Keep the plugin's checked resource; native code owns install diagnostics. */
export function installUpdate(update: Pick<Update, "rid">, onEvent: (event: DownloadEvent) => void): Promise<void> {
	const channel = new Channel<DownloadEvent>();
	channel.onmessage = onEvent;
	return rpc("install_app_update", { rid: update.rid, onEvent: channel });
}
