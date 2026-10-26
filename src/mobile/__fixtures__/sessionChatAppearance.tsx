/** Layout-only session Chat: production transcript, switch and input components.
 * Text is synthetic presentation input, not evidence of an agent transport. */
import { render } from "solid-js/web";
import commander from "../../../src-tauri/src/themes/commander.json";
import paper from "../../../src-tauri/src/themes/vscode-light.json";
import answerScope from "../../__tests__/components/fixtures/chat-answer-scope-2158.json";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import { ComposePanel } from "../../components/ComposePanel/ComposePanel";
import { ViewModeToggle } from "../../components/Terminal/TerminalChatView";
import reading from "../../components/Terminal/TerminalChatView.module.css";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";
import { terminalsStore } from "../../stores/terminals";
import { CommandInput } from "../components/CommandInput";
import { useSessionChatView } from "../components/useSessionChatView";

async function mount() {
	const params = new URLSearchParams(location.search);
	const mobile = params.has("mobile");
	if (mobile) await import("../mobile.css");
	else {
		await import("../../global.css");
		await import("../../styles.css");
	}
	const light = params.get("theme") === "light";
	document.documentElement.dataset.theme = light ? "vscode-light" : "commander";
	document.documentElement.style.colorScheme = light ? "light" : "dark";
	const names: Record<string, string> = {
		background: "bg-primary",
		surface: "bg-secondary",
		surfaceElevated: "bg-tertiary",
		highlight: "bg-highlight",
		foreground: "fg-primary",
		foregroundSecondary: "fg-secondary",
		mutedForeground: "fg-muted",
		accentForeground: "text-on-accent",
		errorForeground: "text-on-error",
		successForeground: "text-on-success",
	};
	for (const [key, value] of Object.entries((light ? paper : commander).appChrome)) {
		const name = names[key] ?? key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
		document.documentElement.style.setProperty(`--${name}`, value);
	}
	const codeSample = params.has("longCode")
		? `const screen = "${"mobile".repeat(30)}";\nconsole.log(screen);`
		: 'const screen = "mobile";';
	const entries: AcpTranscriptEntry[] = [
		{ id: "u", kind: "user", text: "Check the session Chat layout on a narrow screen." },
		{ id: "s", kind: "agent", text: "Inspecting the shared styles." },
		{ id: "t", kind: "tool", call: { toolCallId: "read", title: "Read: shared presentation", status: "completed" } },
		{
			id: "a",
			kind: "agent",
			text: params.has("answerScope")
				? answerScope.text
				: "💬 The answer uses the same reading layout.\n\n> A genuine quotation.\n\n- Shared user prompt tint\n- Shared folded tool activity\n\n```ts\n" +
					codeSample +
					"\n```",
		},
	];
	const terminalId = terminalsStore.add({
		sessionId: "appearance-only",
		cwd: "/layout-only",
		name: "Claude",
		fontSize: 14,
		awaitingInput: null,
		agentType: "claude",
		agentSessionId: null,
	});
	terminalsStore.setViewMode(terminalId, "chat");
	render(() => {
		const mobileChat = useSessionChatView({
			sessionId: () => "appearance-only",
			agentType: () => "claude",
			sessionExists: () => true,
			subscribeWake: async () => () => {},
		});
		return (
			<section style={{ height: "100dvh", display: "flex", "flex-direction": "column", "min-width": "0" }}>
				<header
					style={{
						display: "flex",
						"justify-content": "space-between",
						"align-items": "center",
						"min-height": "56px",
						padding: "0 12px",
						position: "relative",
					}}
				>
					<strong>Claude session Chat</strong>
					{mobile ? <mobileChat.Toggle /> : <ViewModeToggle terminalId={terminalId} />}
				</header>
				<div class={reading.chatView}>
					<Transcript
						terminalAgent="claude"
						mobile={mobile}
						entries={() => entries}
						busy={() => false}
						emptyMessage=""
						observeToolDuration={false}
					/>
				</div>
				{mobile ? (
					<CommandInput sessionId="appearance-only" sessionExists={false} />
				) : (
					<ComposePanel
						isOpen={() => true}
						initialText={() => ""}
						onClose={() => {}}
						onSend={() => {}}
						onEnqueue={() => {}}
						canEnqueue={() => false}
						queuedCount={() => 0}
						onClearQueue={() => {}}
						onLoadQueue={async () => []}
						onRemoveQueued={() => {}}
						pinned={() => true}
						persistent={() => true}
						onTogglePin={() => {}}
						onDismiss={() => {}}
						focusRequest={() => 0}
					/>
				)}
			</section>
		);
	}, document.getElementById("mobile-app")!);
	if (mobile) {
		const chat = [...document.querySelectorAll<HTMLButtonElement>("button")].find(
			(button) => button.textContent === "Chat",
		);
		chat?.click();
	}
}
void mount();
