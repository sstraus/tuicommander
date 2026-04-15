import { promptLibraryStore, type SavedPrompt } from "../stores/promptLibrary";
import { terminalsStore } from "../stores/terminals";
import { githubStore } from "../stores/github";
import { repositoriesStore } from "../stores/repositories";
import { agentConfigsStore, llmApiStore } from "../stores/agentConfigs";
import { usePty } from "./usePty";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { isWindows } from "../platform";

export interface SmartPromptResult {
  ok: boolean;
  reason?: string;
  /** For headless mode: command output. For unresolved_variables: JSON array of variable names. */
  output?: string;
}

/**
 * Minimal shell-word splitter for headless templates.
 * Respects single and double quotes; backslash escapes the next char (outside single quotes).
 * Does NOT perform variable expansion, command substitution, or globbing — those would
 * re-introduce the injection vector we are removing. Metacharacters like `;` and backticks
 * are treated as literal characters inside the resulting argv tokens.
 */
export function shellSplit(input: string): string[] {
  const tokens: string[] = [];
  let cur = "";
  let quote: '"' | "'" | null = null;
  let hasToken = false;
  let escaped = false;
  for (const ch of input) {
    if (escaped) {
      cur += ch;
      escaped = false;
      hasToken = true;
      continue;
    }
    if (quote === "'") {
      if (ch === "'") { quote = null; }
      else { cur += ch; }
      hasToken = true;
      continue;
    }
    if (quote === '"') {
      if (ch === '"') { quote = null; }
      else if (ch === "\\") { escaped = true; }
      else { cur += ch; }
      hasToken = true;
      continue;
    }
    if (ch === "'" || ch === '"') { quote = ch; hasToken = true; continue; }
    if (ch === "\\") { escaped = true; continue; }
    if (/\s/.test(ch)) {
      if (hasToken) { tokens.push(cur); cur = ""; hasToken = false; }
      continue;
    }
    cur += ch;
    hasToken = true;
  }
  if (hasToken) tokens.push(cur);
  return tokens;
}

export function useSmartPrompts() {
  const pty = usePty();

  /** Check if a smart prompt can be executed right now */
  function canExecute(prompt: SavedPrompt): { ok: boolean; reason?: string } {
    if (prompt.enabled === false) return { ok: false, reason: "Prompt is disabled" };

    if (prompt.executionMode === "shell") {
      return { ok: true };
    }

    if (prompt.executionMode === "api") {
      if (!llmApiStore.isConfigured()) return { ok: false, reason: "LLM API not configured — set provider, model, and API key in Settings → Agents" };
      return { ok: true };
    }

    if (prompt.executionMode === "headless") {
      const agentType = agentConfigsStore.getHeadlessAgent();
      if (!agentType) return { ok: false, reason: "No headless agent configured — set one in Settings → Agents" };
      // When headless agent is "api", validate API config instead of CLI template
      if (agentType === "api") {
        if (!llmApiStore.isConfigured()) return { ok: false, reason: "LLM API not configured — set provider, model, and API key in Settings → Agents" };
        return { ok: true };
      }
      const template = agentConfigsStore.getHeadlessTemplate(agentType);
      if (!template) return { ok: false, reason: `No headless template for ${agentType}` };
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
        if (pr.checks) {
          vars["pr_checks"] = `${pr.checks.passed} passed, ${pr.checks.failed} failed, ${pr.checks.pending} pending`;
        }
        if (pr.author) vars["pr_author"] = pr.author;
        if (pr.labels?.length) vars["pr_labels"] = pr.labels.map((l) => l.name).join(", ");
        if (pr.additions != null) vars["pr_additions"] = String(pr.additions);
        if (pr.deletions != null) vars["pr_deletions"] = String(pr.deletions);
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

    // If prompt is headless but the configured headless agent is "api", upgrade to API mode
    const rawMode = prompt.executionMode ?? "inject";
    const effectiveMode = rawMode === "headless" && agentConfigsStore.getHeadlessAgent() === "api"
      ? "api"
      : rawMode;

    // Resolve variables
    const activeRepo = repositoriesStore.getActive();
    const repoPath = activeRepo?.path ?? "";
    const autoVars = repoPath ? await resolveAllVariables(repoPath) : {};
    const allVars = { ...autoVars, ...manualVariables };

    // Check for unresolved variables — return them so the UI can show a dialog
    const varNames = await promptLibraryStore.extractVariables(prompt.content);
    const unresolved = varNames.filter((v) => !(v in allVars));
    if (unresolved.length > 0) {
      return { ok: false, reason: "unresolved_variables", output: JSON.stringify(unresolved) };
    }

    // Substitute variables into content
    const processed = await promptLibraryStore.processContent(prompt, allVars);

    if (effectiveMode === "shell") {
      return executeShell(prompt, processed);
    }
    if (effectiveMode === "api") {
      return executeApi(prompt, processed);
    }
    if (effectiveMode === "headless") {
      return executeHeadless(prompt, processed);
    }
    return executeInject(prompt, processed);
  }

  async function executeInject(prompt: SavedPrompt, content: string): Promise<SmartPromptResult> {
    const active = terminalsStore.getActive();
    if (!active?.sessionId) return { ok: false, reason: "No active terminal" };

    try {
      if (prompt.autoExecute === false) {
        // Just inject text, no Enter. Skip Ctrl-U prefix on native Windows
        // shells without a detected agent (cmd.exe/PowerShell echo it literally).
        const prefix = isWindows() && !active.agentType ? "" : "\x15";
        await pty.write(active.sessionId, prefix + content);
      } else {
        await pty.sendCommand(active.sessionId, content, active.agentType);
      }
      promptLibraryStore.markAsUsed(prompt.id);
      return { ok: true };
    } catch (err) {
      appLogger.error("prompts", `Failed to inject prompt "${prompt.name}"`, err);
      return { ok: false, reason: String(err) };
    }
  }

  async function executeHeadless(prompt: SavedPrompt, content: string): Promise<SmartPromptResult> {
    const headlessVal = agentConfigsStore.getHeadlessAgent();
    if (!headlessVal) {
      return { ok: false, reason: "No headless agent configured — set one in Settings → Agents" };
    }

    // Resolve headless_agent: "type:configName" format from grouped dropdown, or plain agent type.
    // Prompt content is sent via stdin — {prompt} tokens in args/templates are dropped, never
    // interpolated. Args are passed as a structured argv array (no shell) to eliminate injection.
    let command: string | undefined;
    let args: string[] = [];
    let envVars: Record<string, string> | undefined;
    let fallbackTemplate: string | undefined;
    if (headlessVal.includes(":")) {
      // Run config selected — parse "agentType:configName"
      const [agentType, configName] = headlessVal.split(":", 2);
      const configs = agentConfigsStore.getRunConfigs(agentType as import("../agents").AgentType);
      const cfg = configs.find((c) => c.name === configName);
      if (cfg) {
        command = cfg.command;
        args = cfg.args.filter((a) => a !== "{prompt}");
        envVars = Object.keys(cfg.env).length > 0 ? cfg.env : undefined;
      } else {
        // Config not found, fall back to agent type template
        fallbackTemplate = agentConfigsStore.getHeadlessTemplate(agentType as import("../agents").AgentType);
      }
    } else {
      fallbackTemplate = agentConfigsStore.getHeadlessTemplate(headlessVal as import("../agents").AgentType);
    }

    if (!command && fallbackTemplate) {
      const tokens = shellSplit(fallbackTemplate).filter((t) => t !== "{prompt}");
      command = tokens[0];
      args = tokens.slice(1);
    }

    if (!command) {
      return { ok: false, reason: "No headless template found for the configured agent" };
    }

    const active = terminalsStore.getActive();
    const repoPath = active?.cwd ?? repositoriesStore.getActive()?.path ?? "";
    try {
      const output = await invoke<string>("execute_headless_prompt", {
        command,
        args,
        stdinContent: content,
        timeoutMs: 300000,
        repoPath,
        env: envVars,
      });
      promptLibraryStore.markAsUsed(prompt.id);

      // Route output based on prompt's outputTarget
      routeHeadlessOutput(prompt, output);

      return { ok: true, output };
    } catch (err) {
      appLogger.error("prompts", `Headless execution failed for "${prompt.name}"`, err);
      return { ok: false, reason: String(err) };
    }
  }

  async function executeShell(prompt: SavedPrompt, content: string): Promise<SmartPromptResult> {
    const active = terminalsStore.getActive();
    const repoPath = active?.cwd ?? repositoriesStore.getActive()?.path ?? "";
    try {
      const output = await invoke<string>("execute_shell_script", {
        scriptContent: content,
        timeoutMs: 60000,
        repoPath,
      });
      promptLibraryStore.markAsUsed(prompt.id);
      routeHeadlessOutput(prompt, output);
      return { ok: true, output };
    } catch (err) {
      appLogger.error("prompts", `Shell execution failed for "${prompt.name}"`, err);
      return { ok: false, reason: String(err) };
    }
  }

  async function executeApi(prompt: SavedPrompt, content: string): Promise<SmartPromptResult> {
    try {
      const output = await invoke<string>("execute_api_prompt", {
        systemPrompt: prompt.systemPrompt || null,
        content,
        timeoutMs: 60000,
      });
      promptLibraryStore.markAsUsed(prompt.id);
      routeHeadlessOutput(prompt, output);
      return { ok: true, output };
    } catch (err) {
      appLogger.error("prompts", `API execution failed for "${prompt.name}"`, err);
      return { ok: false, reason: String(err) };
    }
  }

  /** Route headless output to the appropriate destination */
  function routeHeadlessOutput(prompt: SavedPrompt, output: string): void {
    if (!output) return;
    switch (prompt.outputTarget) {
      case "clipboard":
        navigator.clipboard.writeText(output).then(
          () => appLogger.info("prompts", `"${prompt.name}" output copied to clipboard`),
          (err) => appLogger.error("prompts", `Failed to copy to clipboard`, err),
        );
        break;
      case "toast":
        appLogger.info("prompts", `${prompt.name}: ${output.slice(0, 500)}`);
        break;
      case "commit-message":
        // Emit a custom event that the Git Panel commit textarea can listen to
        window.dispatchEvent(new CustomEvent("smart-prompt:commit-message", { detail: output }));
        appLogger.info("prompts", `"${prompt.name}" output sent to commit message`);
        break;
      case "panel":
        // For now, log the output — a dedicated panel can be added later
        appLogger.info("prompts", `${prompt.name} result:\n${output.slice(0, 2000)}`);
        break;
      default:
        // No routing — output is available in the SmartPromptResult
        break;
    }
  }

  return {
    canExecute,
    executeSmartPrompt,
    resolveAllVariables,
  };
}
