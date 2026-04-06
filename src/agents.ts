/** Supported agent types */
export type AgentType = "claude" | "gemini" | "opencode" | "aider" | "codex" | "amp" | "cursor" | "warp" | "droid" | "git" | "api";

/** Runtime array for validating backend strings against AgentType */
export const AGENT_TYPES: readonly AgentType[] = ["claude", "gemini", "opencode", "aider", "codex", "amp", "cursor", "warp", "droid", "git", "api"] as const;

/** Session discovery config for agents that persist sessions as local files */
export interface SessionDiscoveryConfig {
  /** Build a resume command for a discovered session ID */
  resumeWithId: (id: string) => string;
}

/** Agent configuration */
export interface AgentConfig {
  type: AgentType;
  name: string;
  binary: string;
  description: string;
  resumeCommand: string | null;
  /**
   * Session file discovery config. When set, TUICommander will scan the agent's
   * session storage directory to find the session ID for manually-launched agents.
   * Null for agents with no local session storage (cloud-only, single-file, SQLite, etc.).
   */
  sessionDiscovery: SessionDiscoveryConfig | null;
  spawnArgs: (prompt: string, options?: AgentSpawnOptions) => string[];
  outputFormat: "text" | "jsonl" | "markdown";
  /** Default headless command template. Used when user hasn't configured a custom one. */
  defaultHeadlessTemplate?: string;
  /** Agent manages its own terminal tab title — intent_tab_title defaults to off */
  managesOwnTabTitle?: boolean;
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
    defaultHeadlessTemplate: "claude --bare --print --output-format text --no-session-persistence --system-prompt \"Output only the raw requested text. No explanations, no markdown fences, no commentary.\" -p \"{prompt}\"",
    managesOwnTabTitle: true,
    resumeCommand: "claude --continue",
    sessionDiscovery: { resumeWithId: (id) => `claude --resume ${id}` },
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
    defaultHeadlessTemplate: "gemini \"{prompt}\"",
    resumeCommand: "gemini --resume",
    sessionDiscovery: { resumeWithId: (id) => `gemini --resume ${id}` },
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
    defaultHeadlessTemplate: "opencode \"{prompt}\"",
    resumeCommand: "opencode -c",
    sessionDiscovery: null, // sessions stored in SQLite DB — not yet supported
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
    defaultHeadlessTemplate: "aider --yes-always --message \"{prompt}\"",
    resumeCommand: "aider --restore-chat-history",
    sessionDiscovery: null, // single .aider.chat.history.md per project, no session IDs
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
    defaultHeadlessTemplate: "codex \"{prompt}\"",
    managesOwnTabTitle: true,
    resumeCommand: "codex resume --last",
    sessionDiscovery: { resumeWithId: (id) => `codex resume ${id}` },
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
    defaultHeadlessTemplate: "amp \"{prompt}\"",
    resumeCommand: "amp threads continue",
    sessionDiscovery: null, // cloud-only, no local session files
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
  cursor: {
    type: "cursor",
    name: "Cursor Agent",
    binary: "cursor-agent",
    description: "Cursor's standalone coding agent CLI",
    defaultHeadlessTemplate: "cursor-agent \"{prompt}\"",
    managesOwnTabTitle: true,
    resumeCommand: "cursor-agent resume",
    sessionDiscovery: null, // closed-source, storage path undocumented
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
    defaultHeadlessTemplate: "oz agent run \"{prompt}\"",
    resumeCommand: null,
    sessionDiscovery: null,
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
  droid: {
    type: "droid",
    name: "Droid",
    binary: "droid",
    description: "Factory's agent-native software development CLI",
    defaultHeadlessTemplate: "droid \"{prompt}\"",
    resumeCommand: null,
    sessionDiscovery: null,
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
    sessionDiscovery: null,
    spawnArgs: () => [],
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [],
      completion: [],
      error: [/error:/i, /failed:/i, /fatal:/i],
      prompt: [],
    },
  },
  api: {
    type: "api",
    name: "External API",
    binary: "",
    description: "Direct LLM API calls (no agent CLI needed)",
    defaultHeadlessTemplate: "",
    resumeCommand: null,
    sessionDiscovery: null,
    spawnArgs: () => [],
    outputFormat: "text",
    detectPatterns: {
      rateLimit: [],
      completion: [],
      error: [],
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
  auto_retry_on_error?: boolean;
  headless_template?: string;
  /** Environment feature flags — key→value pairs injected into every spawn of this agent */
  env_flags?: Record<string, string>;
  /** Per-agent override: show intent as tab title. Undefined = use agent-aware default. */
  intent_tab_title?: boolean;
  /** Per-agent override: show suggested follow-ups. Undefined = use global default. */
  suggest_followups?: boolean;
}

/** Full agents config (matches Rust AgentsConfig) */
export interface AgentsConfig {
  agents: Record<string, AgentSettingsConfig>;
  /** Which agent CLI to use for headless prompt execution when no agent is in the active terminal */
  headless_agent?: AgentType;
}

/** Which agents support MCP configuration */
export const MCP_SUPPORT: Record<AgentType, boolean> = {
  claude: true,
  gemini: true,
  opencode: false,
  aider: false,
  codex: false,
  amp: true,
  cursor: true,
  warp: false,
  droid: false,
  git: false,
  api: false,
};

/** Agent display info for UI */
export const AGENT_DISPLAY: Record<AgentType, { icon: string; color: string }> = {
  claude: { icon: "C", color: "#d97706" },
  gemini: { icon: "G", color: "#4285f4" },
  opencode: { icon: "O", color: "#10a37f" },
  aider: { icon: "A", color: "#9333ea" },
  codex: { icon: "X", color: "#ef4444" },
  amp: { icon: "A", color: "#ff5543" },
  cursor: { icon: "C", color: "#000000" },
  warp: { icon: "W", color: "#01a4ff" },
  droid: { icon: "D", color: "#f97316" },
  git: { icon: "G", color: "#f05032" },
  api: { icon: "⚡", color: "#06b6d4" },
};

