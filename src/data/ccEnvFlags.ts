/**
 * Known Claude Code environment flags and parameters.
 *
 * Last updated: 2026-04-02
 * Sources: Official Anthropic docs, cc-changes.md, GitHub issues, community gists
 * See ideas/cc-env-flags.md for the full catalog (109 vars).
 */

export type EnvFlagType = "boolean" | "boolean_inverted" | "enum" | "number";

export interface EnvFlagDef {
  /** Environment variable name */
  key: string;
  /** Human-readable description */
  description: string;
  /** Value type */
  type: EnvFlagType;
  /** For enum types: allowed values */
  options?: string[];
  /** Default value (if known) */
  defaultValue?: string;
  /** Category for UI grouping */
  category: EnvFlagCategory;
}

export type EnvFlagCategory =
  | "privacy"
  | "rendering"
  | "model"
  | "memory"
  | "tasks"
  | "plugins"
  | "tools"
  | "network"
  | "provider"
  | "agents"
  | "sdk";

export const ENV_FLAG_CATEGORIES: Record<EnvFlagCategory, string> = {
  privacy: "Privacy & Telemetry",
  rendering: "Rendering & UI",
  model: "Model & Thinking",
  memory: "Memory & Context",
  tasks: "Tasks & Background",
  plugins: "Plugins",
  tools: "Tools & Bash",
  network: "Network & API",
  provider: "Provider Routing",
  agents: "Agent Teams",
  sdk: "SDK & Headless",
};

/** Display order for categories */
export const CATEGORY_ORDER: EnvFlagCategory[] = [
  "model",
  "memory",
  "tools",
  "rendering",
  "privacy",
  "tasks",
  "plugins",
  "network",
  "provider",
  "agents",
  "sdk",
];

export const CC_ENV_FLAGS: EnvFlagDef[] = [
  // ---------------------------------------------------------------------------
  // Privacy & Telemetry
  // ---------------------------------------------------------------------------
  {
    key: "CLAUDE_CODE_SIMPLE",
    description: "Strip skills/memory/agents/MCP for minimal mode",
    type: "boolean",
    category: "privacy",
  },
  {
    key: "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
    description: "Master switch: disables autoupdater, bug command, error reporting, and telemetry",
    type: "boolean",
    category: "privacy",
  },
  {
    key: "DISABLE_AUTOUPDATER",
    description: "Disable update check notifications",
    type: "boolean",
    category: "privacy",
  },
  {
    key: "DISABLE_BUG_COMMAND",
    description: "Disable the /bug command",
    type: "boolean",
    category: "privacy",
  },
  {
    key: "DISABLE_ERROR_REPORTING",
    description: "Opt out of Sentry error reporting",
    type: "boolean",
    category: "privacy",
  },
  {
    key: "DISABLE_TELEMETRY",
    description: "Opt out of Statsig telemetry",
    type: "boolean",
    category: "privacy",
  },
  {
    key: "DISABLE_COST_WARNINGS",
    description: "Suppress cost warning messages",
    type: "boolean",
    category: "privacy",
  },
  {
    key: "DISABLE_FEEDBACK_SURVEY",
    description: "Disable feedback survey prompts",
    type: "boolean",
    category: "privacy",
  },
  {
    key: "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB",
    description: "Scrub credentials from subprocesses",
    type: "boolean",
    category: "privacy",
  },
  {
    key: "CCR_ENABLE_BUNDLE",
    description: "Enable code bundle uploads for Claude Code Review",
    type: "boolean",
    category: "privacy",
  },

  // ---------------------------------------------------------------------------
  // Rendering & UI
  // ---------------------------------------------------------------------------
  {
    key: "CLAUDE_CODE_NO_FLICKER",
    description: "Flicker-free alt-screen rendering with virtualized scrollback (research preview)",
    type: "boolean",
    category: "rendering",
  },
  {
    key: "CLAUDE_CODE_DISABLE_MOUSE",
    description: "Disable mouse tracking in fullscreen rendering",
    type: "boolean",
    category: "rendering",
  },
  {
    key: "CLAUDE_CODE_DISABLE_TERMINAL_TITLE",
    description: "Disable terminal title updates",
    type: "boolean",
    category: "rendering",
  },
  {
    key: "CLAUDE_CODE_SYNTAX_HIGHLIGHT",
    description: "Enable/disable syntax highlighting in diffs",
    type: "boolean",
    category: "rendering",
  },

  // ---------------------------------------------------------------------------
  // Model & Thinking
  // ---------------------------------------------------------------------------
  {
    key: "CLAUDE_CODE_DISABLE_THINKING",
    description: "Force-disable extended thinking",
    type: "boolean",
    category: "model",
  },
  {
    key: "CLAUDE_CODE_DISABLE_ADAPTIVE_THINKING",
    description: "Disable adaptive reasoning for Opus/Sonnet 4.6",
    type: "boolean",
    category: "model",
  },
  {
    key: "CLAUDE_CODE_DISABLE_FAST_MODE",
    description: "Disable fast mode",
    type: "boolean",
    category: "model",
  },
  {
    key: "CLAUDE_CODE_ALWAYS_ENABLE_EFFORT",
    description: "Force effort support across all models",
    type: "boolean",
    category: "model",
  },
  {
    key: "CLAUDE_CODE_DISABLE_1M_CONTEXT",
    description: "Remove 1M context model variants from picker",
    type: "boolean",
    category: "model",
  },
  {
    key: "CLAUDE_CODE_EFFORT_LEVEL",
    description: "Persist effort level across sessions",
    type: "enum",
    options: ["low", "medium", "high"],
    defaultValue: "medium",
    category: "model",
  },
  {
    key: "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
    description: "Max output tokens per response",
    type: "number",
    defaultValue: "32000",
    category: "model",
  },
  {
    key: "MAX_THINKING_TOKENS",
    description: "Extended thinking token budget",
    type: "number",
    category: "model",
  },

  // ---------------------------------------------------------------------------
  // Memory & Context
  // ---------------------------------------------------------------------------
  {
    key: "CLAUDE_CODE_DISABLE_AUTO_MEMORY",
    description: "Disable automatic memory creation/loading",
    type: "boolean",
    category: "memory",
  },
  {
    key: "CLAUDE_CODE_DISABLE_CLAUDE_MDS",
    description: "Prevent loading CLAUDE.md memory files",
    type: "boolean",
    category: "memory",
  },
  {
    key: "CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD",
    description: "Load CLAUDE.md from --add-dir directories",
    type: "boolean",
    category: "memory",
  },
  {
    key: "DISABLE_AUTO_COMPACT",
    description: "Disable automatic compaction",
    type: "boolean",
    category: "memory",
  },
  {
    key: "DISABLE_COMPACT",
    description: "Disable all compaction (manual + auto)",
    type: "boolean",
    category: "memory",
  },
  {
    key: "CLAUDE_CODE_AUTOCOMPACT_PCT_OVERRIDE",
    description: "Context % at which auto-compaction triggers (recommended: 75)",
    type: "number",
    defaultValue: "80",
    category: "memory",
  },
  {
    key: "CLAUDE_CODE_DISABLE_GIT_INSTRUCTIONS",
    description: "Remove built-in git instructions from system prompt",
    type: "boolean",
    category: "memory",
  },

  // ---------------------------------------------------------------------------
  // Tasks & Background
  // ---------------------------------------------------------------------------
  {
    key: "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS",
    description: "Disable all background task functionality",
    type: "boolean",
    category: "tasks",
  },
  {
    key: "CLAUDE_CODE_ENABLE_TASKS",
    description: "Enable task tracking in non-interactive mode",
    type: "boolean",
    category: "tasks",
  },
  {
    key: "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS",
    description: "Enable Agent Teams (TeamCreate/TaskCreate/SendMessage)",
    type: "boolean",
    category: "tasks",
  },
  {
    key: "CLAUDE_CODE_DISABLE_CRON",
    description: "Disable cron jobs mid-session",
    type: "boolean",
    category: "tasks",
  },

  // ---------------------------------------------------------------------------
  // Plugins
  // ---------------------------------------------------------------------------
  {
    key: "CLAUDE_CODE_SYNC_PLUGIN_INSTALL",
    description: "Wait for plugin installation before first query",
    type: "boolean",
    category: "plugins",
  },
  {
    key: "FORCE_AUTOUPDATE_PLUGINS",
    description: "Force plugin auto-updates even if main updater disabled",
    type: "boolean",
    category: "plugins",
  },

  // ---------------------------------------------------------------------------
  // Tools & Bash
  // ---------------------------------------------------------------------------
  {
    key: "ENABLE_TOOL_SEARCH",
    description: "Control tool search behavior",
    type: "enum",
    options: ["auto", "auto:5", "auto:10", "true", "false"],
    defaultValue: "auto",
    category: "tools",
  },
  {
    key: "CLAUDE_CODE_GLOB_HIDDEN",
    description: "Include dotfiles in Glob results",
    type: "boolean",
    category: "tools",
  },
  {
    key: "CLAUDE_CODE_GLOB_NO_IGNORE",
    description: "Ignore .gitignore rules in Glob tool",
    type: "boolean",
    category: "tools",
  },
  {
    key: "CLAUDE_CODE_USE_POWERSHELL_TOOL",
    description: "Enable PowerShell tool on Windows (preview)",
    type: "boolean",
    category: "tools",
  },
  {
    key: "CLAUDE_BASH_MAINTAIN_PROJECT_WORKING_DIR",
    description: "Return to original working directory after each Bash command",
    type: "boolean",
    category: "tools",
  },
  {
    key: "MAX_MCP_OUTPUT_TOKENS",
    description: "Max tokens per MCP tool response",
    type: "number",
    defaultValue: "25000",
    category: "tools",
  },
  {
    key: "BASH_DEFAULT_TIMEOUT_MS",
    description: "Default Bash tool command timeout (ms)",
    type: "number",
    defaultValue: "120000",
    category: "tools",
  },
  {
    key: "BASH_MAX_TIMEOUT_MS",
    description: "Maximum allowed Bash timeout (ms)",
    type: "number",
    defaultValue: "600000",
    category: "tools",
  },

  // ---------------------------------------------------------------------------
  // Network & API
  // ---------------------------------------------------------------------------
  {
    key: "MCP_TIMEOUT",
    description: "Startup timeout for MCP servers (ms)",
    type: "number",
    category: "network",
  },
  {
    key: "MCP_CONNECTION_NONBLOCKING",
    description: "Make MCP server connections non-blocking",
    type: "boolean",
    category: "network",
  },
  {
    key: "ENABLE_PROMPT_CACHING_1H_BEDROCK",
    description: "Request 1-hour prompt cache TTL on Bedrock",
    type: "boolean",
    category: "network",
  },
  {
    key: "FALLBACK_FOR_ALL_PRIMARY_MODELS",
    description: "Trigger fallback for all primary models on overload",
    type: "boolean",
    category: "network",
  },
  {
    key: "ENABLE_CLAUDEAI_MCP_SERVERS",
    description: "Allow claude.ai MCP servers in CC",
    type: "boolean_inverted",
    defaultValue: "true",
    category: "network",
  },

  // ---------------------------------------------------------------------------
  // Provider Routing
  // ---------------------------------------------------------------------------
  {
    key: "CLAUDE_CODE_USE_BEDROCK",
    description: "Route API calls through AWS Bedrock",
    type: "boolean",
    category: "provider",
  },
  {
    key: "CLAUDE_CODE_USE_VERTEX",
    description: "Route API calls through Google Vertex AI",
    type: "boolean",
    category: "provider",
  },
  {
    key: "CLAUDE_CODE_USE_FOUNDRY",
    description: "Route API calls through Microsoft Foundry",
    type: "boolean",
    category: "provider",
  },
  {
    key: "CLAUDE_CODE_AUTO_CONNECT_IDE",
    description: "Auto-connect to IDE extensions",
    type: "boolean_inverted",
    defaultValue: "true",
    category: "provider",
  },

  // ---------------------------------------------------------------------------
  // Agent Teams
  // ---------------------------------------------------------------------------
  {
    key: "CLAUDE_CODE_TASK_LIST_ID",
    description: "Share a task list across sessions/team members via a common ID",
    type: "enum",
    options: [],  // free-form ID
    category: "agents",
  },
  {
    key: "CLAUDE_CODE_SUBAGENT_MODEL",
    description: "Override model for subagents/worker agents",
    type: "enum",
    options: [],  // free-form model ID
    category: "agents",
  },
  {
    key: "CLAUDE_CODE_TEAMMATE_COMMAND",
    description: "Override executable for spawning teammate instances",
    type: "enum",
    options: [],  // free-form path
    category: "agents",
  },
  {
    key: "CLAUDE_CODE_AGENT_COLOR",
    description: "Assign a color to a spawned teammate",
    type: "enum",
    options: [],  // free-form color
    category: "agents",
  },
  {
    key: "CLAUDE_CODE_TEAM_NAME",
    description: "Agent team name for this teammate",
    type: "enum",
    options: [],  // free-form string
    category: "agents",
  },

  // ---------------------------------------------------------------------------
  // SDK & Headless
  // ---------------------------------------------------------------------------
  {
    key: "CLAUDE_CODE_RESUME_INTERRUPTED_TURN",
    description: "Auto-resume if previous session ended mid-turn",
    type: "boolean",
    category: "sdk",
  },
  {
    key: "CLAUDE_CODE_EAGER_FLUSH",
    description: "Force eager transcript/storage flushing",
    type: "boolean",
    category: "sdk",
  },
  {
    key: "CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION",
    description: "Enable/disable prompt suggestions",
    type: "boolean",
    category: "sdk",
  },
  {
    key: "CLAUDE_CODE_NEW_INIT",
    description: "Opt into new interactive /init flow",
    type: "boolean",
    category: "sdk",
  },
];
