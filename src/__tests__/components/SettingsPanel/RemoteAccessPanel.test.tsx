import { fireEvent, render } from "@solidjs/testing-library";
import { ErrorBoundary } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));
// Only `rpc` is replaced: `invoke` (the config defaults) needs the real
// `isTauri` and connection resolver to reach the Tauri mock.
vi.mock("../../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../transport")>()),
	rpc: vi.fn(),
}));
vi.mock("../../../utils/updateAppConfig", () => ({ updateAppConfig: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../stores/ui", async () => {
	const { createStore } = await import("solid-js/store");
	const [state, setState] = createStore({ settingsExpertMode: false });
	return {
		uiStore: {
			state,
			setSettingsExpertMode: vi.fn((enabled: boolean) => setState("settingsExpertMode", enabled)),
		},
	};
});

import { RemoteQrDialog } from "../../../components/RemoteQrDialog/RemoteQrDialog";
import { RemoteAccessPanel } from "../../../components/SettingsPanel/tabs/services/RemoteAccessPanel";
import { settingsExpertStore } from "../../../stores/settingsExpert";
import { uiStore } from "../../../stores/ui";
import { rpc } from "../../../transport";
import { updateAppConfig } from "../../../utils/updateAppConfig";
import { mockInvoke } from "../../mocks/tauri";

vi.mock("qrcode", () => ({ default: { toDataURL: vi.fn().mockResolvedValue("data:image/png;base64,AA==") } }));

describe("RemoteAccessPanel", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.mocked(rpc).mockImplementation((command: string) => {
			if (command === "get_mcp_status") {
				return Promise.resolve({
					enabled: true,
					running: true,
					remote_port: null,
					active_sessions: 0,
					mcp_clients: 0,
					max_sessions: 10,
					reachable: null,
				});
			}
			if (command === "get_relay_status") {
				return Promise.resolve({ enabled: false, connected: false, url: "", session_id: "" });
			}
			if (command === "load_config") {
				return Promise.resolve({
					services: {
						server: { enabled: false, port: 9876, ipv6_enabled: false },
						auth: { username: "", password_hash: "", session_token_duration_secs: 86400, lan_auth_bypass: false },
						relay: { enabled: false, url: "", session_id: "" },
					},
				});
			}
			if (command === "get_local_ips") return Promise.resolve([]);
			if (command === "get_tailscale_status") return Promise.resolve({ state: "NotInstalled" });
			return Promise.resolve(undefined);
		});
	});

	afterEach(() => {
		settingsExpertStore._resetForTests();
		uiStore.setSettingsExpertMode(false);
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.clearAllMocks();
	});

	it.each(["settings", "dialog"] as const)(
		"keeps the %s surface after rejected IP discovery and recovers on retry",
		async (surface) => {
			const base = vi.mocked(rpc).getMockImplementation()!;
			let discoveries = 0;
			vi.mocked(rpc).mockImplementation(async (command, args) => {
				if (command === "get_local_ips") {
					if (++discoveries < 3) throw new Error("Discovery unavailable: " + "network-interface/".repeat(20));
					return [{ ip: "192.168.1.42", label: "LAN" }];
				}
				if (command === "get_connect_url") return "http://192.168.1.42:9876/connect";
				const result = await base(command, args);
				if (command === "load_config") {
					const config = result as { services: { server: { enabled: boolean } } };
					config.services.server.enabled = true;
				}
				return result;
			});
			const view = render(() => (
				<ErrorBoundary fallback={<p>Whole surface crashed</p>}>
					{surface === "settings" ? <RemoteAccessPanel /> : <RemoteQrDialog onClose={() => {}} />}
				</ErrorBoundary>
			));
			await vi.advanceTimersByTimeAsync(0);
			// Catches: a failed resource read replaces the entire surface, or Retry cannot recover it.
			expect(view.queryByText("Whole surface crashed")).toBeNull();
			expect(view.getByText(surface === "settings" ? "Remote Access" : "Remote Mobile Connection")).toBeDefined();
			expect(view.getByRole("alert").textContent).toContain("Discovery unavailable:");
			if (surface === "settings") {
				expect(view.getByLabelText("Username")).toBeDefined();
				expect(view.getByLabelText("Relay Server URL")).toBeDefined();
			}
			for (let attempt = 0; attempt < 2; attempt++) {
				fireEvent.click(view.getByRole("button", { name: "Retry" }));
				await vi.advanceTimersByTimeAsync(0);
				if (attempt === 0) expect(view.getByRole("alert").textContent).toContain("Discovery unavailable:");
			}
			expect(discoveries).toBe(3);
			expect(view.queryByRole("alert")).toBeNull();
			expect(view.getByText("http://192.168.1.42:9876/connect")).toBeDefined();
			expect(
				view.getByAltText(surface === "settings" ? "QR code" : "QR code for remote mobile connection"),
			).toBeDefined();
			view.unmount();
		},
	);

	it("does not advertise HTTPS after certificate provisioning failed", async () => {
		const base = vi.mocked(rpc).getMockImplementation()!;
		vi.mocked(rpc).mockImplementation(async (command, args) => {
			if (command === "get_tailscale_status") {
				// CertDomains remained available during the recorded bundle EPERM failure.
				return { state: "Running", fqdn: "dgqt92cjfp.tail911da.ts.net", https_enabled: true };
			}
			const result = await base(command, args);
			if (command === "get_mcp_status") return { ...(result as object), tls_active: false };
			return result;
		});
		const view = render(() => <RemoteAccessPanel />);
		await vi.advanceTimersByTimeAsync(0);
		expect(view.queryByText(/^HTTPS active/)).toBeNull();
		expect(view.getByText("Running (HTTPS unavailable)")).toBeDefined();
		view.unmount();
	});

	it("tracks listener readiness and shutdown through the existing status poll", async () => {
		const base = vi.mocked(rpc).getMockImplementation()!;
		let tlsActive = true;
		vi.mocked(rpc).mockImplementation(async (command, args) => {
			if (command === "get_tailscale_status") {
				return { state: "Running", fqdn: "dgqt92cjfp.tail911da.ts.net", https_enabled: true };
			}
			const result = await base(command, args);
			if (command === "get_mcp_status") return { ...(result as object), tls_active: tlsActive };
			return result;
		});
		const view = render(() => <RemoteAccessPanel />);
		await vi.advanceTimersByTimeAsync(0);
		expect(view.getByText("HTTPS active (dgqt92cjfp.tail911da.ts.net)")).toBeDefined();
		// Catches a retained capability snapshot keeping HTTPS active after shutdown.
		tlsActive = false;
		await vi.advanceTimersByTimeAsync(3000);
		expect(view.queryByText(/^HTTPS active/)).toBeNull();
		expect(view.getByText("Running (HTTPS unavailable)")).toBeDefined();
		tlsActive = true;
		await vi.advanceTimersByTimeAsync(3000);
		expect(view.getByText("HTTPS active (dgqt92cjfp.tail911da.ts.net)")).toBeDefined();
		view.unmount();
	});

	it("shows the remote-access content it owns", async () => {
		const view = render(() => <RemoteAccessPanel />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.getByText("Remote Access")).toBeDefined();
		expect(view.getByText("Cloud Relay")).toBeDefined();
		expect(view.getByText("Enable remote access")).toBeDefined();

		view.unmount();
	});

	it("never renders HTTP API Server or TUIC Tools content — that belongs to LocalMcpPanel", async () => {
		const view = render(() => <RemoteAccessPanel />);
		await vi.advanceTimersByTimeAsync(0);

		expect(view.queryByText("HTTP API Server")).toBeNull();
		expect(view.queryByText("TUIC Tools")).toBeNull();
		expect(view.queryByText("Server Status")).toBeNull();

		view.unmount();
	});

	it("owns a single get_mcp_status poll and clears it on unmount", async () => {
		const view = render(() => <RemoteAccessPanel />);
		const statusCalls = () => vi.mocked(rpc).mock.calls.filter(([command]) => command === "get_mcp_status").length;

		await vi.advanceTimersByTimeAsync(0);
		expect(statusCalls()).toBe(1);

		await vi.advanceTimersByTimeAsync(3000);
		expect(statusCalls()).toBe(2);

		view.unmount();
		await vi.advanceTimersByTimeAsync(6000);
		expect(statusCalls()).toBe(2);
	});

	/** Render the page over a saved `services` config. */
	async function renderWith(services: Record<string, unknown>) {
		const base = vi.mocked(rpc).getMockImplementation();
		vi.mocked(rpc).mockImplementation((command: string, args?: Record<string, unknown>) =>
			command === "load_config" ? Promise.resolve({ services }) : (base?.(command, args) as Promise<never>),
		);
		const view = render(() => <RemoteAccessPanel />);
		await vi.advanceTimersByTimeAsync(0);
		return view;
	}

	describe("expert controls", () => {
		const DEFAULT_SERVER = { enabled: false, port: 9876, ipv6_enabled: false };
		const DEFAULT_AUTH = {
			username: "",
			password_hash: "",
			session_token_duration_secs: 86400,
			lan_auth_bypass: false,
		};
		const DEFAULTS = {
			app: { services: { server: DEFAULT_SERVER, auth: DEFAULT_AUTH, relay: {} } },
			notifications: {},
			agent_settings: {},
		};

		/** Remote access switched on (the expert controls live under it), with the
		 * given overrides and the Rust defaults loaded. */
		async function renderEnabled(server: Record<string, unknown> = {}, auth: Record<string, unknown> = {}) {
			mockInvoke.mockImplementation((cmd: string) =>
				cmd === "get_config_defaults" ? Promise.resolve(DEFAULTS) : Promise.resolve(undefined),
			);
			await settingsExpertStore.open();
			return renderWith({
				server: { ...DEFAULT_SERVER, enabled: true, ...server },
				auth: { ...DEFAULT_AUTH, ...auth },
				relay: { enabled: false, url: "", session_id: "" },
			});
		}

		const EXPERT_LABELS = ["Port", "Session Token Duration", "Enable IPv6 (dual-stack)"];
		const shown = (view: ReturnType<typeof render>) =>
			EXPERT_LABELS.filter((label) => view.queryByText(label) !== null);

		it("hides Port, Token duration and IPv6 at their defaults", async () => {
			const view = await renderEnabled();
			expect(shown(view)).toEqual([]);
			// The basic credentials stay.
			expect(view.getByText("Username")).toBeDefined();
			expect(view.getByText("Password")).toBeDefined();
			view.unmount();
		});

		it("shows each control whose saved value differs from its default, and only that one", async () => {
			const cases: [Record<string, unknown>, Record<string, unknown>, string][] = [
				[{ port: 9999 }, {}, "Port"],
				[{}, { session_token_duration_secs: 3600 }, "Session Token Duration"],
				[{ ipv6_enabled: true }, {}, "Enable IPv6 (dual-stack)"],
			];
			for (const [server, auth, label] of cases) {
				const view = await renderEnabled(server, auth);
				expect(shown(view)).toEqual([label]);
				view.unmount();
			}
		});

		it("does not offer the retired LAN bypass from a saved config in expert mode", async () => {
			uiStore.setSettingsExpertMode(true);
			const view = await renderEnabled({}, { lan_auth_bypass: true });
			expect(view.queryByText("Allow LAN access without authentication")).toBeNull();
			expect(view.getByText("Username")).toBeDefined();
			expect(view.getByText("Password")).toBeDefined();
			expect(updateAppConfig).not.toHaveBeenCalled();
			view.unmount();
		});

		it("shows every control at its default in expert mode", async () => {
			uiStore.setSettingsExpertMode(true);
			const view = await renderEnabled();
			expect(shown(view)).toEqual(EXPERT_LABELS);
			view.unmount();
		});
	});

	it("does not revoke remote sessions before explicit confirmation", async () => {
		const view = await renderWith({
			server: { enabled: true, port: 9876, ipv6_enabled: false },
			auth: { username: "operator", password_hash: "", session_token_duration_secs: 86400 },
			relay: { enabled: false, url: "", session_id: "" },
		});
		fireEvent.click(view.getByRole("button", { name: "Regenerate Token" }));
		expect(vi.mocked(rpc).mock.calls.some(([command]) => command === "regenerate_session_token")).toBe(false);
		expect(view.getByText("Regenerate remote access token?")).toBeDefined();
		fireEvent.click(view.getByRole("button", { name: "Regenerate token" }));
		await vi.advanceTimersByTimeAsync(0);
		expect(vi.mocked(rpc).mock.calls.filter(([command]) => command === "regenerate_session_token")).toHaveLength(1);
		view.unmount();
	});

	describe("cloud relay configuration", () => {
		const relayOff = {
			server: { enabled: false, port: 9876, ipv6_enabled: false },
			auth: { username: "", password_hash: "", session_token_duration_secs: 86400, lan_auth_bypass: false },
			relay: { enabled: false, url: "", session_id: "" },
		};
		const input = (view: ReturnType<typeof render>, label: string) => view.getByLabelText(label) as HTMLInputElement;

		// The relay starts only when it is enabled AND has a URL AND a token
		// (`relay_client.rs`), so both must be settable before switching it on —
		// and a search for either must land on a page that shows it.
		it("renders the relay URL and bearer token with the relay disabled", async () => {
			const view = await renderWith(relayOff);
			expect(input(view, "Relay Server URL")).not.toBeNull();
			expect(input(view, "Bearer Token")).not.toBeNull();
			view.unmount();
		});

		it("saves the relay URL and token while the relay stays disabled", async () => {
			const view = await renderWith(relayOff);
			fireEvent.input(input(view, "Relay Server URL"), { target: { value: "wss://relay.example" } });
			fireEvent.input(input(view, "Bearer Token"), { target: { value: "secret" } });

			const config = { services: structuredClone(relayOff) } as {
				services: typeof relayOff & { relay: Record<string, unknown> };
			};
			for (const [updater] of vi.mocked(updateAppConfig).mock.calls) (updater as (c: unknown) => void)(config);
			expect(config.services.relay).toEqual({
				enabled: false,
				url: "wss://relay.example",
				token: "secret",
				token_exists: true,
				session_id: "",
			});
			view.unmount();
		});

		it("keeps the connection status and session ID hidden until the relay is enabled", async () => {
			const view = await renderWith(relayOff);
			expect(view.queryByText("Session ID")).toBeNull();
			expect(view.queryByText("Disconnected")).toBeNull();
			view.unmount();
		});
	});
});
