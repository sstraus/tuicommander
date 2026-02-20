/** Supported agent types */
export type AgentType = "claude" | "gemini" | "opencode" | "aider" | "codex";

/** Agent configuration */
export interface AgentConfig {
  type: AgentType;
  name: string;
  binary: string;
  description: string;
  resumeCommand: string | null;
  spawnArgs: (prompt: string, options?: AgentSpawnOptions) => string[];
  outputFormat: "text" | "jsonl" | "markdown";
  detectPatterns: {
    rateLimit: RegExp[];
    completion: RegExp[];
    error: RegExp[];
    prompt: RegExp[];
  };
}

/** Options for spawning an agent */
export interface AgentSpawnOptions {
  model?: string;
  printMode?: boolean;
  outputFormat?: string;
  cwd?: string;
}

/** Agent definitions */
export const AGENTS: Record<AgentType, AgentConfig> = {
  claude: {
    type: "claude",
    name: "Claude Code",
    binary: "claude",
    description: "Anthropic's Claude Code CLI",
    resumeCommand: "claude --continue",
    spawnArgs: (prompt, options = {}) => {
      const args: string[] = [];
      if (options.printMode) args.push("--print");
      if (options.outputFormat) args.push("--output-format", options.outputFormat);
      if (options.model) args.push("--model", options.model);
      args.push(prompt);
      return args;
    },
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [
        /rate.?limit/i,
        /429/,
        /too many requests/i,
        /overloaded/i,
      ],
      completion: [
        /Done \(\d+ tool uses?/i,
        /completed successfully/i,
      ],
      error: [
        /error:/i,
        /failed:/i,
        /exception:/i,
      ],
      prompt: [
        /^\s*\d+[.)]\s+.+$/gm,
        /\[y\/n\]/i,
        /select an option/i,
      ],
    },
  },
  gemini: {
    type: "gemini",
    name: "Gemini CLI",
    binary: "gemini",
    description: "Google's Gemini CLI",
    resumeCommand: "gemini --resume",
    spawnArgs: (prompt, options = {}) => {
      const args: string[] = [];
      if (options.model) args.push("--model", options.model);
      args.push(prompt);
      return args;
    },
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [
        /rate.?limit/i,
        /429/,
        /quota exceeded/i,
        /RESOURCE_EXHAUSTED/i,
      ],
      completion: [
        /task completed/i,
        /done/i,
      ],
      error: [
        /error:/i,
        /failed:/i,
      ],
      prompt: [
        /^\s*\d+[.)]\s+.+$/gm,
      ],
    },
  },
  opencode: {
    type: "opencode",
    name: "OpenCode",
    binary: "opencode",
    description: "OpenAI-based coding assistant",
    resumeCommand: "opencode -c",
    spawnArgs: (prompt, options = {}) => {
      const args: string[] = [];
      if (options.model) args.push("--model", options.model);
      args.push(prompt);
      return args;
    },
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [
        /rate.?limit/i,
        /429/,
        /too many requests/i,
      ],
      completion: [
        /completed/i,
      ],
      error: [
        /error:/i,
        /failed:/i,
      ],
      prompt: [
        /^\s*\d+[.)]\s+.+$/gm,
      ],
    },
  },
  aider: {
    type: "aider",
    name: "Aider",
    binary: "aider",
    description: "AI pair programming in terminal",
    resumeCommand: "aider --restore-chat-history",
    spawnArgs: (prompt, options = {}) => {
      const args: string[] = ["--yes-always"];
      if (options.model) args.push("--model", options.model);
      args.push("--message", prompt);
      return args;
    },
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [
        /rate.?limit/i,
        /429/,
        /too many requests/i,
      ],
      completion: [
        /Applied edit/i,
        /Committed/i,
      ],
      error: [
        /error:/i,
        /failed:/i,
      ],
      prompt: [
        /\[Y\/n\]/,
        /Enter.*:/i,
      ],
    },
  },
  codex: {
    type: "codex",
    name: "Codex CLI",
    binary: "codex",
    description: "OpenAI Codex CLI",
    resumeCommand: "codex resume --last",
    spawnArgs: (prompt, options = {}) => {
      const args: string[] = [];
      if (options.model) args.push("--model", options.model);
      args.push(prompt);
      return args;
    },
    outputFormat: "jsonl",
    detectPatterns: {
      rateLimit: [
        /rate.?limit/i,
        /429/,
        /too many requests/i,
      ],
      completion: [
        /completed/i,
      ],
      error: [
        /error:/i,
        /failed:/i,
      ],
      prompt: [
        /^\s*\d+[.)]\s+.+$/gm,
      ],
    },
  },
};

/** Agent display info for UI */
export const AGENT_DISPLAY: Record<AgentType, { icon: string; color: string }> = {
  claude: { icon: "C", color: "#d97706" },
  gemini: { icon: "G", color: "#4285f4" },
  opencode: { icon: "O", color: "#10a37f" },
  aider: { icon: "A", color: "#9333ea" },
  codex: { icon: "X", color: "#ef4444" },
};

