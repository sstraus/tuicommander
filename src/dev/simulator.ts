/**
 * Dev-mode simulator for testing PR/Git/Agent state combinations.
 * Exposes `window.__tuic` console API — only loaded in dev builds.
 */
import type { AgentType } from "../agents";
import type { NotificationSound } from "../notifications";
import type { RateLimitInfo } from "../rate-limit";
import type { AwaitingInputType } from "../stores/terminals";
import { githubStore } from "../stores/github";
import { rateLimitStore } from "../stores/ratelimit";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { notificationsStore } from "../stores/notifications";
import { prNotificationsStore, type PrNotificationType } from "../stores/prNotifications";
import { uiStore } from "../stores/ui";
import { pluginRegistry } from "../plugins/pluginRegistry";
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

/** Simulate a terminal awaiting input (shows ? icon on branch) */
  question(options?: { type?: AwaitingInputType; clear?: boolean }): void {
    const { branch } = ensureRepo();
    const active = repositoriesStore.getActive();
    if (!active) return;

    const branchState = active.branches[branch];
    if (!branchState?.terminals.length) {
      console.error("[tuic] No terminals on active branch. Open a terminal first.");
      return;
    }

    const termId = branchState.terminals[0];
    if (options?.clear) {
      terminalsStore.clearAwaitingInput(termId);
      console.log(`[tuic] Cleared awaitingInput on terminal ${termId}`);
    } else {
      const type = options?.type ?? "question";
      terminalsStore.setAwaitingInput(termId, type);
      console.log(`[tuic] Set awaitingInput="${type}" on terminal ${termId} (branch: ${branch})`);
    }
  },

  /** Trigger notification sound */
  notification(options: { sound: NotificationSound }): void {
    notificationsStore.testSound(options.sound);
    console.log(`[tuic] Notification triggered: ${options.sound}`);
  },

  /** Simulate PR state notification (toolbar bell) */
  prNotif(options?: { type?: PrNotificationType; branch?: string; pr?: number; title?: string }): void {
    const { repoPath } = ensureRepo();
    const type = options?.type ?? "merged";
    const branch = options?.branch ?? "feature/sim";
    const prNumber = options?.pr ?? 42;
    const title = options?.title ?? `Simulated ${type} notification`;
    prNotificationsStore.add({ repoPath, branch, prNumber, title, type });
    console.log(`[tuic] PR notification: ${type} for #${prNumber} on ${branch}`);
  },

  /** Simulate multiple PR notifications at once */
  prNotifMulti(): void {
    const { repoPath } = ensureRepo();
    const scenarios: Array<{ type: PrNotificationType; branch: string; prNumber: number; title: string }> = [
      { type: "merged", branch: "feature/auth", prNumber: 99, title: "feat: add OAuth2 flow" },
      { type: "ci_failed", branch: "fix/payments", prNumber: 101, title: "fix: payment webhook" },
      { type: "ready", branch: "feature/dashboard", prNumber: 103, title: "feat: new dashboard" },
      { type: "changes_requested", branch: "refactor/api", prNumber: 105, title: "refactor: API layer" },
      { type: "blocked", branch: "feature/export", prNumber: 107, title: "feat: CSV export" },
    ];
    for (const s of scenarios) {
      prNotificationsStore.add({ repoPath, ...s });
    }
    console.log(`[tuic] Injected ${scenarios.length} PR notifications`);
  },

  /** Clear all PR notifications */
  prNotifClear(): void {
    prNotificationsStore.clearAll();
    console.log("[tuic] Cleared all PR notifications");
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
  },

  /** List available presets */
  presets(): void {
    console.log("[tuic] Available presets:");
    for (const [key, preset] of Object.entries(PRESETS)) {
      console.log(`  ${key.padEnd(16)} ${preset.description}`);
    }
  },

  /** Simulate a plan file detection */
  plan(path?: string): void {
    const planPath = path ?? "plans/example-feature.md";
    pluginRegistry.dispatchStructuredEvent("plan-file", { path: planPath }, "simulator");
    console.log(`[tuic] Plan file detected: ${planPath}`);
  },

  /** Toggle a panel for testing */
  panel(name: "diff" | "markdown" | "files" | "notes"): void {
    switch (name) {
      case "diff":
        uiStore.toggleDiffPanel();
        break;
      case "markdown":
        uiStore.toggleMarkdownPanel();
        break;
      case "files":
        uiStore.toggleFileBrowserPanel();
        break;
      case "notes":
        uiStore.toggleNotesPanel();
        break;
      default:
        console.error(`[tuic] Unknown panel: "${name}". Options: diff, markdown, files, notes`);
        return;
    }
    console.log(`[tuic] Toggled panel: ${name}`);
  },

  /** Clear all mocks and resume polling */
  reset(): void {
    rateLimitStore.clearAll();
    prNotificationsStore.clearAll();

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

── PR States ──────────────────────────────────────────────────
  __tuic.pr()                                     Default: open PR, approved, CI green
  __tuic.pr({ mergeable: 'CONFLICTING' })         Merge conflict
  __tuic.pr({ mergeable: 'UNKNOWN' })             GitHub hasn't computed mergeability
  __tuic.pr({ review_decision: 'APPROVED' })      Review approved
  __tuic.pr({ review_decision: 'CHANGES_REQUESTED' })  Changes requested
  __tuic.pr({ review_decision: 'REVIEW_REQUIRED' })    Awaiting review
  __tuic.pr({ is_draft: true })                   Draft PR
  __tuic.pr({ state: 'MERGED' })                  Merged PR
  __tuic.pr({ state: 'CLOSED' })                  Closed PR
  __tuic.pr({ checks: { passed: 2, failed: 1, pending: 1, total: 4 } })  Mixed CI
  __tuic.pr({ checks: { passed: 0, failed: 3, pending: 0, total: 3 } })  All CI failing
  __tuic.pr({ checks: { passed: 0, failed: 0, pending: 5, total: 5 } })  All CI pending
  __tuic.pr({ merge_state_status: 'BEHIND' })     Behind base branch
  __tuic.pr({ additions: 500, deletions: 200 })   Large PR
  __tuic.pr({ labels: [{ name: 'bug', color: 'fc2929', text_color: '#fff', background_color: '#fc2929' }] })

── Git ────────────────────────────────────────────────────────
  __tuic.git({ branch: 'feature/test', additions: 50, deletions: 10 })

── Rate Limits ────────────────────────────────────────────────
  __tuic.rateLimit({ agent: 'claude', minutes: 15 })
  __tuic.rateLimit({ agent: 'gemini', minutes: 10 })

── Question / Awaiting Input ──────────────────────────────────
  __tuic.question()                              Show ? icon (question type)
  __tuic.question({ type: 'error' })             Show ? icon (error type)
  __tuic.question({ type: 'confirmation' })      Show ? icon (confirmation type)
  __tuic.question({ clear: true })               Clear ? icon

── Notifications ──────────────────────────────────────────────
  __tuic.notification({ sound: 'question' })
  __tuic.notification({ sound: 'error' })
  __tuic.notification({ sound: 'completion' })
  __tuic.notification({ sound: 'warning' })

── PR State Notifications (toolbar bell) ──────────────────────
  __tuic.prNotif()                                   Merged notification
  __tuic.prNotif({ type: 'ci_failed' })              CI failed notification
  __tuic.prNotif({ type: 'blocked' })                Conflicts notification
  __tuic.prNotif({ type: 'changes_requested' })      Changes requested
  __tuic.prNotif({ type: 'ready' })                  Ready to merge
  __tuic.prNotif({ type: 'closed' })                 PR closed
  __tuic.prNotifMulti()                              Multiple notifications at once
  __tuic.prNotifClear()                              Clear all PR notifications

── Presets (combined scenarios) ────────────────────────────────
  __tuic.scenario('pr-ready')      Open PR, approved, all CI green
  __tuic.scenario('pr-conflict')   Merge conflict, changes requested, CI failing
  __tuic.scenario('pr-draft')      Draft PR, CI failing, review required
  __tuic.scenario('pr-behind')     Behind base branch
  __tuic.scenario('ci-pending')    Checks still running
  __tuic.scenario('rate-limited')  Claude rate limited, Gemini fallback
  __tuic.scenario('all-down')      All agents rate limited
  __tuic.presets()                 List all presets

── Plan Button ────────────────────────────────────────────────
  __tuic.plan()                              Show plan button (default path)
  __tuic.plan('plans/my-feature.md')         Show plan button (custom path)

── Panels ─────────────────────────────────────────────────────
  __tuic.panel('diff')             Toggle diff panel
  __tuic.panel('markdown')         Toggle markdown panel
  __tuic.panel('files')            Toggle file browser panel
  __tuic.panel('notes')            Toggle notes/ideas panel

── Control ────────────────────────────────────────────────────
  __tuic.reset()                   Clear all mocks, resume polling
  __tuic.help()                    This help
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
