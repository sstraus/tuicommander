import { render } from "solid-js/web";
import commander from "../../../src-tauri/src/themes/commander.json";
import paper from "../../../src-tauri/src/themes/vscode-light.json";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";

const params = new URLSearchParams(location.search);
const mobile = params.get("mobile") === "true";
if (mobile) await import("../../mobile/mobile.css");
else await import("../../global.css");
const palette = params.get("theme") === "light" ? paper.appChrome : commander.appChrome;
const names: Record<string, string> = {
	background: "bg-primary",
	surface: "bg-secondary",
	surfaceElevated: "bg-tertiary",
	highlight: "bg-highlight",
	foreground: "fg-primary",
	foregroundSecondary: "fg-secondary",
	mutedForeground: "fg-muted",
	accentForeground: "text-on-accent",
};
for (const [name, value] of Object.entries(palette)) {
	document.documentElement.style.setProperty(
		`--${names[name] ?? name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`,
		value,
	);
}
document.body.style.cssText =
	"margin:0;padding:24px;background:var(--bg-primary);color:var(--fg-primary);overflow:auto";
const entries: AcpTranscriptEntry[] = [
	{
		id: "prose",
		kind: "agent",
		text: "I am checking the conversation styling. This explanation must remain easy to read while the work continues.",
	},
	{ id: "tool", kind: "tool", call: { toolCallId: "read", title: "Read conversation styles", status: "completed" } },
	{
		id: "answer",
		kind: "agent",
		text: "💬 The answer includes these details:\n- First detail stays inside the answer indent.\n- Second detail wraps naturally when the conversation is narrow.\n  - Nested detail keeps its normal indent.\n\nUnmarked prose must not acquire an answer rule.\n\n💬 Ordered details:\n1. First ordered detail.\n2. Second ordered detail.\n\n> A real quotation keeps its own border.\n\n💬 Code follows:\n```txt\nexample\n```",
	},
];
entries.push(
	{
		id: "marked-list",
		kind: "agent",
		text: "💬 Answer paragraph reference.\n\nSeparate list:\n\n- 💬 First marked answer.\n- 💬 Second marked answer wraps naturally on the phone.\n\n1. 💬 First ordered answer.\n2. 💬 Second ordered answer.",
	},
	{ id: "mixed-list", kind: "agent", text: "Mixed list:\n\n- 💬 Marked mixed answer.\n- Unmarked mixed sibling." },
);
render(
	() => (
		<main>
			<h2>Chat readability</h2>
			<section id="terminal">
				<Transcript
					entries={() => entries}
					busy={() => false}
					emptyMessage=""
					terminalAgent="claude"
					mobile={mobile}
					observeToolDuration={false}
				/>
			</section>
			<h2>AI Chat</h2>
			<section id="acp">
				<Transcript
					entries={() => entries}
					busy={() => false}
					emptyMessage=""
					mobile={mobile}
					observeToolDuration={false}
				/>
			</section>
		</main>
	),
	document.body,
);

// Read layout from the real rendering engine, not parsed CSS or ownership attributes alone.
(window as unknown as { chatReadability: () => unknown }).chatReadability = () => {
	const sample = (scope: string) => {
		const root = document.querySelector(scope)!;
		const prose = root.querySelector("[data-intermediate] #markdown-content")!;
		const foreground = getComputedStyle(document.body).color;
		return { color: getComputedStyle(prose).color, foreground };
	};
	const lists = Array.from(
		document.querySelectorAll<HTMLElement>("#terminal ul[data-tuic-answer], #terminal ol[data-tuic-answer]"),
	).map((list) => {
		const style = getComputedStyle(list);
		const rail = list.closest<HTMLElement>("[data-tuic-answer-region]")!;
		const railBorder = parseFloat(getComputedStyle(rail).borderLeftWidth);
		const item = list.querySelector("li")!;
		const range = document.createRange();
		range.selectNodeContents(item.firstChild!);
		return {
			tag: list.tagName,
			border: railBorder,
			padding: parseFloat(style.paddingLeft),
			font: parseFloat(style.fontSize),
			ruleRight: rail.getBoundingClientRect().left + railBorder,
			itemLeft: item.getBoundingClientRect().left,
			textLeft: range.getBoundingClientRect().left,
			marker: getComputedStyle(item, "::marker").color,
		};
	});
	const answerLists = Array.from(document.querySelectorAll<HTMLElement>("#terminal ul, #terminal ol"))
		.filter(
			(list) =>
				list.textContent?.includes("marked answer") ||
				list.textContent?.includes("ordered answer") ||
				list.textContent?.includes("mixed answer"),
		)
		.map((list) => ({
			listBorder: parseFloat(getComputedStyle(list).borderLeftWidth),
			ruleLeft: list.closest("[data-tuic-answer-region]")!.getBoundingClientRect().left,
			railBorder: parseFloat(getComputedStyle(list.closest("[data-tuic-answer-region]")!).borderLeftWidth),
			items: Array.from(list.children).map((item) => ({
				border: parseFloat(getComputedStyle(item).borderLeftWidth),
				answer: item.hasAttribute("data-tuic-answer"),
				left: item.getBoundingClientRect().left,
			})),
		}));
	const reference = Array.from(document.querySelectorAll("#terminal p")).find(
		(p) => p.textContent === "Answer paragraph reference.",
	)!;
	const unmarked = Array.from(document.querySelectorAll("#terminal p")).find((p) =>
		p.textContent?.startsWith("Unmarked"),
	)!;
	return {
		mobile,
		theme: params.get("theme"),
		terminal: sample("#terminal"),
		acp: sample("#acp"),
		lists,
		unmarkedBorder: getComputedStyle(unmarked).borderLeftWidth,
		answerLists,
		paragraphRuleLeft: reference.closest("[data-tuic-answer-region]")!.getBoundingClientRect().left,
	};
};
