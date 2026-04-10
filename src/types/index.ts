// Shared types for TUICommander

import type { Terminal } from "@xterm/xterm";
import type { FitAddon } from "@xterm/addon-fit";

/** Terminal pane instance */
export interface TerminalPane {
  id: string;
  sessionId: string | null;
  terminal: Terminal;
  fitAddon: FitAddon;
  element: HTMLElement;
  fontSize: number;
  name: string;
}

/** PTY output event from Tauri */
export interface PtyOutput {
  session_id: string;
  data: string;
}

/** Repository info from git */
export interface RepoInfo {
  path: string;
  name: string;
  initials: string;
  branch: string;
  status: "clean" | "dirty" | "conflict" | "merge" | "not-git" | "unknown";
  is_git_repo: boolean;
}

/** Saved repository reference */
export interface Repository {
  path: string;
  displayName: string;
}

/** PTY configuration for creating sessions */
export interface PtyConfig {
  rows: number;
  cols: number;
  shell: string | null;
  cwd: string | null;
  /** Pre-generated stable session UUID — injected as `TUIC_SESSION` env var in the PTY. */
  tuic_session?: string | null;
  /** Extra environment variables injected into the PTY process (e.g. agent feature flags). */
  env?: Record<string, string>;
}

/** PTY exit event data */
export interface PtyExit {
  session_id: string;
  code: number | null;
}

/**
 * IPty interface matching tauri-plugin-pty style API.
 * Implemented via usePty hook and Tauri event listeners.
 */
export interface IPty {
  /** Session identifier */
  readonly sessionId: string;
  /** Write data to the PTY */
  write(data: string): Promise<void>;
  /** Resize the PTY terminal */
  resize(rows: number, cols: number): Promise<void>;
  /** Kill/close the PTY session */
  kill(cleanupWorktree?: boolean): Promise<void>;
}

/**
 * PTY event handler types for Tauri event listeners.
 * Usage: listen<PtyOutput>(`pty-output-${sessionId}`, handler)
 * Usage: listen<PtyExit>(`pty-exit-${sessionId}`, handler)
 */
export type PtyDataHandler = (data: PtyOutput) => void;
export type PtyExitHandler = (data: PtyExit) => void;

/** Git remote + branch status (PR/CI data comes from githubStore via batch query) */
export interface GitHubStatus {
  has_remote: boolean;
  current_branch: string;
  ahead: number;
  behind: number;
}

/** CI check summary counts */
export interface CheckSummary {
  passed: number;
  failed: number;
  pending: number;
  total: number;
}

/** Individual CI check detail */
export interface CheckDetail {
  context: string;
  state: string;
}

/** Merge state: MERGEABLE, CONFLICTING, UNKNOWN */
export type MergeableState = "MERGEABLE" | "CONFLICTING" | "UNKNOWN";

/** Merge state status: BEHIND, BLOCKED, CLEAN, DIRTY, DRAFT, HAS_HOOKS, UNKNOWN, UNSTABLE */
export type MergeStateStatus = "BEHIND" | "BLOCKED" | "CLEAN" | "DIRTY" | "DRAFT" | "HAS_HOOKS" | "UNKNOWN" | "UNSTABLE";

/** PR status for a branch from batch endpoint */
export type ReviewDecision = "APPROVED" | "CHANGES_REQUESTED" | "REVIEW_REQUIRED" | "";

/** PR label with name, color, and pre-computed display colors from Rust */
export interface PrLabel {
  name: string;
  color: string;
  text_color: string;
  background_color: string;
}

export interface BranchPrStatus {
  branch: string;
  number: number;
  title: string;
  state: string;
  url: string;
  additions: number;
  deletions: number;
  checks: CheckSummary;
  check_details: CheckDetail[];
  author: string;
  commits: number;
  mergeable: MergeableState;
  merge_state_status: MergeStateStatus;
  review_decision: ReviewDecision;
  labels: PrLabel[];
  is_draft: boolean;
  base_ref_name: string;
  head_ref_oid: string;
  created_at: string;
  updated_at: string;
  merge_state_label: { label: string; css_class: string } | null;
  review_state_label: { label: string; css_class: string } | null;
  merge_commit_allowed: boolean;
  squash_merge_allowed: boolean;
  rebase_merge_allowed: boolean;
}

/** A single match from cross-terminal buffer search */
export interface TerminalMatch {
  terminalId: string;
  terminalName: string;
  lineIndex: number;       // absolute buffer line index (for scrollToLine)
  lineText: string;        // full line text (translateToString)
  matchStart: number;      // column offset of match start
  matchEnd: number;        // column offset of match end
}

/** Agent statistics */
export interface AgentStats {
  toolUses: number;
  tokens: number;
  duration: number;
}

/** Detected agent prompt */
export interface DetectedPrompt {
  question: string;
  options: string[];
  sessionId: string;
}

/** Session state for persistence */
export interface SessionState {
  terminals: SavedTerminal[];
  savedAt: string;
}

/** Saved terminal for session restore */
export interface SavedTerminal {
  name: string;
  cwd: string | null;
  fontSize: number;
  agentType: import("../agents").AgentType | null;
  /** Agent session ID — enables session-specific resume (e.g. claude --resume <uuid>) */
  agentSessionId?: string | null;
  /** Stable tab UUID — injected as TUIC_SESSION env var, persists across restarts */
  tuicSession?: string | null;
}

/** GitHub Issue from GraphQL API */
export interface GitHubIssue {
  number: number;
  title: string;
  state: string;        // OPEN, CLOSED
  url: string;
  author: string;
  labels: PrLabel[];    // Reuses PrLabel — same GitHub schema
  assignees: string[];
  milestone: string | null;
  comments_count: number;
  created_at: string;
  updated_at: string;
}

/** Issue filter mode for the GitHub panel */
export type IssueFilterMode = "assigned" | "created" | "mentioned" | "all";

/** Orchestrator stats from backend */
export interface OrchestratorStats {
  active_sessions: number;
  max_sessions: number;
  available_slots: number;
}

