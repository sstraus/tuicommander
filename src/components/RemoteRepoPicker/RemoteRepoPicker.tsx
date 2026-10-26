import { type Component, createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import { registerModal } from "../../stores/modalStack";
import { HttpRpcError, rpc } from "../../transport";
import type { DirEntry } from "../../types/fs";
import d from "../shared/dialog.module.css";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import s from "./RemoteRepoPicker.module.css";

export interface RemoteRepoPickerProps {
	visible: boolean;
	/** The daemon to browse; omitted in browser mode to use the serving TUIC. */
	connectionId?: string;
	/** Shown in the header, so the user can see which machine they are walking. */
	connectionName: string;
	onClose: () => void;
	onConfirm: (absolutePath: string) => void;
}

/**
 * Where each machine was last left, so reopening the dialog does not start over
 * at the root every time. Deliberately in memory only: it is a convenience, and
 * a stale persisted path on a machine that has changed is worse than one click.
 */
const lastVisited = new Map<string, string>();

/**
 * The remote half of "Add Repository".
 *
 * The local flow opens the OS file dialog, which enumerates the local disk
 * through the OS and cannot be pointed at another machine — mounting the remote
 * host would not fix it either, because the path it would then hand back is the
 * local mount point rather than the path the daemon knows. So browsing a remote
 * machine needs its own picker, and this is it: `list_directory` over the
 * connection, which the daemon already serves from `shared_routes()`.
 */
export const RemoteRepoPicker: Component<RemoteRepoPickerProps> = (props) => {
	const [cwd, setCwd] = createSignal("/");
	const [draft, setDraft] = createSignal("/");
	const [entries, setEntries] = createSignal<DirEntry[]>([]);
	const [loading, setLoading] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);

	/** Split on either separator: the daemon may be a Windows machine. */
	const segments = (path: string) => path.split(/[\\/]+/).filter(Boolean);

	const isRoot = (path: string) => segments(path).length === 0 || /^[a-zA-Z]:[\\/]?$/.test(path.trim());

	const join = (base: string, name: string) => (base.endsWith("/") ? `${base}${name}` : `${base}/${name}`);

	const parentOf = (path: string) => {
		const parts = segments(path);
		if (parts.length <= 1) return path.startsWith("/") ? "/" : path;
		parts.pop();
		return path.startsWith("/") ? `/${parts.join("/")}` : parts.join("/");
	};

	const load = async (path: string) => {
		setLoading(true);
		setError(null);
		try {
			// An explicit empty connection keeps the serving TUIC even if this path
			// also belongs to a registered remote repository.
			// subdir "" means "list the path itself". `list_directory` canonicalizes
			// whatever it is given and does not require a registered repository,
			// which is the whole reason this works before the repo exists.
			const result = await rpc<DirEntry[]>("list_directory", { repoPath: path, subdir: "" }, props.connectionId ?? "");
			setEntries(result.filter((e) => e.is_dir).sort((a, b) => a.name.localeCompare(b.name)));
			setCwd(path);
			setDraft(path);
			lastVisited.set(props.connectionId ?? "current-server", path);
		} catch (e) {
			// The message is the daemon's own. A path that is not there and a
			// machine that stopped answering are different failures, and the user
			// can only tell them apart if we do not rewrite them.
			// Keep the attempted location so Up can escape even when its first
			// directory listing was denied.
			setCwd(path);
			setDraft(path);
			setError(e instanceof HttpRpcError ? e.detail : e instanceof Error ? e.message : String(e));
			setEntries([]);
		} finally {
			setLoading(false);
		}
	};

	createEffect(() => {
		if (!props.visible) return;
		const connectionId = props.connectionId;
		const remembered = lastVisited.get(connectionId ?? "current-server");
		if (remembered) {
			void load(remembered);
			return;
		}
		setLoading(true);
		void rpc<string>("get_home_directory", {}, connectionId ?? "")
			.then((home) => {
				if (props.visible && props.connectionId === connectionId) void load(home);
			})
			.catch((e) => {
				if (!props.visible || props.connectionId !== connectionId) return;
				setError(`Could not find this machine's home directory: ${e instanceof HttpRpcError ? e.detail : String(e)}`);
				setLoading(false);
			});
	});

	createEffect(() => {
		if (!props.visible) return;
		registerModal(props.onClose);

		const handleKeydown = (e: KeyboardEvent) => {
			if (e.key === "Enter" && !loading() && cwd().trim()) {
				e.preventDefault();
				props.onConfirm(cwd().trim());
				props.onClose();
			}
		};
		document.addEventListener("keydown", handleKeydown);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown));
	});

	return (
		<Show when={props.visible}>
			<div class={d.overlay} onClick={props.onClose}>
				<div class={d.popover} onClick={(e) => e.stopPropagation()}>
					<div class={d.header}>
						<div class={d.headerText}>
							<h4>{t("remoteRepoPicker.title", "Add Repository")}</h4>
							<p class={d.subtitle}>
								{t("remoteRepoPicker.subtitle", "Browsing")} {props.connectionName}
							</p>
						</div>
					</div>
					<div class={d.body}>
						<Input
							type="text"
							value={draft()}
							onInput={(e) => setDraft((e.target as HTMLInputElement).value)}
							onKeyDown={(e) => {
								// Enter in the path field navigates rather than confirming:
								// the field is the escape hatch for a path the user already
								// knows, and jumping there is what they meant.
								if (e.key === "Enter") {
									e.preventDefault();
									e.stopPropagation();
									void load(draft().trim() || "/");
								}
							}}
							placeholder={t("remoteRepoPicker.pathPlaceholder", "Absolute path on the remote machine")}
							autocomplete="off"
							autocorrect="off"
							spellcheck={false}
						/>
						<div class={s.list}>
							<Show when={!isRoot(cwd())}>
								<button
									type="button"
									data-control-layout="row"
									class={s.row}
									onClick={() => void load(parentOf(cwd()))}
									disabled={loading()}
								>
									<span class={s.name}>..</span>
								</button>
							</Show>
							<Show when={error()}>
								<p class={s.error}>{error()}</p>
							</Show>
							<Show when={loading()}>
								<p class={s.hint}>{t("remoteRepoPicker.loading", "Loading…")}</p>
							</Show>
							<Show when={!loading() && !error() && entries().length === 0}>
								<p class={s.hint}>{t("remoteRepoPicker.empty", "No subfolders here.")}</p>
							</Show>
							<For each={entries()}>
								{(entry) => (
									<button
										type="button"
										data-control-layout="row"
										class={s.row}
										onClick={() => void load(join(cwd(), entry.name))}
										disabled={loading()}
									>
										<span class={s.name}>{entry.name}</span>
									</button>
								)}
							</For>
						</div>
					</div>
					<div class={d.actions}>
						<Button onClick={props.onClose}>{t("remoteRepoPicker.cancel", "Cancel")}</Button>
						<Button
							variant="primary"
							onClick={() => {
								props.onConfirm(cwd().trim());
								props.onClose();
							}}
							disabled={loading() || !cwd().trim()}
						>
							{t("remoteRepoPicker.confirm", "Add This Folder")}
						</Button>
					</div>
				</div>
			</div>
		</Show>
	);
};

export default RemoteRepoPicker;
