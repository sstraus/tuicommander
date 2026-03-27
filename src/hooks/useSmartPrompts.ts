import { promptLibraryStore, type SavedPrompt } from "../stores/promptLibrary";
import { terminalsStore } from "../stores/terminals";
import { githubStore } from "../stores/github";
import { repositoriesStore } from "../stores/repositories";
import { agentConfigsStore } from "../stores/agentConfigs";
import { usePty } from "./usePty";
import { invoke } from "../invoke";
import { isTauri } from "../transport";
import { appLogger } from "../stores/appLogger";

export interface SmartPromptResult {
  ok: boolean;
  reason?: string;
  /** For headless mode: command output. For unresolved_variables: JSON array of variable names. */
  output?: string;
}

export function useSmartPrompts() {
  const pty = usePty();

  /** Check if a smart prompt can be executed right now */
  function canExecute(prompt: SavedPrompt): { ok: boolean; reason?: string } {
    if (prompt.enabled === false) return { ok: false, reason: "Prompt is disabled" };

    if (prompt.executionMode === "headless") {
      // Headless requires Tauri backend for subprocess execution
      if (!isTauri()) {
        // In PWA mode, headless falls back to inject — check inject requirements
        return canExecuteInject(prompt);
      }
      // Headless requires a configured template for the active agent
      const active = terminalsStore.getActive();
      const agentType = active?.agentType;
      const template = agentType ? agentConfigsStore.getHeadlessTemplate(agentType) : undefined;
      if (!template) {
        return { ok: false, reason: "No headless template configured for this agent" };
      }
      return { ok: true };
    }

    return canExecuteInject(prompt);
  }

  function canExecuteInject(prompt: SavedPrompt): { ok: boolean; reason?: string } {
    const active = terminalsStore.getActive();
    if (!active?.sessionId) return { ok: false, reason: "No active terminal" };
    if (!active.agentType) return { ok: false, reason: "No agent detected in terminal" };
    if (prompt.requiresIdle !== false) {
      const busy = terminalsStore.isBusy(active.id);
      if (busy) return { ok: false, reason: "Agent is busy" };
    }
    return { ok: true };
  }

  /** Resolve all context variables: git vars from Rust + frontend store data (GitHub, agent) */
  async function resolveAllVariables(repoPath: string): Promise<Record<string, string>> {
    // Git variables from Rust backend
    const vars = await promptLibraryStore.resolveVariables(repoPath);

    // GitHub/PR variables from frontend store
    const repo = repositoriesStore.get(repoPath);
    const branch = repo?.activeBranch ?? "";
    if (branch) {
      const pr = githubStore.getBranchPrData(repoPath, branch);
      if (pr) {
        vars["pr_number"] = String(pr.number);
        vars["pr_title"] = pr.title;
        vars["pr_url"] = pr.url;
        vars["pr_state"] = pr.state;
        vars["merge_status"] = pr.mergeable;
        vars["review_decision"] = pr.review_decision;
        vars["pr_checks"] = `${pr.checks.passed} passed, ${pr.checks.failed} failed, ${pr.checks.pending} pending`;
      }
    }

    // Agent/terminal variables
    const activeTerminal = terminalsStore.getActive();
    if (activeTerminal?.agentType) {
      vars["agent_type"] = activeTerminal.agentType;
    }
    if (activeTerminal?.cwd) {
      vars["cwd"] = activeTerminal.cwd;
    }

    return vars;
  }

  /** Execute a smart prompt via inject or headless mode */
  async function executeSmartPrompt(
    prompt: SavedPrompt,
    manualVariables?: Record<string, string>,
  ): Promise<SmartPromptResult> {
    const check = canExecute(prompt);
    if (!check.ok) {
      appLogger.warn("prompts", `Cannot execute "${prompt.name}": ${check.reason}`);
      return check;
    }

    // Headless falls back to inject in PWA mode
    const effectiveMode = prompt.executionMode === "headless" && !isTauri()
      ? "inject"
      : (prompt.executionMode ?? "inject");

    // Resolve variables
    const activeRepo = repositoriesStore.getActive();
    const repoPath = activeRepo?.path ?? "";
    const autoVars = repoPath ? await resolveAllVariables(repoPath) : {};
    const allVars = { ...autoVars, ...manualVariables };

    // Check for unresolved variables — return them so the UI can show a dialog
    const varNames = await promptLibraryStore.extractVariables(prompt.content);
    const unresolved = varNames.filter((v) => !(v in allVars));
    if (unresolved.length > 0 && !manualVariables) {
      return { ok: false, reason: "unresolved_variables", output: JSON.stringify(unresolved) };
    }

    // Substitute variables into content
    const processed = await promptLibraryStore.processContent(prompt, allVars);

    if (effectiveMode === "headless") {
      return executeHeadless(prompt, processed);
    }
    return executeInject(prompt, processed);
  }

  async function executeInject(prompt: SavedPrompt, content: string): Promise<SmartPromptResult> {
    const active = terminalsStore.getActive();
    if (!active?.sessionId) return { ok: false, reason: "No active terminal" };

    // Append newline to auto-execute (send as Enter keypress)
    const data = prompt.autoExecute !== false ? content + "\n" : content;

    try {
      await pty.write(active.sessionId, data);
      promptLibraryStore.markAsUsed(prompt.id);
      return { ok: true };
    } catch (err) {
      appLogger.error("prompts", `Failed to inject prompt "${prompt.name}"`, err);
      return { ok: false, reason: String(err) };
    }
  }

  async function executeHeadless(prompt: SavedPrompt, content: string): Promise<SmartPromptResult> {
    const active = terminalsStore.getActive();
    const agentType = active?.agentType;
    const template = agentType ? agentConfigsStore.getHeadlessTemplate(agentType) : undefined;
    if (!template) {
      // No template configured — fall back to inject
      return executeInject(prompt, content);
    }

    // Substitute {prompt} in template, escaping double quotes in content
    const escaped = content.replace(/"/g, '\\"');
    const commandLine = template.replace("{prompt}", escaped);

    const repoPath = active?.cwd ?? repositoriesStore.getActive()?.path ?? "";
    try {
      const output = await invoke<string>("execute_headless_prompt", {
        commandLine,
        timeoutMs: 300000,
        repoPath,
      });
      promptLibraryStore.markAsUsed(prompt.id);
      return { ok: true, output };
    } catch (err) {
      appLogger.error("prompts", `Headless execution failed for "${prompt.name}"`, err);
      return { ok: false, reason: String(err) };
    }
  }

  return {
    canExecute,
    executeSmartPrompt,
    resolveAllVariables,
  };
}
