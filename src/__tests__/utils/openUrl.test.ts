import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock isTauri and Tauri APIs before importing the module under test
const mockOpenUrl = vi.fn().mockResolvedValue(undefined);
const mockIsTauri = vi.fn<() => boolean>(() => true);
const mockWarn = vi.fn();
const mockError = vi.fn();

vi.mock("../../transport", () => ({
  isTauri: mockIsTauri,
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: mockOpenUrl,
}));

vi.mock("../../stores/appLogger", () => ({
  appLogger: {
    warn: mockWarn,
    error: mockError,
  },
}));

describe("handleOpenUrl", () => {
  let handleOpenUrl: typeof import("../../utils/openUrl").handleOpenUrl;

  beforeEach(async () => {
    vi.resetModules();
    mockOpenUrl.mockReset().mockResolvedValue(undefined);
    mockIsTauri.mockReturnValue(true);
    mockWarn.mockReset();
    mockError.mockReset();

    vi.doMock("../../transport", () => ({ isTauri: mockIsTauri }));
    vi.doMock("@tauri-apps/plugin-opener", () => ({ openUrl: mockOpenUrl }));
    vi.doMock("../../stores/appLogger", () => ({
      appLogger: { warn: mockWarn, error: mockError },
    }));

    handleOpenUrl = (await import("../../utils/openUrl")).handleOpenUrl;
  });

  describe("allowed schemes", () => {
    it("opens http:// URLs via Tauri in native mode", () => {
      handleOpenUrl("http://example.com");
      expect(mockOpenUrl).toHaveBeenCalledWith("http://example.com");
      expect(mockWarn).not.toHaveBeenCalled();
    });

    it("opens https:// URLs via Tauri in native mode", () => {
      handleOpenUrl("https://github.com/org/repo");
      expect(mockOpenUrl).toHaveBeenCalledWith("https://github.com/org/repo");
      expect(mockWarn).not.toHaveBeenCalled();
    });

    it("opens mailto: URLs via Tauri in native mode", () => {
      handleOpenUrl("mailto:user@example.com");
      expect(mockOpenUrl).toHaveBeenCalledWith("mailto:user@example.com");
      expect(mockWarn).not.toHaveBeenCalled();
    });

    it("uses window.open in browser mode for http URLs", () => {
      mockIsTauri.mockReturnValue(false);
      const openSpy = vi.spyOn(window, "open").mockReturnValue(null);

      handleOpenUrl("https://example.com");

      expect(openSpy).toHaveBeenCalledWith("https://example.com", "_blank");
      expect(mockOpenUrl).not.toHaveBeenCalled();
      openSpy.mockRestore();
    });
  });

  describe("blocked schemes", () => {
    it("blocks file:// URLs", () => {
      handleOpenUrl("file:///etc/passwd");
      expect(mockOpenUrl).not.toHaveBeenCalled();
      expect(mockWarn).toHaveBeenCalledWith(
        "app",
        expect.stringContaining("Blocked URL with disallowed scheme"),
      );
    });

    it("blocks smb:// URLs", () => {
      handleOpenUrl("smb://fileserver/share");
      expect(mockOpenUrl).not.toHaveBeenCalled();
      expect(mockWarn).toHaveBeenCalledWith(
        "app",
        expect.stringContaining("Blocked URL with disallowed scheme"),
      );
    });

    it("blocks custom app protocol URLs", () => {
      handleOpenUrl("myapp://open/something");
      expect(mockOpenUrl).not.toHaveBeenCalled();
      expect(mockWarn).toHaveBeenCalledWith(
        "app",
        expect.stringContaining("Blocked URL with disallowed scheme"),
      );
    });

    it("blocks javascript: scheme", () => {
      handleOpenUrl("javascript:alert(1)");
      expect(mockOpenUrl).not.toHaveBeenCalled();
      expect(mockWarn).toHaveBeenCalledWith(
        "app",
        expect.stringContaining("Blocked URL with disallowed scheme"),
      );
    });
  });

  describe("malformed URLs", () => {
    it("blocks completely malformed strings", () => {
      handleOpenUrl("not a url at all");
      expect(mockOpenUrl).not.toHaveBeenCalled();
      expect(mockWarn).toHaveBeenCalledWith(
        "app",
        expect.stringContaining("Blocked malformed URL"),
      );
    });

    it("blocks empty string", () => {
      handleOpenUrl("");
      expect(mockOpenUrl).not.toHaveBeenCalled();
      expect(mockWarn).toHaveBeenCalledWith(
        "app",
        expect.stringContaining("Blocked malformed URL"),
      );
    });

    it("blocks relative paths", () => {
      handleOpenUrl("/relative/path");
      expect(mockOpenUrl).not.toHaveBeenCalled();
      expect(mockWarn).toHaveBeenCalledWith(
        "app",
        expect.stringContaining("Blocked malformed URL"),
      );
    });
  });
});
