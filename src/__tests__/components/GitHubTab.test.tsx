import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

// Shared, hoisted RPC stub + mutable account list so add/remove round-trip.
const h = vi.hoisted(() => {
	type Account = { id: string; host: { host: string }; login: string | null; kind: string };
	const accounts: Account[] = [];
	// repoPath -> resolution DTO returned by github_resolve_repo
	const resolutions: Record<string, unknown> = {};
	const repos: Record<string, { path: string; displayName: string }> = {};
	const rpc = vi.fn((cmd: string, args?: Record<string, unknown>) => {
		switch (cmd) {
			case "github_resolve_repo":
				return Promise.resolve(resolutions[String(args?.repoPath)] ?? { status: "unmonitored" });
			case "github_bind_repo":
				resolutions[String(args?.repoPath)] = {
					status: "bound",
					account: { id: args?.accountId },
					owner: "octocat",
					repo: "hello",
				};
				return Promise.resolve(undefined);
			case "github_unbind_repo":
				resolutions[String(args?.repoPath)] = {
					status: "needs-bind",
					candidates: [
						{
							account_id: "github.com",
							host: { host: "github.com" },
							owner: "octocat",
							repo: "hello",
							remote_name: "origin",
						},
					],
				};
				return Promise.resolve(true);
			case "github_auth_status":
				return Promise.resolve({
					authenticated: false,
					login: null,
					avatar_url: null,
					source: "none",
					scopes: null,
				});
			case "github_diagnostics":
				return Promise.resolve({
					circuit_breaker_open: false,
					circuit_breaker_status: "OK",
					repos_not_found: [],
					repos_monitored: 0,
				});
			case "github_list_accounts":
				return Promise.resolve([...accounts]);
			case "github_start_login":
				return Promise.resolve({
					device_code: "dev-code",
					user_code: "WDJB-MJHT",
					verification_uri: "https://github.com/login/device",
					expires_in: 900,
					interval: 5,
				});
			case "github_poll_login":
			case "github_poll_add_account":
				return Promise.resolve({ status: "pending" });
			case "github_add_account": {
				const acc: Account = {
					id: String(args?.host),
					host: { host: String(args?.host) },
					login: "ent-user",
					kind: "ghe_pat",
				};
				accounts.push(acc);
				return Promise.resolve(acc);
			}
			case "github_remove_account": {
				const idx = accounts.findIndex((a) => a.id === args?.id);
				if (idx >= 0) accounts.splice(idx, 1);
				return Promise.resolve(undefined);
			}
			default:
				return Promise.resolve(undefined);
		}
	});
	return { rpc, accounts, resolutions, repos };
});

vi.mock("../../transport", () => ({ rpc: h.rpc, isTauri: () => true }));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		get state() {
			return { repositories: h.repos };
		},
	},
}));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}));
vi.mock("../../stores/github", () => ({ githubStore: { setIssueFilter: vi.fn(), setPrHideDrafts: vi.fn() } }));
vi.mock("../../stores/settings", () => ({ settingsStore: { state: {} } }));
vi.mock("../../stores/repoDefaults", () => ({ repoDefaultsStore: { state: {} } }));
vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn() }));
vi.mock("../../components/SettingsPanel/SettingFields", () => ({
	SettingSelect: () => null,
	SettingToggle: () => null,
}));

async function loadTab() {
	const mod = await import("../../components/SettingsPanel/tabs/GitHubTab");
	return mod.GitHubTab;
}

function resetState() {
	h.accounts.length = 0;
	for (const k of Object.keys(h.resolutions)) delete h.resolutions[k];
	for (const k of Object.keys(h.repos)) delete h.repos[k];
	h.rpc.mockClear();
}

describe("GitHubTab — single-account collapse (006)", () => {
	let GitHubTab: Awaited<ReturnType<typeof loadTab>>;
	beforeEach(async () => {
		resetState();
		GitHubTab = await loadTab();
	});

	it("collapses the account manager + bindings to a single affordance when there is only the default account", async () => {
		const { findByText, queryByText } = render(() => <GitHubTab />);
		// Only the compact disclosure shows.
		expect(await findByText("Add another GitHub account")).toBeTruthy();
		// The manager body and bindings stay hidden in the 99% case.
		expect(queryByText("Additional GitHub Accounts")).toBeNull();
		expect(queryByText("Repository Bindings")).toBeNull();
		expect(queryByText("No additional accounts configured.")).toBeNull();
	});

	it("reveals the manager when the disclosure is clicked", async () => {
		const { findByText } = render(() => <GitHubTab />);
		fireEvent.click(await findByText("Add another GitHub account"));
		expect(await findByText("Additional GitHub Accounts")).toBeTruthy();
		expect(await findByText("No additional accounts configured.")).toBeTruthy();
	});
});

describe("GitHubTab — additional accounts", () => {
	let GitHubTab: Awaited<ReturnType<typeof loadTab>>;
	beforeEach(async () => {
		resetState();
		GitHubTab = await loadTab();
	});

	it("auto-shows the manager and lists existing accounts when one is configured", async () => {
		h.accounts.push({ id: "ghe.acme.com", host: { host: "ghe.acme.com" }, login: "octocat", kind: "ghe_pat" });
		const { findByText } = render(() => <GitHubTab />);
		// No disclosure click needed — a 2nd account means the manager is shown.
		expect(await findByText("octocat")).toBeTruthy();
		expect(await findByText(/ghe\.acme\.com/)).toBeTruthy();
	});

	it("labels a named github.com account as OAuth (device flow)", async () => {
		h.accounts.push({ id: "octocat2", host: { host: "github.com" }, login: "octocat2", kind: "github_com_oauth" });
		const { findByText } = render(() => <GitHubTab />);
		expect(await findByText("octocat2")).toBeTruthy();
		expect(await findByText(/github\.com · OAuth/)).toBeTruthy();
	});

	it("adds an Enterprise account via github_add_account(host, pat)", async () => {
		const { getByText, getByPlaceholderText, findByText } = render(() => <GitHubTab />);
		// Reveal the manager first (single-account collapse).
		fireEvent.click(await findByText("Add another GitHub account"));
		await findByText("No additional accounts configured.");

		fireEvent.input(getByPlaceholderText("ghe.example.com"), { target: { value: "ghe.acme.com" } });
		fireEvent.input(getByPlaceholderText("Personal Access Token"), { target: { value: "ghp_secret" } });
		fireEvent.click(getByText("Add account"));

		await waitFor(() =>
			expect(h.rpc).toHaveBeenCalledWith("github_add_account", { host: "ghe.acme.com", pat: "ghp_secret" }),
		);
		expect(await findByText("ent-user")).toBeTruthy();
	});

	it("starts a device-flow login when adding a github.com account (003)", async () => {
		const { findByText, getByText } = render(() => <GitHubTab />);
		fireEvent.click(await findByText("Add another GitHub account"));
		fireEvent.click(await findByText("Add github.com account"));
		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("github_start_login"));
		// The polling card shows the "add account" variant.
		expect(getByText(/Add another account — enter this code/)).toBeTruthy();
	});

	it("removes an account via github_remove_account(id) and re-collapses", async () => {
		h.accounts.push({ id: "ghe.acme.com", host: { host: "ghe.acme.com" }, login: "octocat", kind: "ghe_pat" });
		const { findByText, getByText } = render(() => <GitHubTab />);
		await findByText("octocat");

		fireEvent.click(getByText("Remove"));
		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("github_remove_account", { id: "ghe.acme.com" }));
		// Back to the single-account collapsed state.
		expect(await findByText("Add another GitHub account")).toBeTruthy();
	});
});

describe("GitHubTab — Repository bindings", () => {
	let GitHubTab: Awaited<ReturnType<typeof loadTab>>;
	beforeEach(async () => {
		resetState();
		GitHubTab = await loadTab();
	});

	it("binds an ambiguous repo to the chosen candidate via github_bind_repo", async () => {
		h.repos["/work/proj"] = { path: "/work/proj", displayName: "proj" };
		h.resolutions["/work/proj"] = {
			status: "needs-bind",
			candidates: [
				{
					account_id: "github.com",
					host: { host: "github.com" },
					owner: "octocat",
					repo: "hello",
					remote_name: "origin",
				},
				{
					account_id: "github.com",
					host: { host: "github.com" },
					owner: "upstream",
					repo: "hello",
					remote_name: "upstream",
				},
			],
		};
		const { findByText, getByText } = render(() => <GitHubTab />);
		// An ambiguous repo forces the bindings section open (needs attention).
		expect(await findByText("proj")).toBeTruthy();

		fireEvent.click(getByText("Bind"));
		await waitFor(() =>
			expect(h.rpc).toHaveBeenCalledWith("github_bind_repo", {
				repoPath: "/work/proj",
				accountId: "github.com",
				remoteName: "origin",
			}),
		);
	});

	it("unbinds a bound repo via github_unbind_repo when the manager is shown", async () => {
		// A 2nd account keeps the bindings section visible (not the collapsed 99% case).
		h.accounts.push({ id: "octocat2", host: { host: "github.com" }, login: "octocat2", kind: "github_com_oauth" });
		h.repos["/work/proj"] = { path: "/work/proj", displayName: "proj" };
		h.resolutions["/work/proj"] = {
			status: "bound",
			account: { id: "github.com", host: { host: "github.com" }, login: "octocat", kind: "github_com_oauth" },
			owner: "octocat",
			repo: "hello",
		};
		const { findByText, getByText } = render(() => <GitHubTab />);
		expect(await findByText("proj")).toBeTruthy();

		fireEvent.click(getByText("Unbind"));
		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("github_unbind_repo", { repoPath: "/work/proj" }));
	});
});
