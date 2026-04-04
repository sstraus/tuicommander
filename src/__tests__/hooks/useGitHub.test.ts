import { describe, it, expect, beforeEach, vi } from "vitest";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { useGitHub } from "../../hooks/useGitHub";
import { githubStore } from "../../stores/github";
import { testInScope } from "../helpers/store";

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
      testInScope(() => {
        const { status, loading, error } = useGitHub(() => undefined);
        expect(status()).toBeNull();
        expect(loading()).toBe(false);
        expect(error()).toBeNull();
      });
    });
  });

  describe("reads from githubStore", () => {
    it("returns remote status from githubStore for the given repo path", () => {
      testInScope(() => {
        githubStore.updateRepoData("/repos/my-repo", []);
        const { status } = useGitHub(() => "/repos/my-repo");

        // Before any poll, remote status is null (no setter exposed, only populated by pollAll)
        expect(status()).toBeNull();

      });
    });

    it("returns null when path changes to undefined", () => {
      testInScope(() => {
        let repoPath: string | undefined = "/repos/my-repo";
        const { status } = useGitHub(() => repoPath);

        // Change path to undefined
        repoPath = undefined;
        expect(status()).toBeNull();

      });
    });
  });

  describe("refresh()", () => {
    it("delegates to githubStore.pollRepo", () => {
      testInScope(() => {
        const pollSpy = vi.spyOn(githubStore, "pollRepo");
        const { refresh } = useGitHub(() => "/repos/my-repo");

        refresh();
        expect(pollSpy).toHaveBeenCalledWith("/repos/my-repo");

        pollSpy.mockRestore();
      });
    });

    it("does nothing when path is undefined", () => {
      testInScope(() => {
        const pollSpy = vi.spyOn(githubStore, "pollRepo");
        const { refresh } = useGitHub(() => undefined);

        refresh();
        expect(pollSpy).not.toHaveBeenCalled();

        pollSpy.mockRestore();
      });
    });
  });

  describe("no independent polling", () => {
    it("does not call invoke directly", () => {
      testInScope(() => {
        useGitHub(() => "/repos/my-repo");

        // The hook should NOT call invoke — that's githubStore's job
        expect(mockInvoke).not.toHaveBeenCalledWith("get_github_status", expect.anything());

      });
    });

    it("startPolling and stopPolling are no-ops", () => {
      testInScope(() => {
        const { startPolling, stopPolling } = useGitHub(() => "/repos/my-repo");

        // These should not throw
        startPolling();
        stopPolling();

      });
    });
  });
});
