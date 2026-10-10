import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { dictationStore } from "../../stores/dictation";
import { SessionDetailScreen } from "../screens/SessionDetailScreen";
import { SettingsScreen } from "../screens/SettingsScreen";

vi.mock("../components/OutputView", () => ({ OutputView: () => <div /> }));
vi.mock("../components/TerminalKeybar", () => ({ TerminalKeybar: () => <div /> }));
vi.mock("../../utils/browserVoice", () => ({
	connectBrowserVoice: vi.fn(async () => ({ stop: vi.fn() })),
}));
vi.mock("../../utils/earcon", () => ({ primeEarcons: vi.fn(), playEarcon: vi.fn() }));

let restore = () => {};
afterEach(async () => {
	await dictationStore.disarmHandsFree();
	cleanup();
	restore();
	vi.unstubAllGlobals();
});

it("persists the mobile Settings mute checkbox without ending the armed conversation", async () => {
	// catches: mobile mute updates a local checkbox but loses the preference or stops capture.
	const globals = globalThis as Record<string, unknown>;
	const internals = globals.__TAURI_INTERNALS__;
	delete globals.__TAURI_INTERNALS__;
	restore = () => {
		globals.__TAURI_INTERNALS__ = internals;
	};
	let config: Record<string, unknown> = {
		enabled: true,
		hotkey: "F5",
		language: "it",
		model: "large-v3-turbo",
		device: null,
		hands_free_spoken_replies: true,
	};
	let status: { armed: boolean; phase: string; sessionId: string | null; owner: string | null } = {
		armed: false,
		phase: "disarmed",
		sessionId: null,
		owner: null,
	};
	vi.stubGlobal(
		"fetch",
		vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
			const path = new URL(String(input), "http://localhost").pathname;
			let result: unknown = {};
			if (path === "/dictation/config") {
				if (init?.method === "PUT") {
					const body = JSON.parse(String(init.body)) as { config: Record<string, unknown> };
					config = { ...body.config };
				}
				result = { ...config };
			} else if (path.endsWith("/hands-free/arm")) {
				const body = JSON.parse(String(init?.body)) as { sessionId: string; owner: string };
				status = { armed: true, phase: "waiting", sessionId: body.sessionId, owner: body.owner };
				result = status;
			} else if (path.endsWith("/hands-free/disarm")) {
				status = { ...status, armed: false, phase: "disarmed" };
				result = { status };
			} else if (path.endsWith("/hands-free")) result = status;
			else if (path.endsWith("/dictation/status")) result = { audio_level: 0 };
			return new Response(JSON.stringify(result), { status: 200, headers: { "Content-Type": "application/json" } });
		}),
	);
	render(() => (
		<SessionDetailScreen
			session={{
				session_id: "s1",
				cwd: "/repo",
				worktree_path: null,
				worktree_branch: null,
				state: { agent_type: "claude", awaiting_input: false, rate_limited: false, last_activity_ms: 1 },
			}}
			sessionExists={true}
			onBack={() => {}}
			onOpenFiles={() => {}}
		/>
	));
	fireEvent.click(await screen.findByRole("button", { name: "Start voice conversation" }));
	await screen.findByRole("button", { name: "Stop voice conversation" });
	render(() => <SettingsScreen isConnected={true} />);
	const checkbox = (await screen.findByRole("checkbox", { name: "Spoken replies" })) as HTMLInputElement;
	expect(checkbox.checked).toBe(true);
	fireEvent.click(checkbox);
	await waitFor(() => expect(config.hands_free_spoken_replies).toBe(false));
	await waitFor(() => expect(checkbox.checked).toBe(false));
	expect(config.language).toBe("it");
	expect(dictationStore.state.handsFree?.armed).toBe(true);
	expect(status.armed).toBe(true);
	expect(screen.getByRole("button", { name: "Stop voice conversation" })).toBeTruthy();
	await dictationStore.refreshConfig();
	expect(checkbox.checked).toBe(false);
});
