export function formatRelativeTime(elapsedMs: number): string {
	if (elapsedMs < 5000) return "just now";
	if (elapsedMs < 60000) return `${Math.floor(elapsedMs / 1000)}s`;
	if (elapsedMs < 3600000) return `${Math.floor(elapsedMs / 60000)}m`;
	if (elapsedMs < 86400000) return `${Math.floor(elapsedMs / 3600000)}h`;
	return `${Math.floor(elapsedMs / 86400000)}d`;
}
