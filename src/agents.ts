// Agent definitions and management for TUI Commander
import { invoke } from "./invoke";

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

/** Detection result for an agent */
export interface AgentDetectionResult {
  type: AgentType;
  available: boolean;
  path: string | null;
  version: string | null;
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
    resumeCommand: null,
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
    resumeCommand: "aider",
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

/** Agent manager class */
export class AgentManager {
  private detectionCache: Map<AgentType, AgentDetectionResult> = new Map();
  private activeAgent: AgentType = "claude";
  private rateLimitedAgents: Set<AgentType> = new Set();

  /** Get all agent types */
  getAllTypes(): AgentType[] {
    return Object.keys(AGENTS) as AgentType[];
  }

  /** Get agent config by type */
  getConfig(type: AgentType): AgentConfig {
    return AGENTS[type];
  }

  /** Get current active agent */
  getActiveAgent(): AgentType {
    return this.activeAgent;
  }

  /** Set active agent */
  setActiveAgent(type: AgentType): void {
    this.activeAgent = type;
  }

  /** Check if an agent is rate-limited */
  isRateLimited(type: AgentType): boolean {
    return this.rateLimitedAgents.has(type);
  }

  /** Mark agent as rate-limited */
  markRateLimited(type: AgentType): void {
    this.rateLimitedAgents.add(type);
  }

  /** Clear rate limit status */
  clearRateLimit(type: AgentType): void {
    this.rateLimitedAgents.delete(type);
  }

  /** Detect if an agent is available */
  async detectAgent(type: AgentType): Promise<AgentDetectionResult> {
    // Check cache first
    const cached = this.detectionCache.get(type);
    if (cached) return cached;

    const config = AGENTS[type];
    let result: AgentDetectionResult;

    try {
      const detection = await invoke<{ path: string | null; version: string | null }>(
        "detect_agent_binary",
        { binary: config.binary }
      );

      result = {
        type,
        available: detection.path !== null,
        path: detection.path,
        version: detection.version,
      };
    } catch {
      result = {
        type,
        available: false,
        path: null,
        version: null,
      };
    }

    this.detectionCache.set(type, result);
    return result;
  }

  /** Detect all available agents */
  async detectAllAgents(): Promise<AgentDetectionResult[]> {
    const results = await Promise.all(
      this.getAllTypes().map((type) => this.detectAgent(type))
    );
    return results;
  }

  /** Get available agents only */
  async getAvailableAgents(): Promise<AgentDetectionResult[]> {
    const all = await this.detectAllAgents();
    return all.filter((r) => r.available);
  }

  /** Build spawn command for an agent */
  buildSpawnCommand(
    type: AgentType,
    prompt: string,
    options?: AgentSpawnOptions
  ): { binary: string; args: string[] } {
    const config = AGENTS[type];
    return {
      binary: config.binary,
      args: config.spawnArgs(prompt, options),
    };
  }

  /** Check output for rate limit patterns */
  checkRateLimit(type: AgentType, output: string): boolean {
    const config = AGENTS[type];
    return config.detectPatterns.rateLimit.some((pattern) => pattern.test(output));
  }

  /** Check output for completion patterns */
  checkCompletion(type: AgentType, output: string): boolean {
    const config = AGENTS[type];
    return config.detectPatterns.completion.some((pattern) => pattern.test(output));
  }

  /** Check output for error patterns */
  checkError(type: AgentType, output: string): boolean {
    const config = AGENTS[type];
    return config.detectPatterns.error.some((pattern) => pattern.test(output));
  }

  /** Check output for prompt patterns */
  checkPrompt(type: AgentType, output: string): boolean {
    const config = AGENTS[type];
    return config.detectPatterns.prompt.some((pattern) => pattern.test(output));
  }

  /** Find next available agent (for fallback) */
  async findNextAvailableAgent(exclude: AgentType[]): Promise<AgentType | null> {
    const available = await this.getAvailableAgents();
    const candidates = available.filter(
      (a) => !exclude.includes(a.type) && !this.isRateLimited(a.type)
    );
    return candidates.length > 0 ? candidates[0].type : null;
  }

  /** Clear detection cache */
  clearCache(): void {
    this.detectionCache.clear();
  }
}

/** Global agent manager instance */
export const agentManager = new AgentManager();
