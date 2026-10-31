import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AllSettingsTab } from "../../../components/SettingsPanel/AllSettingsTab";
import { AgentsTab } from "../../../components/SettingsPanel/tabs/AgentsTab";
import { NotificationsTab } from "../../../components/SettingsPanel/tabs/NotificationsTab";
import { SmartPromptsTab } from "../../../components/SettingsPanel/tabs/SmartPromptsTab";
import { VariableInputDialog } from "../../../components/SmartPromptsDropdown/VariableInputDialog";
import { agentConfigsStore } from "../../../stores/agentConfigs";
import { uiStore } from "../../../stores/ui";
import { mockInvoke } from "../../mocks/tauri";

vi.mock("../../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}));
vi.mock("../../../stores/notifications", async (original) => {
	const module = await original<typeof import("../../../stores/notifications")>();
	return {
		...module,
		notificationsStore: {
			...module.notificationsStore,
			state: { ...module.notificationsStore.state, isAvailable: true },
		},
	};
});

beforeEach(() => {
	localStorage.clear();
	vi.useFakeTimers();
	mockInvoke.mockImplementation(async (command: string) => {
		if (command === "detect_all_agent_binaries") return {};
		if (command === "load_agent_configs") return { agents: {}, launch_modules: {} };
		return undefined;
	});
	uiStore.setSettingsExpertMode(true);
});
afterEach(async () => {
	cleanup();
	uiStore.setSettingsExpertMode(false);
	await vi.runOnlyPendingTimersAsync();
	vi.useRealTimers();
});

describe("final copy regressions", () => {
	it.each([
		"Configure terminal agents and named launch configurations.",
		"Control terminal-agent presentation and expand an agent to edit its named launch configurations.",
	])("does not restate terminal agent headings: %s", (copy) => {
		const view = render(() => <AgentsTab />);
		expect(view.getByRole("heading", { name: "Agent behavior" })).toBeInTheDocument();
		expect(view.queryByText(copy)).not.toBeInTheDocument();
	});

	// Catches: global hooks remain undiscoverable in basic setup or are advertised for native-signal agents.
	it("offers the hooks tip only in supported disabled agent setup", () => {
		agentConfigsStore.syncHookInstrumentation("grok", false);
		uiStore.setSettingsExpertMode(false);
		const view = render(() => <AgentsTab />);
		expect(view.queryByRole("complementary", { name: "More precise agent status" })).toBeNull();
		fireEvent.click(view.getByRole("button", { name: /^Claude Code/ }));
		expect(view.queryByRole("complementary", { name: "More precise agent status" })).toBeNull();
		fireEvent.click(view.getByRole("button", { name: /Grok Not found/ }));
		expect(view.getByRole("complementary", { name: "More precise agent status" })).toBeTruthy();
		agentConfigsStore.syncHookInstrumentation("grok", true);
		expect(view.queryByRole("complementary", { name: "More precise agent status" })).toBeNull();
		agentConfigsStore.syncHookInstrumentation("grok", false);
	});

	it("does not narrate the background agent card", () => {
		const view = render(() => <SmartPromptsTab />);
		expect(view.getByRole("combobox", { name: /Headless Agent/ })).toBeInTheDocument();
		expect(
			view.queryByText(
				"AI-powered actions from the toolbar, context menus and command palette. Enable, disable or customize each action.",
			),
		).not.toBeInTheDocument();
	});

	it("does not expose a catalogue template as a record label", () => {
		const view = render(() => <AllSettingsTab initialSetting="agents.agents.{key}.auto_retry_on_error" />);
		expect(view.queryByText("Existing record {key}")).not.toBeInTheDocument();
		const record = view.getByRole("textbox", { name: "Record" });
		fireEvent.input(record, { target: { value: "claude" } });
		expect(record).toHaveValue("claude");
	});

	it("does not repeat variable placeholders as label descriptions", () => {
		const view = render(() => (
			<VariableInputDialog
				variables={["goal", "scope"]}
				promptName="Review changes"
				onSubmit={() => {}}
				onCancel={() => {}}
			/>
		));
		for (const name of ["goal", "scope"]) {
			expect(view.queryByText(`Value for ${name}`)).not.toBeInTheDocument();
			expect(view.getByRole("textbox", { name: `{${name}}` })).toHaveAttribute("placeholder", `Value for ${name}`);
		}
	});

	it("does not narrate the audio output device control", () => {
		const view = render(() => <NotificationsTab />);
		expect(view.getByText("Audio Output Device")).toBeInTheDocument();
		expect(view.queryByText(/Choose which speaker or output/)).not.toBeInTheDocument();
		expect(view.queryByText(/macOS may ask for microphone access/)).not.toBeInTheDocument();
	});
});
