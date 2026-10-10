import { appLogger } from "../stores/appLogger";

/** Own the page's Now Playing controls only while this tab owns hands-free. */
export function browserVoiceMediaSession(controls: {
	play(): Promise<void>;
	pause(): Promise<void>;
	stop(): Promise<void>;
}) {
	const media = navigator.mediaSession;
	if (!media) return null;
	const actions = ["play", "pause", "stop"] as const;
	const setHandler = (action: (typeof actions)[number], handler: MediaSessionActionHandler | null) => {
		try {
			media.setActionHandler(action, handler);
		} catch (err) {
			// Browsers can expose Media Session without supporting every action.
			appLogger.warn("dictation", `Now Playing ${action} unavailable`, err);
		}
	};
	if (typeof MediaMetadata !== "undefined") {
		media.metadata = new MediaMetadata({ title: "TUICommander hands-free" });
	}
	for (const action of actions) setHandler(action, () => controls[action]());
	media.playbackState = "playing";
	return {
		update(paused: boolean) {
			// The live conversation stays active while listening, even between replies.
			media.playbackState = paused ? "paused" : "playing";
		},
		stop() {
			for (const action of actions) setHandler(action, null);
			media.metadata = null;
			media.playbackState = "none";
		},
	};
}
