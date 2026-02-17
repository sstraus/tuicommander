import type { BranchPrStatus } from "../types";
import type { RateLimitInfo } from "../rate-limit";
import type { AgentType } from "../agents";

/** Partial PR status for preset overrides */
export type PrOverride = Partial<BranchPrStatus>;

/** Preset scenario definition */
export interface Preset {
  description: string;
  pr?: PrOverride;
  rateLimit?: RateLimitInfo[];
  agent?: { active: AgentType };
}

/** Default PR values shared by presets */
const basePr: BranchPrStatus = {
  branch: "",
  number: 42,
  title: "Simulated PR",
  state: "OPEN",
  url: "https://github.com/sim/repo/pull/42",
  additions: 120,
  deletions: 45,
  checks: { passed: 5, failed: 0, pending: 0, total: 5 },
  check_details: [
    { context: "ci/build", state: "SUCCESS" },
    { context: "ci/test", state: "SUCCESS" },
    { context: "ci/lint", state: "SUCCESS" },
    { context: "ci/typecheck", state: "SUCCESS" },
    { context: "ci/deploy-preview", state: "SUCCESS" },
  ],
  author: "sim-user",
  commits: 3,
  mergeable: "MERGEABLE",
  merge_state_status: "CLEAN",
  review_decision: "APPROVED",
  labels: [],
  is_draft: false,
  base_ref_name: "main",
  created_at: new Date(Date.now() - 86400000).toISOString(),
  updated_at: new Date().toISOString(),
  merge_state_label: { label: "Ready to merge", css_class: "success" },
  review_state_label: { label: "Approved", css_class: "success" },
};

/** Merge a partial override with base PR defaults, using the given branch name */
export function buildPrStatus(branch: string, override?: PrOverride): BranchPrStatus {
  return { ...basePr, branch, ...override };
}

function rateLimitFor(agent: AgentType, minutes: number): RateLimitInfo {
  return {
    agentType: agent,
    sessionId: `sim-${agent}-${Date.now()}`,
    retryAfterMs: minutes * 60 * 1000,
    message: `Simulated rate limit for ${agent}`,
    detectedAt: Date.now(),
  };
}

export const PRESETS: Record<string, Preset> = {
  "pr-ready": {
    description: "Open PR, approved, all CI passing, mergeable",
    pr: {
      state: "OPEN",
      mergeable: "MERGEABLE",
      merge_state_status: "CLEAN",
      review_decision: "APPROVED",
      checks: { passed: 5, failed: 0, pending: 0, total: 5 },
      merge_state_label: { label: "Ready to merge", css_class: "success" },
      review_state_label: { label: "Approved", css_class: "success" },
    },
  },

  "pr-conflict": {
    description: "Open PR, merge conflict, changes requested",
    pr: {
      state: "OPEN",
      mergeable: "CONFLICTING",
      merge_state_status: "DIRTY",
      review_decision: "CHANGES_REQUESTED",
      checks: { passed: 3, failed: 1, pending: 0, total: 4 },
      check_details: [
        { context: "ci/build", state: "SUCCESS" },
        { context: "ci/test", state: "FAILURE" },
        { context: "ci/lint", state: "SUCCESS" },
        { context: "ci/typecheck", state: "SUCCESS" },
      ],
      merge_state_label: { label: "Conflicts", css_class: "danger" },
      review_state_label: { label: "Changes requested", css_class: "warning" },
    },
  },

  "pr-draft": {
    description: "Draft PR, CI failing, review required",
    pr: {
      state: "OPEN",
      is_draft: true,
      mergeable: "UNKNOWN",
      merge_state_status: "DRAFT",
      review_decision: "REVIEW_REQUIRED",
      checks: { passed: 1, failed: 2, pending: 1, total: 4 },
      check_details: [
        { context: "ci/build", state: "FAILURE" },
        { context: "ci/test", state: "FAILURE" },
        { context: "ci/lint", state: "SUCCESS" },
        { context: "ci/typecheck", state: "PENDING" },
      ],
      merge_state_label: { label: "Draft", css_class: "muted" },
      review_state_label: { label: "Review required", css_class: "warning" },
    },
  },

  "pr-behind": {
    description: "Open PR, behind base branch",
    pr: {
      state: "OPEN",
      mergeable: "MERGEABLE",
      merge_state_status: "BEHIND",
      review_decision: "APPROVED",
      checks: { passed: 5, failed: 0, pending: 0, total: 5 },
      merge_state_label: { label: "Behind base", css_class: "warning" },
      review_state_label: { label: "Approved", css_class: "success" },
    },
  },

  "ci-pending": {
    description: "PR with checks still running",
    pr: {
      state: "OPEN",
      mergeable: "UNKNOWN",
      merge_state_status: "BLOCKED",
      review_decision: "REVIEW_REQUIRED",
      checks: { passed: 2, failed: 0, pending: 3, total: 5 },
      check_details: [
        { context: "ci/build", state: "PENDING" },
        { context: "ci/test", state: "PENDING" },
        { context: "ci/lint", state: "SUCCESS" },
        { context: "ci/typecheck", state: "SUCCESS" },
        { context: "ci/deploy-preview", state: "PENDING" },
      ],
      merge_state_label: { label: "Checks running", css_class: "pending" },
      review_state_label: { label: "Review required", css_class: "warning" },
    },
  },

  "rate-limited": {
    description: "Claude rate limited, Gemini fallback active",
    rateLimit: [rateLimitFor("claude", 15)],
    agent: { active: "gemini" },
  },

  "all-down": {
    description: "All agents rate limited",
    rateLimit: [
      rateLimitFor("claude", 15),
      rateLimitFor("gemini", 10),
      rateLimitFor("opencode", 20),
      rateLimitFor("aider", 5),
      rateLimitFor("codex", 30),
    ],
  },
};
