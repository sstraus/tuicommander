import { afterEach, expect, it, vi } from "vitest";

vi.mock("../../utils/browserVoice", () => ({
	connectBrowserVoice: async () => ({ stop: vi.fn() }),
}));
vi.mock("../../utils/earcon", () => ({ primeEarcons: vi.fn(), playEarcon: vi.fn() }));

afterEach(() => {
	vi.clearAllTimers();
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

// Catches: a pre-arm status poll arriving late erases a newly armed Now Playing session and closes its microphone.
it("keeps a newly armed conversation active when a pre-arm unarmed poll arrives late", async () => {
	vi.useFakeTimers();
	vi.stubGlobal("__TAURI_SHIM__", true);
	const media = { metadata: null as MediaMetadata | null, playbackState: "none", setActionHandler: vi.fn() };
	vi.stubGlobal("navigator", { mediaSession: media });
	vi.stubGlobal(
		"MediaMetadata",
		class {
			constructor(public data: MediaMetadataInit) {}
		},
	);
	let answerOldPoll!: (response: Response) => void;
	const oldPoll = new Promise<Response>((resolve) => {
		answerOldPoll = resolve;
	});
	const json = (body: unknown) =>
		new Response(JSON.stringify(body), {
			headers: { "content-type": "application/json" },
		});
	vi.stubGlobal(
		"fetch",
		vi.fn(async (input: RequestInfo | URL) => {
			const path = new URL(String(input), "http://localhost").pathname;
			if (path === "/dictation/hands-free") return oldPoll;
			if (path === "/dictation/hands-free/arm") return json({ armed: true, phase: "waiting", sessionId: "terminal" });
			if (path === "/dictation/speech/status") return json({ paused: false });
			return json({});
		}),
	);
	vi.resetModules();
	const { dictationStore: store } = await import("../../stores/dictation");
	const refresh = store.refreshHandsFree();
	expect(await store.armHandsFree("terminal")).toBe(true);
	const activeMetadata = media.metadata;
	expect(activeMetadata).not.toBeNull();
	answerOldPoll(json({ armed: false, phase: "off" }));
	await refresh;
	expect(media.metadata).toBe(activeMetadata);
	expect(media.playbackState).toBe("playing");
});
