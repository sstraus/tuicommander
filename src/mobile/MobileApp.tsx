import { createEffect, createMemo, createSignal, lazy, Match, on, onCleanup, onMount, Show, Switch } from "solid-js";
import { McpConfirmHost } from "../components/McpConfirmHost/McpConfirmHost";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { ideasStore } from "../stores/ideas";
import { BottomTabs, type TabId } from "./components/BottomTabs";
import { ConnectionBanner } from "./components/ConnectionBanner";
import { MobileToastContainer } from "./components/MobileToastContainer";
import { QuestionBanner } from "./components/QuestionBanner";
import { TopBar } from "./components/TopBar";
import styles from "./MobileApp.module.css";
import { SessionsScreen } from "./screens/SessionsScreen";
import { useMobileNotifications } from "./useMobileNotifications";
import { useSessions } from "./useSessions";
import { useVersionCheck } from "./useVersionCheck";
import { recordSessionOpened } from "./utils/recentSessions";

// Screens behind a bottom-tab tap stay out of the initial mobile graph.
// Eager imports dragged the settings store and the whole i18n string table into
// the initial mobile graph, which is what pushed mobile.html over its gzip budget.
const ActivityScreen = lazy(() => import("./screens/ActivityScreen").then((m) => ({ default: m.ActivityScreen })));
const MobileChatScreen = lazy(() =>
	import("./screens/MobileChatScreen").then((m) => ({ default: m.MobileChatScreen })),
);
const FilesScreen = lazy(() => import("./screens/FilesScreen").then((m) => ({ default: m.FilesScreen })));
const ProgressDialog = lazy(() => import("../components/ProgressDialog").then((m) => ({ default: m.ProgressDialog })));
const SettingsScreen = lazy(() => import("./screens/SettingsScreen").then((m) => ({ default: m.SettingsScreen })));
const SessionDetailScreen = lazy(() =>
	import("./screens/SessionDetailScreen").then((m) => ({ default: m.SessionDetailScreen })),
);

// Register service worker for push notifications (only on HTTPS or localhost)
if (
	"serviceWorker" in navigator &&
	(location.protocol === "https:" || location.hostname === "localhost" || location.hostname === "127.0.0.1")
) {
	navigator.serviceWorker.register("/sw.js").catch((err: unknown) => {
		// Log but don't block — push will be unavailable
		appLogger.warn("sw", "Service worker registration failed", err);
	});
}

/** Extract session ID from deep link path like /mobile/session/<id> */
function sessionIdFromUrl(): string | null {
	const match = location.pathname.match(/^\/mobile\/session\/(.+)/);
	return match ? decodeURIComponent(match[1]) : null;
}

export default function MobileApp() {
	// iOS Safari/PWA keyboard handling.
	// Track visualViewport height and offsetTop to resize and reposition
	// the fixed shell. html/body are height:auto so the document has no
	// scrollable content — iOS can't scroll the page on keyboard open.
	onMount(() => {
		void import("./mobileTheme")
			.then(({ loadMobileTheme }) => loadMobileTheme())
			.catch((error: unknown) => appLogger.warn("app", "Could not load mobile theme", error));
		const vv = window.visualViewport;
		if (!vv) return;
		let raf = 0;
		const update = () => {
			cancelAnimationFrame(raf);
			raf = requestAnimationFrame(() => {
				document.documentElement.style.setProperty("--app-height", `${vv.height}px`);
				document.documentElement.style.setProperty("--app-top", `${vv.offsetTop}px`);
			});
		};
		const onVisible = () => {
			if (document.visibilityState === "visible") update();
		};
		const pinScroll = () => {
			window.scrollTo(0, 0);
		};

		update();
		vv.addEventListener("resize", update);
		vv.addEventListener("scroll", update);
		// A restored document may have new geometry without a viewport event.
		window.addEventListener("pageshow", update);
		document.addEventListener("visibilitychange", onVisible);
		document.addEventListener("resume", update);
		window.addEventListener("scroll", pinScroll);
		onCleanup(() => {
			vv.removeEventListener("resize", update);
			vv.removeEventListener("scroll", update);
			window.removeEventListener("pageshow", update);
			document.removeEventListener("visibilitychange", onVisible);
			document.removeEventListener("resume", update);
			window.removeEventListener("scroll", pinScroll);
			cancelAnimationFrame(raf);
		});
	});

	const [streamError, setStreamError] = createSignal<string | null>(null);
	let retryOutput: (() => void) | undefined;
	const [activeTab, setActiveTab] = createSignal<TabId>("sessions");
	const [chatFileLink, setChatFileLink] = createSignal<{ candidate: string; cwd: string } | null>(null);
	const [recoveryRevision, setRecoveryRevision] = createSignal(0);
	const [progressProjects, setProgressProjects] = createSignal<string[] | undefined>();
	const [progressProjectsError, setProgressProjectsError] = createSignal<string | null>(null);
	createEffect(() => {
		if (activeTab() !== "progress") return;
		recoveryRevision();
		let cancelled = false;
		setProgressProjects(undefined);
		setProgressProjectsError(null);
		void invoke<string[]>("progress_projects")
			.then((projects) => {
				if (!cancelled) setProgressProjects(projects);
			})
			.catch((error: unknown) => {
				if (cancelled) return;
				setProgressProjectsError("Progress projects are unavailable.");
				appLogger.warn("network", "Could not list Progress projects", error);
			});
		onCleanup(() => {
			cancelled = true;
		});
	});
	const [selectedSessionId, setSelectedSessionId] = createSignal<string | null>(sessionIdFromUrl());
	const [sessionFilesOpen, setSessionFilesOpen] = createSignal(false);
	const [sessionFileLink, setSessionFileLink] = createSignal<{
		candidate: string;
		line?: number;
		root?: string;
		openerTuicSession?: string;
	} | null>(null);
	const { aiChatAvailable, sessions, loading, refreshing, error, authError, refresh, markSeen, subscribeChatWake } =
		useSessions(() => setRecoveryRevision((revision) => revision + 1));
	createEffect(() => {
		if (!aiChatAvailable() && activeTab() === "chat") setActiveTab("sessions");
	});
	const { updateAvailable, applyUpdate } = useVersionCheck();
	ideasStore.hydrate();

	// Keep the last known session data so the detail screen stays mounted
	// and can show the "Session ended" overlay after a session closes.
	const [lastKnownSession, setLastKnownSession] = createSignal<ReturnType<typeof sessions>[number] | null>(null);

	const liveSession = createMemo(() => {
		const id = selectedSessionId();
		if (!id) return null;
		return sessions().find((s) => s.session_id === id) ?? null;
	});

	// Feeds the Sessions "Recent" sort; also covers a detail opened from the URL or a notification.
	createEffect(on(selectedSessionId, (id) => id && recordSessionOpened(id)));

	// Update last known session whenever live data arrives; keep stale value when gone
	createEffect(() => {
		const live = liveSession();
		if (live) {
			if (live.unseen) markSeen(live.session_id);
			setLastKnownSession(live);
		}
	});

	const sessionExists = createMemo(() => {
		const id = selectedSessionId();
		if (!id) return false;
		return sessions().some((s) => s.session_id === id);
	});

	function navigateToSession(id: string) {
		markSeen(id);
		setSelectedSessionId(id);
	}

	function handleBack() {
		setSessionFilesOpen(false);
		setSessionFileLink(null);
		setSelectedSessionId(null);
		setLastKnownSession(null);
	}

	const showDetail = () => selectedSessionId() !== null && lastKnownSession() !== null;
	useMobileNotifications(sessions, () => activeTab() === "sessions" && !showDetail());

	const updateBanner = () => (
		<Show when={updateAvailable() && !showDetail() && activeTab() === "sessions"}>
			<div class={styles.updateBanner} onClick={applyUpdate}>
				<span>New version available</span>
				<span>Tap to update</span>
			</div>
		</Show>
	);

	return (
		<div class={styles.shell} data-mobile-shell>
			<ConnectionBanner
				offline={error() !== null || streamError() !== null}
				authError={authError()}
				onRetry={() => {
					refresh();
					retryOutput?.();
				}}
				error={error() ?? streamError()}
				testing={refreshing()}
			/>
			<Show
				when={showDetail()}
				fallback={
					<>
						<TopBar isConnected={error() === null} onOpenSettings={() => setActiveTab("settings")} />
						{updateBanner()}
						<QuestionBanner sessions={sessions()} onNavigate={navigateToSession} />
						<main class={styles.content}>
							<div hidden={error() !== null || loading()} class={styles.views}>
								<Switch>
									<Match when={activeTab() === "chat"}>
										<MobileChatScreen
											recoveryRevision={recoveryRevision()}
											onOpenFile={(candidate, cwd) => {
												setChatFileLink({ candidate, cwd });
												setActiveTab("files");
											}}
										/>
									</Match>
									<Match when={activeTab() === "sessions"}>
										<SessionsScreen
											sessions={sessions()}
											loading={loading()}
											refreshing={refreshing()}
											error={error()}
											onRefresh={refresh}
											onSelectSession={navigateToSession}
										/>
									</Match>
									<Match when={activeTab() === "activity"}>
										<ActivityScreen recoveryRevision={recoveryRevision()} onNavigateSession={navigateToSession} />
									</Match>
									<Match when={activeTab() === "files"}>
										<Show
											when={chatFileLink()}
											fallback={<FilesScreen sessions={sessions()} recoveryRevision={recoveryRevision()} />}
										>
											{(link) => (
												<FilesScreen
													sessions={sessions()}
													recoveryRevision={recoveryRevision()}
													initialRepo={{ cwd: link().cwd, worktreePath: link().cwd }}
													initialLink={{ candidate: link().candidate }}
													onExit={() => {
														setChatFileLink(null);
														setActiveTab("chat");
													}}
												/>
											)}
										</Show>
									</Match>
									<Match when={activeTab() === "progress"}>
										<ProgressDialog
											embedded
											projects={progressProjects()}
											projectsError={progressProjectsError() ?? undefined}
										/>
									</Match>
									<Match when={activeTab() === "settings"}>
										<SettingsScreen recoveryRevision={recoveryRevision()} isConnected={error() === null} />
									</Match>
								</Switch>
							</div>
						</main>
						<BottomTabs
							active={activeTab()}
							aiChatAvailable={aiChatAvailable()}
							onSelect={(tab) => {
								setChatFileLink(null);
								setActiveTab(tab);
							}}
						/>
					</>
				}
			>
				<div class={styles.sessionPane} classList={{ [styles.sessionPaneHidden]: sessionFilesOpen() }}>
					<SessionDetailScreen
						onConnectionError={setStreamError}
						onRegisterRetry={(retry) => {
							retryOutput = retry;
						}}
						subscribeChatWake={subscribeChatWake}
						session={lastKnownSession()!}
						sessionExists={sessionExists()}
						onBack={handleBack}
						onOpenFiles={() => {
							setSessionFileLink(null);
							setSessionFilesOpen(true);
						}}
						onOpenFileLink={(candidate, line, root, openerTuicSession) => {
							setSessionFileLink({ candidate, line, root, openerTuicSession });
							setSessionFilesOpen(true);
						}}
					/>
				</div>
				<Show when={sessionFilesOpen()}>
					<main class={styles.content}>
						<FilesScreen
							sessions={sessions()}
							recoveryRevision={recoveryRevision()}
							initialSessionId={lastKnownSession()!.session_id}
							openerTuicSession={sessionFileLink()?.openerTuicSession}
							initialLink={sessionFileLink() ?? undefined}
							initialRepo={{
								worktreePath: sessionFileLink()?.root ?? lastKnownSession()!.worktree_path,
								cwd: lastKnownSession()!.cwd,
							}}
							onExit={() => {
								setSessionFilesOpen(false);
								setSessionFileLink(null);
							}}
						/>
					</main>
				</Show>
			</Show>
			<MobileToastContainer />
			<McpConfirmHost />
		</div>
	);
}
