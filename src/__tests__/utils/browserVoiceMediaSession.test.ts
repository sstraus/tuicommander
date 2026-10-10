import { afterEach, describe, expect, it, vi } from "vitest";
import { appLogger } from "../../stores/appLogger";
import { browserVoiceMediaSession } from "../../utils/browserVoiceMediaSession";

vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn() } }));

afterEach(() => {
	Reflect.deleteProperty(navigator, "mediaSession");
	vi.unstubAllGlobals();
	vi.clearAllMocks();
});

const controls = () => ({ play: vi.fn(async () => {}), pause: vi.fn(async () => {}), stop: vi.fn(async () => {}) });

describe("browser hands-free Now Playing", () => {
	// Catches: browsers without Media Session cannot arm hands-free.
	it("needs no Media Session API to keep dictation available", () => {
		Object.defineProperty(navigator, "mediaSession", { configurable: true, value: undefined });
		expect(browserVoiceMediaSession(controls())).toBeNull();
	});

	// Catches: an unsupported Stop action prevents Pause/Play and breaks cleanup.
	it("keeps supported controls working when the browser rejects Stop", async () => {
		const handlers = new Map<MediaSessionAction, MediaSessionActionHandler | null>();
		const media = {
			metadata: null,
			playbackState: "none",
			setActionHandler(action: MediaSessionAction, handler: MediaSessionActionHandler | null) {
				if (action === "stop") throw new DOMException("unsupported", "NotSupportedError");
				handlers.set(action, handler);
			},
		};
		Object.defineProperty(navigator, "mediaSession", { configurable: true, value: media });
		const actions = controls();
		const session = browserVoiceMediaSession(actions);
		await handlers.get("pause")?.({ action: "pause" });
		await handlers.get("play")?.({ action: "play" });
		expect(actions.pause).toHaveBeenCalledOnce();
		expect(actions.play).toHaveBeenCalledOnce();
		session?.stop();
		expect(handlers.get("pause")).toBeNull();
		expect(handlers.get("play")).toBeNull();
		expect(media.playbackState).toBe("none");
		expect(media.metadata).toBeNull();
		expect(appLogger.warn).toHaveBeenCalledTimes(2);
	});
});
