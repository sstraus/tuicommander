import { describe, it, expect, beforeEach, vi } from "vitest";
import "../mocks/tauri";
import { terminalsStore } from "../../stores/terminals";
import { useAppLazygit } from "../../hooks/useAppLazygit";
import * as platform from "../../platform";

const isWindowsSpy = vi.spyOn(platform, "isWindows");

function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
}

describe("useAppLazygit", () => {
  const mockPty = {
    close: vi.fn().mockResolvedValue(undefined),
  };

  let lazygit: ReturnType<typeof useAppLazygit>;

  beforeEach(() => {
    resetStores();
    vi.clearAllMocks();
    isWindowsSpy.mockReturnValue(false);

    lazygit = useAppLazygit({
      pty: mockPty,
      getCurrentRepoPath: () => "/repo",
      getDefaultFontSize: () => 14,
    });
  });

  describe("buildLazygitCmd (POSIX)", () => {
    it("builds command with escaped repo path", () => {
      isWindowsSpy.mockReturnValue(false);
      const cmd = lazygit.buildLazygitCmd("/my/repo");
      expect(cmd).toContain("lazygit -p '/my/repo'");
      expect(cmd).toContain(".lazygit.yml");
      expect(cmd).toContain(".lazygit.yaml");
    });

    it("handles repo path with spaces", () => {
      isWindowsSpy.mockReturnValue(false);
      const cmd = lazygit.buildLazygitCmd("/my repo/path");
      expect(cmd).toContain("'/my repo/path'");
    });

    it("uses bash test -f syntax", () => {
      isWindowsSpy.mockReturnValue(false);
      const cmd = lazygit.buildLazygitCmd("/repo");
      expect(cmd).toContain("test -f");
      expect(cmd).toContain("cfg=$(");
    });
  });

  describe("buildLazygitCmd (Windows)", () => {
    it("uses cmd.exe if exist syntax", () => {
      isWindowsSpy.mockReturnValue(true);
      const cmd = lazygit.buildLazygitCmd("C:\\Users\\me\\repo");
      expect(cmd).toContain("if exist");
      expect(cmd).not.toContain("test -f");
      expect(cmd).not.toContain("cfg=$(");
    });

    it("builds command with escaped repo path", () => {
      isWindowsSpy.mockReturnValue(true);
      const cmd = lazygit.buildLazygitCmd("C:\\Users\\me\\repo");
      expect(cmd).toContain("lazygit -p");
      expect(cmd).toContain(".lazygit.yml");
      expect(cmd).toContain(".lazygit.yaml");
    });

    it("uses backslash path separators for config files", () => {
      isWindowsSpy.mockReturnValue(true);
      const cmd = lazygit.buildLazygitCmd("C:\\repo");
      expect(cmd).toContain("\\.lazygit.yml");
      expect(cmd).toContain("\\.lazygit.yaml");
    });
  });

  describe("spawnLazygit", () => {
    it("writes lazygit command to active terminal", () => {
      const mockWrite = vi.fn();
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);
      terminalsStore.update(id, {
        ref: { write: mockWrite, clear: vi.fn(), fit: vi.fn(), writeln: vi.fn(), focus: vi.fn(), getSessionId: vi.fn() },
      });

      lazygit.spawnLazygit();

      expect(mockWrite).toHaveBeenCalled();
      const call = mockWrite.mock.calls[0][0] as string;
      expect(call).toContain("lazygit");
      expect(call).toContain("/repo");
    });

    it("falls back to plain lazygit when no repo path", () => {
      const mockWrite = vi.fn();
      const noRepoLazygit = useAppLazygit({
        pty: mockPty,
        getCurrentRepoPath: () => undefined,
        getDefaultFontSize: () => 14,
      });

      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);
      terminalsStore.update(id, {
        ref: { write: mockWrite, clear: vi.fn(), fit: vi.fn(), writeln: vi.fn(), focus: vi.fn(), getSessionId: vi.fn() },
      });

      noRepoLazygit.spawnLazygit();
      expect(mockWrite).toHaveBeenCalledWith("lazygit\r");
    });

    it("sets tab name to lazygit with nameIsCustom before writing command", () => {
      const mockWrite = vi.fn();
      const id = terminalsStore.add({
        sessionId: null,
        fontSize: 14,
        name: "Original",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);
      terminalsStore.update(id, {
        ref: { write: mockWrite, clear: vi.fn(), fit: vi.fn(), writeln: vi.fn(), focus: vi.fn(), getSessionId: vi.fn() },
      });

      lazygit.spawnLazygit();

      const term = terminalsStore.get(id);
      expect(term?.name).toBe("lazygit");
      expect(term?.nameIsCustom).toBe(true);
    });

    it("does nothing when no active terminal", () => {
      lazygit.spawnLazygit();
      // No error thrown
    });
  });

  describe("openLazygitPane", () => {
    it("creates terminal and sets pane visible", async () => {
      await lazygit.openLazygitPane();

      expect(lazygit.lazygitPaneVisible()).toBe(true);
      expect(lazygit.lazygitTermId()).not.toBeNull();

      const termId = lazygit.lazygitTermId()!;
      const term = terminalsStore.get(termId);
      expect(term?.name).toBe("lazygit");
      expect(term?.cwd).toBe("/repo");
    });

    it("does nothing when no repo path", async () => {
      const noRepoLazygit = useAppLazygit({
        pty: mockPty,
        getCurrentRepoPath: () => undefined,
        getDefaultFontSize: () => 14,
      });

      await noRepoLazygit.openLazygitPane();
      expect(noRepoLazygit.lazygitPaneVisible()).toBe(false);
    });

    it("closes existing lazygit pane before opening new one", async () => {
      await lazygit.openLazygitPane();
      const firstId = lazygit.lazygitTermId()!;

      await lazygit.openLazygitPane();
      const secondId = lazygit.lazygitTermId()!;

      expect(firstId).not.toBe(secondId);
      expect(terminalsStore.get(firstId)).toBeUndefined();
      expect(terminalsStore.get(secondId)).toBeDefined();
    });

    it("closes PTY session of existing pane", async () => {
      await lazygit.openLazygitPane();
      const firstId = lazygit.lazygitTermId()!;
      // Simulate the terminal getting a session
      terminalsStore.update(firstId, { sessionId: "sess-lg" });

      await lazygit.openLazygitPane();

      expect(mockPty.close).toHaveBeenCalledWith("sess-lg");
    });
  });

  describe("closeLazygitPane", () => {
    it("removes terminal and hides pane", async () => {
      await lazygit.openLazygitPane();
      const termId = lazygit.lazygitTermId()!;

      await lazygit.closeLazygitPane();

      expect(lazygit.lazygitPaneVisible()).toBe(false);
      expect(lazygit.lazygitTermId()).toBeNull();
      expect(terminalsStore.get(termId)).toBeUndefined();
    });

    it("closes PTY session if exists", async () => {
      await lazygit.openLazygitPane();
      const termId = lazygit.lazygitTermId()!;
      terminalsStore.update(termId, { sessionId: "sess-lg2" });

      await lazygit.closeLazygitPane();

      expect(mockPty.close).toHaveBeenCalledWith("sess-lg2");
    });

    it("hides pane even when no terminal exists", async () => {
      lazygit.setLazygitPaneVisible(true);
      await lazygit.closeLazygitPane();
      expect(lazygit.lazygitPaneVisible()).toBe(false);
    });
  });

  describe("lazygitAvailable", () => {
    it("defaults to false", () => {
      expect(lazygit.lazygitAvailable()).toBe(false);
    });

    it("can be set to true", () => {
      lazygit.setLazygitAvailable(true);
      expect(lazygit.lazygitAvailable()).toBe(true);
    });
  });

  describe("lazygitFloating", () => {
    it("defaults to false", () => {
      expect(lazygit.lazygitFloating()).toBe(false);
    });

    it("can be toggled", () => {
      lazygit.setLazygitFloating(true);
      expect(lazygit.lazygitFloating()).toBe(true);
    });
  });
});
