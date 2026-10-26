import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ActivityDashboard } from "../../components/ActivityDashboard/ActivityDashboard";
import { snapshotToRows } from "../../panelAdapters/activity";
import { appLogger } from "../../stores/appLogger";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { buildActivitySnapshot } from "../../utils/activitySnapshot";
import { makeTerminal } from "../helpers/store";
import "../mocks/tauri";

const repoPath = "/projects/backend";

beforeEach(() => {
	vi.spyOn(appLogger, "info").mockImplementation(() => {});
});

afterEach(() => {
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	repositoriesStore.remove(repoPath);
	terminalsStore._testCancelPendingTimers();
	repositoriesStore._testCancelPendingSave();
	vi.restoreAllMocks();
});

function session(name: string, cwd: string, workspaceId?: string) {
	const id = terminalsStore.add(makeTerminal({ name, cwd }));
	if (workspaceId) repositoriesStore.addTerminalToWorkspace(repoPath, workspaceId, id);
	return id;
}

describe("Activity Dashboard repository labels", () => {
	// Catches: cwd basenames hide repository identity, opaque workspace ids replace branch names,
	// or detached snapshots lose the owning repository and fall back to cwd again.
	it.each(["inline", "detached"])("labels linked worktrees with their repository and branch (%s)", (mode) => {
		repositoriesStore.add({ path: repoPath, displayName: "Backend API" });
		repositoriesStore.setWorkspace(repoPath, "primary-id", {
			branchName: "main",
			kind: "main",
			worktreePath: repoPath,
		});
		repositoriesStore.setWorkspace(repoPath, "opaque-workspace-id", {
			branchName: "feat/auth",
			kind: "worktree",
			worktreePath: "/worktrees/auth-folder",
		});
		session("main-session", `${repoPath}/src`, "primary-id");
		session("worktree-session", "/worktrees/auth-folder/src", "opaque-workspace-id");
		session("outside-session", "/scratch/notes");
		const rows = mode === "detached" ? snapshotToRows(buildActivitySnapshot()) : undefined;
		const view = render(() => <ActivityDashboard embedded terminals={rows ? () => rows : undefined} />);
		const labelFor = (name: string) => view.getByText(name).parentElement?.textContent;
		expect.soft(labelFor("main-session")).toBe("main-sessionBackend API");
		expect.soft(labelFor("worktree-session")).toBe("worktree-sessionBackend API · feat/auth");
		expect.soft(labelFor("outside-session")).toBe("outside-sessionnotes");
	});
});
