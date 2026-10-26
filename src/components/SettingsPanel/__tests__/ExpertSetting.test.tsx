import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../../../__tests__/mocks/tauri";

vi.mock("../../../stores/ui", async () => {
	const { createStore } = await import("solid-js/store");
	const [state, setState] = createStore({ settingsExpertMode: false });
	return {
		uiStore: {
			state,
			setSettingsExpertMode: vi.fn((enabled: boolean) => setState("settingsExpertMode", enabled)),
		},
	};
});

import { appLogger } from "../../../stores/appLogger";
import { settingsExpertStore } from "../../../stores/settingsExpert";
import { uiStore } from "../../../stores/ui";
import { ExpertSection, ExpertSetting } from "../ExpertSetting";

const DEFAULTS = {
	app: { osc52_clipboard: true, disabled_agents: [], services: { auth: { session_token_duration_secs: 3600 } } },
	notifications: { volume: 0.5 },
	agent_settings: {},
};

/** A stand-in control: the mechanism is tested on it, not on a real setting. */
const TestControl = () => <span data-testid="control">Test control</span>;

const control = (container: HTMLElement) => container.querySelector("[data-testid='control']");

function serveDefaults(defaults: unknown = DEFAULTS) {
	mockInvoke.mockImplementation((cmd: string) =>
		cmd === "get_config_defaults" ? Promise.resolve(defaults) : Promise.resolve(undefined),
	);
}

async function openWithDefaults(defaults: unknown = DEFAULTS) {
	serveDefaults(defaults);
	await settingsExpertStore.open();
}

describe("ExpertSetting", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		uiStore.setSettingsExpertMode(false);
	});

	it("hides an expert control at its default in basic mode", async () => {
		await openWithDefaults();
		const { container } = render(() => (
			<ExpertSetting configKey="app.osc52_clipboard" value={true}>
				<TestControl />
			</ExpertSetting>
		));
		expect(control(container)).toBeNull();
	});

	it("shows an expert control whose value differs from its default", async () => {
		await openWithDefaults();
		const { container } = render(() => (
			<ExpertSetting configKey="app.osc52_clipboard" value={false}>
				<TestControl />
			</ExpertSetting>
		));
		expect(control(container)).not.toBeNull();
	});

	it("compares nested paths and arrays by value, not by reference", async () => {
		await openWithDefaults();
		const { container } = render(() => (
			<>
				<ExpertSetting configKey="app.disabled_agents" value={[]}>
					<span data-testid="at-default">a</span>
				</ExpertSetting>
				<ExpertSetting configKey="app.disabled_agents" value={["codex"]}>
					<span data-testid="list-modified">b</span>
				</ExpertSetting>
				<ExpertSetting configKey="app.services.auth.session_token_duration_secs" value={3600}>
					<span data-testid="nested-default">c</span>
				</ExpertSetting>
			</>
		));
		expect(container.querySelector("[data-testid='at-default']")).toBeNull();
		expect(container.querySelector("[data-testid='list-modified']")).not.toBeNull();
		expect(container.querySelector("[data-testid='nested-default']")).toBeNull();
	});

	it("shows every expert control in expert mode", async () => {
		await openWithDefaults();
		const { container } = render(() => (
			<ExpertSetting configKey="app.osc52_clipboard" value={true}>
				<TestControl />
			</ExpertSetting>
		));
		expect(control(container)).toBeNull();
		uiStore.setSettingsExpertMode(true);
		expect(control(container)).not.toBeNull();
	});

	it("shows the control while defaults are still loading, then hides it at default", async () => {
		let resolve: (value: unknown) => void = () => {};
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "get_config_defaults" ? new Promise((r) => (resolve = r)) : Promise.resolve(undefined),
		);
		settingsExpertStore._resetForTests();
		const loading = settingsExpertStore.open();
		const { container } = render(() => (
			<ExpertSetting configKey="app.osc52_clipboard" value={true}>
				<TestControl />
			</ExpertSetting>
		));
		expect(control(container)).not.toBeNull();
		resolve(DEFAULTS);
		await loading;
		await waitFor(() => expect(control(container)).toBeNull());
	});

	it("shows the control when defaults cannot be loaded", async () => {
		const warning = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		try {
			mockInvoke.mockImplementation((cmd: string) =>
				cmd === "get_config_defaults" ? Promise.reject(new Error("offline")) : Promise.resolve(undefined),
			);
			settingsExpertStore._resetForTests();
			await settingsExpertStore.open();
			const { container } = render(() => (
				<ExpertSetting configKey="app.osc52_clipboard" value={true}>
					<TestControl />
				</ExpertSetting>
			));
			expect(control(container)).not.toBeNull();
			expect(warning).toHaveBeenCalledExactlyOnceWith(
				"config",
				"Failed to load config defaults; expert settings stay visible",
				expect.any(Error),
			);
		} finally {
			warning.mockRestore();
		}
	});

	it("shows a control of a domain the host does not report (dictation outside desktop)", async () => {
		await openWithDefaults();
		const { container } = render(() => (
			<ExpertSetting configKey="dictation.language" value="auto">
				<TestControl />
			</ExpertSetting>
		));
		expect(control(container)).not.toBeNull();
	});

	it("shows a revealed control for the current open only", async () => {
		await openWithDefaults();
		const { container } = render(() => (
			<ExpertSetting configKey="app.osc52_clipboard" value={true}>
				<TestControl />
			</ExpertSetting>
		));
		settingsExpertStore.reveal("app.osc52_clipboard");
		expect(control(container)).not.toBeNull();
		expect(uiStore.state.settingsExpertMode).toBe(false);

		await settingsExpertStore.open();
		expect(control(container)).toBeNull();
	});
});

describe("ExpertSetting pins on a user edit", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		settingsExpertStore._resetForTests();
		uiStore.setSettingsExpertMode(false);
	});

	/** A checkbox bound to app.osc52_clipboard (default true), edited like a real tab control. */
	function renderCheckbox(initial: boolean) {
		const [value, setValue] = createSignal(initial);
		const result = render(() => (
			<ExpertSetting configKey="app.osc52_clipboard" value={value()}>
				<div>
					<input
						type="checkbox"
						data-testid="control"
						checked={value()}
						onChange={(e) => setValue(e.currentTarget.checked)}
					/>
				</div>
			</ExpertSetting>
		));
		return { ...result, setValue };
	}

	it("keeps a control visible when the user edits it back to its default", async () => {
		await openWithDefaults();
		const { container } = renderCheckbox(false);
		const box = control(container) as HTMLInputElement;

		fireEvent.click(box);

		expect(box.checked).toBe(true);
		expect(control(container)).toBe(box);
	});

	it("hides a placeholder value that resolves to the default without a user edit", async () => {
		// LocalMcpPanel-style: the local signal starts as a placeholder until load_config resolves.
		await openWithDefaults();
		const { container, setValue } = renderCheckbox(false);
		expect(control(container)).not.toBeNull();

		setValue(true);

		expect(control(container)).toBeNull();
	});

	it("forgets the edit on the next open", async () => {
		await openWithDefaults();
		const { container } = renderCheckbox(false);
		fireEvent.click(control(container) as HTMLInputElement);

		await settingsExpertStore.open();

		expect(control(container)).toBeNull();
	});
});

describe("ExpertSection", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		uiStore.setSettingsExpertMode(false);
	});

	const section = (container: HTMLElement) => container.querySelector("h3")?.parentElement as HTMLElement;

	it("hides the section in basic mode when all its controls are hidden", async () => {
		await openWithDefaults();
		const { container } = render(() => (
			<ExpertSection>
				<h3>Test section</h3>
				<ExpertSetting configKey="app.osc52_clipboard" value={true}>
					<TestControl />
				</ExpertSetting>
				<ExpertSetting configKey="notifications.volume" value={0.5}>
					<TestControl />
				</ExpertSetting>
			</ExpertSection>
		));
		expect(section(container).hidden).toBe(true);

		uiStore.setSettingsExpertMode(true);
		expect(section(container).hidden).toBe(false);
	});

	it("keeps the section visible while one of its controls is modified", async () => {
		await openWithDefaults();
		const { container } = render(() => (
			<ExpertSection>
				<h3>Test section</h3>
				<ExpertSetting configKey="app.osc52_clipboard" value={true}>
					<TestControl />
				</ExpertSetting>
				<ExpertSetting configKey="notifications.volume" value={0.8}>
					<TestControl />
				</ExpertSetting>
			</ExpertSection>
		));
		expect(section(container).hidden).toBe(false);
	});

	it("shows the section again when one of its controls is revealed", async () => {
		await openWithDefaults();
		const { container } = render(() => (
			<ExpertSection>
				<h3>Test section</h3>
				<ExpertSetting configKey="app.osc52_clipboard" value={true}>
					<TestControl />
				</ExpertSetting>
			</ExpertSection>
		));
		expect(section(container).hidden).toBe(true);
		settingsExpertStore.reveal("app.osc52_clipboard");
		expect(section(container).hidden).toBe(false);
	});
});
