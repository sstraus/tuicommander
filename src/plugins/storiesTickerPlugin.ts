import { invoke, listen } from "../invoke";
import { appLogger } from "../stores/appLogger";
import type { DirEntry } from "../types/fs";
import { joinPath } from "../utils/pathUtils";
import type { PluginHost, TuiPlugin } from "./types";

const PLUGIN_ID = "stories-ticker";
const CLOSED_STATUSES = ["-complete-", "-wontfix-"];

class StoriesTickerPlugin implements TuiPlugin {
	readonly id = PLUGIN_ID;
	private host: PluginHost | null = null;
	private unlistenDirChanged: (() => void) | null = null;
	private watchedDir: string | null = null;

	onload(host: PluginHost): void {
		this.host = host;

		host.onStateChange((event) => {
			if (event.type === "repo-changed") {
				const repo = host.getActiveRepo();
				if (repo) {
					this.stopWatch();
					this.refresh(repo.path);
					this.watchStoriesDir(repo.path);
				} else {
					this.stopWatch();
					host.clearTicker("open-count");
				}
			}
		});

		const repo = host.getActiveRepo();
		if (repo) {
			this.refresh(repo.path);
			this.watchStoriesDir(repo.path);
		}
	}

	private refresh(repoPath: string): void {
		invoke<DirEntry[]>("list_directory", { repoPath, subdir: "stories" })
			.then((entries) => {
				const openCount = entries.filter(
					(e) => !e.is_dir && e.name.endsWith(".md") && !CLOSED_STATUSES.some((s) => e.name.includes(s)),
				).length;

				if (openCount > 0) {
					this.host?.setTicker({
						id: "open-count",
						text: `${openCount} open`,
						label: "Stories",
						priority: 15,
						ttlMs: 0,
					});
				} else {
					this.host?.clearTicker("open-count");
				}
			})
			.catch((err) => {
				const msg = String(err);
				if (!msg.includes("not found") && !msg.includes("No such file")) {
					appLogger.warn("plugin", "[stories-ticker] Failed to scan stories", err);
				}
				this.host?.clearTicker("open-count");
			});
	}

	private watchStoriesDir(repoPath: string): void {
		const storiesDir = joinPath(repoPath, "stories");
		this.watchedDir = storiesDir;

		invoke("start_dir_watcher", { path: storiesDir }).catch((err) => {
			const msg = String(err);
			if (!msg.includes("not found") && !msg.includes("No such file")) {
				appLogger.warn("plugin", "[stories-ticker] Failed to start dir watcher", err);
			}
		});

		listen<{ dir_path: string }>("dir-changed", (event) => {
			if (event.payload.dir_path !== this.watchedDir) return;
			const repo = this.host?.getActiveRepo();
			if (repo) this.refresh(repo.path);
		})
			.then((unlisten) => {
				if (this.watchedDir === storiesDir) {
					this.unlistenDirChanged = unlisten;
				} else {
					unlisten();
				}
			})
			.catch((err) => {
				appLogger.warn("plugin", "[stories-ticker] Failed to register dir-changed listener", err);
			});
	}

	private stopWatch(): void {
		this.unlistenDirChanged?.();
		this.unlistenDirChanged = null;
		if (this.watchedDir) {
			invoke("stop_dir_watcher", { path: this.watchedDir }).catch(() => {});
			this.watchedDir = null;
		}
	}

	onunload(): void {
		this.host?.clearTicker("open-count");
		this.host = null;
		this.stopWatch();
	}
}

export const storiesTickerPlugin: TuiPlugin = new StoriesTickerPlugin();
