import type { LanguageSupport } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { highlightTree, tagHighlighter, tags } from "@lezer/highlight";
import { type Component, createMemo, createResource, For, Show } from "solid-js";
import s from "./AIChatPanel.module.css";

type Part = { text: string; role?: string };
const roles: Record<string, string> = {
	tool: s.toolLabel,
	path: s.toolPath,
	command: s.shellCommand,
	flag: s.shellFlag,
	string: s.shellString,
	operator: s.shellOperator,
	variable: s.shellVariable,
	comment: s.shellComment,
};
const shellHighlight = tagHighlighter([
	{ tag: tags.string, class: "string" },
	{ tag: tags.special(tags.string), class: "string" },
	{ tag: tags.attributeName, class: "flag" },
	{ tag: tags.operator, class: "operator" },
	{ tag: tags.standard(tags.variableName), class: "command" },
	{ tag: tags.keyword, class: "command" },
	{ tag: tags.definition(tags.variableName), class: "variable" },
	{ tag: tags.comment, class: "comment" },
]);
let shellLanguage: Promise<LanguageSupport> | undefined;

async function shellParts(source: string): Promise<Part[]> {
	// Reuse the editor's bundled Shell grammar, including its quote/escape state.
	const language = await (shellLanguage ??= languages.find((language) => language.name === "Shell")!.load());
	const ranges: { from: number; to: number; role: string }[] = [];
	highlightTree(language.language.parser.parse(source), shellHighlight, (from, to, role) => {
		ranges.push({ from, to, role });
	});
	const parts: Part[] = [];
	let end = 0;
	let command = true;
	const gap = (text: string) => {
		// The legacy Shell grammar leaves control operators and unknown executable
		// names unstyled. Fill only its gaps; never reinterpret strings/comments.
		for (const match of text.matchAll(/\s+|&&|\|\||[|;&()<>]|[^\s|;&()<>]+/g)) {
			const word = match[0];
			let role: string | undefined;
			if (/^[|;&()<>]/.test(word)) {
				role = "operator";
				if (/^[|;&(]/.test(word)) command = true;
			} else if (/^\s+$/.test(word)) {
				if (word.includes("\n")) command = true;
			} else {
				if (command) role = "command";
				command = false;
			}
			parts.push({ text: word, role });
		}
	};
	for (const range of ranges) {
		gap(source.slice(end, range.from));
		parts.push({
			text: source.slice(range.from, range.to),
			role: range.role === "command" && !command ? undefined : range.role,
		});
		if (range.role !== "variable" && range.role !== "operator") command = false;
		end = range.to;
	}
	gap(source.slice(end));
	return parts;
}

/** Inline presentation only: retain every title byte and the owner's geometry. */
export const ToolTitle: Component<{ title: string }> = (props) => {
	const title = createMemo(() => /^(\S+)(:\s*)([\s\S]*)$/.exec(props.title));
	const shell = () => !!title() && /^(bash|execute)$/i.test(title()![1]);
	const file = () => !!title() && /^(read|edit|write)$/i.test(title()![1]);
	const [highlighted] = createResource(
		() => shell() && title()![3],
		async (source) => ({
			source,
			parts: await shellParts(source),
		}),
	);
	const parts = () => (!highlighted.error && highlighted()?.source === title()?.[3] ? highlighted()?.parts : undefined);
	return (
		<Show when={title()} fallback={props.title}>
			{(title) => (
				<>
					<span class={s.toolLabel} data-tool-role="tool">
						{title()[1]}
					</span>
					{title()[2]}
					<Show
						when={file()}
						fallback={
							<Show when={shell() && parts()} fallback={title()[3]}>
								{(parts) => (
									<For each={parts()}>
										{(part) => (
											<span class={part.role ? roles[part.role] : undefined} data-tool-role={part.role}>
												{part.text}
											</span>
										)}
									</For>
								)}
							</Show>
						}
					>
						<span class={s.toolPath} data-tool-role="path">
							{title()[3]}
						</span>
					</Show>
				</>
			)}
		</Show>
	);
};
