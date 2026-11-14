import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

const { mockRpc, mockNavigate } = vi.hoisted(() => ({
	mockRpc: vi.fn().mockResolvedValue(undefined),
	mockNavigate: vi.fn(),
}));

vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: mockRpc,
	// Progress/workflow subscriptions are outside this toast listener fixture.
	// Keep their async registration from outliving the test's Tauri mocks.
	subscribeEvents: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("../../utils/navigateToTerminal", () => ({ navigateToTerminal: mockNavigate }));

import { listen } from "@tauri-apps/api/event";
import { type AppInitDeps, initApp } from "../../hooks/useAppInit";
import { activityStore } from "../../stores/activityStore";
import { notificationsStore } from "../../stores/notifications";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { setSessionConnectionLookup } from "../../transportRuntime";
import { makeTerminal } from "../helpers/store";

type Payload = Record<string, unknown>;

function deps(): AppInitDeps {
	return {
		pty: { listActiveSessions: vi.fn().mockResolvedValue([]), close: vi.fn().mockResolvedValue(undefined) },
		setStatusInfo: vi.fn(),
		handleBranchSelect: vi.fn().mockResolvedValue(undefined),
		refreshAllBranchStats: vi.fn(),
		getDefaultFontSize: () => 14,
		stores: {
			hydrate: vi.fn().mockResolvedValue(undefined),
			startPolling: vi.fn(),
			stopPolling: vi.fn(),
			startAutoFetch: vi.fn(),
			startPrNotificationTimer: vi.fn(),
			loadFontFromConfig: vi.fn(),
			startDictationSettingsSync: vi.fn(() => vi.fn()),
			refreshDictationConfig: vi.fn().mockResolvedValue(undefined),
			startUserActivityListening: vi.fn(),
		},
		applyPlatformClass: vi.fn().mockReturnValue("macos"),
		registerRepo: vi.fn().mockResolvedValue(undefined),
	};
}

async function bootToastListener() {
	let callback: ((event: { payload: Payload }) => void) | null = null;
	vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: Payload }) => void) => {
		if (event === "mcp-toast") callback = handler;
		return Promise.resolve(vi.fn());
	}) as unknown as typeof listen);
	await initApp(deps());
	return (payload: Payload) => callback!({ payload });
}

const remote = (connection: string, name = connection) => ({ connection, name });

describe("remote mcp-toast (1439-d84f)", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.mocked(listen).mockReset().mockResolvedValue(vi.fn());
		mockNavigate.mockClear();
		activityStore.clearAll();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		vi.spyOn(notificationsStore, "play").mockResolvedValue(undefined);
	});

	afterEach(() => {
		vi.restoreAllMocks();
		vi.useRealTimers();
		setSessionConnectionLookup(() => undefined);
		repositoriesStore._testCancelPendingSave();
	});

	// Catches: a remote toast naming a session owned by ANOTHER connection (or a
	// local one) focuses that foreign terminal when clicked.
	it.each([
		["another connection", "other"],
		["a local session", undefined],
	])("a click never focuses a terminal owned by %s", async (_label, owner) => {
		const fire = await bootToastListener();
		terminalsStore.add(makeTerminal({ sessionId: "s-foreign" }));
		setSessionConnectionLookup((id) => (id === "s-foreign" ? owner : undefined));
		fire({
			title: "t",
			message: "m",
			level: "info",
			sound: null,
			origin_session_id: "s-foreign",
			__tuic_origin: remote("vps"),
		});
		activityStore.getForSection("messages")[0]?.onClick?.();
		expect(mockNavigate).not.toHaveBeenCalled();
	});

	// Catches: the click handler caches the terminal at arrival time and
	// navigates to / throws on a session that was closed since.
	it("a click after the remote terminal closed is a harmless no-op", async () => {
		const fire = await bootToastListener();
		const id = terminalsStore.add(makeTerminal({ sessionId: "s-1" }));
		setSessionConnectionLookup((sid) => (sid === "s-1" ? "vps" : undefined));
		fire({
			title: "t",
			message: "m",
			level: "info",
			sound: null,
			origin_session_id: "s-1",
			__tuic_origin: remote("vps"),
		});
		terminalsStore.remove(id);
		expect(() => activityStore.getForSection("messages")[0]?.onClick?.()).not.toThrow();
		expect(mockNavigate).not.toHaveBeenCalled();
	});

	// Catches: the remote's own cwd/repo path scopes the bell item to a LOCAL
	// registered repo that happens to share that path.
	it("a remote toast never inherits a local repository scope from the daemon's repo path", async () => {
		repositoriesStore.add({ path: "/srv/app", displayName: "Local app" });
		const fire = await bootToastListener();
		fire({
			title: "t",
			message: "m",
			level: "info",
			sound: null,
			origin_repo_path: "/srv/app",
			__tuic_origin: remote("vps"),
		});
		expect(activityStore.getForSection("messages")[0]?.repoPath).toBeUndefined();
	});

	// Catches: the host prefix is built from an undefined title, showing
	// "[mac-mint] undefined" for a malformed daemon frame.
	it("a remote toast without a string title is not shown as '[host] undefined'", async () => {
		const fire = await bootToastListener();
		fire({ message: "m", level: "info", sound: null, __tuic_origin: remote("vps", "mac-mint") });
		const titles = activityStore.getForSection("messages").map((item) => item.title);
		expect(titles.some((title) => String(title).includes("undefined"))).toBe(false);
	});

	// Catches: two different machines with the same display name raise the same
	// notice within the dedup window and the second one is silently swallowed.
	it("identical notices from two connections sharing a display name both reach the bell", async () => {
		const fire = await bootToastListener();
		const base = { title: "build done", message: "ok", level: "info", sound: null };
		fire({ ...base, __tuic_origin: remote("conn-a", "prod") });
		fire({ ...base, __tuic_origin: remote("conn-b", "prod") });
		expect(activityStore.getForSection("messages")).toHaveLength(2);
	});

	// Catches: dedup keyed on the unlabelled text drops the same notice raised
	// on two differently named hosts.
	it("identical notices from differently named hosts are two bell items", async () => {
		const fire = await bootToastListener();
		const base = { title: "build done", message: "ok", level: "info", sound: null };
		fire({ ...base, __tuic_origin: remote("conn-a", "alpha") });
		fire({ ...base, __tuic_origin: remote("conn-b", "beta") });
		expect(activityStore.getForSection("messages").map((item) => item.title)).toEqual(
			expect.arrayContaining(["[alpha] build done", "[beta] build done"]),
		);
	});

	// Catches: markup in a remote title/message is altered or becomes an HTML
	// sink on the way to the bell (it must arrive as inert text).
	it("markup in a remote notice reaches the bell verbatim as text", async () => {
		const fire = await bootToastListener();
		fire({
			title: "<img src=x onerror=alert(1)>",
			message: "<b>m</b>",
			level: "info",
			sound: null,
			__tuic_origin: remote("vps", "<i>h</i>"),
		});
		const item = activityStore.getForSection("messages")[0];
		expect(item?.title).toBe("[<i>h</i>] <img src=x onerror=alert(1)>");
		expect(item?.subtitle).toBe("<b>m</b>");
	});

	// Catches: a malformed mirrored frame still plays its sound, logs the daemon's
	// payload, or is discarded for a legitimate null message.
	it("a malformed remote frame makes no sound, no bell item and no payload log; null message is accepted", async () => {
		const { appLogger } = await import("../../stores/appLogger");
		const debug = vi.spyOn(appLogger, "debug");
		const fire = await bootToastListener();
		fire({ title: 7, message: "SECRET-BODY", level: "info", sound: "attention", __tuic_origin: remote("vps") });
		fire({ title: "t", message: 5, level: "info", sound: "attention", __tuic_origin: remote("vps") });
		expect(activityStore.getForSection("messages")).toHaveLength(0);
		expect(notificationsStore.play).not.toHaveBeenCalled();
		expect(JSON.stringify(debug.mock.calls)).not.toContain("SECRET-BODY");
		fire({ title: "ok", message: null, level: "info", sound: "attention", __tuic_origin: remote("vps") });
		expect(activityStore.getForSection("messages")).toHaveLength(1);
	});

	// Catches: the connection id in the dedup key breaks local dedup (no id) or
	// stops the same connection's repeat from collapsing.
	it("local notices still dedup, a same-connection repeat dedups, a local/remote twin does not", async () => {
		const fire = await bootToastListener();
		const base = { title: "same", message: "m", level: "info", sound: null };
		fire(base);
		fire(base);
		expect(activityStore.getForSection("messages")).toHaveLength(1);
		fire({ ...base, __tuic_origin: remote("c1", "prod") });
		fire({ ...base, __tuic_origin: remote("c1", "prod") });
		expect(activityStore.getForSection("messages")).toHaveLength(2);
	});
});
