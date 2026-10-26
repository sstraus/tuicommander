// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor, within } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import MobileApp from "../MobileApp";
import { FilesScreen } from "../screens/FilesScreen";

const network = vi.hoisted(() => ({ offline: true, repositoriesFailed: false, hanging: false }));
vi.mock("../../transport", () => ({
	isTauri: () => false,
	rpc: vi.fn(async (command: string, _args: unknown, _connection: unknown, signal?: AbortSignal) => {
		if (command === "list_active_sessions" && network.hanging)
			return new Promise((_resolve, reject) => {
				signal?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), { once: true });
			});
		if (network.offline) throw new TypeError("Load failed");
		if (command === "list_active_sessions") return [];
		if (command === "load_repositories" && network.repositoriesFailed) throw new TypeError("Load failed");
		if (command === "load_repositories") return { repos: { "/recovered-project": {} } };
		return {};
	}),
	subscribeEvents: vi.fn(async () => () => {}),
}));
vi.mock("../../invoke", () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn() } }));
vi.mock("../../stores/ideas", () => ({ ideasStore: { hydrate: vi.fn() } }));
vi.mock("../mobileTheme", () => ({ loadMobileTheme: vi.fn(async () => "commander") }));
vi.mock("../useMobileNotifications", () => ({ useMobileNotifications: vi.fn() }));
vi.mock("../useVersionCheck", () => ({
	useVersionCheck: () => ({ updateAvailable: () => false, serverDown: () => true, applyUpdate: vi.fn() }),
}));
vi.mock("../../components/McpConfirmHost/McpConfirmHost", () => ({ McpConfirmHost: () => <div /> }));

beforeEach(async () => {
	network.offline = true;
	network.repositoriesFailed = false;
	network.hanging = false;
	history.replaceState(null, "", "/mobile");
	await import("../screens/FilesScreen");
});
afterEach(() => {
	cleanup();
	vi.useRealTimers();
});

describe("mobile outage recovery", () => {
	// Catches: all main views disappear on API failure with no connection-test surface in their place.
	it("shows a connection landing with the server address and last error after an API failure", async () => {
		const view = render(() => <MobileApp />);
		await waitFor(() => expect(view.getByRole("heading", { name: "Server unreachable" })).toBeTruthy());
		expect(view.getByText(location.origin)).toBeTruthy();
		expect(
			within(view.getByRole("region", { name: "Connection test" })).getByText("Could not connect to the server."),
		).toBeTruthy();
		fireEvent.click(view.getByRole("button", { name: "Retry now" }));
		network.offline = false;
		window.dispatchEvent(new Event("online"));
		await waitFor(() => expect(view.getByText("No active sessions")).toBeTruthy());
		expect(view.queryByRole("heading", { name: "Server unreachable" })).toBeNull();
	});
	// Catches: a silent TCP connection leaves loading true and all main content hidden forever.
	it("shows the connection landing when the session request never responds", async () => {
		vi.useFakeTimers();
		network.offline = false;
		network.hanging = true;
		const view = render(() => <MobileApp />);
		await vi.advanceTimersByTimeAsync(5_000);
		expect(view.getByRole("heading", { name: "Server unreachable" })).toBeTruthy();
		network.hanging = false;
		fireEvent.click(view.getByRole("button", { name: "Retry now" }));
		await vi.advanceTimersByTimeAsync(0);
		expect(view.queryByRole("region", { name: "Connection test" })).toBeNull();
	});
	// Catches: a persisted page restored without visibilitychange never checks its lost server.
	it("checks the server on persisted pageshow and recovers on resume", async () => {
		network.offline = false;
		const view = render(() => <MobileApp />);
		await waitFor(() => expect(view.getByText("No active sessions")).toBeTruthy());
		network.offline = true;
		window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true }));
		await waitFor(() => expect(view.getByRole("heading", { name: "Server unreachable" })).toBeTruthy());
		network.offline = false;
		document.dispatchEvent(new Event("resume"));
		await waitFor(() => expect(view.queryByRole("region", { name: "Connection test" })).toBeNull());
	});

	// Catches: Files confuses an unsuccessful repository read with an empty successful list and never retries.
	it("retries a failed repository read on recovery without claiming an empty configuration", async () => {
		const [revision, setRevision] = createSignal(0);
		const view = render(() => <FilesScreen recoveryRevision={revision()} />);
		await waitFor(() => expect(view.getByRole("alert")).toBeTruthy());
		expect(view.queryByText("No repositories configured")).toBeNull();
		expect(view.getByRole("alert").textContent).not.toContain("TypeError");
		network.offline = false;
		setRevision(1);
		await waitFor(() => expect(view.getByRole("button", { name: /recovered-project/ })).toBeTruthy());
	});
	// Catches: failed mount-only repository reads never run again when Retry now restores sessions.
	it("reloads Files after manual retry and clears stale version-health offline state", async () => {
		const view = render(() => <MobileApp />);
		await waitFor(() => expect(view.getByText(/Server unreachable/)).toBeTruthy());
		fireEvent.click(view.getByRole("button", { name: "Files" }));
		await waitFor(() => expect(view.queryByText("No repositories configured")).toBeNull());
		expect(view.queryByText(/TypeError: Load failed/)).toBeNull();
		network.offline = false;
		fireEvent.click(view.getByText("Retry now"));
		await waitFor(() => expect(view.getByRole("button", { name: /recovered-project/ })).toBeTruthy());
		expect(view.queryByText(/Server unreachable/)).toBeNull();
	});
	// Catches: online/automatic recovery refreshes sessions but leaves the selected view stuck.
	it("reloads the selected Files view after an online recovery", async () => {
		const view = render(() => <MobileApp />);
		await waitFor(() => expect(view.getByText(/Server unreachable/)).toBeTruthy());
		fireEvent.click(view.getByRole("button", { name: "Files" }));
		network.offline = false;
		window.dispatchEvent(new Event("online"));
		await waitFor(() => expect(view.getByRole("button", { name: /recovered-project/ })).toBeTruthy());
		expect(view.queryByText(/Server unreachable/)).toBeNull();
	});
});
