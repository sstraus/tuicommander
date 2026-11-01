/**
 * Agent usage ticker — one status bar slot, whichever agent is in the terminal.
 *
 * Claude and Codex both expose rate limits, and showing the wrong one is worse
 * than showing none: a "7d: 12%" that belongs to the other vendor reads as
 * headroom the user does not have. So the ticker follows the active terminal's
 * detected `agentType` and polls only that agent's endpoint.
 *
 * When the active tab is a shell, or an agent with no usage API, the last known
 * agent stays on screen rather than blanking — switching to a shell for one
 * command should not wipe the number you were watching.
 *
 * Until an agent with a usage API has been seen at least once, the ticker stays
 * empty. There is no default: a Codex-only install must never be told "Claude —
 * no token", which is the wrong-vendor number this feature exists to avoid.
 *
 * Called from plugins/index.ts when the `claude-usage` feature is toggled.
 */

import { createEffect, createRoot } from "solid-js";

import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { mdTabsStore } from "../stores/mdTabs";
import { statusBarTicker } from "../stores/statusBarTicker";
import { terminalsStore } from "../stores/terminals";
import { HttpRpcError } from "../transport/http";
import { buildTickerText, getTickerPriority, type UsageApiResponse } from "./claudeUsage";
import { buildCodexTickerText, type CodexUsageApiResponse, getCodexTickerPriority } from "./codexUsage";
import { buildGrokTickerText, type GrokUsageApiResponse, getGrokTickerPriority } from "./grokUsage";

const FEATURE_ID = "claude-usage";
const TICKER_ID = "claude-usage:rate";

/** Poll every 5 minutes — both Rust backends cache for 5 min. */
const API_POLL_MS = 5 * 60 * 1000;

/** Chart icon (inline SVG, monochrome) */
const CHART_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M0 11.5a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 .5.5v4a.5.5 0 0 1-.5.5h-4a.5.5 0 0 1-.5-.5v-4zm6-4a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 .5.5v8a.5.5 0 0 1-.5.5h-4a.5.5 0 0 1-.5-.5v-8zm6-7a.5.5 0 0 1 .5-.5h3a.5.5 0 0 1 .5.5v15a.5.5 0 0 1-.5.5h-3a.5.5 0 0 1-.5-.5V.5z"/></svg>`;

/** Agents that expose a usage API. Anything else leaves the ticker as it was. */
export type UsageAgent = "claude" | "codex" | "grok" | "ego";

interface AgentUsageSpec {
	/** Ticker label — the whole point of the feature: name the agent on screen. */
	label: string;
	command: string;
	buildText: (api: never) => string;
	tooltip?: (api: never) => string | undefined;
	priority: (api: never) => number;
	/** Substring of the backend's "no credentials" error, for a clearer ticker. */
	missingTokenHint: string;
	/** Opens the agent's usage dashboard tab when the ticker is clicked. */
	openDashboard?: (sessionId?: string) => void;
}

const SPECS: Record<UsageAgent, AgentUsageSpec> = {
	ego: {
		label: "ego",
		command: "get_ego_usage",
		buildText: (api: { text: string | null }) => api.text ?? "",
		tooltip: (api: { tooltip?: string }) => api.tooltip,
		priority: () => 0,
		missingTokenHint: "not configured",
	} as AgentUsageSpec,
	claude: {
		label: "Claude",
		command: "get_claude_usage_api",
		buildText: (api: UsageApiResponse) => buildTickerText(api),
		priority: (api: UsageApiResponse) => getTickerPriority(api),
		missingTokenHint: "No Claude OAuth token",
		openDashboard: (sessionId) => mdTabsStore.addClaudeUsage(sessionId),
	} as AgentUsageSpec,
	codex: {
		label: "Codex",
		command: "get_codex_usage_api",
		buildText: (api: CodexUsageApiResponse) => buildCodexTickerText(api),
		priority: (api: CodexUsageApiResponse) => getCodexTickerPriority(api),
		missingTokenHint: "not logged in",
		openDashboard: () => mdTabsStore.addCodexUsage(),
	} as AgentUsageSpec,
	grok: {
		label: "Grok",
		command: "get_grok_usage_api",
		buildText: (api: GrokUsageApiResponse) => buildGrokTickerText(api),
		priority: (api: GrokUsageApiResponse) => getGrokTickerPriority(api),
		missingTokenHint: "not logged in",
		openDashboard: () => mdTabsStore.addGrokUsage(),
	} as AgentUsageSpec,
};

/** Map a detected agentType onto an agent with a usage API, or null. */
export function toUsageAgent(agentType: string | null | undefined): UsageAgent | null {
	return agentType === "claude" || agentType === "codex" || agentType === "grok" || agentType === "ego"
		? agentType
		: null;
}

/** Classify a poll failure into the short text the ticker shows. */
export function describeUsageError(error: unknown, missingTokenHint: string): string {
	const errStr = String(error);
	const detail = error instanceof HttpRpcError ? error.detail : errStr;
	if (detail === "Claude API-key authentication" || detail === "Error: Claude API-key authentication") return "API key";
	if (errStr.toLowerCase().includes(missingTokenHint.toLowerCase()) || errStr.includes("No Codex credentials"))
		return "no token";
	if (errStr.includes("401") || errStr.includes("403")) return "token expired";
	if (errStr.includes("Failed to parse")) return "API changed";
	if (/method not found|unknown method|unsupported method/i.test(errStr)) return "API changed";
	if (errStr.includes("404")) return "API moved";
	return "offline";
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

let pollTimer: ReturnType<typeof setInterval> | null = null;
let disposeEffect: (() => void) | null = null;
let initialized = false;

/**
 * The agent currently on the ticker, or null until one is detected.
 * Sticky once set: a shell tab does not clear it.
 */
let shownAgent: UsageAgent | null = null;
let shownSessionId: string | null = null;

/** Guards against a slow poll for the previous agent overwriting the new one. */
let pollSeq = 0;

function activeUsageSession(): { agent: UsageAgent; sessionId: string | null } | null {
	const id = terminalsStore.state.activeId;
	if (!id) return null;
	const terminal = terminalsStore.state.terminals[id];
	const agent = toUsageAgent(terminal?.agentType);
	return agent ? { agent, sessionId: terminal?.sessionId ?? null } : null;
}

async function poll(agent: UsageAgent, sessionId: string | null): Promise<void> {
	const spec = SPECS[agent];
	const seq = ++pollSeq;

	const write = (text: string, priority: number, tooltip?: string) => {
		// A late response from the agent we just switched away from must not win.
		if (seq !== pollSeq) return;
		if (agent === "ego" && !text) {
			statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID);
			return;
		}
		statusBarTicker.addMessage({
			id: TICKER_ID,
			pluginId: FEATURE_ID,
			label: spec.label,
			...(agent === "ego" && sessionId ? { sessionId } : {}),
			text,
			...(tooltip ? { tooltip } : {}),
			icon: CHART_SVG,
			priority,
			ttlMs: API_POLL_MS + 30_000,
			onClick:
				agent === "ego" || (agent === "claude" && !sessionId)
					? undefined
					: () => spec.openDashboard?.(sessionId ?? undefined),
		});
	};

	try {
		if ((agent === "claude" || agent === "ego") && !sessionId) {
			write(agent === "ego" ? "" : "unknown", 5);
			return;
		}
		const api =
			agent === "claude" || agent === "ego"
				? await invoke<never>(spec.command, { sessionId })
				: await invoke<never>(spec.command);
		write(spec.buildText(api), spec.priority(api), spec.tooltip?.(api));
	} catch (err) {
		const errStr = String(err);
		const text =
			agent === "claude" &&
			/No Claude OAuth token|Cannot (?:resolve|read) Claude session profile|Failed to parse credentials/.test(errStr)
				? "unknown"
				: describeUsageError(err, spec.missingTokenHint);
		if (text !== "no token" && text !== "API key") {
			appLogger.warn("network", `${spec.label} usage poll: ${text}`, errStr);
		}
		write(agent === "ego" ? "" : text, text === "API key" ? 0 : 5);
	}
}

/** Initialize the agent usage ticker (poll + follow the active terminal). */
export function initAgentUsage(): void {
	if (initialized) return;
	initialized = true;

	const active = activeUsageSession();
	shownAgent = active?.agent ?? null;
	shownSessionId = active?.sessionId ?? null;
	if (shownAgent) poll(shownAgent, shownSessionId);
	pollTimer = setInterval(() => {
		if (shownAgent) poll(shownAgent, shownSessionId);
	}, API_POLL_MS);

	// Repoll immediately when the active tab moves to a different usage agent.
	// Tracked outside a component, so it needs its own reactive root.
	disposeEffect = createRoot((dispose) => {
		createEffect(() => {
			const next = activeUsageSession();
			if (!next || (next.agent === shownAgent && next.sessionId === shownSessionId)) return;
			statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID);
			shownAgent = next.agent;
			shownSessionId = next.sessionId;
			poll(next.agent, next.sessionId);
		});
		return dispose;
	});
}

/** Tear down the agent usage ticker. */
export function destroyAgentUsage(): void {
	if (!initialized) return;
	initialized = false;
	++pollSeq;
	shownAgent = null;
	shownSessionId = null;

	if (pollTimer) {
		clearInterval(pollTimer);
		pollTimer = null;
	}
	disposeEffect?.();
	disposeEffect = null;

	statusBarTicker.removeMessage(TICKER_ID, FEATURE_ID);
}
