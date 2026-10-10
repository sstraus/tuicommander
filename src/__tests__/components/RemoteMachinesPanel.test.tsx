import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { actions, connections } = vi.hoisted(() => ({
	actions: {
		hydrate: vi.fn(() => Promise.resolve()),
		addConnection: vi.fn<(connection: unknown) => Promise<void>>(() => Promise.resolve()),
		setPassword: vi.fn(() => Promise.resolve()),
		hasPassword: vi.fn(() => Promise.resolve(false)),
		connect: vi.fn(() => Promise.resolve()),
		disconnect: vi.fn(() => Promise.resolve()),
		install: vi.fn(() => Promise.resolve()),
		uninstall: vi.fn(() => Promise.resolve()),
		prepareUpdate: vi.fn(() =>
			Promise.resolve({
				session_count: 3,
				source: "release",
				remote_build: { version: "1.0", target: "aarch64-apple-darwin" },
				desktop_build: { version: "1.1", target: "aarch64-apple-darwin", sha256: "known-digest" },
			}),
		),
		updateAndRestart: vi.fn(() => Promise.resolve()),
		probeSshHosts: vi.fn<
			() => Promise<
				Array<{
					host: string;
					target: string;
					port: number | null;
					auth: "shell" | "no_shell" | "auth_failed" | "unreachable";
				}>
			>
		>(() => Promise.resolve([])),
		probeSshHost:
			vi.fn<
				(host: { target: string; port: number | null }) => Promise<{
					host: string;
					target: string;
					port: number | null;
					auth: "shell" | "no_shell" | "auth_failed" | "unreachable";
				}>
			>(),
		discoverSshHosts: vi.fn<
			() => Promise<{
				hosts: Array<{
					host: string;
					target: string;
					user: string | null;
					port: number | null;
					source: "config" | "known_hosts";
				}>;
				hashed_count: number;
			}>
		>(() => Promise.resolve({ hosts: [], hashed_count: 0 })),
		sshAgentInfo: vi.fn<
			() => Promise<{ keys: Array<{ fingerprint: string; comment: string; key_type: string }>; agent_type: string }>
		>(() =>
			Promise.resolve({
				keys: [{ fingerprint: "SHA256:abc", comment: "boss@mac", key_type: "ED25519" }],
				agent_type: "SSH Agent",
			}),
		),
		removeConnection: vi.fn(() => Promise.resolve()),
	},
	connections: {} as Record<string, unknown>,
}));

vi.mock("../../stores/remoteConnections", () => ({
	remoteConnectionsStore: {
		...actions,
		getConnections: () => connections,
	},
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { error: vi.fn() },
}));

import { RemoteMachinesPanel } from "../../components/SettingsPanel/tabs/services/RemoteMachinesPanel";

function sshConnection(deploy: "never" | "on_connect" | "installed" = "never") {
	return {
		connection: {
			id: "machine-1",
			name: "Build host",
			transport: {
				type: "Ssh" as const,
				ssh_host: "builder.local",
				ssh_port: 22,
				ssh_user: "dev",
				identity_file: null,
				remote_daemon_port: 9876,
			},
			auth_username: "tuic",
			enabled: true,
			deploy,
			survive_secs: 1800,
		},
		status: "disconnected" as const,
	};
}

describe("RemoteMachinesPanel", () => {
	beforeEach(() => {
		for (const key of Object.keys(connections)) delete connections[key];
		for (const action of Object.values(actions)) action.mockClear();
	});

	afterEach(() => {
		cleanup();
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	it("never renders a stale unreachable error alongside connecting progress", () => {
		connections["machine-1"] = {
			...sshConnection(),
			status: "connecting",
			error: "Unreachable: old error",
			retryAfterSecs: 3,
		};
		const view = render(() => <RemoteMachinesPanel />);
		expect(view.getByText("Connecting...")).toBeTruthy();
		expect(view.queryByText("Unreachable: old error")).toBeNull();
		expect(view.queryByText("Retry delay: 3s")).toBeNull();
	});

	it("renders the backend retry delay with the failed host cause", () => {
		connections["machine-1"] = {
			...sshConnection(),
			status: "error",
			error: "Cannot resolve mac-mint",
			retryAfterSecs: 3,
		};
		const view = render(() => <RemoteMachinesPanel />);
		expect(view.getByText("Cannot resolve mac-mint")).toBeTruthy();
		expect(view.getByText("Retry delay: 3s")).toBeTruthy();
		expect(view.queryByText("Connecting...")).toBeNull();
	});

	it("confirms the live session loss before an update of a Direct remote", async () => {
		connections["machine-1"] = {
			...sshConnection(),
			connection: {
				...sshConnection().connection,
				transport: { type: "Direct", url: "http://builder.local:9877" },
			},
			status: "connected",
			outOfDate: true,
		};
		const confirm = vi.fn(() => true);
		vi.stubGlobal("confirm", confirm);
		const { getByText } = render(() => <RemoteMachinesPanel />);
		expect(getByText("Remote out of date")).toBeTruthy();
		fireEvent.click(getByText("Update & restart remote"));
		await waitFor(() => expect(actions.updateAndRestart).toHaveBeenCalledWith("machine-1", 3, "known-digest"));
		expect(confirm).toHaveBeenCalledWith(expect.stringContaining("3 live sessions will be lost"));
		expect(confirm).toHaveBeenCalledWith(expect.stringContaining("Remote: 1.0 (aarch64-apple-darwin)"));
		expect(confirm).toHaveBeenCalledWith(expect.stringContaining("Selected: 1.1 (aarch64-apple-darwin, release)"));
	});

	it("offers the same update on SSH and keeps sessions when confirmation is cancelled", async () => {
		connections["machine-1"] = { ...sshConnection(), status: "connected", outOfDate: true };
		vi.stubGlobal(
			"confirm",
			vi.fn(() => false),
		);
		const { getByText } = render(() => <RemoteMachinesPanel />);
		fireEvent.click(getByText("Update & restart remote"));
		await waitFor(() => expect(actions.prepareUpdate).toHaveBeenCalledWith("machine-1"));
		expect(actions.updateAndRestart).not.toHaveBeenCalled();
	});

	it("offers the manual update with the live session count", async () => {
		connections["machine-1"] = { ...sshConnection(), status: "connected", outOfDate: true, liveSessions: 3 };
		const { getByText } = render(() => <RemoteMachinesPanel />);
		expect(getByText("3 live sessions. Update available.")).toBeTruthy();
		expect(getByText("Update & restart remote")).toBeTruthy();
	});

	it("announces the automatic update result", () => {
		connections["machine-1"] = {
			...sshConnection(),
			status: "connected",
			updateNotice: "Remote updated successfully.",
		};
		const { getByRole } = render(() => <RemoteMachinesPanel />);
		expect(getByRole("status").textContent).toBe("Remote updated successfully.");
	});

	it("disables manual update while the automatic update is running", () => {
		connections["machine-1"] = {
			...sshConnection(),
			status: "connected",
			outOfDate: true,
			updateInProgress: true,
		};
		const { getByText } = render(() => <RemoteMachinesPanel />);
		const button = getByText("Update & restart remote") as HTMLButtonElement;
		expect(button.disabled).toBe(true);
		fireEvent.click(button);
		expect(actions.prepareUpdate).not.toHaveBeenCalled();
	});

	it("shows the reason when a manual update cannot be prepared", async () => {
		connections["machine-1"] = { ...sshConnection(), status: "connected", outOfDate: true };
		actions.prepareUpdate.mockRejectedValueOnce(new Error("requires --no-default-features"));
		const { getByText, getByRole } = render(() => <RemoteMachinesPanel />);
		fireEvent.click(getByText("Update & restart remote"));
		await waitFor(() => expect(getByRole("alert").textContent).toBe("Error: requires --no-default-features"));
	});

	it("offers auto update per connection, off for a new machine", async () => {
		const { getByLabelText, getByTitle, getByPlaceholderText, getByText } = render(() => <RemoteMachinesPanel />);
		fireEvent.click(getByTitle("Add remote machine"));
		const toggle = getByLabelText("Auto-update remote daemons") as HTMLInputElement;
		expect(toggle.checked).toBe(false);
		fireEvent.input(getByPlaceholderText("Name (e.g. dev-server, staging)"), { target: { value: "Builder" } });
		fireEvent.input(getByPlaceholderText("Host (e.g. 192.168.1.100)"), { target: { value: "builder.local" } });
		fireEvent.click(toggle);
		fireEvent.click(getByText("Save"));
		await waitFor(() =>
			expect(actions.addConnection).toHaveBeenCalledWith(expect.objectContaining({ auto_update: true })),
		);
	});

	it("persists deployment mode and survive minutes when saving an SSH machine", async () => {
		const { getByTitle, getByPlaceholderText, getByText, container } = render(() => <RemoteMachinesPanel />);
		fireEvent.click(getByTitle("Add remote machine"));

		fireEvent.input(getByPlaceholderText("Name (e.g. dev-server, staging)"), {
			target: { value: "Builder" },
		});
		fireEvent.input(getByPlaceholderText("Host (e.g. 192.168.1.100)"), {
			target: { value: "builder.local" },
		});
		const deployment = Array.from(container.querySelectorAll("select")).find((select) =>
			select.textContent?.includes("Deploy on connect"),
		);
		expect(deployment).toBeTruthy();
		fireEvent.change(deployment!, { target: { value: "on_connect" } });
		const survive = Array.from(container.querySelectorAll('input[type="number"]')).at(-1);
		expect(survive).toBeTruthy();
		fireEvent.input(survive!, { target: { value: "45" } });
		fireEvent.click(getByText("Save"));

		await waitFor(() => expect(actions.addConnection).toHaveBeenCalledTimes(1));
		expect(actions.addConnection.mock.calls[0][0]).toMatchObject({
			name: "Builder",
			deploy: "on_connect",
			survive_secs: 2700,
		});
	});

	it("renders deployment progress from the backend status payload", () => {
		connections["machine-1"] = {
			...sshConnection(),
			status: "deploying",
			deployStep: "starting daemon",
		};
		const { getByText } = render(() => <RemoteMachinesPanel />);
		expect(getByText("Deploying: starting daemon")).toBeTruthy();
	});

	it("offers install and uninstall based on the persisted deploy mode", async () => {
		connections["machine-1"] = sshConnection();
		let view = render(() => <RemoteMachinesPanel />);
		fireEvent.click(view.getByText("Install"));
		await waitFor(() => expect(actions.install).toHaveBeenCalledWith("machine-1"));
		view.unmount();

		connections["machine-1"] = sshConnection("installed");
		view = render(() => <RemoteMachinesPanel />);
		fireEvent.click(view.getByText("Uninstall"));
		await waitFor(() => expect(actions.uninstall).toHaveBeenCalledWith("machine-1"));
	});

	it("lists probed host states while keeping the SSH host input free-form", async () => {
		actions.probeSshHosts.mockResolvedValueOnce([
			{ host: "shell-host", target: "shell-host", port: null, auth: "shell" },
			{ host: "git-only", target: "git-only", port: null, auth: "no_shell" },
		]);
		const { getByTitle, getByText, getByPlaceholderText, container } = render(() => <RemoteMachinesPanel />);
		fireEvent.click(getByTitle("Add remote machine"));
		fireEvent.click(getByText("Probe SSH hosts"));

		await waitFor(() => expect(container.querySelectorAll("datalist option")).toHaveLength(2));
		const options = Array.from(container.querySelectorAll("datalist option"));
		expect(options.map((option) => option.getAttribute("label"))).toEqual([
			"shell-host — shell",
			"git-only — no shell",
		]);

		const hostInput = getByPlaceholderText("Host (e.g. 192.168.1.100)") as HTMLInputElement;
		fireEvent.input(hostInput, { target: { value: "other.example" } });
		expect(hostInput.value).toBe("other.example");
	});

	it("lists discovered hosts without opening the Add form and probes nothing until asked", async () => {
		actions.discoverSshHosts.mockResolvedValueOnce({
			hosts: [
				{ host: "vps", target: "vps.example", user: "boss", port: null, source: "config" },
				{ host: "10.0.0.5", target: "10.0.0.5", user: null, port: 2222, source: "known_hosts" },
			],
			hashed_count: 7,
		});
		const { findByText, getByText } = render(() => <RemoteMachinesPanel />);
		expect(await findByText("boss@vps")).toBeTruthy();
		expect(getByText("10.0.0.5:2222")).toBeTruthy();
		expect(getByText("7 known_hosts entries are hashed and cannot be listed.")).toBeTruthy();
		expect(actions.probeSshHosts).not.toHaveBeenCalled();
		expect(actions.probeSshHost).not.toHaveBeenCalled();

		actions.probeSshHosts.mockResolvedValueOnce([{ host: "vps", target: "vps.example", port: null, auth: "shell" }]);
		fireEvent.click(getByText("Probe config hosts"));
		expect(await findByText("shell")).toBeTruthy();
		expect(actions.probeSshHost).not.toHaveBeenCalled();
	});

	it("probes a known_hosts host only from its own button and shows the result on that row", async () => {
		actions.discoverSshHosts.mockResolvedValueOnce({
			hosts: [
				{ host: "db", target: "10.0.0.9", user: null, port: null, source: "config" },
				{ host: "db", target: "db", user: null, port: null, source: "known_hosts" },
			],
			hashed_count: 0,
		});
		actions.probeSshHost.mockResolvedValueOnce({ host: "db", target: "db", port: null, auth: "auth_failed" });
		const { findAllByText, getAllByText, queryAllByText } = render(() => <RemoteMachinesPanel />);
		await findAllByText("db");
		// Second row is the known_hosts entry: its Probe button is the second one.
		fireEvent.click(getAllByText("Probe")[1]);
		await waitFor(() =>
			expect(actions.probeSshHost).toHaveBeenCalledWith(
				expect.objectContaining({ target: "db", source: "known_hosts" }),
			),
		);
		await waitFor(() => expect(queryAllByText("auth failed")).toHaveLength(1));
		// Same display name, different machine: the config row must not inherit the state.
		expect(getAllByText("ssh config")).toHaveLength(1);
	});

	it("prefills the Add form with host, user and port from a discovered host", async () => {
		actions.discoverSshHosts.mockResolvedValueOnce({
			hosts: [{ host: "vps", target: "vps", user: "boss", port: 2222, source: "config" }],
			hashed_count: 0,
		});
		const { findByText, getByPlaceholderText, container } = render(() => <RemoteMachinesPanel />);
		fireEvent.click(await findByText("boss@vps:2222"));
		expect((getByPlaceholderText("Host (e.g. 192.168.1.100)") as HTMLInputElement).value).toBe("vps");
		expect((getByPlaceholderText("SSH user") as HTMLInputElement).value).toBe("boss");
		expect((getByPlaceholderText("Name (e.g. dev-server, staging)") as HTMLInputElement).value).toBe("vps");
		expect((container.querySelector('input[type="number"]') as HTMLInputElement).value).toBe("2222");
	});

	it("warns when the SSH agent holds no identities", async () => {
		actions.sshAgentInfo.mockResolvedValueOnce({ keys: [], agent_type: "Not available" });
		const { findByRole } = render(() => <RemoteMachinesPanel />);
		expect((await findByRole("alert")).textContent).toContain("No SSH agent identities loaded");
	});
});
