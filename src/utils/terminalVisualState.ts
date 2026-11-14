/** Visual priority shared by desktop terminal dots and mobile session badges. */
export type TerminalVisualState = "error" | "question" | "stalled" | "busy" | "unseen" | "idle" | "default";

export function terminalVisualState(state: {
	error?: boolean;
	question?: boolean;
	/** Backend stall flag: a silent "working" agent must not render as busy. */
	stalled?: boolean;
	busy?: boolean;
	unseen?: boolean;
	idle?: boolean;
}): TerminalVisualState {
	if (state.error) return "error";
	if (state.question) return "question";
	if (state.stalled) return "stalled";
	if (state.busy) return "busy";
	if (state.unseen) return "unseen";
	if (state.idle) return "idle";
	return "default";
}
