import { createMemo, createSignal, For, Index, mapArray, Show } from "solid-js";
import { Button } from "../../components/ui/Button";
import { Input } from "../../components/ui/Input";
import { appLogger } from "../../stores/appLogger";
import { rpc } from "../../transport";
import { HeroMetrics } from "../components/HeroMetrics";
import { NewSessionSheet } from "../components/NewSessionSheet";
import { SessionCard } from "../components/SessionCard";
import type { SessionInfo } from "../useSessions";
import {
	readRecentSessionIds,
	readSessionSort,
	recentFirst,
	type SessionSort,
	writeSessionSort,
} from "../utils/recentSessions";
import { ptysLast } from "../utils/sessionKind";
import { nestUnderParents, parentLabel } from "../utils/sessionTree";
import { useDebouncedStatus } from "../utils/useDebouncedStatus";
import styles from "./SessionsScreen.module.css";

interface SessionsScreenProps {
	sessions: SessionInfo[];
	loading: boolean;
	refreshing: boolean;
	error: string | null;
	onRefresh: () => void;
	onSelectSession: (sessionId: string) => void;
}

const PULL_THRESHOLD = 60;

export function SessionsScreen(props: SessionsScreenProps) {
	// Key by session id: polling replaces session objects, but must not reset the busy hold.
	const sessionsById = createMemo(() => new Map(props.sessions.map((session) => [session.session_id, session])));
	const sessionStatuses = mapArray(
		() => [...sessionsById().keys()],
		(sessionId) => ({ sessionId, status: useDebouncedStatus(() => sessionsById().get(sessionId)!) }),
	);
	const statusById = createMemo(() => new Map(sessionStatuses().map(({ sessionId, status }) => [sessionId, status])));
	const counts = createMemo(() => {
		const totals = { busy: 0, idle: 0, question: 0 };
		for (const { status } of sessionStatuses()) {
			const value = status();
			if (value === "sub-tasks") totals.busy++;
			else if (value === "busy" || value === "idle" || value === "question") totals[value]++;
		}
		return totals;
	});
	const [pulling, setPulling] = createSignal(false);
	const [pullY, setPullY] = createSignal(0);
	const [showNewSession, setShowNewSession] = createSignal(false);
	const [searchOpen, setSearchOpen] = createSignal(false);
	const [searchQuery, setSearchQuery] = createSignal("");
	const [repos, setRepos] = createSignal<string[]>([]);
	// Memoized so the sort runs on a real list change, not on every render.
	// `reconcileSessions` hands back the same array reference for an idle poll,
	// which keeps this inert — and keeps `<For>` from rebuilding every card.
	const [sort, setSort] = createSignal<SessionSort>(readSessionSort());
	const [sortOpen, setSortOpen] = createSignal(false);
	// The list screen is unmounted while a detail is open, so reading once per mount sees every open.
	const recentIds = readRecentSessionIds();
	const ordered = createMemo(() =>
		sort() === "recent" ? recentFirst(props.sessions, recentIds) : ptysLast(props.sessions),
	);
	const visibleSessions = createMemo(() => {
		const query = searchQuery().trim().toLowerCase();
		if (!query) return ordered();
		return ordered().filter((session) =>
			[
				session.display_name,
				session.cwd,
				session.worktree_path,
				session.worktree_branch,
				session.state?.agent_type,
			].some((value) => value?.toLowerCase().includes(query)),
		);
	});
	// Nest after filtering: a child whose parent was filtered out stays a visible top-level row.
	const rows = createMemo(() => nestUnderParents(visibleSessions()));
	let startY = 0;
	let listEl: HTMLDivElement | undefined;

	async function handleKill(sessionId: string) {
		if (!window.confirm("Kill this session?")) return;
		try {
			await rpc("close_pty", { sessionId });
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			appLogger.warn("network", `Failed to kill session: ${msg}`);
		}
	}

	async function openNewSessionSheet() {
		try {
			const config = await rpc<{ repos?: Record<string, unknown> }>("load_repositories");
			setRepos(config.repos ? Object.keys(config.repos) : []);
		} catch {
			setRepos([]);
		}
		setShowNewSession(true);
	}

	function onTouchStart(e: TouchEvent) {
		if (listEl && listEl.scrollTop === 0) {
			startY = e.touches[0].clientY;
			setPulling(true);
		}
	}

	function onTouchMove(e: TouchEvent) {
		if (!pulling()) return;
		const dy = Math.max(0, e.touches[0].clientY - startY);
		setPullY(Math.min(dy, PULL_THRESHOLD * 2));
	}

	function onTouchEnd() {
		if (pullY() >= PULL_THRESHOLD) {
			props.onRefresh();
		}
		setPullY(0);
		setPulling(false);
	}

	function chooseSort(next: SessionSort) {
		setSort(next);
		writeSessionSort(next);
		setSortOpen(false);
	}

	function toggleSearch() {
		if (searchOpen()) setSearchQuery("");
		setSearchOpen(!searchOpen());
	}

	return (
		<div ref={listEl} class={styles.list} onTouchStart={onTouchStart} onTouchMove={onTouchMove} onTouchEnd={onTouchEnd}>
			<div class={styles.searchHeader}>
				<div class={styles.searchRow}>
					<span class={styles.sectionTitle}>Sessions</span>
					<div class={styles.sortMenu}>
						<Button
							type="button"
							shape="icon"
							variant="ghost"
							aria-label="Sort sessions"
							aria-haspopup="menu"
							aria-expanded={sortOpen()}
							onClick={() => setSortOpen(!sortOpen())}
						>
							<svg
								width="20"
								height="20"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								stroke-width="2"
								stroke-linecap="round"
								aria-hidden="true"
							>
								<path d="M4 7h16M7 12h10M10 17h4" />
							</svg>
						</Button>
						<Show when={sortOpen()}>
							<div class={styles.sortList} role="menu" aria-label="Sort sessions">
								<For
									each={
										[
											["default", "Default"],
											["recent", "Recent"],
										] as const
									}
								>
									{([value, label]) => (
										<Button
											type="button"
											role="menuitemradio"
											aria-checked={sort() === value}
											variant="ghost"
											class={styles.sortOption}
											classList={{ [styles.sortOptionActive]: sort() === value }}
											onClick={() => chooseSort(value)}
										>
											{label}
										</Button>
									)}
								</For>
							</div>
						</Show>
					</div>
					<Button
						type="button"
						shape="icon"
						variant="ghost"
						aria-label={searchOpen() ? "Close session search" : "Search sessions"}
						aria-expanded={searchOpen()}
						onClick={toggleSearch}
					>
						<Show
							when={searchOpen()}
							fallback={
								<svg
									width="20"
									height="20"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									aria-hidden="true"
								>
									<circle cx="11" cy="11" r="7" />
									<path d="m16 16 5 5" />
								</svg>
							}
						>
							<svg
								width="20"
								height="20"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								stroke-width="2"
								aria-hidden="true"
							>
								<path d="M18 6 6 18M6 6l12 12" />
							</svg>
						</Show>
					</Button>
				</div>
				<Show when={searchOpen()}>
					<div class={styles.searchField}>
						<Input
							ref={(element) => queueMicrotask(() => element.focus())}
							type="search"
							aria-label="Filter sessions"
							placeholder="Name, repository, branch or agent"
							value={searchQuery()}
							onInput={(event) => setSearchQuery(event.currentTarget.value)}
							onKeyDown={(event) => {
								if (event.key === "Escape") toggleSearch();
							}}
						/>
					</div>
				</Show>
			</div>
			<Show when={pullY() > 0}>
				<div class={styles.pullIndicator} style={{ height: `${pullY()}px` }}>
					<span classList={{ [styles.pullReady]: pullY() >= PULL_THRESHOLD }}>
						{pullY() >= PULL_THRESHOLD ? "Release to refresh" : "Pull to refresh"}
					</span>
				</div>
			</Show>

			<Show when={props.refreshing && props.sessions.length > 0}>
				<div class={styles.refreshSpinner}>
					<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
						<path d="M21 12a9 9 0 1 1-6.22-8.56" />
					</svg>
				</div>
			</Show>

			<Show when={props.error}>
				<div class={styles.errorBanner}>{props.error}</div>
			</Show>

			<Show when={props.sessions.length > 0}>
				<HeroMetrics
					workingCount={counts().busy}
					idleCount={counts().idle}
					awaitingCount={counts().question}
					onAwaitingClick={() => {
						const waiting = props.sessions.find((session) => statusById().get(session.session_id)?.() === "question");
						if (waiting) props.onSelectSession(waiting.session_id);
					}}
				/>
			</Show>

			<Show when={props.loading && props.sessions.length === 0}>
				<Index each={[0, 1, 2]}>
					{() => (
						<div class={styles.skeleton}>
							<div class={styles.skeletonIcon} />
							<div class={styles.skeletonBody}>
								<div class={styles.skeletonLine} style={{ width: "40%" }} />
								<div class={styles.skeletonLine} style={{ width: "60%" }} />
							</div>
						</div>
					)}
				</Index>
			</Show>

			<Show when={!props.loading && props.sessions.length === 0 && !props.error}>
				<div class={styles.empty}>
					<div class={styles.emptyIcon}>
						<svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
							<rect x="2" y="3" width="20" height="14" rx="2" />
							<path d="M8 21h8M12 17v4" />
						</svg>
					</div>
					<span class={styles.emptyTitle}>No active sessions</span>
					<span class={styles.emptyHint}>Start an agent from TUICommander to see it here</span>
				</div>
			</Show>

			<Show when={searchQuery().trim() && visibleSessions().length === 0 && props.sessions.length > 0}>
				<div class={styles.noMatch}>
					<span class={styles.emptyTitle}>No matching sessions</span>
					<span class={styles.emptyHint}>Try another name, repository, branch or agent</span>
				</div>
			</Show>

			<For each={rows()}>
				{(row) => (
					<SessionCard
						session={row.session}
						status={statusById().get(row.session.session_id)}
						depth={row.depth}
						lastInGroup={row.last}
						spawnedBy={row.session.parent_session ? parentLabel(rows(), row.session) : undefined}
						onSelect={props.onSelectSession}
						onKill={handleKill}
					/>
				)}
			</For>

			<div class={styles.fab}>
				<Button
					shape="icon"
					circular
					variant="primary"
					aria-label="New session"
					onClick={openNewSessionSheet}
					data-testid="new-session-fab"
				>
					<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
						<line x1="12" y1="5" x2="12" y2="19" />
						<line x1="5" y1="12" x2="19" y2="12" />
					</svg>
				</Button>
			</div>

			<Show when={showNewSession()}>
				<NewSessionSheet
					repos={repos()}
					onDismiss={() => setShowNewSession(false)}
					onCreated={(sessionId) => {
						props.onSelectSession(sessionId);
						props.onRefresh();
					}}
				/>
			</Show>
		</div>
	);
}
