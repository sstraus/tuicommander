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
        githubStore.updateRepoData("/repos/my-repo", []);
        const { status } = useGitHub(() => "/repos/my-repo");

        // Before any poll, remote status is null (no setter exposed, only populated by pollAll)
        expect(status()).toBeNull();

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
