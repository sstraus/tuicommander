/** Visual priority shared by desktop terminal dots and mobile session badges. */
export type TerminalVisualState =
	| "error"
	| "question"
	| "stalled"
	| "busy"
	| "background"
	| "unseen"
	| "idle"
	| "default";

/** A ready composer can coexist with work owned by the agent. */
export function showsBackgroundWork(shellState: string | null | undefined, backgroundWork?: boolean): boolean {
	return shellState === "idle" && backgroundWork === true;
}

export function terminalVisualState(state: {
	error?: boolean;
	question?: boolean;
	/** Backend stall flag: a silent "working" agent must not render as busy. */
	stalled?: boolean;
	busy?: boolean;
	background?: boolean;
	unseen?: boolean;
	idle?: boolean;
}): TerminalVisualState {
	if (state.error) return "error";
	if (state.question) return "question";
	if (state.stalled) return "stalled";
	if (state.busy) return "busy";
	if (state.background) return "background";
	if (state.unseen) return "unseen";
	if (state.idle) return "idle";
	return "default";
}
