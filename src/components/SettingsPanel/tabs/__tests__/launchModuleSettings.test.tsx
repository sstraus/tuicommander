import { fireEvent, render } from "@solidjs/testing-library";
import { expect, it, vi } from "vitest";
import { createAgentConfigsStore } from "../../../../stores/agentConfigs";
import { AgentConfigProvider } from "../agentConfigContext";
import { LaunchModuleSettings } from "../LaunchModuleSettings";

it("catches agent module switches bypassing global off or overwriting additions", async () => {
	const save = vi.fn().mockResolvedValue(undefined);
	const store = createAgentConfigsStore({
		load: async () => ({ agents: {}, launch_modules: { waiting: false }, launch_instructions: "KEEP" }),
		save,
	});
	await store.hydrate();
	const view = render(() => (
		<AgentConfigProvider value={store}>
			<LaunchModuleSettings agentType="ego" />
		</AgentConfigProvider>
	));
	const waiting = view.getByRole("checkbox", { name: "Waiting and long work" }) as HTMLInputElement;
	expect(waiting.checked).toBe(false);
	expect(waiting.disabled).toBe(true);
	fireEvent.click(view.getByRole("checkbox", { name: "Git and worktrees" }));
	expect(store.state.agents.ego.launch_modules?.git_worktree).toBe(false);
	expect(store.getLaunchInstructions()).toBe("KEEP");
	fireEvent.click(view.getByRole("checkbox", { name: "Turn all launch instructions off" }));
	expect(store.state.agents.ego.launch_instructions_disabled).toBe(true);
	expect((view.getByRole("checkbox", { name: "Markers" }) as HTMLInputElement).disabled).toBe(true);
});

it("catches missing module help or a help click changing the module switch", async () => {
	const store = createAgentConfigsStore({
		load: async () => ({ agents: {} }),
		save: vi.fn().mockResolvedValue(undefined),
	});
	await store.hydrate();
	const view = render(() => (
		<AgentConfigProvider value={store}>
			<LaunchModuleSettings />
		</AgentConfigProvider>
	));
	for (const [label, text] of [
		["Markers", "connection, intent and answer markers"],
		["Working with the user", "checkbox choices"],
		["Waiting and long work", "continues independent work"],
		["Git and worktrees", "checkouts and sessions stay tracked"],
		["Process safety", "recorded PIDs"],
		["Children (spawned children only)", "one final RESULT or BLOCKED"],
	]) {
		const toggle = view.getByRole("checkbox", { name: label }) as HTMLInputElement;
		fireEvent.click(view.getByRole("button", { name: `Explain ${label}` }));
		expect(view.getByText(new RegExp(text))).toBeInTheDocument();
		expect(toggle.checked).toBe(true);
	}
});
