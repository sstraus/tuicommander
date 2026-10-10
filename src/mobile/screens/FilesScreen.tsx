import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { rpc } from "../../transport";
import { isAbsolutePath, pathStripPrefix } from "../../utils/pathUtils";
import { repoImageUrl } from "../../utils/repoImageUrl";
import { insertTweakBlockComment, type TweakComment, toggleCheckbox } from "../../utils/tweakComments";
import { ReviewableMarkdown } from "../components/ReviewableMarkdown";
import styles from "./FilesScreen.module.css";

interface FileEntry {
	name: string;
	path: string;
	is_dir: boolean;
	size: number;
}

const MAX_MOBILE_FILE_BYTES = 1_048_576;

interface FilesScreenProps {
	initialRepo?: { worktreePath: string | null; cwd: string | null };
	initialLink?: { candidate: string; line?: number };
	onExit?: () => void;
}

function normalizedPath(path: string): string {
	return path.replaceAll("\\", "/").replace(/\/+$/, "");
}

function withinRoot(path: string, root: string): boolean {
	const file = normalizedPath(path);
	const directory = normalizedPath(root);
	return file === directory || file.startsWith(`${directory}/`);
}

export function FilesScreen(props: FilesScreenProps) {
	const [repos, setRepos] = createSignal<string[]>([]);
	const [repo, setRepo] = createSignal<string | null>(null);
	const [dir, setDir] = createSignal("");
	const [entries, setEntries] = createSignal<FileEntry[]>([]);
	const [searchQuery, setSearchQuery] = createSignal("");
	const [searchResults, setSearchResults] = createSignal<FileEntry[]>([]);
	const [searching, setSearching] = createSignal(false);
	const [previewPath, setPreviewPath] = createSignal<string | null>(null);
	const [file, setFile] = createSignal<string | null>(null);
	const [content, setContent] = createSignal("");
	const [draft, setDraft] = createSignal("");
	const [editing, setEditing] = createSignal(false);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	// Refusals of a review write (file changed on disk, write failed). Unlike `error` it keeps the document visible.
	const [notice, setNotice] = createSignal("");
	const [writing, setWriting] = createSignal(false);
	let requestId = 0;
	let searchRequestId = 0;
	let pathPreviewTimer: ReturnType<typeof setTimeout> | undefined;
	let suppressRepoOpen = false;
	let editorEl: HTMLTextAreaElement | undefined;
	onCleanup(() => {
		if (pathPreviewTimer) clearTimeout(pathPreviewTimer);
	});

	onMount(async () => {
		try {
			if (props.initialLink) {
				await openLinkedFile(props.initialLink);
				return;
			}
			const worktreePath = props.initialRepo?.worktreePath?.trim();
			if (worktreePath) {
				await openDirectory(worktreePath, "");
				return;
			}
			const cwd = props.initialRepo?.cwd?.trim();
			if (props.initialRepo && !cwd) {
				setError("Repository path is unavailable for this session.");
				return;
			}
			const config = await rpc<{ repos?: Record<string, unknown> }>("load_repositories");
			const registered = Object.keys(config.repos ?? {});
			if (cwd) {
				const matching = registered.filter((path) => withinRoot(cwd, path)).sort((a, b) => b.length - a.length);
				if (matching.length === 0) {
					setError("No registered repository contains this session directory.");
					return;
				}
				await openDirectory(matching[0], "");
			} else {
				setRepos(registered);
			}
		} catch (err) {
			setError(`Could not load repositories: ${String(err)}`);
		}
	});

	async function openLinkedFile(link: { candidate: string; line?: number }) {
		const cwd = props.initialRepo?.cwd?.trim() || props.initialRepo?.worktreePath?.trim();
		if (!cwd) {
			setError("Repository path is unavailable for this session.");
			return;
		}
		const resolved = await rpc<{ absolute_path: string; is_directory: boolean } | null>("resolve_terminal_path", {
			cwd,
			candidate: link.candidate,
		});
		if (!resolved) {
			setError("Markdown file is unavailable.");
			return;
		}
		const config = await rpc<{ repos?: Record<string, unknown> }>("load_repositories");
		const roots = [props.initialRepo?.worktreePath, ...Object.keys(config.repos ?? {})].filter(
			(path): path is string => !!path,
		);
		const root = roots
			.filter((path) => withinRoot(resolved.absolute_path, path))
			.sort((a, b) => b.length - a.length)[0];
		if (!root) {
			setError(`${resolved.is_directory ? "Directory" : "File"} is outside an allowed registered repository.`);
			toastsStore.add(
				resolved.is_directory ? "Cannot open directory" : "Cannot open Markdown file",
				resolved.absolute_path,
				"error",
				false,
				undefined,
				undefined,
				undefined,
				undefined,
				false,
			);
			return;
		}
		if (resolved.is_directory) {
			await openDirectory(resolved.absolute_path, "");
			return;
		}
		const stat = await rpc<{ exists: boolean; is_dir: boolean; size: number }>("stat_path", {
			path: resolved.absolute_path,
		});
		if (!stat.exists || stat.is_dir) {
			setError("Markdown file is unavailable.");
			return;
		}
		const relative = normalizedPath(resolved.absolute_path).slice(normalizedPath(root).length + 1);
		setRepo(root);
		setDir(relative.split("/").slice(0, -1).join("/"));
		await openFile(
			{ name: relative.split("/").pop() ?? relative, path: relative, is_dir: false, size: stat.size },
			link.line,
		);
	}

	async function openDirectory(repoPath: string, subdir: string) {
		const currentRequest = ++requestId;
		++searchRequestId;
		setSearchQuery("");
		setSearchResults([]);
		setSearching(false);
		setBusy(true);
		setError("");
		try {
			const result = await rpc<FileEntry[]>("list_directory", { repoPath, subdir });
			if (currentRequest !== requestId) return;
			setRepo(repoPath);
			setDir(subdir);
			setEntries(result);
		} catch (err) {
			if (currentRequest !== requestId) return;
			setError(`Could not open directory: ${String(err)}`);
		} finally {
			if (currentRequest === requestId) setBusy(false);
		}
	}

	async function searchFiles(query: string) {
		setSearchQuery(query);
		const currentRequest = ++searchRequestId;
		if (!query.trim() || !repo()) {
			setSearchResults([]);
			setSearching(false);
			setError("");
			return;
		}
		setSearching(true);
		setError("");
		try {
			const result = await rpc<FileEntry[]>("search_files", { repoPath: repo(), query: query.trim(), limit: 100 });
			if (currentRequest === searchRequestId) setSearchResults(result);
		} catch (err) {
			if (currentRequest === searchRequestId) setError(`Could not search files: ${String(err)}`);
		} finally {
			if (currentRequest === searchRequestId) setSearching(false);
		}
	}

	async function openFile(entry: FileEntry, line?: number) {
		const currentRequest = ++requestId;
		setFile(entry.path);
		setContent("");
		setEditing(false);
		setError("");
		setNotice("");
		if (entry.size > MAX_MOBILE_FILE_BYTES) {
			setError("File too large to open on mobile (1 MB limit).");
			return;
		}
		setBusy(true);
		try {
			const result = await rpc<string>("fs_read_file", { repoPath: repo(), file: entry.path });
			if (currentRequest !== requestId) return;
			if (result.includes("\0")) {
				setError("This is a binary or non-text file.");
			} else {
				setContent(result);
				if (line) {
					setDraft(result);
					setEditing(true);
					queueMicrotask(() => {
						if (currentRequest !== requestId || !editorEl) return;
						const lines = result.split("\n");
						const target = Math.min(line - 1, lines.length - 1);
						const offset = lines.slice(0, target).reduce((sum, text) => sum + text.length + 1, 0);
						editorEl.focus();
						editorEl.setSelectionRange(offset, offset);
					});
				}
			}
		} catch (err) {
			if (currentRequest !== requestId) return;
			const message = String(err);
			setError(/valid UTF-8/i.test(message) ? "This is a binary or non-text file." : `Could not open file: ${message}`);
		} finally {
			if (currentRequest === requestId) setBusy(false);
		}
	}

	function goBack() {
		requestId++;
		setBusy(false);
		setError("");
		setNotice("");
		if (props.initialLink && props.onExit) {
			props.onExit();
			return;
		}
		if (file() !== null) {
			setFile(null);
			setEditing(false);
			return;
		}
		if (dir()) {
			void openDirectory(repo()!, dir().split("/").slice(0, -1).join("/"));
			return;
		}
		if (props.onExit) props.onExit();
		else setRepo(null);
	}

	function markdownImageSrc(relativePath: string): string {
		const repoPath = repo();
		const currentFile = file();
		if (!repoPath || !currentFile) return relativePath;
		if (isAbsolutePath(relativePath)) {
			// Inside the open repository the route can serve it; outside there is no route by design.
			const inRepo = pathStripPrefix(relativePath, repoPath);
			return inRepo ? repoImageUrl(repoPath, inRepo) : relativePath;
		}
		return repoImageUrl(repoPath, `${currentFile.split("/").slice(0, -1).join("/")}/${relativePath}`);
	}

	/** Replace the open file with `next`, but only if the disk still holds what this screen last read. */
	async function writeIfUnchanged(next: string): Promise<boolean> {
		const repoPath = repo();
		const filePath = file();
		if (!repoPath || !filePath) return false;
		const written = await rpc<boolean>("write_file_if_unchanged", {
			repoPath,
			file: filePath,
			expected: content(),
			content: next,
		});
		if (written) setContent(next);
		return written;
	}

	/** Run one write on behalf of a review tap. A stale view is reloaded, never written over. */
	async function reviewWrite(next: () => string | null): Promise<boolean> {
		if (writing()) return false;
		setWriting(true);
		setNotice("");
		try {
			const updated = next();
			if (updated === null || updated === content()) return false;
			if (await writeIfUnchanged(updated)) return true;
			setContent(await rpc<string>("fs_read_file", { repoPath: repo(), file: file() }));
			setNotice("The file changed on disk. It was reloaded; repeat your change.");
			return false;
		} catch (err) {
			appLogger.warn("network", `Failed to save mobile review change: ${String(err)}`);
			setNotice(`Could not save: ${String(err)}`);
			return false;
		} finally {
			setWriting(false);
		}
	}

	const toggleTask = (sourceLine: number, mark: " " | "x" | "~", sourceCol?: number) =>
		void reviewWrite(() => toggleCheckbox(content(), sourceLine, mark, sourceCol));

	const saveBlockComment = (comment: TweakComment, range: { start: number; end: number }) =>
		reviewWrite(() => insertTweakBlockComment(content(), comment, range));

	async function save() {
		if (!repo() || !file()) return;
		setBusy(true);
		setError("");
		try {
			if (await writeIfUnchanged(draft())) {
				setEditing(false);
			} else {
				setError("The file changed on disk since you opened it. Copy your text, cancel, and edit again.");
			}
		} catch (err) {
			appLogger.warn("network", `Failed to save mobile file: ${String(err)}`);
			setError(`Could not save file: ${String(err)}`);
		} finally {
			setBusy(false);
		}
	}

	const repoName = (path: string) => path.split("/").filter(Boolean).pop() ?? path;
	function stopPathPreviewTimer() {
		if (pathPreviewTimer) clearTimeout(pathPreviewTimer);
		pathPreviewTimer = undefined;
	}

	function showPathPreview(path: string) {
		stopPathPreviewTimer();
		suppressRepoOpen = true;
		setPreviewPath(path);
	}

	function startPathPreview(path: string) {
		stopPathPreviewTimer();
		pathPreviewTimer = setTimeout(() => showPathPreview(path), 450);
	}
	const visibleEntries = () =>
		searchQuery().trim()
			? searchResults()
			: entries().toSorted((a, b) => {
					const hidden = Number(a.name.startsWith(".")) - Number(b.name.startsWith("."));
					return hidden || Number(b.is_dir) - Number(a.is_dir) || a.name.localeCompare(b.name);
				});

	return (
		<div class={styles.screen}>
			<header class={styles.header}>
				<Show when={repo() !== null || props.onExit}>
					<button class={styles.back} onClick={goBack} aria-label={props.onExit ? "Back to session" : "Back"}>
						‹ Back
					</button>
				</Show>
				<strong class={styles.title} title={file() || dir() || repo() || "Files"}>
					<bdi dir="ltr">{file() ? repoName(file()!) : dir() || repoName(repo() ?? "") || "Files"}</bdi>
				</strong>
				<Show when={file() !== null && (editing() || (!error() && !busy()))}>
					<Show
						when={editing()}
						fallback={
							<button
								class={styles.action}
								onClick={() => {
									setDraft(content());
									setEditing(true);
								}}
							>
								Edit
							</button>
						}
					>
						<button
							class={styles.action}
							onClick={() => {
								setEditing(false);
								setError("");
							}}
						>
							Cancel
						</button>
						<button class={styles.action} disabled={busy()} onClick={() => void save()}>
							Save
						</button>
					</Show>
				</Show>
			</header>
			<Show when={previewPath()}>
				<div class={styles.pathPreviewBackdrop}>
					<div class={styles.pathPreview} role="dialog" aria-label="Repository path">
						<p>{previewPath()}</p>
						<button
							class={styles.action}
							onClick={() => {
								suppressRepoOpen = false;
								setPreviewPath(null);
							}}
						>
							Close
						</button>
					</div>
				</div>
			</Show>
			<Show when={error()}>
				<p class={styles.error} role="alert">
					{error()}
				</p>
			</Show>
			<Show when={notice()}>
				<p class={styles.error} role="alert">
					{notice()}
				</p>
			</Show>
			<Show when={busy()}>
				<p class={styles.status}>Loading…</p>
			</Show>
			<Show when={repo() === null && !props.initialRepo}>
				<Show when={repos().length > 0} fallback={<p class={styles.status}>No repositories configured</p>}>
					<For each={repos()}>
						{(path) => (
							<button
								class={styles.row}
								title={path}
								onTouchStart={() => startPathPreview(path)}
								onTouchEnd={stopPathPreviewTimer}
								onTouchCancel={stopPathPreviewTimer}
								onContextMenu={(event) => {
									event.preventDefault();
									showPathPreview(path);
								}}
								onClick={() => {
									if (suppressRepoOpen) {
										suppressRepoOpen = false;
										return;
									}
									void openDirectory(path, "");
								}}
							>
								<span class={styles.name}>{repoName(path)}</span>
								<span class={styles.path}>
									<bdi dir="ltr">{path}</bdi>
								</span>
							</button>
						)}
					</For>
				</Show>
			</Show>
			<Show when={repo() !== null && file() === null}>
				<label class={styles.search}>
					<input
						type="search"
						aria-label="Search files"
						placeholder="Search files"
						value={searchQuery()}
						onInput={(event) => void searchFiles(event.currentTarget.value)}
					/>
				</label>
				<Show when={searching()}>
					<p class={styles.status}>Searching…</p>
				</Show>
				<For each={visibleEntries()}>
					{(entry) => (
						<button
							class={styles.row}
							onClick={() => (entry.is_dir ? void openDirectory(repo()!, entry.path) : void openFile(entry))}
						>
							<span class={styles.name}>
								{entry.is_dir ? "▸ " : ""}
								{searchQuery().trim() ? entry.path : entry.name}
							</span>
						</button>
					)}
				</For>
				<Show when={visibleEntries().length === 0 && !busy() && !searching() && !error()}>
					<p class={styles.status}>{searchQuery().trim() ? "No matching files" : "Empty directory"}</p>
				</Show>
			</Show>
			<Show when={file() !== null && (editing() || (!error() && !busy()))}>
				<Show
					when={editing()}
					fallback={
						file()?.toLowerCase().endsWith(".md") ? (
							<div class={styles.markdownView}>
								<ReviewableMarkdown
									content={content()}
									imageSrc={markdownImageSrc}
									onCheckboxToggle={toggleTask}
									onSaveBlockComment={saveBlockComment}
								/>
							</div>
						) : (
							<pre class={styles.viewer}>{content()}</pre>
						)
					}
				>
					<textarea
						ref={editorEl}
						class={styles.editor}
						aria-label="File content"
						wrap="soft"
						value={draft()}
						onInput={(event) => setDraft(event.currentTarget.value)}
						spellcheck={false}
					/>
				</Show>
			</Show>
		</div>
	);
}
