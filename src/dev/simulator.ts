/**
 * Dev-mode simulator for testing PR/Git/Agent state combinations.
 * Exposes `window.__tuic` console API — only loaded in dev builds.
 */
import type { AgentType } from "../agents";
import type { NotificationSound } from "../notifications";
import type { RateLimitInfo } from "../rate-limit";
import { githubStore } from "../stores/github";
import { rateLimitStore } from "../stores/ratelimit";
import { agentFallbackStore } from "../stores/agentFallback";
import { repositoriesStore } from "../stores/repositories";
import { notificationsStore } from "../stores/notifications";
import { PRESETS, buildPrStatus, type PrOverride } from "./presets";

const SIM_REPO_PATH = "/sim/repo";
let pollingWasStopped = false;

/** Ensure a repo + branch exist so PR data can be injected */
function ensureRepo(): { repoPath: string; branch: string } {
  const active = repositoriesStore.getActive();
  if (active?.activeBranch) {
    return { repoPath: active.path, branch: active.activeBranch };
  }

  // Create a temporary simulated repo
  if (!repositoriesStore.get(SIM_REPO_PATH)) {
    repositoriesStore.add({ path: SIM_REPO_PATH, displayName: "Sim Repo", initials: "SR" });
    repositoriesStore.setBranch(SIM_REPO_PATH, "feature/sim", {
      name: "feature/sim",
      isMain: false,
      worktreePath: null,
      terminals: [],
      additions: 0,
      deletions: 0,
    });
    repositoriesStore.setActiveBranch(SIM_REPO_PATH, "feature/sim");
    repositoriesStore.setActive(SIM_REPO_PATH);
  }

  return { repoPath: SIM_REPO_PATH, branch: "feature/sim" };
}

/** Stop polling so mock data isn't overwritten */
function suppressPolling(): void {
  if (!pollingWasStopped) {
    githubStore.stopPolling();
    pollingWasStopped = true;
  }
}

/** Simulator API bound to window.__tuic */
const simulator = {
  /** Inject PR state for active branch */
  pr(override?: PrOverride): void {
    suppressPolling();
    const { repoPath, branch } = ensureRepo();
    const status = buildPrStatus(branch, override);
    githubStore.updateRepoData(repoPath, [status]);
    console.log(`[tuic] PR injected for ${repoPath} @ ${branch}`);
  },

  /** Override git branch/status */
  git(options: { branch?: string; additions?: number; deletions?: number }): void {
    const { repoPath } = ensureRepo();
    if (options.branch) {
      repositoriesStore.setBranch(repoPath, options.branch);
      repositoriesStore.setActiveBranch(repoPath, options.branch);
    }
    const branch = repositoriesStore.get(repoPath)?.activeBranch;
    if (branch) {
      repositoriesStore.updateBranchStats(
        repoPath,
        branch,
        options.additions ?? 0,
        options.deletions ?? 0,
      );
    }
    console.log(`[tuic] Git state updated for ${repoPath}`);
  },

  /** Simulate rate limit for an agent */
  rateLimit(options: { agent: AgentType; minutes?: number; message?: string }): void {
    const info: RateLimitInfo = {
      agentType: options.agent,
      sessionId: `sim-${options.agent}-${Date.now()}`,
      retryAfterMs: (options.minutes ?? 15) * 60 * 1000,
      message: options.message ?? `Simulated rate limit for ${options.agent}`,
      detectedAt: Date.now(),
    };
    rateLimitStore.addRateLimit(info);
    console.log(`[tuic] Rate limit applied: ${options.agent} for ${options.minutes ?? 15}m`);
  },

  /** Override agent availability/active */
  agent(options: { active?: AgentType; unavailable?: AgentType[] }): void {
    if (options.unavailable) {
      for (const agent of options.unavailable) {
        agentFallbackStore.markUnavailable(agent);
      }
    }
    if (options.active) {
      agentFallbackStore._devOverrideActive(options.active);
      console.log(`[tuic] Active agent: ${options.active}`);
    }
  },

  /** Trigger notification sound */
  notification(options: { sound: NotificationSound }): void {
    notificationsStore.testSound(options.sound);
    console.log(`[tuic] Notification triggered: ${options.sound}`);
  },

  /** Apply a named preset scenario */
  scenario(name: string): void {
    const preset = PRESETS[name];
    if (!preset) {
      console.error(`[tuic] Unknown preset: "${name}". Use __tuic.presets() to list.`);
      return;
    }

    suppressPolling();
    console.log(`[tuic] Applying preset: ${name} — ${preset.description}`);

    if (preset.pr) {
      simulator.pr(preset.pr);
    }
    if (preset.rateLimit) {
      for (const rl of preset.rateLimit) {
        rateLimitStore.addRateLimit(rl);
      }
    }
    if (preset.agent) {
      simulator.agent(preset.agent);
    }
  },

  /** List available presets */
  presets(): void {
    console.log("[tuic] Available presets:");
    for (const [key, preset] of Object.entries(PRESETS)) {
      console.log(`  ${key.padEnd(16)} ${preset.description}`);
    }
  },

  /** Clear all mocks and resume polling */
  reset(): void {
    rateLimitStore.clearAll();
    agentFallbackStore.forceResetToPrimary();

    // Clean up sim repo if it was created
    if (repositoriesStore.get(SIM_REPO_PATH)) {
      repositoriesStore.remove(SIM_REPO_PATH);
    }

    if (pollingWasStopped) {
      githubStore.startPolling();
      pollingWasStopped = false;
    }

    console.log("[tuic] All mocks cleared, polling resumed.");
  },

  /** Print usage */
  help(): void {
    console.log(`
[tuic] Dev Simulator — inject mock states for UI testing

  __tuic.pr({ mergeable: 'CONFLICTING', review_decision: 'APPROVED' })
  __tuic.git({ branch: 'feature/test', additions: 50, deletions: 10 })
  __tuic.rateLimit({ agent: 'claude', minutes: 15 })
  __tuic.agent({ active: 'gemini' })
  __tuic.notification({ sound: 'completion' })
  __tuic.scenario('pr-ready')
  __tuic.presets()
  __tuic.reset()
  __tuic.help()
`);
  },
};

// Bind to window
declare global {
  interface Window {
    __tuic: typeof simulator;
  }
}

window.__tuic = simulator;
console.log("[tuic] Dev simulator loaded. Type __tuic.help() for usage.");
