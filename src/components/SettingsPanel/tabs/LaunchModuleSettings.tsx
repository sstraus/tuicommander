import type { Component } from "solid-js";
import type { AgentType } from "../../../agents";
import { Switch } from "../../ui/Switch";
import a from "./AgentsTab.module.css";
import { useAgentConfig } from "./agentConfigContext";

/** Global flags are hard ceilings; agent flags can opt out without copying text. */
export const LaunchModuleSettings: Component<{ agentType?: AgentType }> = (props) => {
	const store = useAgentConfig();
	const own = () => (props.agentType ? store.state.agents[props.agentType] : store.state);
	const disabled = () =>
		!store.state.loaded || store.state.launch_instructions_disabled || (own()?.launch_instructions_disabled ?? false);
	const ModuleToggle: Component<{ id: string; label: string }> = (toggle) => (
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
			<ModuleToggle id="markers" label="Markers" />
			<ModuleToggle id="working_with_user" label="Working with the user" />
			<ModuleToggle id="waiting" label="Waiting and long work" />
			<ModuleToggle id="git_worktree" label="Git and worktrees" />
			<ModuleToggle id="process_safety" label="Process safety" />
			<ModuleToggle id="children" label="Children (spawned children only)" />
		</div>
	);
};
