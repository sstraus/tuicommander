import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { WorkingEffect } from "../../components/AIChatPanel/WorkingIndicator";
import { Select } from "../../components/ui/Select";
import "../../global.css";
import "../../styles.css";
import commander from "../../../src-tauri/src/themes/commander.json";
import paper from "../../../src-tauri/src/themes/vscode-light.json";
import "./chatWorkingEffects.css";

const params = new URLSearchParams(location.search);
const effects: WorkingEffect[] = ["shimmer", "dots", "orb", "typewriter"];
const initial = params.get("effect") as WorkingEffect;
const [effect, setEffect] = createSignal<WorkingEffect>(effects.includes(initial) ? initial : "shimmer");
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
				<h1>Chat · working effects</h1>
				<p>Four candidates. No product default selected.</p>
				<label>
					Effect{" "}
					<Select value={effect()} onChange={(event) => setEffect(event.currentTarget.value as WorkingEffect)}>
						<option value="shimmer">01 · Shimmer sweep</option>
						<option value="dots">02 · Accent dots</option>
						<option value="orb">03 · Breathing orb</option>
						<option value="typewriter">04 · Typewriter cycle</option>
					</Select>
				</label>
			</header>
			<section>
				<h2>
					Session Chat <small>Current: Working…</small>
				</h2>
				<div class="working-transcript">
					<Transcript
						entries={entries}
						busy={() => true}
						busyLabel="Working…"
						workingEffect={effect()}
						terminalAgent="claude"
						emptyMessage=""
						observeToolDuration={false}
					/>
				</div>
			</section>
			<section>
				<h2>
					AI Chat <small>Current: pulsing ellipsis</small>
				</h2>
				<div class="working-transcript">
					<Transcript
						entries={entries}
						busy={() => true}
						workingEffect={effect()}
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
