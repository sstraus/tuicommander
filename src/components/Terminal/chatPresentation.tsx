import { type Component, createMemo, createSignal, Show } from "solid-js";
import type { AcpTranscriptEntry } from "../../stores/acpTranscript";
import type { AcpToolCall } from "../../types/acp";
import { Button } from "../ui";
import s from "./ChatPresentation.module.css";

/**
 * The joined row last handed out for an answer entry. A row is rebuilt only when its joined text
 * changes: <For> keys by reference, so a fresh object per call would rebuild every earlier answer
 * (and drop its renderer state) each time the transcript grows.
 */
const joinedRows = new WeakMap<object, AcpTranscriptEntry>();

/** Presentation only: keep status rows in place and join the answer at its last row. */
export function terminalAnswerRows(entries: AcpTranscriptEntry[], intermediate: Set<string>): AcpTranscriptEntry[] {
	const rows: AcpTranscriptEntry[] = [];
	let turn: AcpTranscriptEntry[] = [];
	const flush = () => {
		const answers = turn.filter((entry) => entry.kind === "agent" && !intermediate.has(entry.id));
		const ids = new Set(answers.map((entry) => entry.id));
		const last = answers.at(-1);
		for (const entry of turn) {
			if (!ids.has(entry.id)) rows.push(entry);
			else if (entry === last && entry.kind === "agent") {
				const text = answers.map((answer) => (answer.kind === "agent" ? answer.text : "")).join("\n\n");
				let row = joinedRows.get(entry);
				if (row?.kind !== "agent" || row.text !== text) {
					row = { ...entry, text };
					joinedRows.set(entry, row);
				}
				rows.push(row);
			}
		}
		turn = [];
	};
	for (const entry of entries) {
		if (
			entry.kind === "user" ||
			entry.kind === "settled" ||
			entry.kind === "failed" ||
			entry.kind === "notice" ||
			(turn.length && turn[0].inherited !== entry.inherited)
		)
			flush();
		turn.push(entry);
	}
	flush();
	return rows;
}

export function terminalToolTitle(call: AcpToolCall, agent?: string): string {
	if (agent === "claude" && /^Bash(?::|$)/.test(call.title) && call.rawInput && typeof call.rawInput === "object") {
		const description = (call.rawInput as Record<string, unknown>).description;
		if (typeof description === "string" && description.trim()) return description;
	}
	return call.title;
}

/** Require a CLI call followed by its output marker, not indentation or multiline prose alone. */
function pasteStart(text: string): number {
	const lines = text.split("\n");
	const marker = lines.findIndex((line) => /^\s*⎿(?:\s|$)/.test(line));
	if (marker < 1) return -1;
	const call = /\b(?:Bash|Read|Write|Edit|Grep|Glob)\([^\n]*\)/.exec(lines[marker - 1]);
	if (!call) return -1;
	return lines.slice(0, marker - 1).reduce((length, line) => length + line.length + 1, 0) + call.index;
}

export const TerminalUserText: Component<{ text: string; prose: (text: string) => import("solid-js").JSX.Element }> = (
	props,
) => {
	const [expanded, setExpanded] = createSignal(false);
	const start = createMemo(() => pasteStart(props.text));
	const lines = createMemo(() => props.text.slice(start()).split("\n"));
	const extra = () => Math.max(0, lines().length - 2);
	return (
		<Show when={start() >= 0} fallback={props.prose(props.text)}>
			{props.prose(props.text.slice(0, start()))}
			<div class={s.paste}>
				<pre>{(expanded() ? lines() : lines().slice(0, 2)).join("\n")}</pre>
				<Show when={extra() > 0}>
					<Button
						variant="ghost"
						type="button"
						aria-expanded={expanded()}
						aria-label={expanded() ? "Collapse pasted output" : `Show ${extra()} more lines`}
						onClick={() => setExpanded(!expanded())}
					>
						{expanded() ? "Show less" : `+ ${extra()} lines`}
					</Button>
				</Show>
			</div>
		</Show>
	);
};
