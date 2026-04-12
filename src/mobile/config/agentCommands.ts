/** Per-agent quick commands and model lists for the CommandWidget. */

export interface AgentCommand {
  label: string;
  /** The text sent to the PTY (without trailing \r — added by the widget). */
  command: string;
  /** Optional category for grouping. */
  category?: "model" | "control" | "info";
}

export interface AgentCommandSet {
  commands: AgentCommand[];
  models?: string[];
  /** The slash-command prefix to switch models (e.g. "/model"). */
  modelCommand?: string;
  /** Escape sequence for permission toggle (e.g. Shift+Tab = "\x1b[Z"). */
  permissionToggleSeq?: string;
}

const CLAUDE_CODE: AgentCommandSet = {
  commands: [
    { label: "/compact", command: "/compact", category: "control" },
    { label: "/clear", command: "/clear", category: "control" },
    { label: "/cost", command: "/cost", category: "info" },
    { label: "/help", command: "/help", category: "info" },
  ],
  models: ["opus", "sonnet", "haiku"],
  modelCommand: "/model",
  permissionToggleSeq: "\x1b[Z", // Shift+Tab
};

const CODEX: AgentCommandSet = {
  commands: [
    { label: "/help", command: "/help", category: "info" },
  ],
};

const GEMINI: AgentCommandSet = {
  commands: [
    { label: "/help", command: "/help", category: "info" },
  ],
};

const AIDER: AgentCommandSet = {
  commands: [
    { label: "/clear", command: "/clear", category: "control" },
    { label: "/help", command: "/help", category: "info" },
    { label: "/tokens", command: "/tokens", category: "info" },
  ],
};

const FALLBACK: AgentCommandSet = {
  commands: [],
};

const AGENT_COMMANDS: Record<string, AgentCommandSet> = {
  claude: CLAUDE_CODE,
  codex: CODEX,
  gemini: GEMINI,
  aider: AIDER,
};

export function getAgentCommands(agentType?: string | null): AgentCommandSet {
  if (!agentType) return FALLBACK;
  return AGENT_COMMANDS[agentType] ?? FALLBACK;
}
