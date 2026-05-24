import { AGENTS, type AgentType } from "../agents";
import { agentConfigsStore } from "../stores/agentConfigs";
import { rpc } from "../transport";
import { pathBasename } from "./pathUtils";

/**
 * Apply the agent's default run config to a resume command.
 *
 * The AGENTS table hardcodes the binary (e.g. "claude --resume <id>"), but the
 * user may have configured a custom command/args in their run config (e.g. a
 * "c2" wrapper with "--model claude-opus-4-6"). This helper strips the original
 * binary, swaps in the run config's command, and appends the run config's args
 * AFTER the resume flag so model/profile settings apply to the resumed session.
 *
 * Example: "claude --resume abc" + run config (c2, --model opus) →
 *          "c2 --resume abc --model opus"
 *
 * Returns the original command unchanged when there's no run config (keeps
 * fallback behaviour; used by tests that don't set up the store).
 */
/**
 * Apply a run config to a resume command, preferring the original launch command
 * over the current default. This ensures that e.g. resuming a session started
 * with `c` (claude with custom flags/config-dir) doesn't switch to `c2`.
 */
function applyDefaultRunConfig(agentType: AgentType, command: string, launchCommand?: string | null): string {
	const launchParts = launchCommand?.split(" ");
	const runConfig = launchParts ? null : agentConfigsStore.getDefaultConfig(agentType);
	if (!launchParts && !runConfig) return command;

	const parts = command.split(" ");
	const resumeFlags = parts.slice(1); // drop the hardcoded binary

	if (launchParts) {
		// Use the original launch binary + its args, with resume flags in between
		const [launchBinary, ...launchArgs] = launchParts;
		return [launchBinary, ...resumeFlags, ...launchArgs].join(" ");
	}
	// eslint-disable-next-line @typescript-eslint/no-non-null-assertion
	return [runConfig!.command, ...resumeFlags, ...runConfig!.args].join(" ");
}

/**
 * Build the launch command for an agent, injecting --session-id when applicable.
 *
 * Only Claude Code supports --session-id. For other agents the command is returned unchanged.
 * The command string may include a binary path and extra args (e.g. "claude --model opus").
 *
 * When `agentType` is provided, it takes precedence over the binary-name heuristic.
 * This is important for custom commands (aliases, wrappers) like "C2" that don't
 * contain "claude" in the name but still need --session-id injection.
 */
export function buildAgentLaunchCommand(
	command: string,
	agentSessionId?: string | null,
	agentType?: AgentType | null,
): string {
	if (!agentSessionId) return command;

	const parts = command.split(" ");
	const binary = parts[0];
	const binaryName = pathBasename(binary) ?? "";

	const isClaude = agentType === "claude" || binaryName.startsWith("claude");
	if (!isClaude) return command;

	// Insert --session-id right after the binary
	const rest = parts.slice(1);
	return [binary, "--session-id", agentSessionId, ...rest].join(" ");
}

/**
 * Build the resume command for restoring an agent session.
 *
 * For Claude Code with a persisted session UUID, returns "claude --resume <uuid>".
 * For all other cases, falls back to the static resumeCommand from AGENTS config.
 */
export function buildResumeCommand(
	agentType: AgentType,
	agentSessionId?: string | null,
	launchCommand?: string | null,
): string | null {
	let base: string | null = null;
	if (agentSessionId) {
		const disc = AGENTS[agentType].sessionDiscovery;
		if (disc) base = disc.resumeWithId(agentSessionId);
	}
	if (base === null) base = AGENTS[agentType].resumeCommand;
	if (base === null) return null;
	return applyDefaultRunConfig(agentType, base, launchCommand);
}

/**
 * Verify a session UUID against the agent's local session storage, then
 * build the appropriate resume command.
 *
 * For Claude: uses agentSessionId (discovered from disk during the session).
 * For other agents: tries tuicSession first, then agentSessionId.
 * Falls back gracefully when verify_agent_session is unavailable (browser mode).
 */
export async function verifyAndBuildResumeCommand(
	agentType: AgentType,
	cwd: string | null,
	tuicSession?: string | null,
	agentSessionId?: string | null,
	launchCommand?: string | null,
): Promise<string | null> {
	const disc = AGENTS[agentType].sessionDiscovery;

	// For Claude, agentSessionId is the source of truth (discovered from newest session file).
	// For other agents, tuicSession is preferred (injected at launch via shell wrapper).
	const sessionId = agentType === "claude" ? agentSessionId : (tuicSession ?? agentSessionId);

	if (sessionId && cwd && disc) {
		try {
			// At restore time the agent process has exited, so agentPid is null.
			// The backend falls back to default paths (or run-config env if provided).
			const exists = await rpc<boolean>("verify_agent_session", {
				agentType,
				sessionId,
				cwd,
				agentPid: null,
				envOverrides: {},
			});
			if (exists) {
				const cmd = disc.resumeWithId(sessionId);
				return applyDefaultRunConfig(agentType, cmd, launchCommand);
			}
			return null;
		} catch {
			// verify_agent_session unavailable (browser mode) — fall through
		}
	}

	// No verified session — fall back to static resumeCommand
	return buildResumeCommand(agentType, agentSessionId, launchCommand);
}
