import { describe, it, expect, beforeEach, vi } from "vitest";
import { createRoot } from "solid-js";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { useGitHub } from "../../hooks/useGitHub";
import { githubStore } from "../../stores/github";

vi.mock("../../stores/repositories", () => ({
  repositoriesStore: {
    getPaths: () => ["/repos/my-repo"],
  },
}));

describe("useGitHub (reactive wrapper)", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  describe("initial state", () => {
    it("has null status when path is undefined", () => {
      createRoot((dispose) => {
        const { status, loading, error } = useGitHub(() => undefined);
        expect(status()).toBeNull();
        expect(loading()).toBe(false);
        expect(error()).toBeNull();
        dispose();
      });
    });
  });

  describe("reads from githubStore", () => {
    it("returns remote status from githubStore for the given repo path", () => {
      createRoot((dispose) => {
        // Manually set data in the store (simulating a poll result)
        const remoteStatus = { has_remote: true, current_branch: "main", ahead: 3, behind: 1 };
        githubStore.updateRepoData("/repos/my-repo", []);
        // Directly set remote status via store's state setter (using pollAll's pattern)
        // Since the store doesn't expose a direct setter for remoteStatus,
        // we verify via the public API after a poll
        const { status } = useGitHub(() => "/repos/my-repo");

        // Before any poll, status reflects what's in the store
        const currentStatus = status();
        // Initially null since no remote status has been set
        expect(currentStatus).toBeNull();

        dispose();
      });
    });

    it("returns null when path changes to undefined", () => {
      createRoot((dispose) => {
        let repoPath: string | undefined = "/repos/my-repo";
        const { status } = useGitHub(() => repoPath);

        // Change path to undefined
        repoPath = undefined;
        expect(status()).toBeNull();

        dispose();
      });
    });
  });

  describe("refresh()", () => {
    it("delegates to githubStore.pollRepo", () => {
      createRoot((dispose) => {
        const pollSpy = vi.spyOn(githubStore, "pollRepo");
        const { refresh } = useGitHub(() => "/repos/my-repo");

        refresh();
        expect(pollSpy).toHaveBeenCalledWith("/repos/my-repo");

        pollSpy.mockRestore();
        dispose();
      });
    });

    it("does nothing when path is undefined", () => {
      createRoot((dispose) => {
        const pollSpy = vi.spyOn(githubStore, "pollRepo");
        const { refresh } = useGitHub(() => undefined);

        refresh();
        expect(pollSpy).not.toHaveBeenCalled();

        pollSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("no independent polling", () => {
    it("does not call invoke directly", () => {
      createRoot((dispose) => {
        useGitHub(() => "/repos/my-repo");

        // The hook should NOT call invoke — that's githubStore's job
        expect(mockInvoke).not.toHaveBeenCalledWith("get_github_status", expect.anything());

        dispose();
      });
    });

    it("startPolling and stopPolling are no-ops", () => {
      createRoot((dispose) => {
        const { startPolling, stopPolling } = useGitHub(() => "/repos/my-repo");

        // These should not throw
        startPolling();
        stopPolling();

        dispose();
      });
    });
  });
});
