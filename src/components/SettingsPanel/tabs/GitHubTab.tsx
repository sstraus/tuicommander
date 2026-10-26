import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import oauthScopes from "../../../data/github-oauth-scopes.txt?raw";
import { t } from "../../../i18n";
import { settingsRefresh } from "../../../services/settingsRefresh";
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
import { repositoriesStore } from "../../../stores/repositories";
import { settingsStore } from "../../../stores/settings";
import { rpc } from "../../../transport";
import type { IssueFilterMode } from "../../../types";
import type { AccountSecretFormStatus, AccountSecretRequest } from "../../../types/accounts";
import { writeClipboard } from "../../../utils/clipboard";
import { handleOpenUrl } from "../../../utils/openUrl";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { Textarea } from "../../ui/Textarea";
import { ContextualHelp } from "../ContextualHelp";
import { ExpertSetting } from "../ExpertSetting";
import { SettingSelect, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";
import { SectionCard, SettingPageHeader } from "../SettingsPrimitives";
import g from "./GitHubTab.module.css";
import w from "./WorkspaceTabs.module.css";

interface DeviceCodeResponse {
	device_code: string;
	user_code: string;
	verification_uri: string;
	expires_in: number;
	interval: number;
}

interface PollResult {
	status: "pending" | "slow_down" | "success" | "expired" | "access_denied";
	scope?: string;
}

interface AuthStatus {
	authenticated: boolean;
	login: string | null;
	avatar_url: string | null;
	source: "env" | "oauth" | "none";
	scopes: string | null;
	error?: string | null;
}

interface RepoAccessErrorGroup {
	account_id: string;
	host: string;
	owner: string;
	repos: {
		repo: string;
		reason: "not_found" | "forbidden" | "unknown";
		message: string;
	}[];
	oauth_app_url: string | null;
}

const ACCESS_REASONS = {
	not_found: "Not found or not visible to this token",
	forbidden: "Access forbidden",
	unknown: "Access reason unknown",
};

interface GitHubDiagnostics {
	circuit_breaker_open: boolean;
	circuit_breaker_status: string;
	repos_not_found: string[];
	repo_access_errors?: RepoAccessErrorGroup[];
	repos_monitored: number;
}

interface CircleCiTokenStatus {
	configured: boolean;
	source: "vault" | "env" | "cliconfig" | "none";
}

type AccountKind = "github_com_oauth" | "github_com_env" | "ghe_pat";

interface GitHubHost {
	host: string;
}

interface GitHubAccount {
	id: string;
	host: GitHubHost;
	login: string | null;
	kind: AccountKind;
}

interface BindCandidate {
	account_id: string;
	host: GitHubHost;
	owner: string;
	repo: string;
	remote_name: string;
}

interface RepoResolutionDto {
	status: "bound" | "needs-bind" | "needs-account" | "unmonitored";
	account?: GitHubAccount;
	owner?: string;
	repo?: string;
	candidates?: BindCandidate[];
}

const SOURCE_LABELS: Record<string, string> = {
	oauth: "OAuth (Device Flow)",
	env: "Environment variable",
	pat: "Personal Access Token",
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
	// compute_diagnostics reports a cooled-down NOT_FOUND repo in both lists; the access error already names it.
	const unexplainedNotFound = () => {
		const d = diagnostics();
		const explained = new Set(
			(d?.repo_access_errors ?? []).flatMap((group) =>
				group.repos.flatMap((repo) => {
					const key = `${group.owner}/${repo.repo}`;
					// Non-ambient (GHE/named) accounts key their cooldown as `<account_id>:owner/repo`.
					return [key, `${group.account_id}:${key}`];
				}),
			),
		);
		return (d?.repos_not_found ?? []).filter((key) => !explained.has(key));
	};
	// DEFERRED (2026-10-17) — CircleCI settings catalogue owner is tracked in 1991-bd88.
	const [circleCiStatus, setCircleCiStatus] = createSignal<CircleCiTokenStatus | null>(null);
	const [circleCiToken, setCircleCiToken] = createSignal("");
	const [circleCiError, setCircleCiError] = createSignal<string | null>(null);

	// Additional accounts (beyond the ambient github.com default): extra
	// github.com accounts added via device-flow, plus GitHub Enterprise (PAT).
	const [accounts, setAccounts] = createSignal<GitHubAccount[]>([]);
	const [newHost, setNewHost] = createSignal("");
	const [addingAccount, setAddingAccount] = createSignal(false);
	const [accountError, setAccountError] = createSignal<string | null>(null);
	// Device-flow "add account" mode (vs the primary login) + the disclosure that
	// reveals the add UI for the single-account 99% case (Repository Bindings and
	// the account manager stay hidden until there's a 2nd account or ambiguity).
	const [addMode, setAddMode] = createSignal(false);
	const [showAddPanel, setShowAddPanel] = createSignal(false);

	// Any repo that can't be silently resolved (ambiguous, or a github.com host
	// with no account) — these force the binding manager to show.
	function needsAttention(): boolean {
		return Object.values(resolutions()).some((r) => r.status === "needs-bind" || r.status === "needs-account");
	}
	// The 99% case: only the ambient github.com default (the registry, which
	// excludes it, is empty) and nothing ambiguous to resolve.
	function isSingleAccountSetup(): boolean {
		return accounts().length === 0 && !needsAttention();
	}

	let pollTimer: ReturnType<typeof setTimeout> | undefined;
	let cancelled = false;

	const readAccounts = async () => {
		const repoPaths = activeRepos().map((repo) => repo.path);
		const [accounts, auth, resolutions] = await Promise.all([
			rpc<GitHubAccount[]>("github_list_accounts"),
			rpc<AuthStatus>("github_auth_status"),
			rpc<Record<string, RepoResolutionDto>>("github_resolve_repos", { repoPaths }),
		]);
		return { accounts, auth, resolutions };
	};
	for (const section of ["github_accounts", "github_bindings"])
		onCleanup(
			settingsRefresh.register({
				section,
				read: readAccounts,
				apply: (snapshot) => {
					setAccounts(snapshot.accounts);
					setAuthStatus(snapshot.auth);
					setResolutions(snapshot.resolutions);
				},
			}),
		);

	onCleanup(
		settingsRefresh.register({
			section: "circleci",
			read: () => rpc<CircleCiTokenStatus>("circleci_token_status"),
			apply: setCircleCiStatus,
		}),
	);

	onMount(() => {
		fetchStatus();
		fetchDiagnostics();
		fetchAccounts();
		fetchResolutions();
		void rpc<CircleCiTokenStatus>("circleci_token_status")
			.then(setCircleCiStatus)
			.catch((error) => {
				appLogger.warn("github", "Failed to load CircleCI token status", error);
				setCircleCiError(error instanceof Error ? error.message : String(error));
			});
	});

	async function saveCircleCiToken() {
		const token = circleCiToken().trim();
		if (!token) return setCircleCiError(t("github.circleCiTokenRequired", "Enter a CircleCI token before saving"));
		try {
			await rpc("circleci_set_token", { token });
			setCircleCiToken("");
			setCircleCiStatus(await rpc<CircleCiTokenStatus>("circleci_token_status"));
			setCircleCiError(null);
		} catch (error) {
			setCircleCiError(error instanceof Error ? error.message : String(error));
		}
	}

	async function removeCircleCiToken() {
		try {
			await rpc("circleci_delete_token");
			setCircleCiStatus(await rpc<CircleCiTokenStatus>("circleci_token_status"));
			setCircleCiError(null);
		} catch (error) {
			setCircleCiError(error instanceof Error ? error.message : String(error));
		}
	}

	async function fetchAccounts() {
		try {
			setAccounts(await rpc<GitHubAccount[]>("github_list_accounts"));
		} catch (e) {
			appLogger.warn("github", "Failed to list GitHub accounts", e);
		}
	}

	async function addAccount() {
		const host = newHost().trim();
		if (!host) {
			setAccountError("Host is required");
			return;
		}
		setAccountError(null);
		setAddingAccount(true);
		try {
			const request: AccountSecretRequest = {
				action: "add",
				account: {
					provider: "github",
					host,
					kind: "bearer",
					label: host,
					username: null,
				},
			};
			const result = await rpc<AccountSecretFormStatus>("account_secret_form", { request });
			if (result.status === "declined") return;
			if (result.status !== "stored") throw new Error("Account verification or storage failed");
			setNewHost("");
			await fetchAccounts();
		} catch (e) {
			setAccountError(e instanceof Error ? e.message : String(e));
		} finally {
			setAddingAccount(false);
		}
	}

	async function removeAccount(id: string) {
		try {
			await rpc("github_remove_account", { id });
			await fetchAccounts();
			await fetchResolutions();
		} catch (e) {
			setAccountError(e instanceof Error ? e.message : String(e));
		}
	}

	// Repo→account binding chooser: resolution per repo path + the candidate the
	// user picked for an ambiguous repo.
	const [resolutions, setResolutions] = createSignal<Record<string, RepoResolutionDto>>({});
	const [chosenRemote, setChosenRemote] = createSignal<Record<string, string>>({});

	function activeRepos(): { path: string; name: string }[] {
		return Object.values(repositoriesStore.state.repositories)
			.map((r) => ({ path: r.path, name: r.displayName }))
			.sort((a, b) => a.name.localeCompare(b.name));
	}

	async function fetchResolutions() {
		const repos = activeRepos();
		if (repos.length === 0) {
			setResolutions({});
			return;
		}
		try {
			// One batched call — the backend loads the registry/bindings once and
			// resolves every repo, instead of N sequential round-trips.
			const map = await rpc<Record<string, RepoResolutionDto>>("github_resolve_repos", {
				repoPaths: repos.map((r) => r.path),
			});
			setResolutions(map);
		} catch (e) {
			appLogger.debug("github", "resolve_repos failed", e);
		}
	}

	async function bindRepo(repoPath: string, accountId: string, remoteName: string) {
		try {
			await rpc("github_bind_repo", { repoPath, accountId, remoteName });
			await fetchResolutions();
		} catch (e) {
			setAccountError(e instanceof Error ? e.message : String(e));
		}
	}

	async function unbindRepo(repoPath: string) {
		try {
			await rpc("github_unbind_repo", { repoPath });
			await fetchResolutions();
		} catch (e) {
			setAccountError(e instanceof Error ? e.message : String(e));
		}
	}

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

	async function fetchDiagnostics(refresh = false) {
		try {
			const diag = await rpc<GitHubDiagnostics>("github_diagnostics", { refresh });
			setDiagnostics(diag);
		} catch (e) {
			appLogger.warn("github", "Failed to fetch diagnostics", e);
		}
	}

	async function startLogin(additional = false) {
		cancelled = false;
		setError(null);
		setAddMode(additional);
		setLoading(true);
		try {
			const resp = await rpc<DeviceCodeResponse>("github_start_login");
			setDeviceCode(resp);
			setPolling(true);
			setLoading(false);

			// Copy code to clipboard
			try {
				await writeClipboard(resp.user_code);
			} catch (e) {
				appLogger.warn("github", "Clipboard auto-copy failed", e);
			}

			// Open GitHub in browser
			handleOpenUrl(resp.verification_uri);

			// Start polling
			pollForToken(resp.device_code, resp.interval, additional);
		} catch (e) {
			setLoading(false);
			setError(e instanceof Error ? e.message : String(e));
			appLogger.error("github", "Failed to start Device Flow", e);
		}
	}

	async function pollForToken(code: string, interval: number, additional = false) {
		if (cancelled) return;

		try {
			// "add account" mode registers a NAMED github.com account without
			// touching the ambient default; normal mode logs in the default.
			const result = await rpc<PollResult>(additional ? "github_poll_add_account" : "github_poll_login", {
				deviceCode: code,
			});

			switch (result.status) {
				case "success":
					setPolling(false);
					setDeviceCode(null);
					setAddMode(false);
					if (additional) {
						// New named account persisted — refresh the manager, leave
						// the ambient default's auth status untouched.
						await fetchAccounts();
						await fetchResolutions();
					} else {
						await fetchStatus();
					}
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
				pollTimer = setTimeout(() => pollForToken(code, interval, additional), interval * 1000);
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
		setAddMode(false);
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
			await writeClipboard(code);
			setCopied(true);
			setTimeout(() => setCopied(false), 2000);
		} catch {
			/* ignore */
		}
	}

	return (
		<div class={`${s.section} ${w.page}`}>
			<SettingPageHeader title="Git & GitHub" scope="Application defaults" breadcrumb="Workspace / Git & GitHub" />
			<SectionCard title="GitHub Authentication">
				{/* Error display */}
				<Show when={error()}>
					<div class={g.error}>{error()}</div>
				</Show>

				{/* Polling state — waiting for user to authorize */}
				<Show when={polling() && deviceCode()}>
					<div class={g.codeCard}>
						<div class={g.codeLabel}>
							{addMode() ? "Add another account — enter this code on GitHub:" : "Enter this code on GitHub:"}
						</div>
						<div class={g.userCode}>{deviceCode()!.user_code}</div>
						<div class={g.actions} style="justify-content: center">
							<Button data-non-setting="Copy the device-login code without changing authentication" onClick={copyCode}>
								{copied() ? "Copied!" : "Copy code"}
							</Button>
							<Button
								data-non-setting="Open the provider verification page"
								onClick={() => handleOpenUrl(deviceCode()!.verification_uri)}
							>
								Open GitHub
							</Button>
						</div>
						<div class={g.codeHint}>
							Waiting for authorization<span class={g.pollingDots}>...</span>
						</div>
						<div class={g.actions} style="justify-content: center; margin-top: 12px">
							<Button
								data-non-setting="Typed GitHub authentication or account lifecycle workflow; not an independently editable field"
								variant="danger"
								onClick={cancelPolling}
							>
								Cancel
							</Button>
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

					{/* Only show logout for OAuth tokens — env tokens are managed externally */}
					<Show when={authStatus()?.source === "oauth"}>
						<div class={g.actions}>
							<Button
								data-non-setting="Typed GitHub authentication or account lifecycle workflow; not an independently editable field"
								variant="danger"
								onClick={logout}
								disabled={loading()}
							>
								{loading() ? "Logging out..." : "Logout"}
							</Button>
						</div>
					</Show>

					<Show when={authStatus()?.source === "env"}>
						<div class={s.hint}>Token provided via environment variable (GH_TOKEN or GITHUB_TOKEN).</div>
						<div class={g.actions}>
							<Button
								data-non-setting="Typed GitHub authentication or account lifecycle workflow; not an independently editable field"
								onClick={() => startLogin()}
								disabled={loading()}
							>
								{loading() ? "Starting..." : "Switch to OAuth"}
							</Button>
							<Button
								data-non-setting="Typed GitHub authentication or account lifecycle workflow; not an independently editable field"
								variant="danger"
								onClick={disconnect}
								disabled={loading()}
							>
								Disconnect
							</Button>
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

						<For each={diagnostics()?.repo_access_errors}>
							{(group) => (
								<div class={g.diagWarning}>
									<div>
										<strong>{group.owner}</strong>
										<div class={g.diagHint}>
											{group.host} — {group.account_id}
										</div>
										<ul class={g.diagRepoList}>
											<For each={group.repos}>
												{(repo) => (
													<li>
														<strong>{repo.repo}</strong>: {ACCESS_REASONS[repo.reason]}
														<div class={g.diagHint}>{repo.message}</div>
													</li>
												)}
											</For>
										</ul>
										<Show when={group.oauth_app_url}>
											{(url) => (
												<Button
													data-non-setting="Open the authorized OAuth app page"
													onClick={() => handleOpenUrl(url())}
												>
													Grant or request organization access
												</Button>
											)}
										</Show>
										<div class={g.diagHint}>
											Access denials are retried on the next monitoring poll. Refresh status also clears the 24-hour
											NOT_FOUND retry delay for all accounts.
										</div>
									</div>
								</div>
							)}
						</For>
						<Button
							data-non-setting="Refresh GitHub access diagnostics without changing authentication"
							onClick={() => fetchDiagnostics(true)}
						>
							Refresh status
						</Button>

						{/* Not-found repos in cooldown carry no access error (compute_diagnostics skips them) */}
						<Show when={unexplainedNotFound().length > 0}>
							<div class={g.diagWarning}>
								<div>
									<strong>Cannot access {unexplainedNotFound().length} repo(s) with this token</strong>
									<ul class={g.diagRepoList}>
										<For each={unexplainedNotFound()}>{(repo) => <li>{repo}</li>}</For>
									</ul>
								</div>
							</div>
						</Show>

						{/* Summary — always rendered so Status is never empty */}
						<Show
							when={(diagnostics()?.repos_monitored ?? 0) > 0}
							fallback={<div class={g.diagHint}>No repositories are monitored yet</div>}
						>
							<div class={g.diagOk}>
								<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" style="flex-shrink: 0">
									<path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" />
								</svg>
								<span>
									{diagnostics()?.circuit_breaker_open ? "" : "PR monitoring active — "}
									{diagnostics()?.repos_monitored} repo(s) tracked
								</span>
							</div>
						</Show>
					</div>
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
							<Button
								data-non-setting="Typed GitHub authentication or account lifecycle workflow; not an independently editable field"
								variant="primary"
								onClick={() => startLogin()}
								disabled={loading()}
							>
								{loading() ? "Starting..." : "Login with GitHub"}
							</Button>
						</div>
						<div class={g.scopeList} style="margin-top: 16px">
							Requested permissions: <strong>{oauthScopes.trim()}</strong>
						</div>
					</div>
				</Show>
			</SectionCard>

			{/* PR settings — shown when authenticated */}
			<Show when={!polling() && authStatus()?.authenticated}>
				<SectionCard title="Pull Requests">
					<SettingToggle
						settingId="app.auto_show_pr_popover"
						row
						checked={settingsStore.state.autoShowPrPopover}
						onChange={(v) => settingsStore.setAutoShowPrPopover(v)}
						label="Auto-show PR popover"
						hint="Automatically open the PR panel when a branch has an associated pull request"
					/>

					<SettingToggle
						row
						settingId="app.pr_hide_drafts"
						checked={settingsStore.state.prHideDrafts}
						onChange={(v) => {
							settingsStore.setPrHideDrafts(v);
							githubStore.setPrHideDrafts(v);
						}}
						label="Hide Draft PRs"
						hint="Exclude draft pull requests from the Pull Requests list"
					/>

					<SettingToggle
						row
						settingId="app.pr_hide_conflicting"
						checked={settingsStore.state.prHideConflicting}
						onChange={(v) => settingsStore.setPrHideConflicting(v)}
						label="Hide Conflicting PRs"
						hint="Exclude pull requests with merge conflicts from the Pull Requests list"
					/>

					<SettingToggle
						row
						settingId="app.pr_hide_ci_failing"
						checked={settingsStore.state.prHideCiFailing}
						onChange={(v) => settingsStore.setPrHideCiFailing(v)}
						label="Hide CI Failing PRs"
						hint="Exclude pull requests with failing CI checks from the Pull Requests list"
					/>

					<ExpertSetting
						configKey="repo_defaults.auto_delete_on_pr_close"
						value={repoDefaultsStore.state.autoDeleteOnPrClose}
					>
						<SettingSelect
							settingId="repo_defaults.auto_delete_on_pr_close"
							row
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
					</ExpertSetting>
				</SectionCard>
				<SectionCard title="Issues">
					<SettingToggle
						settingId="app.issue_filter"
						row
						checked={settingsStore.state.issueFilter !== "disabled"}
						onChange={(v) => githubStore.setIssueFilter(v ? "assigned" : "disabled")}
						label="Show issues"
						hint="Display the Issues section in the GitHub panel"
					/>

					<Show when={settingsStore.state.issueFilter !== "disabled"}>
						<SettingSelect
							settingId="app.issue_filter"
							row
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
				</SectionCard>
			</Show>

			{/* Repository & Worktree defaults — shown when authenticated */}
			<Show when={!polling() && authStatus()?.authenticated}>
				<SectionCard title="Repository Defaults">
					<p class={s.hint} style={{ "margin-bottom": "12px" }}>
						These defaults apply to all repositories unless overridden per-repo
					</p>

					<SettingSelect
						settingId="repo_defaults.base_branch"
						row
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

					<ExpertSetting configKey="repo_defaults.copy_ignored_files" value={repoDefaultsStore.state.copyIgnoredFiles}>
						<SettingToggle
							settingId="repo_defaults.copy_ignored_files"
							row
							checked={repoDefaultsStore.state.copyIgnoredFiles}
							onChange={(v) => repoDefaultsStore.setCopyIgnoredFiles(v)}
							label="Copy ignored files"
						/>
					</ExpertSetting>

					<ExpertSetting
						configKey="repo_defaults.copy_untracked_files"
						value={repoDefaultsStore.state.copyUntrackedFiles}
					>
						<SettingToggle
							settingId="repo_defaults.copy_untracked_files"
							row
							checked={repoDefaultsStore.state.copyUntrackedFiles}
							onChange={(v) => repoDefaultsStore.setCopyUntrackedFiles(v)}
							label="Copy untracked files"
						/>
					</ExpertSetting>

					<div class={s.group}>
						<div class={s.helpLabel}>
							<label for="default-setup-script">Default Setup Script</label>
							<ContextualHelp label="Default Setup Script" />
						</div>
						<Textarea
							data-setting-id="repo_defaults.setup_script"
							id="default-setup-script"
							value={repoDefaultsStore.state.setupScript}
							onInput={(e) => repoDefaultsStore.setSetupScript(e.currentTarget.value)}
							placeholder="#!/bin/bash&#10;npm install"
							rows={4}
						/>
						<p class={s.hint}>Shell script run when creating a new worktree</p>
					</div>

					<div class={s.group}>
						<div class={s.helpLabel}>
							<label for="default-run-script">Default Run Script</label>
							<ContextualHelp label="Default Run Script" />
						</div>
						<Textarea
							data-setting-id="repo_defaults.run_script"
							id="default-run-script"
							value={repoDefaultsStore.state.runScript}
							onInput={(e) => repoDefaultsStore.setRunScript(e.currentTarget.value)}
							placeholder="#!/bin/bash&#10;npm run dev"
							rows={4}
						/>
						<p class={s.hint}>Shell script run when launching the worktree</p>
					</div>

					<div class={s.group}>
						<div class={s.helpLabel}>
							<label for="default-archive-script">Default Archive Script</label>
							<ContextualHelp label="Default Archive Script" />
						</div>
						<Textarea
							data-setting-id="repo_defaults.archive_script"
							id="default-archive-script"
							value={repoDefaultsStore.state.archiveScript}
							onInput={(e) => repoDefaultsStore.setArchiveScript(e.currentTarget.value)}
							placeholder="#!/bin/bash&#10;docker compose down"
							rows={4}
						/>
						<p class={s.hint}>Shell script run before archiving or deleting a worktree</p>
					</div>
				</SectionCard>
				<SectionCard title="Worktree Defaults">
					<p class={s.hint} style={{ "margin-bottom": "12px" }}>
						Default worktree behavior for all repositories
					</p>

					<ExpertSetting configKey="repo_defaults.worktree_storage" value={repoDefaultsStore.state.worktreeStorage}>
						<SettingSelect
							settingId="repo_defaults.worktree_storage"
							row
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
					</ExpertSetting>

					<SettingToggle
						settingId="repo_defaults.prompt_on_create"
						row
						checked={repoDefaultsStore.state.promptOnCreate}
						onChange={(v) => repoDefaultsStore.setPromptOnCreate(v)}
						label="Prompt for branch name during creation"
						hint="Show dialog when creating worktrees from '+' button. When off, creates instantly with auto-generated name"
					/>

					<SettingToggle
						settingId="repo_defaults.delete_branch_on_remove"
						row
						checked={repoDefaultsStore.state.deleteBranchOnRemove}
						onChange={(v) => repoDefaultsStore.setDeleteBranchOnRemove(v)}
						label="Delete local branch when removing worktree"
					/>

					<ExpertSetting
						configKey="repo_defaults.auto_archive_merged"
						value={repoDefaultsStore.state.autoArchiveMerged}
					>
						<SettingToggle
							settingId="repo_defaults.auto_archive_merged"
							row
							checked={repoDefaultsStore.state.autoArchiveMerged}
							onChange={(v) => repoDefaultsStore.setAutoArchiveMerged(v)}
							label="Auto-archive merged worktrees"
							hint="Move worktree to archive directory when its PR is merged"
						/>
					</ExpertSetting>

					<ExpertSetting configKey="repo_defaults.orphan_cleanup" value={repoDefaultsStore.state.orphanCleanup}>
						<SettingSelect
							settingId="repo_defaults.orphan_cleanup"
							row
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
					</ExpertSetting>

					<ExpertSetting
						configKey="repo_defaults.orphan_cleanup_countdown_seconds"
						value={repoDefaultsStore.state.orphanCleanupCountdownSeconds}
					>
						<SettingSelect
							settingId="repo_defaults.orphan_cleanup_countdown_seconds"
							row
							label="Safe orphan cleanup countdown"
							value={String(repoDefaultsStore.state.orphanCleanupCountdownSeconds)}
							onChange={(v) => repoDefaultsStore.setOrphanCleanupCountdownSeconds(Number(v))}
							options={[5, 10, 20, 30].map((seconds) => ({ value: String(seconds), label: `${seconds} seconds` }))}
							hint="Automatically remove clean orphaned worktrees after this countdown"
						/>
					</ExpertSetting>

					<SettingSelect
						settingId="repo_defaults.pr_merge_strategy"
						row
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

					<ExpertSetting configKey="repo_defaults.after_merge" value={repoDefaultsStore.state.afterMerge}>
						<SettingSelect
							settingId="repo_defaults.after_merge"
							row
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
					</ExpertSetting>

					<ExpertSetting
						configKey="repo_defaults.auto_fetch_interval_minutes"
						value={repoDefaultsStore.state.autoFetchIntervalMinutes}
					>
						<SettingSelect
							settingId="repo_defaults.auto_fetch_interval_minutes"
							row
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
					</ExpertSetting>
				</SectionCard>
			</Show>

			{/* Additional accounts. The 99% single-account setup (only the ambient
			    github.com default, nothing ambiguous) collapses to a single
			    "Add another account" affordance — the manager + bindings appear
			    only once there's a 2nd account or a repo needs disambiguation. */}
			<Show when={!polling()}>
				<Show
					when={!isSingleAccountSetup() || showAddPanel()}
					fallback={
						// Only the entry point is expert. The manager above renders once an
						// account exists or a repo needs one, and it stays visible then.
						<ExpertSetting configKey="github_accounts.accounts" value={accounts()}>
							<div class={g.actions} style={{ "margin-top": "16px" }}>
								<Button data-non-setting="Open the account-creation editor" onClick={() => setShowAddPanel(true)}>
									Add another GitHub account
								</Button>
							</div>
						</ExpertSetting>
					}
				>
					<SectionCard title="Additional GitHub Accounts">
						<p class={s.hint} style={{ "margin-bottom": "12px" }}>
							Add a second github.com account (device-flow) or a GitHub Enterprise Server account (Personal Access
							Token). The github.com account you logged in with above is your default.
						</p>

						<Show when={accountError()}>
							<div class={g.error}>{accountError()}</div>
						</Show>

						<For each={accounts()}>
							{(acc) => (
								<div class={g.statusCard}>
									<div class={g.userInfo}>
										<div class={g.userName}>{acc.login ?? acc.host.host}</div>
										<div class={g.tokenSource}>
											{acc.host.host} · {acc.kind === "ghe_pat" ? SOURCE_LABELS.pat : SOURCE_LABELS.oauth}
										</div>
									</div>
									<div class={g.actions}>
										<Button
											data-non-setting="Typed GitHub authentication or account lifecycle workflow; not an independently editable field"
											variant="danger"
											onClick={() => removeAccount(acc.id)}
										>
											Remove
										</Button>
									</div>
								</div>
							)}
						</For>

						<Show when={accounts().length === 0}>
							<p class={s.hint}>No additional accounts configured.</p>
						</Show>

						<div class={s.group}>
							<label>Add another github.com account</label>
							<div class={g.actions}>
								<Button
									data-non-setting="Typed GitHub authentication or account lifecycle workflow; not an independently editable field"
									variant="primary"
									onClick={() => startLogin(true)}
									disabled={loading()}
								>
									{loading() ? "Starting..." : "Add github.com account"}
								</Button>
							</div>
						</div>

						<div class={s.group}>
							<div class={s.helpLabel}>
								<label for="github-enterprise-host">Add Enterprise account</label>
								<ContextualHelp label="Add Enterprise account" />
							</div>
							<Input
								data-setting-id="github_accounts.accounts.{id}.host.host"
								type="text"
								id="github-enterprise-host"
								placeholder="ghe.example.com"
								value={newHost()}
								onInput={(e) => setNewHost(e.currentTarget.value)}
							/>
							<div class={g.actions}>
								<Button
									data-non-setting="Typed GitHub authentication or account lifecycle workflow; not an independently editable field"
									variant="primary"
									onClick={addAccount}
									disabled={addingAccount()}
								>
									{addingAccount() ? "Validating..." : "Add account"}
								</Button>
							</div>
						</div>
					</SectionCard>
				</Show>
			</Show>

			<SectionCard title="CircleCI">
				<p class={s.hint}>
					{t("github.circleCiHint", "Use a read-only CircleCI token. Stored tokens are never shown again.")}
				</p>
				<div class={s.group} data-setting-id="circleci.token">
					<Input
						type="password"
						aria-label="CircleCI read-only token"
						placeholder={t("github.circleCiPlaceholder", "CircleCI read-only token")}
						data-setting-id="circleci.token"
						value={circleCiToken()}
						onInput={(e) => setCircleCiToken(e.currentTarget.value)}
					/>
					<div class={g.actions}>
						<Button
							data-setting-id="circleci.token"
							data-ready={circleCiToken().trim() ? "true" : undefined}
							onClick={saveCircleCiToken}
						>
							{t("github.circleCiSave", "Save token")}
						</Button>
						<Button
							data-setting-id="circleci.token"
							onClick={removeCircleCiToken}
							disabled={!circleCiStatus()?.configured}
						>
							{t("github.circleCiRemove", "Remove token")}
						</Button>
					</div>
					<Show when={circleCiError()}>{(message) => <div class={g.error}>{message()}</div>}</Show>
					<div class={g.tokenSource}>
						{circleCiStatus()?.configured
							? t("github.circleCiConfigured", "Configured ({source})", { source: circleCiStatus()?.source ?? "" })
							: t("github.circleCiNotConfigured", "Not configured")}
					</div>
				</div>
			</SectionCard>
			{/* Repository → account bindings. Every repo resolves to one account;
			    ambiguous repos (multiple GitHub remotes/accounts) are chosen here.
			    Hidden in the single-account setup — there's nothing to choose. */}
			<Show when={!isSingleAccountSetup()}>
				<SectionCard title="Repository Bindings">
					<p class={s.hint} style={{ "margin-bottom": "12px" }}>
						Each repository is monitored through exactly one account. Ambiguous repositories need you to choose — no
						remote is picked silently.
					</p>

					<Show when={activeRepos().length > 0} fallback={<p class={s.hint}>No repositories in the workspace yet.</p>}>
						<For each={activeRepos()}>
							{(repo) => {
								const res = () => resolutions()[repo.path];
								return (
									<Show when={res() && res().status !== "unmonitored"}>
										<div class={g.statusCard}>
											<div class={g.userInfo}>
												<div class={g.userName}>{repo.name}</div>
												<Show when={res().status === "bound"}>
													<div class={g.tokenSource}>
														→ {res().owner}/{res().repo} via{" "}
														{res().account?.login ?? res().account?.host.host ?? res().account?.id}
													</div>
												</Show>
												<Show when={res().status === "needs-account"}>
													<div class={g.tokenSource}>No account for this host — add one above.</div>
												</Show>
												<Show when={res().status === "needs-bind"}>
													<Select
														data-setting-id="github_bindings.bindings.{key}.remote_name"
														class={g.bindSelect}
														value={chosenRemote()[repo.path] ?? res().candidates?.[0]?.remote_name ?? ""}
														onChange={(e) => setChosenRemote({ ...chosenRemote(), [repo.path]: e.currentTarget.value })}
													>
														<For each={res().candidates}>
															{(c) => (
																<option value={c.remote_name}>
																	{c.owner}/{c.repo} via {c.remote_name} ({c.account_id})
																</option>
															)}
														</For>
													</Select>
												</Show>
											</div>
											<div class={g.actions}>
												<Show when={res().status === "bound"}>
													<Button
														data-non-setting="Typed registered-repository account-binding workflow"
														onClick={() => unbindRepo(repo.path)}
													>
														Unbind
													</Button>
												</Show>
												<Show when={res().status === "needs-bind"}>
													<Button
														data-non-setting="Typed registered-repository account-binding workflow"
														variant="primary"
														onClick={() => {
															const remote = chosenRemote()[repo.path] ?? res().candidates?.[0]?.remote_name ?? "";
															const cand = res().candidates?.find((c) => c.remote_name === remote);
															if (cand) bindRepo(repo.path, cand.account_id, cand.remote_name);
														}}
													>
														Bind
													</Button>
												</Show>
											</div>
										</div>
									</Show>
								);
							}}
						</For>
					</Show>
				</SectionCard>
			</Show>
		</div>
	);
};
