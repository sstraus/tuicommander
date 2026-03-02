import { AGENTS, type AgentType } from "../agents";

/**
 * Build the launch command for an agent, injecting --session-id when applicable.
 *
 * Only Claude Code supports --session-id. For other agents the command is returned unchanged.
 * The command string may include a binary path and extra args (e.g. "claude --model opus").
 */
export function buildAgentLaunchCommand(command: string, claudeSessionId?: string | null): string {
  if (!claudeSessionId) return command;

  // Only inject for claude — check if the binary name (last segment of path) starts with "claude"
  const parts = command.split(" ");
  const binary = parts[0];
  const binaryName = binary.split("/").pop() ?? "";
  if (!binaryName.startsWith("claude")) return command;

  // Insert --session-id right after the binary
  const rest = parts.slice(1);
  return [binary, "--session-id", claudeSessionId, ...rest].join(" ");
}

/**
 * Build the resume command for restoring an agent session.
 *
 * For Claude Code with a persisted session UUID, returns "claude --resume <uuid>".
 * For all other cases, falls back to the static resumeCommand from AGENTS config.
 */
export function buildResumeCommand(agentType: AgentType, claudeSessionId?: string | null): string | null {
  if (agentType === "claude" && claudeSessionId) {
    return `claude --resume ${claudeSessionId}`;
  }
  return AGENTS[agentType].resumeCommand;
}
