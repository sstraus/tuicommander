/**
 * Theme gallery — `theme-gallery.html`, dev server only.
 *
 * One row per bundled theme: the shipped theme and, beside it, the edit
 * proposed in themeProposals.ts. Each card mounts the REAL chrome components
 * (Toolbar, Sidebar, TabBar, StatusBar) over the singleton stores, seeded once
 * below. The terminal is the one stand-in: a static text block in the theme's
 * ANSI colors, since a real pane needs a PTY. Vite HMR repaints every card
 * when a component or stylesheet changes, and the cards re-read the theme
 * JSONs from `/__tuic_themes` (vite.config.ts) every second, so a theme edit
 * shows up without a restart too.
 *
 * theme-gallery.html installs a fake `__TAURI_INTERNALS__` so `isTauri()` is
 * true and every `invoke` lands in `answerInvoke` below instead of the
 * network. Stores that persist on write (ui prefs, settings, activity) are
 * answered with null and simply never save.
 *
 * Open it at http://127.0.0.1:1421/theme-gallery.html while `make dev` runs.
 * As a TUIC `ui` tab inside a dev build use http://localhost:1421/... instead:
 * the dev app itself is served from 127.0.0.1:1421, and PluginPanel resets a
 * same-origin iframe to about:blank (`guardSameOriginNav`).
 * It is not a rollup input, so it never ships in a build.
 */
import { createSignal, For, type JSX, onCleanup } from "solid-js";
import { render } from "solid-js/web";
import "../global.css";
import "../styles.css";
import { Sidebar } from "../components/Sidebar/Sidebar";
import { StatusBar } from "../components/StatusBar/StatusBar";
import dialog from "../components/shared/dialog.module.css";
import { TabBar } from "../components/TabBar/TabBar";
import { Toolbar } from "../components/Toolbar/Toolbar";
import { Button } from "../components/ui/Button";
import { applyPlatformClass } from "../platform";
import { diffTabsStore } from "../stores/diffTabs";
import { editorTabsStore } from "../stores/editorTabs";
import { githubStore } from "../stores/github";
import { mdTabsStore } from "../stores/mdTabs";
import { prNotificationsStore } from "../stores/prNotifications";
import { repoSettingsStore } from "../stores/repoSettings";
import { repositoriesStore } from "../stores/repositories";
import { settingsStore } from "../stores/settings";
import { statusBarTicker } from "../stores/statusBarTicker";
import { terminalsStore } from "../stores/terminals";
import { buildPrStatus } from "./presets";
import { THEME_PROPOSALS, type ThemeProposal } from "./themeProposals";

/** Windows Terminal-style theme JSON as shipped in src-tauri/src/themes/. */
interface ThemeJson {
	key: string;
	name?: string;
	appChrome?: Record<string, string>;
	[color: string]: string | Record<string, string> | undefined;
}

/**
 * JSON `appChrome` field -> CSS custom property. Mirrors `map_app_chrome_json`
 * in src-tauri/src/themes.rs and the camelToKebab pass in src/themes.ts; the
 * gallery has no backend to ask, so the mapping lives here too.
 */
const CHROME_VARS: readonly [json: string, cssVar: string][] = [
	["background", "--bg-primary"],
	["surface", "--bg-secondary"],
	["surfaceElevated", "--bg-tertiary"],
	["highlight", "--bg-highlight"],
	["foreground", "--fg-primary"],
	["foregroundSecondary", "--fg-secondary"],
	["mutedForeground", "--fg-muted"],
	["accent", "--accent"],
	["accentHover", "--accent-hover"],
	["border", "--border"],
	["success", "--success"],
	["warning", "--warning"],
	["error", "--error"],
	["accentForeground", "--text-on-accent"],
	["errorForeground", "--text-on-error"],
	["successForeground", "--text-on-success"],
];

/** JSON ANSI key -> `--ansi-*` suffix (the JSON says purple, the app says magenta). */
const ANSI_VARS: readonly [json: string, suffix: string][] = [
	["black", "black"],
	["red", "red"],
	["green", "green"],
	["yellow", "yellow"],
	["blue", "blue"],
	["purple", "magenta"],
	["cyan", "cyan"],
	["white", "white"],
	["brightBlack", "bright-black"],
	["brightRed", "bright-red"],
	["brightGreen", "bright-green"],
	["brightYellow", "bright-yellow"],
	["brightBlue", "bright-blue"],
	["brightPurple", "bright-magenta"],
	["brightCyan", "bright-cyan"],
	["brightWhite", "bright-white"],
];

/** The shipped theme with a proposal's overrides merged over it. */
function applyProposal(t: ThemeJson, p: ThemeProposal): ThemeJson {
	return { ...t, ...p.colors, appChrome: { ...t.appChrome, ...p.appChrome } };
}

function str(t: ThemeJson, key: string): string {
	const v = t[key];
	return typeof v === "string" ? v : "";
}

function hexToRgb(hex: string): string {
	const h = hex.replace("#", "");
	return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)).join(", ");
}

function cssVars(t: ThemeJson): JSX.CSSProperties {
	const vars: Record<string, string> = {};
	const ac = t.appChrome ?? {};
	for (const [json, cssVar] of CHROME_VARS) vars[cssVar] = ac[json] ?? "";
	vars["--accent-rgb"] = hexToRgb(ac.accent ?? "#000000");
	for (const [json, suffix] of ANSI_VARS) vars[`--ansi-${suffix}`] = str(t, json);
	vars["--term-bg"] = str(t, "background");
	vars["--term-fg"] = str(t, "foreground");
	return vars as JSX.CSSProperties;
}

// ---------------------------------------------------------------------------
// Fake backend
// ---------------------------------------------------------------------------

const noop = () => {};
const seen = new Set<string>();

/** Answers for the invokes the mounted components fire on their own. Anything
 *  else resolves to null once, logged so a new caller is noticed, not hidden. */
const INVOKE_ANSWERS: Record<string, unknown> = {
	get_remote_url: "git@github.com:sstraus/tuicommander.git",
	get_working_tree_status: { staged: [1], unstaged: [1, 2], untracked: ["theme-gallery.html"] },
	detect_installed_ides: [],
	"plugin:event|listen": 1,
	"plugin:event|unlisten": null,
	push_log: null,
	// Persistence and pollers the seeded stores trigger; nothing to answer.
	github_poll_repo: null,
	github_update_paths: null,
	save_repo_settings: null,
	set_hot_repos: null,
	set_session_visible: null,
	warm_content_index: null,
};

function answerInvoke(cmd: string, _args: unknown): Promise<unknown> {
	if (cmd in INVOKE_ANSWERS) return Promise.resolve(INVOKE_ANSWERS[cmd]);
	if (!seen.has(cmd)) {
		seen.add(cmd);
		console.debug(`[gallery] unanswered invoke: ${cmd}`);
	}
	return Promise.resolve(null);
}

// ---------------------------------------------------------------------------
// Store seed — one fixture shared by every card
// ---------------------------------------------------------------------------

const REPO = "/Users/me/Gits/personal/tuicommander";
const WIZ = "/Users/me/Gits/wiz-agents";
const STEPS = "/Users/me/Gits/steps";
const BRAIN = "/Users/me/Gits/brainstorming";

function addTerminal(name: string, repoPath: string, cwd = repoPath): string {
	return terminalsStore.add({
		sessionId: null,
		fontSize: settingsStore.state.defaultFontSize,
		name,
		cwd,
		repoPath,
		awaitingInput: null,
	});
}

function addRepo(path: string, displayName: string, color?: string): void {
	repositoriesStore.add({ path, displayName });
	if (color) {
		repoSettingsStore.getOrCreate(path, displayName);
		repoSettingsStore.update(path, { color });
	}
}

function seedStores(): void {
	// HMR re-runs this module; the stores survive, so seed only once.
	if (repositoriesStore.get(REPO)) return;
	settingsStore.setAutoShowPrPopover(false);

	const group = repositoriesStore.createGroup("Progetti");
	addRepo(BRAIN, "brainstorming", "#b19cd9");
	if (group) {
		repositoriesStore.setGroupColor(group, "#b19cd9");
		repositoriesStore.addRepoToGroup(BRAIN, group);
	}
	repositoriesStore.setWorkspace(BRAIN, "main");
	repositoriesStore.addTerminalToWorkspace(BRAIN, "main", addTerminal("shell", BRAIN));

	addRepo(STEPS, "steps");
	repositoriesStore.setWorkspace(STEPS, "master", { additions: 215, deletions: 317 });

	addRepo(REPO, "tuicommander", "#ff6b6b");
	repositoriesStore.setWorkspace(REPO, "main", { additions: 3900, deletions: 404 });
	repositoriesStore.setWorkspace(REPO, "feat/ai-fingerprint-coverage", { worktreePath: `${REPO}/.wt/fp` });
	repositoriesStore.setWorkspace(REPO, "POC-00013-lock", {
		worktreePath: `${REPO}/.wt/lock`,
		additions: 844,
		deletions: 195,
	});
	repositoriesStore.setWorkspace(REPO, "POC-00001-tool", { worktreePath: `${REPO}/.wt/tool` });
	repositoriesStore.setWorkspace(REPO, "split-config", { worktreePath: `${REPO}/.wt/split` });
	repositoriesStore.setActiveWorkspace(REPO, "main");
	repositoriesStore.setActive(REPO);

	const agents: [string, string, Partial<Parameters<typeof terminalsStore.update>[1]>][] = [
		[
			"main",
			"Clean theme: neutral dark look",
			{ agentType: "claude", shellState: "busy", activity: true, sessionId: "gallery-parent" },
		],
		[
			"835-314c Give the progress toast a button",
			"Give the progress toast a button",
			{ agentType: "claude", shellState: "idle", parentSession: "gallery-parent" },
		],
		[
			"Handoff sessione",
			"creating stories from the remote plan",
			{ agentType: "codex", shellState: "idle", unseen: true },
		],
		[
			"Reload forzato",
			"Reload forzato",
			{ agentType: "claude", shellState: "idle", awaitingInput: "question", awaitingInputConfident: true },
		],
	];
	let first: string | null = null;
	for (const [name, intent, patch] of agents) {
		const id = addTerminal(name, REPO);
		first ??= id;
		terminalsStore.update(id, { ...patch, agentIntent: intent });
		repositoriesStore.addTerminalToWorkspace(REPO, "main", id);
	}
	settingsStore.setTabTreeEnabled(true);
	for (const id of Object.keys(terminalsStore.state.terminals)) {
		terminalsStore.update(id, { lastDataAt: Date.now() - 4 * 60_000 });
	}

	githubStore.updateRepoData(REPO, [
		buildPrStatus("feat/ai-fingerprint-coverage", { number: 12, title: "AI fingerprint coverage" }),
		buildPrStatus("POC-00013-lock", { number: 256, title: "Lock handling", review_decision: "REVIEW_REQUIRED" }),
		buildPrStatus("POC-00001-tool", { number: 98, title: "Tool routing", state: "MERGED" }),
		buildPrStatus("split-config", {
			number: 1078,
			title: "Split configuration",
			mergeable: "CONFLICTING",
			merge_state_status: "DIRTY",
		}),
		// No local workspace for this branch: a remote-only PR is what makes the
		// repo header show its GitHub badge (RepoSection ghBadgeCount).
		buildPrStatus("fix/remote-review", { number: 1102, title: "Remote review fixes" }),
	]);

	addRepo(WIZ, "wiz-agents", "#ffb347");
	repositoriesStore.setWorkspace(WIZ, "master", { additions: 4, deletions: 85 });

	mdTabsStore.add(REPO, "docs/FEATURES.md");
	editorTabsStore.add(REPO, "src-tauri/src/themes.rs", 42, { background: true });
	diffTabsStore.add(REPO, "src/global.css", "M");
	mdTabsStore.openUiTab("mission-control", "Mission Control", "", true, "http://127.0.0.1:14319/", false);
	// Back to the first agent tab: every add above activated itself.
	if (first) terminalsStore.setActive(first);

	prNotificationsStore.add({
		repoPath: REPO,
		branch: "POC-00001-tool",
		prNumber: 98,
		title: "Tool routing",
		type: "merged",
	});
	prNotificationsStore.add({
		repoPath: REPO,
		branch: "split-config",
		prNumber: 1078,
		title: "Split configuration",
		type: "blocked",
	});
	statusBarTicker.addMessage({
		id: "stories",
		pluginId: "wiz",
		label: "Stories",
		text: "13 open",
		priority: 0,
		ttlMs: 0,
	});
}

// ---------------------------------------------------------------------------
// Card
// ---------------------------------------------------------------------------

const Terminal = () => (
	<div class="gallery-terminal" style={{ "font-size": `${settingsStore.state.defaultFontSize}px` }}>
		<span style={{ color: "var(--ansi-blue)" }}>agent/src/main/**</span> non ha <b>nessuna</b> voce.
		<br />
		<span style={{ color: "var(--ansi-green)" }}>✓ Bash ×12</span> |{" "}
		<span style={{ color: "var(--ansi-yellow)" }}>merge-main</span> resta in attesa
		<br />
		<span style={{ color: "var(--ansi-red)" }}>-const port = 8080;</span>
		<br />
		<span style={{ color: "var(--ansi-green)" }}>+const port = 3000;</span>
		<br />
		<span style={{ color: "var(--ansi-magenta)" }}>▸ bypass permissions on</span> ·{" "}
		<span style={{ color: "var(--ansi-cyan)" }}>1 shell</span> ·{" "}
		<span style={{ color: "var(--ansi-bright-black)" }}>gpt-5.6-sol medium</span>
	</div>
);

const SWATCH_KEYS = [
	"background",
	"surface",
	"surfaceElevated",
	"highlight",
	"border",
	"foreground",
	"foregroundSecondary",
	"mutedForeground",
	"accent",
	"accentHover",
	"success",
	"warning",
	"error",
];

const Card = (p: { theme: ThemeJson; label: string; note?: string }) => (
	<section class="gallery-card" data-theme={p.theme.key} style={cssVars(p.theme)}>
		<h2>
			{p.theme.name ?? p.theme.key} <code>{p.theme.key}</code> <span class="gallery-label">{p.label}</span>
		</h2>
		<p class="gallery-note">{p.note ?? "\u00a0"}</p>
		<div class="gallery-swatches">
			<For each={SWATCH_KEYS}>
				{(k) => (
					<span
						title={`${k} ${p.theme.appChrome?.[k] ?? ""}`}
						style={{ background: p.theme.appChrome?.[k] ?? "transparent" }}
					/>
				)}
			</For>
		</div>
		{/* Same ids as App.tsx so the layout rules in global.css / styles.css apply. */}
		<div id="app">
			<Toolbar repoPath={REPO} />
			<div id="app-body">
				<Sidebar
					onBranchSelect={noop}
					onAddTerminal={noop}
					onRemoveBranch={noop}
					onRenameBranch={noop}
					onAddWorktree={noop}
					onAddRepo={noop}
					onRepoSettings={noop}
					onRemoveRepo={noop}
					onOpenSettings={noop}
				/>
				<main id="main">
					<div id="tab-bar">
						<TabBar onTabSelect={noop} onTabClose={noop} onCloseOthers={noop} onCloseToRight={noop} onNewTab={noop} />
					</div>
					<Terminal />
					<div class="gallery-dialog-slot">
						<div class={dialog.popover}>
							<div class={dialog.header}>
								<span class={dialog.headerText}>Remove worktree?</span>
							</div>
							<div class={dialog.body}>
								<a href="#top" style={{ color: "var(--accent)" }}>
									feat/ai-fingerprint-coverage
								</a>{" "}
								has uncommitted changes.
							</div>
							<div class={dialog.actions}>
								<Button type="button">Cancel</Button>
								<Button type="button" variant="primary">
									Remove
								</Button>
							</div>
						</div>
					</div>
				</main>
			</div>
			<StatusBar
				fontSize={settingsStore.state.defaultFontSize}
				defaultFontSize={settingsStore.state.defaultFontSize}
				statusInfo="Ready"
				currentRepoPath={REPO}
				cwd={REPO}
				repoRoot={REPO}
				onToggleDiff={noop}
				onToggleMarkdown={noop}
				onDictationStart={noop}
				onDictationStop={noop}
			/>
		</div>
	</section>
);

/** Shipped theme on the left, the proposal from themeProposals.ts on the right. */
const Pair = (p: { theme: ThemeJson }) => {
	const proposal = () => THEME_PROPOSALS[p.theme.key];
	return (
		<div class="gallery-pair">
			<Card theme={p.theme} label="current" />
			{(() => {
				const prop = proposal();
				if (!prop) return <div class="gallery-verdict">No change proposed.</div>;
				if (prop.remove)
					return (
						<div class="gallery-verdict gallery-remove">
							<b>Proposed removal</b>
							<span>{prop.note}</span>
						</div>
					);
				return <Card theme={applyProposal(p.theme, prop)} label="proposed" note={prop.note} />;
			})()}
		</div>
	);
};

const GALLERY_CSS = `
html, body { overflow: auto; }
body { background: #111; color: #ddd; padding: 16px; font-family: var(--font-ui); }
.gallery-head { font-size: 15px; font-weight: 600; color: #aaa; margin-bottom: 12px; }
.gallery-head small { font-weight: 400; color: #666; margin-left: 10px; }
.gallery-grid { display: flex; flex-direction: column; gap: 28px; }
.gallery-pair { display: grid; grid-template-columns: 1fr 1fr; gap: 18px; }
.gallery-card { min-width: 0; }
.gallery-label { font-size: 11px; font-weight: 500; color: #888; margin-left: 6px; text-transform: uppercase; letter-spacing: 0.05em; }
.gallery-note { font-size: 12px; color: #999; margin: 0 0 6px; min-height: 16px; }
.gallery-verdict { display: flex; flex-direction: column; justify-content: center; align-items: center; gap: 8px; border: 1px dashed #333; border-radius: 6px; color: #666; font-size: 13px; padding: 24px; text-align: center; }
.gallery-remove { border-color: #7a2e2e; color: #d88; }
.gallery-card h2 { font-size: 13px; font-weight: 600; color: #ccc; margin: 0 0 6px; }
.gallery-card h2 code { font-weight: 400; color: #777; margin-left: 6px; }
.gallery-swatches { display: flex; gap: 2px; margin-bottom: 6px; }
.gallery-swatches span { width: 20px; height: 12px; border-radius: 2px; border: 1px solid #000; }
/* #app is sized to the viewport in global.css; a card is a fixed window instead. */
/* The app inherits text color from html/body (global.css); the gallery body is fixed #ddd, so each card restores it. */
.gallery-card #app { height: 600px; width: auto; border: 1px solid #000; border-radius: 6px; overflow: hidden; color: var(--fg-primary); background: var(--bg-primary); }
.gallery-dialog-slot { display: flex; justify-content: center; padding: 0 0 24px; }
.gallery-terminal { flex: 1; background: var(--term-bg); color: var(--term-fg); font-family: var(--font-mono); line-height: 1.6; padding: 8px 10px; overflow: hidden; }
`;

function Gallery() {
	const [themes, setThemes] = createSignal<ThemeJson[]>([]);
	const [error, setError] = createSignal("");
	let lastBody = "";
	const poll = async () => {
		try {
			const body = await (await fetch("/__tuic_themes")).text();
			if (body !== lastBody) {
				lastBody = body;
				setThemes(JSON.parse(body) as ThemeJson[]);
			}
			setError("");
		} catch (e) {
			setError(String(e));
		}
	};
	void poll();
	const timer = setInterval(poll, 1000);
	onCleanup(() => clearInterval(timer));
	return (
		<>
			<style>{GALLERY_CSS}</style>
			<div class="gallery-head">
				{themes().length} bundled themes — current on the left, proposal (src/dev/themeProposals.ts) on the right
				<small>
					live: components and stylesheets via HMR, theme JSONs polled every second {error() && `— ${error()}`}
				</small>
			</div>
			<div class="gallery-grid">
				<For each={themes()}>{(t) => <Pair theme={t} />}</For>
			</div>
		</>
	);
}

(window as unknown as { __GALLERY_INVOKE__: typeof answerInvoke }).__GALLERY_INVOKE__ = answerInvoke;
applyPlatformClass();
seedStores();
const mount = document.getElementById("gallery");
if (mount) render(() => <Gallery />, mount);
