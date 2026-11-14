import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import "../../../src/global.css";
import { SessionDetailScreen } from "../../../src/mobile/screens/SessionDetailScreen";
import { SettingsScreen } from "../../../src/mobile/screens/SettingsScreen";
import { browserAudioOwner, dictationStore } from "../../../src/stores/dictation";

let config = {
	enabled: true,
	hotkey: "F5",
	language: "it",
	model: "large-v3-turbo",
	device: null,
	speech_engine: "edge",
	hands_free_spoken_replies: true,
};
let armed = true;
const requests: { method: string; path: string }[] = [];
window.fetch = async (input, init) => {
	const path = new URL(String(input), location.href).pathname;
	const method = init?.method ?? "GET";
	requests.push({ method, path });
	let result: unknown = {};
	if (path === "/dictation/config") {
		if (method === "PUT") config = (JSON.parse(String(init?.body)) as { config: typeof config }).config;
		result = { ...config };
	} else if (path.endsWith("/hands-free/disarm")) {
		armed = false;
		result = { status: { armed, phase: "disarmed" } };
	} else if (path.endsWith("/hands-free")) {
		result = {
			armed,
			phase: armed ? "waiting" : "disarmed",
			sessionId: "owned-voice-preview",
			owner: browserAudioOwner,
		};
	} else if (path.endsWith("/dictation/status")) result = { audio_level: 0 };
	else if (path.includes("/output")) result = { lines: [], screen: ["Hands-free conversation"], total_lines: 0 };
	else if (path === "/config/themes") result = [];
	else if (path === "/api/version") result = { version: "visual evidence" };
	return new Response(JSON.stringify(result), { headers: { "Content-Type": "application/json" } });
};

declare global {
	interface Window {
		voiceEvidence: {
			state: () => {
				spokenReplies: boolean;
				persisted: boolean;
				armed: boolean | undefined;
				requests: typeof requests;
			};
			settings: () => void;
		};
	}
}
render(() => {
	const [settings, setSettings] = createSignal(false);
	window.voiceEvidence = {
		state: () => ({
			spokenReplies: dictationStore.state.spokenReplies,
			persisted: config.hands_free_spoken_replies,
			armed: dictationStore.state.handsFree?.armed,
			requests,
		}),
		settings: () => setSettings(true),
	};
	return (
		<div
			style={{
				height: "100dvh",
				display: "flex",
				"flex-direction": "column",
				background: "var(--bg-primary)",
				color: "var(--fg-primary)",
			}}
		>
			<Show
				when={settings()}
				fallback={
					<SessionDetailScreen
						session={{
							session_id: "owned-voice-preview",
							cwd: "/demo/voice",
							worktree_path: null,
							worktree_branch: null,
							state: { agent_type: "claude", awaiting_input: false, rate_limited: false, last_activity_ms: Date.now() },
						}}
						sessionExists={true}
						onBack={() => {}}
						onOpenFiles={() => {}}
					/>
				}
			>
				<SettingsScreen isConnected={true} />
			</Show>
		</div>
	);
}, document.getElementById("preview")!);
void dictationStore.refreshConfig().then(() => dictationStore.refreshHandsFree());
