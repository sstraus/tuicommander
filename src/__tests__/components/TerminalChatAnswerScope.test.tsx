// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, expect, it } from "vitest";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import answerScope from "./fixtures/chat-answer-scope-2158.json";

const terminalStyles = readFileSync("src/components/AIChatPanel/AIChatPanel.module.css", "utf8");

const message = answerScope.text;

let style: HTMLStyleElement | undefined;
afterEach(() => {
	cleanup();
	style?.remove();
});

// Exercise the production terminal rules in jsdom, where CSS Modules are non-scoped.
// Resolve the theme variable because jsdom cannot resolve it in border shorthands.
function applyTerminalStyles() {
	style = document.createElement("style");
	style.textContent = (terminalStyles.replace(/\/\*[\s\S]*?\*\//g, "").match(/[^{}]+\{[^{}]*\}/g) ?? [])
		.filter((rule) => rule.trim().startsWith(".messageList[data-terminal-chat]"))
		.map((rule) => rule.replace(/:global\((.*)\)/g, "$1").replace(/var\(--(?:success|answer-rule)\)/g, "green"))
		.join("\n");
	document.head.append(style);
}

// Catches: the final-message card border bypassing the CLI extent and tinting status/list/proposal prose.
it.each([false, true])("rules only the answer paragraph, not the terminal Chat message (mobile=%s)", async (mobile) => {
	applyTerminalStyles();
	const { container } = render(() => (
		<Transcript
			entries={() => [{ id: "answer", kind: "agent", text: message }]}
			busy={() => false}
			emptyMessage=""
			terminalAgent="claude"
			mobile={mobile}
			observeToolDuration={false}
		/>
	));
	await Promise.resolve();
	const blocks = Array.from(container.querySelectorAll<HTMLElement>("#markdown-content > div > *"));
	expect(blocks.map((block) => block.hasAttribute("data-tuic-answer"))).toEqual([
		true,
		false,
		false,
		false,
		false,
		false,
	]);
	const rules = Array.from(style!.sheet!.cssRules).filter(
		(rule): rule is CSSStyleRule => rule.type === CSSRule.STYLE_RULE,
	);
	const ruled = Array.from(container.querySelectorAll<HTMLElement>("*")).filter((element) =>
		rules.some(
			(rule) =>
				element.matches(rule.selectorText) &&
				parseFloat(rule.style.getPropertyValue("border-left") || rule.style.getPropertyValue("border-left-width")) > 0,
		),
	);
	expect(ruled.map((element) => element.textContent)).toEqual([
		"Boss, both things are done: the mute button and the bench results.",
	]);
});

// Catches: moving the bar to paragraphs alone dropping uninterrupted list continuations,
// sweeping an unmarked final reply into the answer, or losing a later marked paragraph.
it.each<[string, boolean[]]>([
	["💬 Result:\n  continued\n- Attached detail", [true, true]],
	["Unmarked final reply.", [false]],
	["💬 First.\n\nUnmarked middle.\n\n💬 Second.", [true, false, true]],
])("keeps CLI answer ownership for %s", async (text, expected) => {
	const { container } = render(() => (
		<Transcript
			entries={() => [{ id: "answer", kind: "agent", text }]}
			busy={() => false}
			emptyMessage=""
			terminalAgent="claude"
			observeToolDuration={false}
		/>
	));
	await Promise.resolve();
	expect(
		Array.from(container.querySelectorAll("#markdown-content > div > *")).map((block) =>
			block.hasAttribute("data-tuic-answer"),
		),
	).toEqual(expected);
});
