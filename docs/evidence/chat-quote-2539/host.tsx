// Evidence host for story 2539-6c53: the real Transcript (the renderer Terminal Chat and AI Chat share)
// beside a draft box on the real prompt-draft store. `?before` omits the host quote hook, as Terminal
// Chat was before the change. `?theme=light` selects Paper.
import { render } from "solid-js/web";
import { Transcript } from "../../../src/components/AIChatPanel/Transcript";
import { createPromptDraft } from "../../../src/stores/promptDraft";
import "../../../src/global.css";
import "../../../src/styles.css";
import commander from "../../../src-tauri/src/themes/commander.json";
import paper from "../../../src-tauri/src/themes/vscode-light.json";

const params = new URLSearchParams(location.search);
const light = params.get("theme") === "light";
document.documentElement.style.colorScheme = light ? "light" : "dark";
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
for (const [name, value] of Object.entries((light ? paper : commander).appChrome)) {
	const variable = variables[name] ?? name.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
	document.documentElement.style.setProperty(`--${variable}`, value);
}
document.body.style.background = "var(--bg-primary)";
document.body.style.color = "var(--fg-primary)";

const draft = createPromptDraft();
draft.activate("evidence");
const entries = () => [
	{ id: "u1", kind: "user" as const, text: "Why is the build slow?" },
	{ id: "a1", kind: "agent" as const, text: "The cache was cold after a toolchain update.\nA warm build takes 20 s." },
];

render(
	() => (
		<div style={{ display: "flex", "flex-direction": "column", height: "100vh", "max-width": "720px", margin: "0 auto" }}>
			<div style={{ flex: "1", "min-height": 0, display: "flex", "flex-direction": "column" }}>
				<Transcript
					entries={entries}
					busy={() => false}
					emptyMessage=""
					terminalAgent="claude"
					observeToolDuration={false}
					onQuote={params.has("before") ? undefined : (text) => draft.append(text)}
				/>
			</div>
			<textarea
				aria-label="Composer draft"
				rows={9}
				value={draft.text()}
				onInput={(event) => draft.set(event.currentTarget.value)}
				style={{ margin: "8px", padding: "8px", background: "var(--bg-secondary)", color: "var(--fg-primary)", border: "1px solid var(--border)", font: "13px monospace" }}
			/>
		</div>
	),
	document.body,
);
