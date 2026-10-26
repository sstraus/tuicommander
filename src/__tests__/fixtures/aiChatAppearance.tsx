import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { Composer } from "../../components/AIChatPanel/Composer";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { AcpChat } from "../../components/AIChatPanel/useAcpChat";
import "../../global.css";
import "../../styles.css";
import commander from "../../../src-tauri/src/themes/commander.json";
import paper from "../../../src-tauri/src/themes/vscode-light.json";

const [busy, setBusy] = createSignal(true);
const chat = {
	sessionId: () => "appearance-fixture",
	busy,
	queuedPrompts: () => [],
	configOptions: () => [],
	send: async () => {},
	cancel: async () => {},
} as unknown as AcpChat;
const entries = () => [
	{ id: "answer", kind: "agent" as const, text: "💬 A borderless answer.\n\n> A genuine quotation." },
];
const theme = new URLSearchParams(location.search).get("theme");
document.documentElement.style.colorScheme = theme === "light" ? "light" : "dark";
const palette = theme === "light" ? paper.appChrome : commander.appChrome;
const variables = {
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
for (const [name, value] of Object.entries(palette)) {
	const variable =
		variables[name as keyof typeof variables] ?? name.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
	document.documentElement.style.setProperty(`--${variable}`, value);
}

render(
	() => (
		<div style={{ width: "390px", height: "850px", display: "flex", "flex-direction": "column" }}>
			<section id="reply">
				<Transcript entries={entries} busy={() => false} emptyMessage="" observeToolDuration={false} />
			</section>
			<section id="terminal">
				<Transcript
					entries={entries}
					busy={() => false}
					emptyMessage=""
					terminalAgent="claude"
					observeToolDuration={false}
				/>
			</section>
			<section id="composer">
				<Composer chat={chat} />
			</section>
		</div>
	),
	document.body,
);
const bounds = (selector: string) => {
	const element = document.querySelector<HTMLElement>(selector);
	if (!element) throw new Error(`Missing ${selector}`);
	const rect = element.getBoundingClientRect();
	return { height: rect.height, center: rect.y + rect.height / 2 };
};
const controls = () => ({
	input: bounds("textarea"),
	park: bounds('[aria-label="Park draft"]'),
	send: bounds(busy() ? '[aria-label="Queue"]' : '[aria-label="Send"]'),
	...(busy() ? { stop: bounds('[title="Stop"]') } : {}),
});
setTimeout(() => {
	try {
		const answer = document.querySelector<HTMLElement>("#reply [data-tuic-answer]")!;
		const quote = document.querySelector<HTMLElement>("#reply blockquote")!;
		const card = document.querySelector<HTMLElement>("#terminal [data-answer-card]")!;
		const result = {
			answer: {
				content: getComputedStyle(answer, "::after").content,
				border: getComputedStyle(answer).borderLeftWidth,
			},
			quoteBorder: getComputedStyle(quote).borderLeftWidth,
			terminalBorder: getComputedStyle(card).borderLeftWidth,
			terminalAnswerBorder: getComputedStyle(card.querySelector("[data-tuic-answer]")!).borderLeftWidth,
			busy: controls(),
			idle: {},
		};
		setBusy(false);
		result.idle = controls();
		const output = document.createElement("pre");
		output.id = "appearance-result";
		output.textContent = JSON.stringify(result);
		document.body.append(output);
	} catch (error) {
		document.body.setAttribute("data-appearance-error", String(error));
	}
}, 1500);
