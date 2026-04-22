import { AGENTS, type AgentType } from "../agents";
import { rpc } from "../transport";

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
export function buildAgentLaunchCommand(command: string, agentSessionId?: string | null, agentType?: AgentType | null): string {
  if (!agentSessionId) return command;

  const parts = command.split(" ");
  const binary = parts[0];
  const binaryName = binary.split("/").pop() ?? "";

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
export function buildResumeCommand(agentType: AgentType, agentSessionId?: string | null): string | null {
  if (agentSessionId) {
    const disc = AGENTS[agentType].sessionDiscovery;
    if (disc) return disc.resumeWithId(agentSessionId);
  }
  return AGENTS[agentType].resumeCommand;
}

/**
 * Verify a TUIC_SESSION UUID against the agent's local session storage, then
 * build the appropriate resume command.
 *
 * Priority: tuicSession (verified on disk) > agentSessionId > static resumeCommand.
 * Falls back gracefully when verify_agent_session is unavailable (browser mode).
 */
export async function verifyAndBuildResumeCommand(
  agentType: AgentType,
  cwd: string | null,
  tuicSession?: string | null,
  agentSessionId?: string | null,
): Promise<string | null> {
  // Try tuicSession first — it's the stable tab UUID injected as env var
  if (tuicSession && cwd && AGENTS[agentType].sessionDiscovery) {
    try {
      const exists = await rpc<boolean>("verify_agent_session", {
        agentType,
        sessionId: tuicSession,
        cwd,
      });
      if (exists) {
        return AGENTS[agentType].sessionDiscovery!.resumeWithId(tuicSession);
      }
    } catch {
      // verify_agent_session unavailable (browser mode) — fall through
    }
  }

  // Fall back to discovered agentSessionId or static resumeCommand
  return buildResumeCommand(agentType, agentSessionId);
}
