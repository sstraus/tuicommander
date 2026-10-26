import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import { Button, Input, Select } from "../../components/ui";
import { t } from "../../i18n";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { rpc } from "../../transport";
import { buildMarkdownReviewPrompt } from "../../utils/markdownReviewPrompt";
import { isAbsolutePath, joinPath, pathStripPrefix } from "../../utils/pathUtils";
import { repoImageUrl } from "../../utils/repoImageUrl";
import {
	insertTweakBlockComment,
	parseTweakComments,
	type TweakComment,
	toggleCheckbox,
} from "../../utils/tweakComments";
import { ReviewableMarkdown } from "../components/ReviewableMarkdown";
import type { SessionInfo } from "../useSessions";
import styles from "./FilesScreen.module.css";

interface FileEntry {
	name: string;
	path: string;
	is_dir: boolean;
	size: number;
}

const MAX_MOBILE_FILE_BYTES = 1_048_576;

interface FilesScreenProps {
	sessions?: SessionInfo[];
	initialSessionId?: string;
	openerTuicSession?: string;
	recoveryRevision?: number;
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
	const [editedSinceSend, setEditedSinceSend] = createSignal(false);
	const [sentCommentIds, setSentCommentIds] = createSignal<Set<string>>(new Set());
	const [sendingChanges, setSendingChanges] = createSignal(false);
	const [selectedAgentSession, setSelectedAgentSession] = createSignal("");
	const [agentChosenByUser, setAgentChosenByUser] = createSignal(false);
	const reviewComments = createMemo(() => parseTweakComments(content()));
	const hasReviewChanges = () =>
		editedSinceSend() || reviewComments().some((comment) => !sentCommentIds().has(comment.id));
	const reviewAgents = createMemo(() =>
		(props.sessions ?? []).filter(
			(session) =>
				!!session.state?.agent_type &&
				!!repo() &&
				(session.worktree_path
					? normalizedPath(session.worktree_path) === normalizedPath(repo()!)
					: !!session.cwd &&
						withinRoot(session.cwd, repo()!) &&
						!repos().some(
							(root) =>
								normalizedPath(root) !== normalizedPath(repo()!) &&
								withinRoot(root, repo()!) &&
								withinRoot(session.cwd!, root),
						)),
		),
	);
	createEffect(() => {
		const agents = reviewAgents();
		if (agentChosenByUser() && agents.some((agent) => agent.session_id === selectedAgentSession())) return;
		setSelectedAgentSession(
			agents.find((agent) => props.openerTuicSession && agent.tuic_session === props.openerTuicSession)?.session_id ??
				agents.find((agent) => agent.session_id === props.initialSessionId)?.session_id ??
				agents[0]?.session_id ??
				"",
		);
	});
	async function sendChanges() {
		const sessionId = selectedAgentSession();
		const filePath = file();
		const repoPath = repo();
		if (!sessionId || !filePath || !repoPath || !hasReviewChanges() || sendingChanges() || writing()) return;
		const sentContent = content();
		const comments = reviewComments();
		const currentRequest = requestId;
		setSendingChanges(true);
		setNotice("");
		try {
			const outcome = await rpc<{ typed: boolean; queued: number }>("enqueue_agent_command", {
				sessionId,
				text: buildMarkdownReviewPrompt(
					isAbsolutePath(filePath) ? filePath : joinPath(repoPath, filePath),
					comments.length,
				),
			});
			if (currentRequest === requestId) {
				setSentCommentIds(new Set(comments.map((comment) => comment.id)));
				if (content() === sentContent) setEditedSinceSend(false);
				toastsStore.add(
					t("markdownTab.sentToAgent", "Sent to agent"),
					outcome.typed
						? t("markdownTab.sentToAgentNow", "The review request was delivered now.")
						: t("markdownTab.sentToAgentQueued", "The review request is queued for the agent's next idle window."),
					"info",
				);
			}
		} catch (err) {
			appLogger.error("network", `Failed to queue Markdown review: ${String(err)}`);
			if (currentRequest === requestId) setNotice(t("markdownTab.sendFailed", "Couldn't send changes"));
		} finally {
			setSendingChanges(false);
		}
	}
	let openedEntry: FileEntry | undefined;
	let requestId = 0;
	let searchRequestId = 0;
	let pathPreviewTimer: ReturnType<typeof setTimeout> | undefined;
	let suppressRepoOpen = false;
	let editorEl: HTMLTextAreaElement | undefined;
	onCleanup(() => {
		if (pathPreviewTimer) clearTimeout(pathPreviewTimer);
	});

	let initializationFailed = false;
	let initialLoadFinished = false;
	createEffect(
		on(
			() => props.recoveryRevision,
			() => {
				if (!initialLoadFinished) return;
				if (initializationFailed) void loadInitial();
				else if (error() && repo() && !editing()) {
					if (file() && openedEntry) void openFile(openedEntry);
					else void openDirectory(repo()!, dir());
				}
			},
		),
	);
	onMount(() => void loadInitial());
	async function loadInitial() {
		setBusy(true);
		setError("");
		try {
			if (props.initialLink) {
				await openLinkedFile(props.initialLink);
				initializationFailed = !!error();
				return;
			}
			const worktreePath = props.initialRepo?.worktreePath?.trim();
			if (worktreePath) {
				await openDirectory(worktreePath, "");
				initializationFailed = !!error();
				return;
			}
			const cwd = props.initialRepo?.cwd?.trim();
			if (props.initialRepo && !cwd) {
				setError("Repository path is unavailable for this session.");
				return;
			}
			const config = await rpc<{ repos?: Record<string, unknown> }>("load_repositories");
			const registered = Object.keys(config.repos ?? {});
			setRepos(registered);
			if (cwd) {
				const matching = registered.filter((path) => withinRoot(cwd, path)).sort((a, b) => b.length - a.length);
				if (matching.length === 0) {
					setError("No registered repository contains this session directory.");
					return;
				}
				await openDirectory(matching[0], "");
			} else {
				setRepos(registered);
				initializationFailed = false;
			}
		} catch (err) {
			initializationFailed = true;
			appLogger.warn("network", "Could not load mobile repositories", err);
			setError("Could not load repositories. Retry when the server is available.");
		} finally {
			initialLoadFinished = true;
			setBusy(false);
		}
	}

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
		setRepos(Object.keys(config.repos ?? {}));
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
		setRepo(repoPath);
		setDir(subdir);
		setBusy(true);
		setError("");
		try {
			const result = await rpc<FileEntry[]>("list_directory", { repoPath, subdir });
			if (currentRequest !== requestId) return;
			setEntries(result);
		} catch (err) {
			appLogger.warn("network", "Could not open mobile directory", err);
			if (currentRequest !== requestId) return;
			setError("Could not open directory. Try again.");
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
			appLogger.warn("network", "Could not search mobile files", err);
			if (currentRequest === searchRequestId) setError("Could not search files. Try again.");
		} finally {
			if (currentRequest === searchRequestId) setSearching(false);
		}
	}

	async function openFile(entry: FileEntry, line?: number) {
		openedEntry = entry;
		const currentRequest = ++requestId;
		setFile(entry.path);
		setEditedSinceSend(false);
		setSentCommentIds(new Set<string>());
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
			setError(/valid UTF-8/i.test(message) ? "This is a binary or non-text file." : "Could not open file. Try again.");
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
		if (written) {
			setContent(next);
			setEditedSinceSend(true);
		}
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
			setNotice("Could not save. Check the connection and try again.");
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
			setError("Could not save file. Check the connection and try again.");
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
					<Button class={styles.back} onClick={goBack} aria-label={props.onExit ? "Back to session" : "Back"}>
						‹ Back
					</Button>
				</Show>
				<strong class={styles.title} title={file() || dir() || repo() || "Files"}>
					<bdi dir="ltr">{file() ? repoName(file()!) : dir() || repoName(repo() ?? "") || "Files"}</bdi>
				</strong>
				<Show when={file() !== null && (editing() || (!error() && !busy()))}>
					<Show
						when={editing()}
						fallback={
							<Button
								class={styles.action}
								onClick={() => {
									setDraft(content());
									setEditing(true);
								}}
							>
								Edit
							</Button>
						}
					>
						<Button
							class={styles.action}
							onClick={() => {
								setEditing(false);
								setError("");
							}}
						>
							Cancel
						</Button>
						<Button class={styles.action} disabled={busy()} onClick={() => void save()}>
							Save
						</Button>
					</Show>
				</Show>
			</header>
			<Show when={previewPath()}>
				<div class={styles.pathPreviewBackdrop}>
					<div class={styles.pathPreview} role="dialog" aria-label="Repository path">
						<p>{previewPath()}</p>
						<Button
							class={styles.action}
							onClick={() => {
								suppressRepoOpen = false;
								setPreviewPath(null);
							}}
						>
							Close
						</Button>
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
			<Show when={repo() === null && !props.initialRepo && !busy() && !error()}>
				<Show when={repos().length > 0} fallback={<p class={styles.status}>No repositories configured</p>}>
					<For each={repos()}>
						{(path) => (
							<button
								data-control-layout="row"
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
					<Input
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
							data-control-layout="row"
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
							<>
								<Show when={hasReviewChanges()}>
									<div class={styles.reviewActions}>
										<label for="mobile-review-agent">{t("markdownTab.agentLabel", "Agent")}</label>
										<Select
											id="mobile-review-agent"
											aria-label={t("markdownTab.reviewAgent", "Review agent")}
											value={selectedAgentSession()}
											disabled={!reviewAgents().length || sendingChanges()}
											onChange={(event) => {
												setAgentChosenByUser(true);
												setSelectedAgentSession(event.currentTarget.value);
											}}
										>
											<Show when={!reviewAgents().length}>
												<option value="">{t("markdownTab.noAgents", "No agents")}</option>
											</Show>
											<For each={reviewAgents()}>
												{(agent) => <option value={agent.session_id}>{agent.display_name || agent.session_id}</option>}
											</For>
										</Select>
										<Button
											class={styles.action}
											data-ready={selectedAgentSession() && !sendingChanges() && !writing() ? "true" : undefined}
											disabled={!selectedAgentSession() || sendingChanges() || writing()}
											title={t("markdownTab.sendChanges", "Send changes to agent")}
											aria-label={t("markdownTab.sendChanges", "Send changes to agent")}
											onClick={() => void sendChanges()}
										>
											{sendingChanges() ? t("markdownTab.sending", "Sending…") : t("markdownTab.send", "Send")}
										</Button>
									</div>
								</Show>
								<div class={styles.markdownView}>
									<ReviewableMarkdown
										content={content()}
										imageSrc={markdownImageSrc}
										onCheckboxToggle={toggleTask}
										onSaveBlockComment={saveBlockComment}
									/>
								</div>
							</>
						) : (
							<pre class={styles.viewer}>{content()}</pre>
						)
					}
				>
					<textarea
						data-control-layout="editor"
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
