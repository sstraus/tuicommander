/** Render IPC rejection objects without losing their diagnostic code or message. */
export function promptError(error: unknown): string {
	if (error instanceof Error) return String(error);
	if (error && typeof error === "object" && "message" in error && typeof error.message === "string") {
		return "code" in error && typeof error.code === "string" && error.code
			? `${error.code}: ${error.message}`
			: error.message;
	}
	return String(error);
}
