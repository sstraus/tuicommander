/** Supported agent types */
export type AgentType = "claude" | "gemini" | "opencode" | "aider" | "codex" | "amp" | "jules" | "cursor" | "warp" | "ona" | "droid" | "git";

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
  amp: {
    type: "amp",
    name: "Amp",
    binary: "amp",
    description: "Sourcegraph's AI coding agent",
    resumeCommand: null,
    spawnArgs: (prompt) => {
      return [prompt];
    },
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [
        /rate.?limit/i,
        /429/,
        /too many requests/i,
        /overloaded/i,
      ],
      completion: [],
      error: [
        /error:/i,
        /failed:/i,
      ],
      prompt: [
        /Allow this command\?/i,
        /\[y\/n\/!\]/i,
        /\[y\/n\]/i,
      ],
    },
  },
  jules: {
    type: "jules",
    name: "Jules",
    binary: "jules",
    description: "Google's async cloud coding agent",
    resumeCommand: null,
    spawnArgs: (prompt) => {
      return [prompt];
    },
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [
        /RESOURCE_EXHAUSTED/i,
        /429/,
      ],
      completion: [],
      error: [
        /error:/i,
        /failed:/i,
      ],
      prompt: [
        /\/remote/,
        /\/new/,
      ],
    },
  },
  cursor: {
    type: "cursor",
    name: "Cursor Agent",
    binary: "cursor-agent",
    description: "Cursor's standalone coding agent CLI",
    resumeCommand: null,
    spawnArgs: (prompt) => {
      return [prompt];
    },
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [
        /User Provided API Key Rate Limit Exceeded/i,
        /RateLimitError/,
        /429/,
      ],
      completion: [],
      error: [
        /error:/i,
        /failed:/i,
      ],
      prompt: [
        /\(Y\)es\/\(N\)o/,
      ],
    },
  },
  warp: {
    type: "warp",
    name: "Warp Oz",
    binary: "oz",
    description: "Warp's AI agent (local + cloud)",
    resumeCommand: null,
    spawnArgs: (prompt) => {
      return ["agent", "run", prompt];
    },
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [
        /rate.?limit/i,
        /429/,
      ],
      completion: [],
      error: [
        /error:/i,
        /failed:/i,
      ],
      prompt: [],
    },
  },
  ona: {
    type: "ona",
    name: "ONA",
    binary: "gitpod",
    description: "ONA (formerly Gitpod) cloud environment agent",
    resumeCommand: null,
    spawnArgs: (prompt) => {
      return [prompt];
    },
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [
        /rate.?limit/i,
        /429/,
      ],
      completion: [],
      error: [
        /error:/i,
        /failed:/i,
      ],
      prompt: [],
    },
  },
  droid: {
    type: "droid",
    name: "Droid",
    binary: "droid",
    description: "Factory's agent-native software development CLI",
    resumeCommand: null,
    spawnArgs: (prompt) => {
      return [prompt];
    },
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [
        /rate.?limit/i,
        /429/,
        /too many requests/i,
      ],
      completion: [],
      error: [
        /error:/i,
        /failed:/i,
      ],
      prompt: [
        /\[y\/n\]/i,
      ],
    },
  },
  git: {
    type: "git",
    name: "Git",
    binary: "git",
    description: "Background git operations (pull, push, fetch, stash)",
    resumeCommand: null,
    spawnArgs: () => [],
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [],
      completion: [],
      error: [/error:/i, /failed:/i, /fatal:/i],
      prompt: [],
    },
  },
};

/** Run configuration for an agent (matches Rust AgentRunConfig) */
export interface AgentRunConfig {
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  is_default: boolean;
}

/** Per-agent settings (matches Rust AgentSettings) */
export interface AgentSettingsConfig {
  run_configs: AgentRunConfig[];
}

/** Full agents config (matches Rust AgentsConfig) */
export interface AgentsConfig {
  agents: Record<string, AgentSettingsConfig>;
}

/** Which agents support MCP configuration */
export const MCP_SUPPORT: Record<AgentType, boolean> = {
  claude: true,
  gemini: true,
  opencode: false,
  aider: false,
  codex: false,
  amp: true,
  jules: false,
  cursor: true,
  warp: false,
  ona: false,
  droid: false,
  git: false,
};

/** Agent display info for UI */
export const AGENT_DISPLAY: Record<AgentType, { icon: string; color: string }> = {
  claude: { icon: "C", color: "#d97706" },
  gemini: { icon: "G", color: "#4285f4" },
  opencode: { icon: "O", color: "#10a37f" },
  aider: { icon: "A", color: "#9333ea" },
  codex: { icon: "X", color: "#ef4444" },
  amp: { icon: "A", color: "#ff5543" },
  jules: { icon: "J", color: "#4285f4" },
  cursor: { icon: "C", color: "#000000" },
  warp: { icon: "W", color: "#01a4ff" },
  ona: { icon: "O", color: "#ffe400" },
  droid: { icon: "D", color: "#f97316" },
  git: { icon: "G", color: "#f05032" },
};

