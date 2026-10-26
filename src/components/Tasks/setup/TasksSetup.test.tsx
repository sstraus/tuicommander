import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const reloadPlugin = vi.fn();
vi.mock("../../../invoke", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("../../../plugins/pluginLoader", () => ({ reloadPlugin: (...args: unknown[]) => reloadPlugin(...args) }));

import { appLogger } from "../../../stores/appLogger";
import { pluginStore } from "../../../stores/pluginStore";
import { TasksSetup } from "./TasksSetup";

const entry = (id: string, name: string) => ({
	id,
	name,
	description: "",
	author: "",
	repo: "",
	latestVersion: "1.0.0",
	minAppVersion: "0.1.0",
	capabilities: [],
	downloadUrl: `https://example.invalid/${id}.zip`,
});

beforeEach(() => {
	invoke.mockReset();
	reloadPlugin.mockReset();
	invoke.mockImplementation(async (command: string) => {
		if (command === "fetch_plugin_registry")
			return [entry("github-work", "GitHub Work"), entry("gitlab", "GitLab"), entry("jira", "Jira")];
		return null;
	});
});
afterEach(() => {
	cleanup();
	pluginStore.clear();
	vi.restoreAllMocks();
});

describe("Tasks setup", () => {
	// Catches: an empty Tasks tab that says "install a plugin" without offering the install.
	it("rejects provider narration while retaining every install control", async () => {
		render(() => <TasksSetup />);
		const install = await screen.findByRole("button", { name: "Install GitHub Work" });
		for (const narration of ["GitHub Work shows", "GitLab shows", "Jira shows"]) {
			expect(screen.queryByText(new RegExp(narration))).not.toBeInTheDocument();
		}
		fireEvent.click(install);
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("install_plugin_from_url", {
				url: "https://example.invalid/github-work.zip",
			}),
		);
		expect(screen.getByRole("button", { name: "Install GitLab" })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Install Jira" })).toBeInTheDocument();
	});

	// Catches: the matching plugin not being pointed out when GitHub is already connected.
	it("says which plugin matches the connected GitHub account", async () => {
		invoke.mockImplementation(async (command: string) => {
			if (command === "get_github_viewer_login") return "octocat";
			if (command === "fetch_plugin_registry") return [entry("github-work", "GitHub Work")];
			return null;
		});
		render(() => <TasksSetup />);
		expect(await screen.findByText(/GitHub is connected/)).toBeInTheDocument();
	});

	// Catches: an installed but unconfigured plugin leaving the user to hand-write data/config.json.
	it("names the missing setting and saves the form without JSON", async () => {
		pluginStore.registerPlugin("github-work", { loaded: true });
		render(() => <TasksSetup />);
		expect(await screen.findByText(/GitHub Work needs a repository/)).toBeInTheDocument();
		const save = screen.getByRole("button", { name: "Save GitHub Work" });
		await waitFor(() => expect(save).toBeEnabled());
		fireEvent.click(save);
		expect(await screen.findByText(/Repository is required/)).toBeInTheDocument();
		expect(invoke).not.toHaveBeenCalledWith("write_plugin_data", expect.anything());

		fireEvent.input(screen.getByLabelText("Repository"), { target: { value: "owner/repo" } });
		fireEvent.click(save);
		await waitFor(() => expect(reloadPlugin).toHaveBeenCalledWith("github-work"));
		expect(invoke).toHaveBeenCalledWith(
			"write_plugin_data",
			expect.objectContaining({ pluginId: "github-work", path: "work-items.json" }),
		);
	});

	// Catches: a save error being swallowed, leaving the form looking saved.
	it("shows a save failure", async () => {
		const logged = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		pluginStore.registerPlugin("github-work", { loaded: true });
		invoke.mockImplementation(async (command: string) => {
			if (command === "write_plugin_data") throw new Error("disk full");
			return null;
		});
		render(() => <TasksSetup />);
		const save = await screen.findByRole("button", { name: "Save GitHub Work" });
		await waitFor(() => expect(save).toBeEnabled());
		fireEvent.input(screen.getByLabelText("Repository"), { target: { value: "owner/repo" } });
		fireEvent.click(save);
		expect(await screen.findByText(/disk full/)).toBeInTheDocument();
		expect(logged).toHaveBeenCalledWith(
			"plugin",
			"Failed to save the GitHub Work setup",
			expect.objectContaining({ message: "disk full" }),
		);
		expect(reloadPlugin).not.toHaveBeenCalled();
	});

	// Catches: Settings reusing the form for one plugin while showing the cards of the others.
	it("renders only the requested plugin when pluginId is given", async () => {
		pluginStore.registerPlugin("gitlab", { loaded: true });
		render(() => <TasksSetup pluginId="gitlab" />);
		expect(await screen.findByLabelText("Project")).toBeInTheDocument();
		expect(screen.queryByLabelText("Repository")).not.toBeInTheDocument();
		expect(screen.queryByRole("button", { name: /Install/ })).not.toBeInTheDocument();
	});
});
