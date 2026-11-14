/** The review contract shared by desktop and mobile Markdown views. */
export function buildMarkdownReviewPrompt(path: string, count: number, changedLines?: string | null): string {
	const noun = count === 1 ? "comment" : "comments";
	const tweaks = count > 0 ? `apply the ${count} embedded tweak review ${noun}, ` : "";
	const cleanup = count > 0 ? "remove each resolved tweak marker, and leave" : "and leave";
	const reading = changedLines
		? `read changed lines ${changedLines} with their enclosing sections`
		: "re-read the whole file";
	const answers = changedLines
		? "treat checkbox toggles and edited text there as the user's answers"
		: "treat every other change since your last write (checkbox toggles, edited text) as the user's answer";
	return `Open ${path.replace(/[\r\n\0]/g, "")}, ${reading}: ${tweaks}${answers}, ${cleanup} unrelated files unchanged.`;
}
