import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { IdeLauncher } from "../../components/IdeLauncher/IdeLauncher";
import { IDE_NAMES, settingsStore } from "../../stores/settings";
import { mockInvoke } from "../mocks/tauri";
import "../mocks/tauri";

beforeEach(() => {
	vi.clearAllMocks();
	mockInvoke.mockImplementation(async (command) =>
		command === "detect_installed_ides" ? ["vscode", "terminal"] : undefined,
	);
	vi.spyOn(settingsStore, "setIde").mockImplementation(() => {});
});
afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
});

it("catches the main action losing focused-file targeting or completion callback", async () => {
	const onOpenInIde = vi.fn();
	const view = render(() => <IdeLauncher repoPath="/repo" focusedFilePath="/repo/file.ts" onOpenInIde={onOpenInIde} />);
	fireEvent.click(view.getByTitle(`Open in ${IDE_NAMES.vscode}`));
	await waitFor(() => expect(onOpenInIde).toHaveBeenCalledWith("vscode"));
	expect(mockInvoke).toHaveBeenCalledWith("open_in_app", {
		path: "/repo/file.ts",
		app: "vscode",
		line: undefined,
		col: undefined,
	});
});

it("catches choosing a terminal opening the focused file instead of the repository", async () => {
	const view = render(() => <IdeLauncher repoPath="/repo" focusedFilePath="/repo/file.ts" />);
	fireEvent.click(view.getByRole("button", { name: "Choose editor" }));
	const terminal = await view.findByRole("button", { name: /Terminal/ });
	fireEvent.click(terminal);
	await waitFor(() =>
		expect(mockInvoke).toHaveBeenCalledWith("open_in_app", {
			path: "/repo",
			app: "terminal",
			line: undefined,
			col: undefined,
		}),
	);
	expect(settingsStore.setIde).toHaveBeenCalledWith("terminal");
	expect(view.queryByRole("button", { name: /Terminal/ })).toBeNull();
});

it("catches Run losing the Shift modifier or Escape leaving the editor menu open", async () => {
	const onRun = vi.fn();
	const view = render(() => <IdeLauncher repoPath="/repo" runCommand="pnpm dev" onRun={onRun} />);
	fireEvent.click(view.getByRole("button", { name: "Choose editor" }));
	fireEvent.click(view.getByRole("button", { name: /Run: pnpm dev/ }), { shiftKey: true });
	expect(onRun).toHaveBeenCalledWith(true);
	fireEvent.click(view.getByRole("button", { name: "Choose editor" }));
	fireEvent.keyDown(document, { key: "Escape" });
	expect(view.queryByRole("button", { name: /Run: pnpm dev/ })).toBeNull();
	await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("detect_installed_ides"));
});

it("catches launch controls becoming enabled without a repository", async () => {
	const view = render(() => <IdeLauncher />);
	expect(view.getByTitle(`Open in ${IDE_NAMES.vscode}`)).toBeDisabled();
	fireEvent.click(view.getByRole("button", { name: "Choose editor" }));
	expect(view.getByRole("button", { name: /Run/ })).toBeDisabled();
	const terminal = await view.findByRole("button", { name: /Terminal/ });
	expect(terminal).toBeDisabled();
});
