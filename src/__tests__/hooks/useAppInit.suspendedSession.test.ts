import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

const { mockRpc } = vi.hoisted(() => ({ mockRpc: vi.fn().mockResolvedValue(undefined) }));

vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: mockRpc,
}));

import { listen } from "@tauri-apps/api/event";
import { type AppInitDeps, initApp } from "../../hooks/useAppInit";
import { paneLayoutStore } from "../../stores/paneLayout";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";

function createMockDeps(): AppInitDeps {
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

type Handler = (event: { payload: unknown }) => void;

async function start() {
	const handlers = new Map<string, Handler>();
	vi.mocked(listen).mockImplementation(((event: string, h: Handler) => {
		handlers.set(event, h);
		return Promise.resolve(vi.fn());
	}) as unknown as typeof listen);
	await initApp(createMockDeps());
	return handlers;
}

describe("Rust suspended-session events", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		mockRpc.mockReset().mockResolvedValue(undefined);
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	});
	afterEach(() => {
		vi.restoreAllMocks();
		vi.useRealTimers();
		repositoriesStore._testCancelPendingSave();
		paneLayoutStore._testCancelPendingSave();
	});
	// Catches: EOF removing a remotely suspended tab because its Rust record was ignored.
	it("adopts suspension before the session-closed event", async () => {
		const handlers = await start();
		const id = terminalsStore.add({
			sessionId: "old-pty",
			cwd: "/Gits/alpha",
			name: "shell",
			fontSize: 14,
			awaitingInput: null,
			tuicSession: "peer",
		});
		handlers.get("session-suspended")?.({
			payload: {
				record: {
					tuic_session: "peer",
					session_id: "old-pty",
					cwd: "/Gits/alpha",
					name: "shell",
					agent_type: null,
					agent_session_id: null,
					launch_command: null,
					alias: "al-1",
					pane_slot: null,
					worktree: null,
					reason: "user",
				},
			},
		});
		handlers.get("session-closed")?.({ payload: { session_id: "old-pty" } });
		await vi.advanceTimersByTimeAsync(60_000);
		expect(terminalsStore.get(id)).toMatchObject({ suspended: true, sessionId: null, alias: "al-1" });
	});
});
