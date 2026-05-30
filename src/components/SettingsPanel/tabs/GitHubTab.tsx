import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { appLogger } from "../../../stores/appLogger";
import { githubStore } from "../../../stores/github";
import type {
	AutoDeleteOnPrClose,
	MergeStrategy,
	OrphanCleanup,
	WorktreeAfterMerge,
	WorktreeStorage,
} from "../../../stores/repoDefaults";
import { repoDefaultsStore } from "../../../stores/repoDefaults";
import { settingsStore } from "../../../stores/settings";
import { rpc } from "../../../transport";
import type { IssueFilterMode } from "../../../types";
import { cx } from "../../../utils";
import { handleOpenUrl } from "../../../utils/openUrl";
import { SettingSelect, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";
import g from "./GitHubTab.module.css";

interface DeviceCodeResponse {
	device_code: string;
	user_code: string;
	verification_uri: string;
	expires_in: number;
	interval: number;
}

interface PollResult {
	status: "pending" | "slow_down" | "success" | "expired" | "access_denied";
	access_token?: string;
	scope?: string;
}

interface AuthStatus {
	authenticated: boolean;
	login: string | null;
	avatar_url: string | null;
	source: "env" | "oauth" | "gh_cli" | "none";
	scopes: string | null;
	error?: string | null;
}

interface GitHubDiagnostics {
	circuit_breaker_open: boolean;
	circuit_breaker_status: string;
	repos_not_found: string[];
	repos_monitored: number;
}

const SOURCE_LABELS: Record<string, string> = {
	oauth: "OAuth (Device Flow)",
	env: "Environment variable",
	gh_cli: "gh CLI",
	none: "Not connected",
};

export const GitHubTab: Component = () => {
	const [authStatus, setAuthStatus] = createSignal<AuthStatus | null>(null);
	const [loading, setLoading] = createSignal(false);
	const [polling, setPolling] = createSignal(false);
	const [deviceCode, setDeviceCode] = createSignal<DeviceCodeResponse | null>(null);
	const [error, setError] = createSignal<string | null>(null);
	const [copied, setCopied] = createSignal(false);
	const [avatarBroken, setAvatarBroken] = createSignal(false);
	const [diagnostics, setDiagnostics] = createSignal<GitHubDiagnostics | null>(null);

	let pollTimer: ReturnType<typeof setTimeout> | undefined;
	let cancelled = false;

	onMount(() => {
		fetchStatus();
		fetchDiagnostics();
	});

	onCleanup(() => {
		cancelled = true;
		if (pollTimer) clearTimeout(pollTimer);
	});

	async function fetchStatus() {
		try {
			const status = await rpc<AuthStatus>("github_auth_status");
			setAvatarBroken(false);
			setAuthStatus(status);
			// Surface backend-reported errors (e.g. 401, network failures)
			if (status.error) {
				setError(status.error);
			}
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e);
			setError(msg);
			// On fetch failure, show disconnected state rather than stale connected state
			setAuthStatus({ authenticated: false, login: null, avatar_url: null, source: "none", scopes: null });
			appLogger.error("github", "Failed to fetch auth status", e);
		}
	}

	async function fetchDiagnostics() {
		try {
			const diag = await rpc<GitHubDiagnostics>("github_diagnostics");
			setDiagnostics(diag);
		} catch (e) {
			appLogger.warn("github", "Failed to fetch diagnostics", e);
		}
	}

	async function startLogin() {
		cancelled = false;
		setError(null);
		setLoading(true);
		try {
			const resp = await rpc<DeviceCodeResponse>("github_start_login");
			setDeviceCode(resp);
			setPolling(true);
			setLoading(false);

			// Copy code to clipboard
			try {
				await navigator.clipboard.writeText(resp.user_code);
			} catch (e) {
				appLogger.warn("github", "Clipboard auto-copy failed", e);
			}

			// Open GitHub in browser
			handleOpenUrl(resp.verification_uri);

			// Start polling
			pollForToken(resp.device_code, resp.interval);
		} catch (e) {
			setLoading(false);
			setError(e instanceof Error ? e.message : String(e));
			appLogger.error("github", "Failed to start Device Flow", e);
		}
	}

	async function pollForToken(code: string, interval: number) {
		if (cancelled) return;

		try {
			const result = await rpc<PollResult>("github_poll_login", { deviceCode: code });

			switch (result.status) {
				case "success":
					setPolling(false);
					setDeviceCode(null);
					await fetchStatus();
					return;

				case "pending":
					// Continue polling
					break;

				case "slow_down":
					interval += 5;
					break;

				case "expired":
					setPolling(false);
					setDeviceCode(null);
					setError("Code expired. Please try again.");
					return;

				case "access_denied":
					setPolling(false);
					setDeviceCode(null);
					setError("Access denied. You cancelled the authorization.");
					return;
			}

			// Schedule next poll
			if (!cancelled) {
				pollTimer = setTimeout(() => pollForToken(code, interval), interval * 1000);
			}
		} catch (e) {
			setPolling(false);
			setDeviceCode(null);
			setError(e instanceof Error ? e.message : String(e));
			appLogger.error("github", "Device Flow poll failed", e);
		}
	}

	function cancelPolling() {
		cancelled = true;
		if (pollTimer) clearTimeout(pollTimer);
		setPolling(false);
		setDeviceCode(null);
	}

	async function logout() {
		setLoading(true);
		try {
			await rpc<void>("github_logout");
			await fetchStatus();
		} catch (e) {
			setError(e instanceof Error ? e.message : String(e));
			appLogger.error("github", "Failed to logout", e);
		} finally {
			setLoading(false);
		}
	}

	async function disconnect() {
		setLoading(true);
		try {
			await rpc<void>("github_disconnect");
			setError(null);
			// Force unauthenticated state immediately, then refresh from backend
			setAuthStatus({ authenticated: false, login: null, avatar_url: null, source: "none", scopes: null });
			await fetchStatus();
		} catch (e) {
			// Even on error, ensure we show the disconnected state
			setAuthStatus({ authenticated: false, login: null, avatar_url: null, source: "none", scopes: null });
			setError(e instanceof Error ? e.message : String(e));
			appLogger.error("github", "Failed to disconnect", e);
		} finally {
			setLoading(false);
		}
	}

	async function copyCode() {
		const code = deviceCode()?.user_code;
		if (!code) return;
		try {
			await navigator.clipboard.writeText(code);
			setCopied(true);
			setTimeout(() => setCopied(false), 2000);
		} catch {
			/* ignore */
		}
	}

	return (
		<div class={s.section}>
			<h3>GitHub Authentication</h3>

			{/* Error display */}
			<Show when={error()}>
				<div class={g.error}>{error()}</div>
			</Show>

			{/* Polling state — waiting for user to authorize */}
			<Show when={polling() && deviceCode()}>
				<div class={g.codeCard}>
					<div class={g.codeLabel}>Enter this code on GitHub:</div>
					<div class={g.userCode}>{deviceCode()!.user_code}</div>
					<div class={g.actions} style="justify-content: center">
						<button class={cx(g.btn)} onClick={copyCode}>
							{copied() ? "Copied!" : "Copy code"}
						</button>
						<button class={cx(g.btn)} onClick={() => handleOpenUrl(deviceCode()!.verification_uri)}>
							Open GitHub
						</button>
					</div>
					<div class={g.codeHint}>
						Waiting for authorization<span class={g.pollingDots}>...</span>
					</div>
					<div class={g.actions} style="justify-content: center; margin-top: 12px">
						<button class={cx(g.btn, g.btnDanger)} onClick={cancelPolling}>
							Cancel
						</button>
					</div>
				</div>
			</Show>

			{/* Authenticated state */}
			<Show when={!polling() && authStatus()?.authenticated}>
				<div class={g.statusCard}>
					<Show
						when={authStatus()?.avatar_url && !avatarBroken()}
						fallback={
							<div class={g.avatarPlaceholder}>
								<svg width="24" height="24" viewBox="0 0 24 24" fill="none">
									<path
										d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"
										fill="currentColor"
									/>
								</svg>
							</div>
						}
					>
						<img class={g.avatar} src={authStatus()!.avatar_url!} alt="" onError={() => setAvatarBroken(true)} />
					</Show>
					<div class={g.userInfo}>
						<div class={g.userName}>{authStatus()?.login ?? "Authenticated"}</div>
						<div class={g.tokenSource}>{SOURCE_LABELS[authStatus()?.source ?? "none"]}</div>
						<Show when={authStatus()?.scopes}>
							<div class={g.tokenScopes}>
								{authStatus()!
									.scopes!.split(",")
									.map((s) => s.trim())
									.filter(Boolean)
									.join(", ")}
							</div>
						</Show>
					</div>
				</div>

				{/* Only show logout for OAuth tokens — env/CLI tokens are managed externally */}
				<Show when={authStatus()?.source === "oauth"}>
					<div class={g.actions}>
						<button class={cx(g.btn, g.btnDanger)} onClick={logout} disabled={loading()}>
							{loading() ? "Logging out..." : "Logout"}
						</button>
					</div>
				</Show>

				<Show when={authStatus()?.source === "env" || authStatus()?.source === "gh_cli"}>
					<div class={s.hint}>
						Token provided via{" "}
						{authStatus()?.source === "env" ? "environment variable (GH_TOKEN or GITHUB_TOKEN)" : "gh CLI"}.
					</div>
					<div class={g.actions}>
						<button class={cx(g.btn)} onClick={startLogin} disabled={loading()}>
							{loading() ? "Starting..." : "Switch to OAuth"}
						</button>
						<button class={cx(g.btn, g.btnDanger)} onClick={disconnect} disabled={loading()}>
							Disconnect
						</button>
					</div>
				</Show>
			</Show>

			{/* Diagnostics section — shown when authenticated */}
			<Show when={!polling() && authStatus()?.authenticated && diagnostics()}>
				<div class={g.diagnosticsSection}>
					<h4 class={g.diagnosticsTitle}>Status</h4>

					{/* Circuit breaker */}
					<Show when={diagnostics()?.circuit_breaker_open}>
						<div class={g.diagWarning}>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" style="flex-shrink: 0">
								<path d="M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z" />
							</svg>
							<span>API requests paused: {diagnostics()?.circuit_breaker_status}</span>
						</div>
					</Show>

					{/* Repos not found */}
					<Show when={(diagnostics()?.repos_not_found?.length ?? 0) > 0}>
						<div class={g.diagWarning}>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" style="flex-shrink: 0">
								<path d="M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z" />
							</svg>
							<div>
								<div>Cannot access {diagnostics()?.repos_not_found?.length} repo(s) with this token:</div>
								<ul class={g.diagRepoList}>
									<For each={diagnostics()?.repos_not_found}>{(repo) => <li>{repo}</li>}</For>
								</ul>
								<div class={g.diagHint}>
									The token may lack organization access or SSO authorization for these repos.
								</div>
							</div>
						</div>
					</Show>

					{/* All good */}
					<Show when={!diagnostics()?.circuit_breaker_open && diagnostics()?.repos_not_found?.length === 0}>
						<div class={g.diagOk}>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" style="flex-shrink: 0">
								<path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" />
							</svg>
							<span>PR monitoring active — {diagnostics()?.repos_monitored} repo(s) tracked</span>
						</div>
					</Show>

					<Show when={(diagnostics()?.repos_monitored ?? 0) > 0 && (diagnostics()?.repos_not_found?.length ?? 0) > 0}>
						<div class={g.diagOk}>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" style="flex-shrink: 0">
								<path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" />
							</svg>
							<span>{diagnostics()?.repos_monitored} repo(s) tracked successfully</span>
						</div>
					</Show>
				</div>
			</Show>

			{/* PR settings — shown when authenticated */}
			<Show when={!polling() && authStatus()?.authenticated}>
				<h3>Pull Requests</h3>

				<SettingToggle
					checked={settingsStore.state.autoShowPrPopover}
					onChange={(v) => settingsStore.setAutoShowPrPopover(v)}
					label="Auto-show PR popover"
					hint="Automatically open the PR panel when a branch has an associated pull request"
				/>

				<SettingToggle
					checked={settingsStore.state.prHideDrafts}
					onChange={(v) => {
						settingsStore.setPrHideDrafts(v);
						githubStore.setPrHideDrafts(v);
					}}
					label="Hide Draft PRs"
					hint="Exclude draft pull requests from the Pull Requests list"
				/>

				<SettingToggle
					checked={settingsStore.state.prHideConflicting}
					onChange={(v) => settingsStore.setPrHideConflicting(v)}
					label="Hide Conflicting PRs"
					hint="Exclude pull requests with merge conflicts from the Pull Requests list"
				/>

				<SettingToggle
					checked={settingsStore.state.prHideCiFailing}
					onChange={(v) => settingsStore.setPrHideCiFailing(v)}
					label="Hide CI Failing PRs"
					hint="Exclude pull requests with failing CI checks from the Pull Requests list"
				/>

				<SettingSelect
					label="Auto-Delete on PR Close"
					value={repoDefaultsStore.state.autoDeleteOnPrClose}
					onChange={(v) => repoDefaultsStore.setAutoDeleteOnPrClose(v as AutoDeleteOnPrClose)}
					options={[
						{ value: "off", label: "Off" },
						{ value: "ask", label: "Ask before deleting" },
						{ value: "auto", label: "Auto-delete silently" },
					]}
					hint="Delete local branch when its PR is merged or closed on GitHub"
				/>

				<h3>Issues</h3>

				<SettingToggle
					checked={settingsStore.state.issueFilter !== "disabled"}
					onChange={(v) => githubStore.setIssueFilter(v ? "assigned" : "disabled")}
					label="Show issues"
					hint="Display the Issues section in the GitHub panel"
				/>

				<Show when={settingsStore.state.issueFilter !== "disabled"}>
					<SettingSelect
						label="Issue Filter"
						value={settingsStore.state.issueFilter}
						onChange={(v) => githubStore.setIssueFilter(v as IssueFilterMode)}
						options={[
							{ value: "assigned", label: "Assigned to me" },
							{ value: "created", label: "Created by me" },
							{ value: "mentioned", label: "Mentioning me" },
							{ value: "all", label: "All open" },
						]}
						hint="Which issues to show in the GitHub panel"
					/>
				</Show>
			</Show>

			{/* Repository & Worktree defaults — shown when authenticated */}
			<Show when={!polling() && authStatus()?.authenticated}>
				<h3>Repository Defaults</h3>
				<p class={s.hint} style={{ "margin-bottom": "12px" }}>
					These defaults apply to all repositories unless overridden per-repo
				</p>

				<SettingSelect
					label="Default Base Branch"
					value={repoDefaultsStore.state.baseBranch}
					onChange={(v) => repoDefaultsStore.setBaseBranch(v)}
					options={[
						{ value: "automatic", label: "Automatic" },
						{ value: "main", label: "main" },
						{ value: "master", label: "master" },
						{ value: "develop", label: "develop" },
					]}
					hint="Default base branch for new worktrees"
				/>

				<div class={s.group}>
					<label>File Handling Defaults</label>
					<div class={s.toggle}>
						<input
							type="checkbox"
							checked={repoDefaultsStore.state.copyIgnoredFiles}
							onChange={(e) => repoDefaultsStore.setCopyIgnoredFiles(e.currentTarget.checked)}
						/>
						<span>Copy ignored files</span>
					</div>
					<div class={s.toggle}>
						<input
							type="checkbox"
							checked={repoDefaultsStore.state.copyUntrackedFiles}
							onChange={(e) => repoDefaultsStore.setCopyUntrackedFiles(e.currentTarget.checked)}
						/>
						<span>Copy untracked files</span>
					</div>
				</div>

				<div class={s.group}>
					<label>Default Setup Script</label>
					<textarea
						value={repoDefaultsStore.state.setupScript}
						onInput={(e) => repoDefaultsStore.setSetupScript(e.currentTarget.value)}
						placeholder="#!/bin/bash&#10;npm install"
						rows={4}
					/>
					<p class={s.hint}>Shell script run when creating a new worktree</p>
				</div>

				<div class={s.group}>
					<label>Default Run Script</label>
					<textarea
						value={repoDefaultsStore.state.runScript}
						onInput={(e) => repoDefaultsStore.setRunScript(e.currentTarget.value)}
						placeholder="#!/bin/bash&#10;npm run dev"
						rows={4}
					/>
					<p class={s.hint}>Shell script run when launching the worktree</p>
				</div>

				<div class={s.group}>
					<label>Default Archive Script</label>
					<textarea
						value={repoDefaultsStore.state.archiveScript}
						onInput={(e) => repoDefaultsStore.setArchiveScript(e.currentTarget.value)}
						placeholder="#!/bin/bash&#10;docker compose down"
						rows={4}
					/>
					<p class={s.hint}>Shell script run before archiving or deleting a worktree</p>
				</div>

				<h3>Worktree Defaults</h3>
				<p class={s.hint} style={{ "margin-bottom": "12px" }}>
					Default worktree behavior for all repositories
				</p>

				<SettingSelect
					label="Storage Strategy"
					value={repoDefaultsStore.state.worktreeStorage}
					onChange={(v) => repoDefaultsStore.setWorktreeStorage(v as WorktreeStorage)}
					options={[
						{ value: "sibling", label: "Sibling directory (__wt)" },
						{ value: "app-dir", label: "App config directory" },
						{ value: "inside-repo", label: "Inside repository (.worktrees)" },
						{ value: "claude-code-default", label: "Claude Code default (.claude/worktrees)" },
					]}
					hint="Where to create worktree directories"
				/>

				<SettingToggle
					checked={repoDefaultsStore.state.promptOnCreate}
					onChange={(v) => repoDefaultsStore.setPromptOnCreate(v)}
					label="Prompt for branch name during creation"
					hint="Show dialog when creating worktrees from '+' button. When off, creates instantly with auto-generated name"
				/>

				<SettingToggle
					checked={repoDefaultsStore.state.deleteBranchOnRemove}
					onChange={(v) => repoDefaultsStore.setDeleteBranchOnRemove(v)}
					label="Delete local branch when removing worktree"
				/>

				<SettingToggle
					checked={repoDefaultsStore.state.autoArchiveMerged}
					onChange={(v) => repoDefaultsStore.setAutoArchiveMerged(v)}
					label="Auto-archive merged worktrees"
					hint="Move worktree to archive directory when its PR is merged"
				/>

				<SettingSelect
					label="Orphan Worktree Cleanup"
					value={repoDefaultsStore.state.orphanCleanup}
					onChange={(v) => repoDefaultsStore.setOrphanCleanup(v as OrphanCleanup)}
					options={[
						{ value: "ask", label: "Ask before removing" },
						{ value: "on", label: "Auto-remove" },
						{ value: "off", label: "Keep (mark as detached)" },
					]}
					hint="Handle worktrees whose branch was deleted"
				/>

				<SettingSelect
					label="PR Merge Strategy"
					value={repoDefaultsStore.state.prMergeStrategy}
					onChange={(v) => repoDefaultsStore.setPrMergeStrategy(v as MergeStrategy)}
					options={[
						{ value: "merge", label: "Merge" },
						{ value: "squash", label: "Squash" },
						{ value: "rebase", label: "Rebase" },
					]}
					hint="Default merge strategy for worktree branches"
				/>

				<SettingSelect
					label="After Merge Behavior"
					value={repoDefaultsStore.state.afterMerge}
					onChange={(v) => repoDefaultsStore.setAfterMerge(v as WorktreeAfterMerge)}
					options={[
						{ value: "archive", label: "Archive worktree" },
						{ value: "delete", label: "Delete worktree" },
						{ value: "ask", label: "Ask each time" },
					]}
					hint="What to do with the worktree after merging its branch"
				/>

				<SettingSelect
					label="Auto-Fetch Interval"
					value={String(repoDefaultsStore.state.autoFetchIntervalMinutes)}
					onChange={(v) => repoDefaultsStore.setAutoFetchIntervalMinutes(Number(v))}
					options={[
						{ value: "0", label: "Disabled" },
						{ value: "5", label: "5 minutes" },
						{ value: "15", label: "15 minutes" },
						{ value: "30", label: "30 minutes" },
						{ value: "60", label: "60 minutes" },
					]}
					hint="Periodically fetch from remote to detect upstream changes"
				/>
			</Show>

			{/* Not authenticated / disconnected state */}
			<Show when={!polling() && !authStatus()?.authenticated}>
				<div class={g.disconnected}>
					<div class={g.disconnectedIcon}>
						<svg width="48" height="48" viewBox="0 0 24 24" fill="currentColor">
							<path d="M12 2C6.477 2 2 6.477 2 12c0 4.42 2.87 8.17 6.84 9.5.5.08.66-.23.66-.5v-1.69c-2.77.6-3.36-1.34-3.36-1.34-.46-1.16-1.11-1.47-1.11-1.47-.91-.62.07-.6.07-.6 1 .07 1.53 1.03 1.53 1.03.87 1.52 2.34 1.07 2.91.83.09-.65.35-1.09.63-1.34-2.22-.25-4.55-1.11-4.55-4.92 0-1.11.38-2 1.03-2.71-.1-.25-.45-1.29.1-2.64 0 0 .84-.27 2.75 1.02.79-.22 1.65-.33 2.5-.33.85 0 1.71.11 2.5.33 1.91-1.29 2.75-1.02 2.75-1.02.55 1.35.2 2.39.1 2.64.65.71 1.03 1.6 1.03 2.71 0 3.82-2.34 4.66-4.57 4.91.36.31.69.92.69 1.85V21c0 .27.16.59.67.5C19.14 20.16 22 16.42 22 12A10 10 0 0012 2z" />
						</svg>
					</div>
					<div class={g.disconnectedTitle}>Connect to GitHub</div>
					<div class={g.disconnectedHint}>
						Sign in with your GitHub account to view pull requests, CI status, and repository information from private
						and organization repositories.
					</div>
					<div class={g.actions} style="justify-content: center">
						<button class={cx(g.btn, g.btnPrimary)} onClick={startLogin} disabled={loading()}>
							{loading() ? "Starting..." : "Login with GitHub"}
						</button>
					</div>
					<div class={g.scopeList} style="margin-top: 16px">
						Requested permissions: <strong>repo</strong>
					</div>
				</div>
			</Show>
		</div>
	);
};
