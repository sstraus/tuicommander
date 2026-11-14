/** Local to prompt editors; never install this on a terminal or document. */
export function promptKeyAction(event: KeyboardEvent): "park" | "send" | "newline" | null {
	if (event.isComposing || event.keyCode === 229 || event.altKey) return null;
	if (event.key.toLowerCase() === "s" && (event.ctrlKey || event.metaKey) && !event.shiftKey) return "park";
	if (event.key !== "Enter") return null;
	return event.shiftKey ? "newline" : "send";
}
