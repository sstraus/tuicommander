import { invoke } from "../invoke";
import { isWindows } from "../platform";
import { agentConfigsStore } from "../stores/agentConfigs";
import { appLogger } from "../stores/appLogger";
import { githubStore } from "../stores/github";
import { promptLibraryStore, type SavedPrompt } from "../stores/promptLibrary";
import { providerRegistryStore } from "../stores/providerRegistry";
import { repositoriesStore } from "../stores/repositories";
import { terminalsStore } from "../stores/terminals";
import { isTauri } from "../transport";
import { prContextVariables } from "../utils/promptContext";
import { usePty } from "./usePty";

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
			if (ch === "'") {
				quote = null;
			} else {
				cur += ch;
			}
			hasToken = true;
			continue;
		}
		if (quote === '"') {
			if (ch === '"') {
				quote = null;
			} else if (ch === "\\") {
				escaped = true;
			} else {
				cur += ch;
			}
			hasToken = true;
			continue;
		}
		if (ch === "'" || ch === '"') {
			quote = ch;
			hasToken = true;
			continue;
		}
		if (ch === "\\") {
			escaped = true;
			continue;
		}
		if (/\s/.test(ch)) {
			if (hasToken) {
				tokens.push(cur);
				cur = "";
				hasToken = false;
			}
			continue;
		}
		cur += ch;
		hasToken = true;
	}
	if (hasToken) tokens.push(cur);
	return tokens;
}

interface ResolvedAgent {
	agent: string | null;
	isApi: boolean;
}

function resolveHeadlessAgent(prompt: SavedPrompt): ResolvedAgent {
	const preferred = prompt.preferredAgent;
	const global = agentConfigsStore.getHeadlessAgent();

	if (preferred) {
		if (preferred === "api") return { agent: "api", isApi: true };
		const template = agentConfigsStore.getHeadlessTemplate(preferred);
		if (template) return { agent: preferred, isApi: false };
		appLogger.warn("prompts", `Preferred agent "${preferred}" has no template, falling back to global`);
	}

	if (!global) return { agent: null, isApi: false };
	return { agent: global, isApi: global === "api" };
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
			if (!providerRegistryStore.resolveSlot("headless"))
				return {
					ok: false,
					reason:
						"Headless provider not configured — add a provider and assign the Headless slot in Settings → Providers",
				};
			return { ok: true };
		}

		if (prompt.executionMode === "headless") {
			const resolved = resolveHeadlessAgent(prompt);
			if (resolved.isApi) {
				if (!providerRegistryStore.resolveSlot("headless"))
					return {
						ok: false,
						reason:
							"Headless provider not configured — add a provider and assign the Headless slot in Settings → Providers",
					};
				return { ok: true };
			}
			if (!resolved.agent) return { ok: false, reason: "No headless agent configured — set one in Settings → Agents" };
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

	/** Frontend-only variables (GitHub PR, agent/terminal) — no IPC needed. */
	function resolveFrontendVars(repoPath: string): Record<string, string> {
		const vars: Record<string, string> = {};
		const repo = repositoriesStore.get(repoPath);
		const branch = repo?.activeBranch ?? "";
		if (branch) {
			const pr = githubStore.getBranchPrData(repoPath, branch);
			if (pr) Object.assign(vars, prContextVariables(pr));
		}
		const activeTerminal = terminalsStore.getActive();
		if (activeTerminal?.agentType) {
			vars["agent_type"] = activeTerminal.agentType;
		}
		if (activeTerminal?.cwd) {
			vars["cwd"] = activeTerminal.cwd;
		}
		return vars;
	}

	/** Resolve all context variables (git + frontend). */
	async function resolveAllVariables(repoPath: string): Promise<Record<string, string>> {
		const vars = await promptLibraryStore.resolveVariables(repoPath);
		return { ...vars, ...resolveFrontendVars(repoPath) };
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

		// If prompt is headless but the resolved agent is "api", upgrade to API mode
		const rawMode = prompt.executionMode ?? "inject";
		const effectiveMode = rawMode === "headless" && resolveHeadlessAgent(prompt).isApi ? "api" : rawMode;

		// Single IPC: extract needed variable names + resolve only those from git.
		const activeRepo = repositoriesStore.getActive();
		const repoPath = activeRepo?.path ?? "";
		const { vars: gitVars, needed: varNames } = await invoke<{ vars: Record<string, string>; needed: string[] }>(
			"resolve_prompt_variables",
			{ content: prompt.content, repoPath: repoPath || null },
		);
		const allVars = { ...gitVars, ...resolveFrontendVars(repoPath), ...manualVariables };
		const unresolved = varNames.filter((v) => !(v in allVars));
		if (unresolved.length > 0) {
			return { ok: false, reason: "unresolved_variables", output: JSON.stringify(unresolved) };
		}

		// Substitute variables into content. In shell mode, values go through
		// shell-quoting so repo-controlled variables (branch/pr_*/commit_log) can't
		// escape their argument inside `sh -c` / `cmd /C`.
		const processed = await promptLibraryStore.processContent(prompt, allVars, {
			shellSafe: effectiveMode === "shell",
		});

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
			// DEFERRED (2026-04-23) — Desktop compose routing bypasses autoExecute=false.
			// All prompts go to ComposePanel where the user decides to send or not.
			// This is intentional: ComposePanel replaces the "inject without execute" behavior.
			if (isTauri() && active.ref?.openComposeWithText) {
				active.ref.openComposeWithText(content);
			} else if (prompt.autoExecute === false) {
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
		const resolved = resolveHeadlessAgent(prompt);
		const headlessVal = resolved.agent;
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
				timeoutMs: 120000,
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
