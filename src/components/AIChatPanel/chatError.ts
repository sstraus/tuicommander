export interface ChatErrorView {
	text: string;
	retryable: boolean;
	/** The agent refused to reopen this conversation; only a new chat helps. */
	startNew: boolean;
}

const UNCERTAIN_TEXT = "This conversation cannot be reopened: its last run ended in an uncertain state.";

/**
 * The HTTP transport turns a refusal into `RPC <command> failed: <status> <body>`
 * with the `AcpClientError` JSON as body. Read that body when it is there; any
 * other text is shown as it is and stays retryable.
 */
export function chatErrorView(message: string): ChatErrorView {
	const start = message.indexOf("{");
	if (start >= 0) {
		try {
			const body: unknown = JSON.parse(message.slice(start));
			if (body && typeof body === "object" && "message" in body && typeof body.message === "string") {
				const retryable = !("retryable" in body) || body.retryable !== false;
				const startNew = !retryable && /start a new chat/i.test(body.message);
				return { text: startNew ? UNCERTAIN_TEXT : body.message, retryable, startNew };
			}
		} catch {
			// Not a JSON body: fall through and show the text.
		}
	}
	return { text: message, retryable: true, startNew: false };
}

/**
 * ego 0.4.3 with no provider or model fails the first prompt with
 * `no model: pass --model provider/model, set EGO_MODEL, or put model in a config file`
 * (story 2595-4245 probe, 2026-11-06).
 */
export function isNoModelConfigured(message: string | null): boolean {
	return message?.includes("no model: pass --model") === true;
}
