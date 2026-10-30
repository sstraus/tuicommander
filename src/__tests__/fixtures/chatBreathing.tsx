import { render } from "solid-js/web";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import "../../global.css";
import "../../styles.css";
import commander from "../../../src-tauri/src/themes/commander.json";
import paper from "../../../src-tauri/src/themes/vscode-light.json";
import "./chatBreathing.css";

const params = new URLSearchParams(location.search);
const palette = params.get("theme") === "light" ? paper.appChrome : commander.appChrome;
const variables: Record<string, string> = {
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
for (const [key, value] of Object.entries(palette)) {
	document.documentElement.style.setProperty(
		`--${variables[key] ?? key.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`)}`,
		value,
	);
}
document.documentElement.style.colorScheme = params.get("theme") === "light" ? "light" : "dark";
const entries = () => [
	{ id: "request", kind: "user" as const, text: "Check the changes and prepare a short summary." },
	{ id: "reply", kind: "agent" as const, text: "I will check the affected components and their tests." },
];
render(
	() => (
		<main class="working-comparison">
			<header>
				<h1>Chat · breathing orb</h1>
				<p>Default working feedback · theme accent · reduced-motion support</p>
			</header>
			<section>
				<h2>Session Chat</h2>
				<div class="working-transcript">
					<Transcript
						entries={entries}
						busy={() => true}
						busyLabel="Working…"
						terminalAgent="claude"
						emptyMessage=""
						observeToolDuration={false}
					/>
				</div>
			</section>
			<section>
				<h2>AI Chat</h2>
				<div class="working-transcript">
					<Transcript entries={entries} busy={() => true} emptyMessage="" observeToolDuration={false} />
				</div>
			</section>
			<section>
				<h2>Mobile Chat</h2>
				<div class="working-transcript mobile-working-transcript">
					<Transcript
						entries={entries}
						busy={() => true}
						mobile
						busyLabel="Working…"
						emptyMessage=""
						observeToolDuration={false}
					/>
				</div>
			</section>
			<footer>Real shared Transcript · simulated busy prop · no agent or backend launched</footer>
		</main>
	),
	document.body,
);
