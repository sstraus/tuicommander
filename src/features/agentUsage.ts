import { createEffect, createRoot, untrack } from "solid-js";
import { appLogger } from "../stores/appLogger";
import { mdTabsStore } from "../stores/mdTabs";
import { providerQuotaStore, type QuotaView, type SubscriptionKey } from "../stores/providerQuota";
import { statusBarTicker, type TickerMessage } from "../stores/statusBarTicker";
import { terminalsStore } from "../stores/terminals";
import { HttpRpcError } from "../transport/http";
import { formatUsageTime } from "../utils/sessionUsageFormatting";
import { formatResetCompact } from "./claudeUsage";

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

/** Display label of the subscription provider in a registry key; the agent name when the provider is unknown. */
const PROVIDER_LABELS: Record<string, string> = { anthropic: "Claude", "openai-codex": "Codex", xai: "Grok" };
function usageLabel(provider: string, agent: UsageAgent): string {
	return PROVIDER_LABELS[provider] ?? (agent === "ego" ? "ego" : agent[0].toUpperCase() + agent.slice(1));
}

export type UsagePlacement = "badge" | "nowhere" | "ticker";

/**
 * The one rule for where a ticker message is shown. The usage message of the active agent
 * session belongs to that session's agent badge; any other usage message (the previous
 * session's result during a tab switch) is shown nowhere. Every other message stays in the
 * ticker. The ticker pool and the agent badge both read this rule, so they cannot disagree.
 */
export function usagePlacement(
	message: Pick<TickerMessage, "pluginId" | "sessionId">,
	active: { agentType?: string | null; sessionId?: string | null } | null | undefined,
): UsagePlacement {
	if (message.pluginId !== FEATURE_ID) return "ticker";
	return active?.agentType && active.sessionId && message.sessionId === active.sessionId ? "badge" : "nowhere";
}

export interface AgentUsageText {
	text: string;
	tooltip?: string;
	priority: number;
}

const priorityOf = (used: number) => (used >= 96 ? 96 : used >= 70 ? 50 : 10);
const resetSuffix = (resetsAt: string | null | undefined, now: number) => {
	const left = formatResetCompact(resetsAt ?? null, now);
	return left ? ` -${left}` : "";
};

/**
 * The one formatter for agent usage, for every provider and for the badge and ticker alike.
 * Fixed segment order: the weekly window percent (only when the snapshot has one), then what
 * is left before the monthly limit (a monthly window as percent left; a credit balance as an
 * amount). Claude extra usage is a pay-as-you-go overage budget, not a subscription limit, so it
 * never makes a segment. Each segment carries the time to its own reset (" -4d").
 * Falls back to an account-wide unclassified window without inferring its duration.
 * Returns null when the snapshot has no displayable quota data.
 */
export function formatAgentUsage(snapshot: QuotaView["snapshot"], now: number): AgentUsageText | null {
	if (!snapshot) return null;
	const windows = snapshot.windows ?? [];
	const pick = (kind: string) =>
		windows.find((window) => window.kind === kind && !window.model) ?? windows.find((window) => window.kind === kind);
	const segments: string[] = [];
	const used: number[] = [];
	const weekly = pick("weekly");
	if (weekly) {
		segments.push(`${weekly.label}: ${Math.round(weekly.used_percent)}%${resetSuffix(weekly.resets_at, now)}`);
		used.push(weekly.used_percent);
	}
	const monthly = pick("monthly");
	const credits = snapshot.credits === null ? Number.NaN : Number(snapshot.credits);
	if (monthly) {
		segments.push(
			`${monthly.label}: ${Math.max(0, 100 - Math.round(monthly.used_percent))}% left${resetSuffix(monthly.resets_at, now)}`,
		);
		used.push(monthly.used_percent);
	} else if (Number.isFinite(credits)) {
		segments.push(`credits: ${Math.floor(credits)} left`);
	}
	if (!segments.length) {
		const other = windows.find((window) => window.kind === "other" && !window.model);
		if (other) {
			segments.push(`${other.label}: ${Math.round(other.used_percent)}%${resetSuffix(other.resets_at, now)}`);
			used.push(other.used_percent);
		}
	}
	if (!segments.length) return null;
	const resets = windows
		.filter((window) => window.resets_at)
		.map((window) => {
			const name = `${window.model ? `${window.model} ` : ""}${window.label}: ${Math.round(window.used_percent)}%`;
			return `${name}, resets ${formatUsageTime(window.resets_at!) ?? window.resets_at}`;
		});
	return {
		text: segments.join(" · "),
		...(resets.length ? { tooltip: resets.join("\n") } : {}),
		priority: priorityOf(Math.max(0, ...used)),
	};
}

const keyId = (key: SubscriptionKey) => JSON.stringify([key.owner, key.provider, key.credential, key.profile ?? null]);

let disposeEffect: (() => void) | null = null;

/**
 * Follow the active session; quota updates come from the shared registry store. The ticker
 * store is written only when the shown content changes, so a republished snapshot with the
 * same values, or a push for another account, causes no write and no repaint.
 */
export function initAgentUsage(): void {
	if (disposeEffect) return;
	void providerQuotaStore.start().catch((error) => appLogger.warn("network", "Quota store unavailable", String(error)));
	let written: string | null = null;
	/** Last good text per subscription, kept through stale and backoff states without a snapshot. */
	const lastGood = new Map<string, AgentUsageText>();
	const retract = () => {
		if (written === null) return;
		written = null;
		statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID);
	};
	disposeEffect = createRoot((dispose) => {
		createEffect(() => {
			const id = terminalsStore.state.activeId;
			const terminal = id ? terminalsStore.state.terminals[id] : null;
			const agent = toUsageAgent(terminal?.agentType);
			const sessionId = terminal?.sessionId;
			const view = agent && sessionId ? providerQuotaStore.viewForAgent(sessionId, agent) : null;
			untrack(() => {
				if (!agent || !sessionId || !view) return retract();
				const account = keyId(view.key);
				const formatted = formatAgentUsage(view.snapshot, Date.now());
				if (formatted) lastGood.set(account, formatted);
				const keep = view.state === "stale" || view.state === "backoff" ? lastGood.get(account) : undefined;
				const shown =
					formatted ??
					keep ??
					(view.snapshot ? null : { text: view.state.replaceAll("_", " "), priority: priorityOf(0) });
				if (!shown) return retract();
				const label = usageLabel(view.key.provider, agent);
				const signature = JSON.stringify([sessionId, agent, label, shown.text, shown.tooltip, shown.priority]);
				if (signature === written) return;
				written = signature;
				statusBarTicker.addMessage({
					id: TICKER_ID,
					pluginId: FEATURE_ID,
					label,
					sessionId,
					text: shown.text,
					...(shown.tooltip ? { tooltip: shown.tooltip } : {}),
					icon: CHART_SVG,
					priority: shown.priority,
					ttlMs: 0,
					// ego reads its own `auth usage`; its badge never opens a dashboard that reads native CLI credentials.
					onClick:
						agent === "claude"
							? () => mdTabsStore.addClaudeUsage(sessionId)
							: agent === "codex"
								? () => mdTabsStore.addCodexUsage(sessionId)
								: agent === "grok"
									? () => mdTabsStore.addGrokUsage(sessionId)
									: undefined,
				});
			});
		});
		return () => {
			dispose();
			retract();
		};
	});
}

export function destroyAgentUsage(): void {
	disposeEffect?.();
	disposeEffect = null;
	untrack(() => statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID));
}
