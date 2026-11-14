import { createEffect, createRoot, untrack } from "solid-js";
import { appLogger } from "../stores/appLogger";
import { mdTabsStore } from "../stores/mdTabs";
import { providerQuotaStore } from "../stores/providerQuota";
import { statusBarTicker } from "../stores/statusBarTicker";
import { terminalsStore } from "../stores/terminals";
import { HttpRpcError } from "../transport/http";

const FEATURE_ID = "claude-usage";
const TICKER_ID = "claude-usage:rate";
const CHART_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M0 11.5a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 .5.5v4a.5.5 0 0 1-.5.5h-4a.5.5 0 0 1-.5-.5v-4zm6-4a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 .5.5v8a.5.5 0 0 1-.5.5h-4a.5.5 0 0 1-.5-.5v-8zm6-7a.5.5 0 0 1 .5-.5h3a.5.5 0 0 1 .5.5v15a.5.5 0 0 1-.5.5h-3a.5.5 0 0 1-.5-.5V.5z"/></svg>`;
export type UsageAgent = "claude" | "codex" | "grok" | "ego";
export function toUsageAgent(agentType: string | null | undefined): UsageAgent | null {
	return agentType === "claude" || agentType === "codex" || agentType === "grok" || agentType === "ego"
		? agentType
		: null;
}

export function describeUsageError(error: unknown, missingTokenHint: string): string {
	const errStr = String(error);
	const detail = error instanceof HttpRpcError ? error.detail : errStr;
	const retryMessage = /^(?:Error: )?(?:Claude|Codex) usage (rate-limited, retry \d{2}:\d{2})$/.exec(detail);
	if (retryMessage) return retryMessage[1];
	if (detail === "Claude API-key authentication" || detail === "Error: Claude API-key authentication") return "API key";
	if (errStr.toLowerCase().includes(missingTokenHint.toLowerCase()) || errStr.includes("No Codex credentials"))
		return "no token";
	if (errStr.includes("401") || errStr.includes("403")) return "token expired";
	if (errStr.includes("Failed to parse")) return "API changed";
	if (/method not found|unknown method|unsupported method/i.test(errStr)) return "API changed";
	if (errStr.includes("404")) return "API moved";
	return "offline";
}

let disposeEffect: (() => void) | null = null;

/** Follow the active session; quota updates come from the shared registry store. */
export function initAgentUsage(): void {
	if (disposeEffect) return;
	void providerQuotaStore.start().catch((error) => appLogger.warn("network", "Quota store unavailable", String(error)));
	disposeEffect = createRoot((dispose) => {
		createEffect(() => {
			const id = terminalsStore.state.activeId;
			const terminal = id ? terminalsStore.state.terminals[id] : null;
			const agent = toUsageAgent(terminal?.agentType);
			const sessionId = terminal?.sessionId;
			if (!agent || !sessionId) {
				untrack(() => statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID));
				return;
			}
			const view = providerQuotaStore.view(sessionId);
			if (!view) {
				untrack(() => statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID));
				return;
			}
			const windows = view.snapshot?.windows ?? [];
			const text = windows.length
				? windows
						.map(
							(window) =>
								`${window.model ? `${window.model} ` : ""}${window.label}: ${Math.round(window.used_percent)}%`,
						)
						.join(" · ")
				: view.state.replaceAll("_", " ");
			const priority = windows.some((window) => window.used_percent >= 96)
				? 96
				: windows.some((window) => window.used_percent >= 70)
					? 50
					: 10;
			const label = agent === "ego" ? "ego" : agent[0].toUpperCase() + agent.slice(1);
			untrack(() =>
				statusBarTicker.addMessage({
					id: TICKER_ID,
					pluginId: FEATURE_ID,
					label,
					sessionId,
					text,
					icon: CHART_SVG,
					priority,
					ttlMs: 0,
					onClick:
						agent === "claude"
							? () => mdTabsStore.addClaudeUsage(sessionId)
							: agent === "codex"
								? () => mdTabsStore.addCodexUsage()
								: agent === "grok"
									? () => mdTabsStore.addGrokUsage()
									: undefined,
				}),
			);
		});
		return dispose;
	});
}

export function destroyAgentUsage(): void {
	disposeEffect?.();
	disposeEffect = null;
	untrack(() => statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID));
}
