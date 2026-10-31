import type { Component } from "solid-js";
import type { AgentType } from "../../../agents";
import { Switch } from "../../ui/Switch";
import { InfoBadge } from "../InfoBadge";
import a from "./AgentsTab.module.css";
import { useAgentConfig } from "./agentConfigContext";

/** Global flags are hard ceilings; agent flags can opt out without copying text. */
export const LaunchModuleSettings: Component<{ agentType?: AgentType }> = (props) => {
	const store = useAgentConfig();
	const own = () => (props.agentType ? store.state.agents[props.agentType] : store.state);
	const disabled = () =>
		!store.state.loaded || store.state.launch_instructions_disabled || (own()?.launch_instructions_disabled ?? false);
	const ModuleToggle: Component<{ id: string; label: string; help: string }> = (toggle) => (
		<div class={a.toggleRow}>
			<label class={a.toggleRow}>
				<Switch
					role="checkbox"
					data-setting-id={props.agentType ? "agents.agents.{key}.launch_modules.{key}" : "agents.launch_modules.{key}"}
					checked={(store.state.launch_modules[toggle.id] ?? true) && (own()?.launch_modules?.[toggle.id] ?? true)}
					disabled={disabled() || (!!props.agentType && store.state.launch_modules[toggle.id] === false)}
					onChange={(e) => void store.setLaunchModule(toggle.id, e.currentTarget.checked, props.agentType)}
				/>
				<span>{toggle.label}</span>
			</label>
			<InfoBadge label={`Explain ${toggle.label}`}>{toggle.help}</InfoBadge>
		</div>
	);
	return (
		<div class={a.expandedSection}>
			<h3>Launch modules</h3>
			<label class={a.toggleRow}>
				<Switch
					role="checkbox"
					data-setting-id={
						props.agentType ? "agents.agents.{key}.launch_instructions_disabled" : "agents.launch_instructions_disabled"
					}
					checked={store.state.launch_instructions_disabled || (own()?.launch_instructions_disabled ?? false)}
					disabled={!store.state.loaded || (!!props.agentType && store.state.launch_instructions_disabled)}
					onChange={(e) => void store.setLaunchInstructionsDisabled(e.currentTarget.checked, props.agentType)}
				/>
				<span>Turn all launch instructions off</span>
			</label>
			<ModuleToggle
				id="markers"
				label="Markers"
				help="Requires connection, intent and answer markers, plus suggestions when enabled. These let TUIC show agent activity, tab titles and follow-up actions."
			/>
			<ModuleToggle
				id="working_with_user"
				label="Working with the user"
				help="Keeps short answers in chat and longer decisions in Markdown tabs. Agents can collect checkbox choices, apply edits, contact you through Telegram and request secrets privately."
			/>
			<ModuleToggle
				id="waiting"
				label="Waiting and long work"
				help="Runs long tasks in the background and continues independent work. Yields only for a needed result or your answer, using the launched mechanism’s completion wake instead of polling. Reports what still runs without claiming completion. Terminal commands submit text and Enter together."
			/>
			<ModuleToggle
				id="git_worktree"
				label="Git and worktrees"
				help="Creates and removes worktrees through TUIC so checkouts and sessions stay tracked. Prevents branch switching in the agent’s own worktree, which would close its session."
			/>
			<ModuleToggle
				id="process_safety"
				label="Process safety"
				help="Limits process cleanup to recorded PIDs the agent started. Forbids pattern-based kills and extra desktop instances, and requires cleanup before finishing."
			/>
			<ModuleToggle
				id="children"
				label="Children (spawned children only)"
				help="Keeps spawned children within their assigned scope, without further delegation. Requires one final RESULT or BLOCKED report to the parent, with evidence and remaining gaps, and no heartbeats."
			/>
		</div>
	);
};
