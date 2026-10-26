import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { appLogger } from "../../stores/appLogger";
import { transcriptPreferences } from "../../stores/transcriptPreferences";
import MobileApp from "../MobileApp";
import { SettingsScreen } from "../screens/SettingsScreen";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn() } }));
vi.mock("../../utils/windowBackground", () => ({ syncWindowBackground: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke, listen: vi.fn() }));
vi.mock("../useMobileVoice", () => ({ useMobileVoice: () => ({ available: () => false }) }));
vi.mock("../../stores/ideas", () => ({ ideasStore: { hydrate: vi.fn() } }));
vi.mock("../useSessions", () => ({
	useSessions: () => ({
		aiChatAvailable: () => true,
		sessions: () => [],
		loading: () => false,
		refreshing: () => false,
		error: () => null,
		authError: () => false,
		refresh: vi.fn(),
		markSeen: vi.fn(),
	}),
}));
vi.mock("../useMobileNotifications", () => ({ useMobileNotifications: vi.fn() }));
vi.mock("../useVersionCheck", () => ({
	useVersionCheck: () => ({ updateAvailable: () => false, serverDown: () => false, applyUpdate: vi.fn() }),
}));
vi.mock("../../components/McpConfirmHost/McpConfirmHost", () => ({ McpConfirmHost: () => <div /> }));

const appChrome = (background: string) => ({
	bg_primary: background,
	bg_secondary: background,
	bg_tertiary: background,
	bg_highlight: background,
	fg_primary: "#333333",
	fg_secondary: "#555555",
	fg_muted: "#777777",
	accent: "#005fb8",
	accent_hover: "#0258a8",
	border: "#dddddd",
	success: "#008000",
	warning: "#999900",
	error: "#cc0000",
	text_on_accent: "#ffffff",
	text_on_error: "#ffffff",
	text_on_success: "#ffffff",
});
const themes = [
	{
		key: "commander",
		name: "Commander",
		terminal: { background: "#1e1e1e", foreground: "#cccccc", cursor: "#cccccc", ansi: [] },
		app_chrome: appChrome("#1e1e1e"),
	},
	{
		key: "vscode-light",
		name: "Paper",
		terminal: { background: "#ffffff", foreground: "#333333", cursor: "#333333", ansi: [] },
		app_chrome: appChrome("#ffffff"),
	},
];
let serverPrefs: Record<string, unknown>;

beforeEach(() => {
	localStorage.clear();
	vi.mocked(appLogger.warn).mockClear();
	serverPrefs = { mobile_theme: "commander", sidebar_visible: true };
	invoke.mockImplementation(async (command: string, args?: { config?: Record<string, unknown> }) => {
		if (command === "list_themes") return themes;
		if (command === "load_ui_prefs") return { ...serverPrefs };
		if (command === "save_ui_prefs") serverPrefs = { ...serverPrefs, ...args?.config };
		return undefined;
	});
	vi.stubGlobal(
		"fetch",
		vi.fn(async () => ({ ok: true, json: async () => ({ version: "2.4.1", git_hash: "abc" }) })),
	);
});
afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
	invoke.mockReset();
});

describe("mobile Settings", () => {
	// Catches: desktop Telegram setup being offered again in mobile Settings, even offline.
	it.each([true, false])("does not offer Telegram setup when connected=%s", async (isConnected) => {
		render(() => <SettingsScreen isConnected={isConnected} />);
		await waitFor(() =>
			expect((screen.getByRole("combobox", { name: "Theme" }) as HTMLSelectElement).disabled).toBe(false),
		);
		expect(screen.queryByRole("button", { name: /telegram/i })).toBeNull();
		expect(screen.queryByText(/telegram/i)).toBeNull();
		expect(screen.getByText("Open Desktop UI").tagName).toBe("A");
	});

	// Catches: mobile rendering the indicator without offering a working opt-out.
	it("lets the mobile client disable and persist the new-text indicator", async () => {
		transcriptPreferences.setEnabled(true);
		render(() => <SettingsScreen isConnected />);
		await screen.findByRole("combobox", { name: "Theme" });
		fireEvent.click(screen.getByRole("switch", { name: "New text indicator" }));
		expect((screen.getByRole("switch", { name: "New text indicator" }) as HTMLInputElement).checked).toBe(false);
		expect(localStorage.getItem("tui-commander-new-text-indicator")).toBe("false");
		transcriptPreferences.setEnabled(true);
	});

	it("applies the desktop Paper theme and restores it after Settings remounts", async () => {
		const first = render(() => <SettingsScreen isConnected />);
		const theme = await screen.findByRole("combobox", { name: "Theme" });
		await waitFor(() => expect((theme as HTMLSelectElement).disabled).toBe(false));
		fireEvent.change(theme, { target: { value: "vscode-light" } });
		await waitFor(() => expect(document.documentElement.style.getPropertyValue("--bg-primary")).toBe("#ffffff"));
		await waitFor(() => expect(serverPrefs.mobile_theme).toBe("vscode-light"));
		first.unmount();
		document.documentElement.style.removeProperty("--bg-primary");
		render(() => <MobileApp />);
		await waitFor(() => expect(document.documentElement.style.getPropertyValue("--bg-primary")).toBe("#ffffff"));
	});

	it("shows the app build and the connected server version", async () => {
		render(() => <SettingsScreen isConnected />);
		expect(screen.getByText("App version")).toBeTruthy();
		expect(screen.getByText(__APP_VERSION__)).toBeTruthy();
		await waitFor(() => expect(screen.getByText("2.4.1")).toBeTruthy());
	});

	it("keeps a selected theme out of browser localStorage", async () => {
		render(() => <SettingsScreen isConnected />);
		const theme = await screen.findByRole("combobox", { name: "Theme" });
		await waitFor(() => expect((theme as HTMLSelectElement).disabled).toBe(false));
		fireEvent.change(theme, { target: { value: "vscode-light" } });
		await waitFor(() => expect(document.documentElement.style.getPropertyValue("--bg-primary")).toBe("#ffffff"));
		await waitFor(() => expect(serverPrefs.mobile_theme).toBe("vscode-light"));
		expect(localStorage.getItem("tuic-mobile-theme")).toBeNull();
	});

	it("falls back to the dark theme for an unknown saved theme", async () => {
		serverPrefs.mobile_theme = "missing-theme";
		render(() => <SettingsScreen isConnected />);
		await waitFor(() => expect(document.documentElement.style.getPropertyValue("--bg-primary")).toBe("#1e1e1e"));
		expect((screen.getByRole("combobox", { name: "Theme" }) as HTMLSelectElement).value).toBe("commander");
	});

	it("keeps the dark selection when the server refuses a theme change", async () => {
		invoke.mockImplementation(async (command: string) => {
			if (command === "list_themes") return themes;
			if (command === "load_ui_prefs") return { ...serverPrefs };
			if (command === "save_ui_prefs") throw new Error("disk full");
		});
		render(() => <SettingsScreen isConnected />);
		const theme = (await screen.findByRole("combobox", { name: "Theme" })) as HTMLSelectElement;
		await waitFor(() => expect(theme.disabled).toBe(false));
		fireEvent.change(theme, { target: { value: "vscode-light" } });
		await screen.findByRole("alert");
		expect(theme.value).toBe("commander");
		expect(serverPrefs.mobile_theme).toBe("commander");
		expect(appLogger.warn).toHaveBeenCalledWith("app", "Could not save mobile theme", expect.any(Error));
		expect(document.documentElement.style.getPropertyValue("--bg-primary")).toBe("#1e1e1e");
	});

	it("keeps the app version visible when the server version is unavailable", async () => {
		vi.stubGlobal(
			"fetch",
			vi.fn(async () => {
				throw new Error("offline");
			}),
		);
		render(() => <SettingsScreen isConnected={false} />);
		expect(screen.getByText(__APP_VERSION__)).toBeTruthy();
		expect(screen.getByText("Unavailable")).toBeTruthy();
		await waitFor(() =>
			expect(appLogger.warn).toHaveBeenCalledWith("network", "Could not load server version", expect.any(Error)),
		);
	});
});

// Catches: the native checkbox keeps its optimistic checked state after permission is dismissed.
it("requests push permission in the enabling gesture and restores an ungranted switch", async () => {
	let finish!: (value: NotificationPermission) => void;
	const requestPermission = vi.fn(
		() =>
			new Promise<NotificationPermission>((resolve) => {
				finish = resolve;
			}),
	);
	vi.stubGlobal("PushManager", class {});
	vi.stubGlobal("Notification", { permission: "default", requestPermission });
	vi.stubGlobal("navigator", {
		userAgent: "Chromium",
		serviceWorker: { ready: Promise.resolve({ pushManager: { getSubscription: async () => null } }) },
	});
	render(() => <SettingsScreen isConnected />);
	const toggle = (await screen.findByRole("switch", { name: "Push notifications" })) as HTMLInputElement;
	expect(toggle.checked).toBe(false);
	fireEvent.click(toggle);
	expect(requestPermission).toHaveBeenCalledTimes(1);
	expect(toggle.disabled).toBe(true);
	finish("default");
	await waitFor(() => expect(toggle.disabled).toBe(false));
	expect(toggle.checked).toBe(false);
	expect(screen.getByText("Permission not granted")).toBeTruthy();
});

// Catches: sound or wake Switch changes no longer reach the persisted preference keys.
it("persists notification sound and screen-wake switch changes across remounts", () => {
	const mount = () => render(() => <SettingsScreen isConnected />);
	mount();
	for (const name of ["Notification sounds", "Keep screen awake while dictating"]) {
		const toggle = screen.getByRole("switch", { name }) as HTMLInputElement;
		expect(toggle.checked).toBe(true);
		fireEvent.click(toggle);
		expect(toggle.checked).toBe(false);
	}
	cleanup();
	mount();
	for (const name of ["Notification sounds", "Keep screen awake while dictating"])
		expect((screen.getByRole("switch", { name }) as HTMLInputElement).checked).toBe(false);
});
