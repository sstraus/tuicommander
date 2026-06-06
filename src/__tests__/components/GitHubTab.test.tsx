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

describe("GitHubTab — Enterprise accounts", () => {
	let GitHubTab: typeof import("../../components/SettingsPanel/tabs/GitHubTab").GitHubTab;

	beforeEach(async () => {
		h.accounts.length = 0;
		for (const k of Object.keys(h.resolutions)) delete h.resolutions[k];
		for (const k of Object.keys(h.repos)) delete h.repos[k];
		h.rpc.mockClear();
		const mod = await import("../../components/SettingsPanel/tabs/GitHubTab");
		GitHubTab = mod.GitHubTab;
	});

	it("lists existing Enterprise accounts from github_list_accounts", async () => {
		h.accounts.push({ id: "ghe.acme.com", host: { host: "ghe.acme.com" }, login: "octocat", kind: "ghe_pat" });
		const { findByText } = render(() => <GitHubTab />);
		expect(await findByText("octocat")).toBeTruthy();
		expect(await findByText(/ghe\.acme\.com/)).toBeTruthy();
	});

	it("shows the empty state with no accounts", async () => {
		const { findByText } = render(() => <GitHubTab />);
		expect(await findByText("No Enterprise accounts configured.")).toBeTruthy();
	});

	it("adds an Enterprise account via github_add_account(host, pat)", async () => {
		const { getByText, getByPlaceholderText, findByText } = render(() => <GitHubTab />);
		await findByText("No Enterprise accounts configured.");

		fireEvent.input(getByPlaceholderText("ghe.example.com"), { target: { value: "ghe.acme.com" } });
		fireEvent.input(getByPlaceholderText("Personal Access Token"), { target: { value: "ghp_secret" } });
		fireEvent.click(getByText("Add account"));

		await waitFor(() =>
			expect(h.rpc).toHaveBeenCalledWith("github_add_account", { host: "ghe.acme.com", pat: "ghp_secret" }),
		);
		// The newly added account appears in the list.
		expect(await findByText("ent-user")).toBeTruthy();
	});

	it("removes an account via github_remove_account(id)", async () => {
		h.accounts.push({ id: "ghe.acme.com", host: { host: "ghe.acme.com" }, login: "octocat", kind: "ghe_pat" });
		const { findByText, getByText } = render(() => <GitHubTab />);
		await findByText("octocat");

		fireEvent.click(getByText("Remove"));
		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("github_remove_account", { id: "ghe.acme.com" }));
		expect(await findByText("No Enterprise accounts configured.")).toBeTruthy();
	});
});

describe("GitHubTab — Repository bindings", () => {
	let GitHubTab: typeof import("../../components/SettingsPanel/tabs/GitHubTab").GitHubTab;

	beforeEach(async () => {
		h.accounts.length = 0;
		for (const k of Object.keys(h.resolutions)) delete h.resolutions[k];
		for (const k of Object.keys(h.repos)) delete h.repos[k];
		h.rpc.mockClear();
		const mod = await import("../../components/SettingsPanel/tabs/GitHubTab");
		GitHubTab = mod.GitHubTab;
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
		// The ambiguous repo surfaces a chooser (no silent origin pick).
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

	it("unbinds a bound repo via github_unbind_repo", async () => {
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
