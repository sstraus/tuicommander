import { appLogger } from "../stores/appLogger";
import { remoteConnectionsStore } from "../stores/remoteConnections";
import { repositoriesStore } from "../stores/repositories";
import { rpc, subscribeEvents } from "../transport";
import { getRemoteBaseUrl } from "../transportRuntime";
import { remoteEventOrigin } from "../utils/remoteEventOrigin";

/** Follow remote repository hints without giving local handlers remote paths. */
export function startRemoteRepositorySync(refresh: (path: string) => Promise<void> | void): () => void {
	let disposed = false;
	let unsubscribe: (() => void) | undefined;
	const timers = new Map<
		string,
		{ timer: ReturnType<typeof setTimeout>; gitState: boolean; reconcileAfterGrace: boolean }
	>();
	const catchups = new Map<string, ReturnType<typeof setTimeout>>();
	const registering = new Set<string>();
	const owns = (path: string, owner: string) =>
		!disposed &&
		Boolean(getRemoteBaseUrl(owner)) &&
		repositoriesStore.getConnectionId(path) === owner &&
		repositoriesStore.getActivePaths().includes(path);

	const schedule = (path: string, owner: string, gitState = true, afterGrace = false) => {
		if (!owns(path, owner)) return;
		const key = JSON.stringify([owner, path]);
		// A continuous event stream must not postpone discovery indefinitely.
		const pending = timers.get(key);
		if (pending) {
			pending.gitState ||= gitState;
			pending.reconcileAfterGrace ||= gitState && !afterGrace;
			return;
		}
		const next = {
			gitState,
			reconcileAfterGrace: gitState && !afterGrace,
			timer: setTimeout(() => {
				timers.delete(key);
				// Ownership can change while this hint waits. Never fall back to IPC.
				if (!owns(path, owner)) return;
				if (next.gitState) repositoriesStore.bumpGitRevision(path);
				else repositoriesStore.bumpRevision(path);
				void Promise.resolve()
					.then(() => {
						if (owns(path, owner)) return refresh(path);
					})
					.then(() => {
						// Reconciliation protects newly discovered workspaces for 60s.
						// A create/remove burst may leave no later filesystem event to
						// prune the row once that grace expires, so read once more.
						if (!next.reconcileAfterGrace || !owns(path, owner)) return;
						// Measure from the latest regular pass, which may be the first
						// time reconciliation observes a newly discovered workspace.
						clearTimeout(catchups.get(key));
						catchups.set(
							key,
							setTimeout(() => {
								catchups.delete(key);
								schedule(path, owner, true, true);
							}, 61_000),
						);
					})
					.catch((error) => appLogger.warn("git", `Remote repository refresh failed for ${path}`, error));
			}, 500),
		};
		timers.set(key, next);
	};
	const resync = (connection?: string) => {
		for (const path of repositoriesStore.getActivePaths()) {
			const owner = repositoriesStore.getConnectionId(path);
			if (!owner || (connection && owner !== connection) || !owns(path, owner)) continue;
			const key = JSON.stringify([owner, path]);
			if (registering.has(key)) continue;
			registering.add(key);
			// Re-register after daemon/SSE restart; the daemon need not have this
			// desktop's repository list persisted in its own configuration.
			void rpc("start_repo_watcher", { repoPath: path }, owner)
				.catch((error) => appLogger.warn("git", `Remote repository watcher failed for ${path}`, error))
				.finally(() => {
					registering.delete(key);
					schedule(path, owner);
				});
		}
	};

	// Install the connection store's status listener first: reconnect hints must
	// see its current URL and token when they register a watcher.
	void remoteConnectionsStore
		.hydrate()
		.then(() => {
			if (disposed) return;
			return subscribeEvents(
				{
					"remote-repo-changed": (raw) => {
						if (!raw || typeof raw !== "object") return;
						const notice = raw as { repo_path?: unknown; kind?: unknown; __tuic_origin?: unknown };
						const origin = remoteEventOrigin(notice);
						if (!origin || typeof notice.repo_path !== "string") return;
						if (notice.kind !== "git-state" && notice.kind !== "working-tree") return;
						schedule(notice.repo_path, origin.connection, notice.kind === "git-state");
					},
					"remote-connection-status": (raw) => {
						if (!raw || typeof raw !== "object") return;
						const notice = raw as { id?: unknown; status?: unknown; __tuic_origin?: unknown };
						if (notice.__tuic_origin !== undefined || notice.status !== "connected" || typeof notice.id !== "string")
							return;
						resync(notice.id);
					},
				},
				{ onResync: () => resync() },
			).then((stop) => {
				if (disposed) return stop();
				unsubscribe = stop;
				resync();
			});
		})
		.catch((error) => appLogger.error("app", "Failed to follow remote repositories", error));
	return () => {
		if (disposed) return;
		disposed = true;
		unsubscribe?.();
		for (const { timer } of timers.values()) clearTimeout(timer);
		timers.clear();
		for (const timer of catchups.values()) clearTimeout(timer);
		catchups.clear();
	};
}
