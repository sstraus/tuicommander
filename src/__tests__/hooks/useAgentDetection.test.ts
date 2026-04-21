import { describe, it, expect, beforeEach } from "vitest";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { useAgentDetection } from "../../hooks/useAgentDetection";
import { testInScope, testInScopeAsync } from "../helpers/store";

describe("useAgentDetection", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  /** Helper: build a batch detection response */
  function batchResponse(available: Record<string, string | null> = {}) {
    const allBinaries = ["claude", "gemini", "opencode", "aider", "codex", "amp", "cursor-agent", "oz", "droid", "git"];
    const result: Record<string, { path: string | null; version: string | null }> = {};
    for (const bin of allBinaries) {
      result[bin] = { path: available[bin] ?? null, version: null };
    }
    return result;
  }

  describe("detectAll()", () => {
    it("detects all agents via single batch call", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockResolvedValueOnce(
          batchResponse({ claude: "/bin/claude", opencode: "/bin/opencode", codex: "/bin/codex" }),
        );

        const { detectAll, detections } = useAgentDetection();
        await detectAll();

        expect(mockInvoke).toHaveBeenCalledTimes(1);
        expect(mockInvoke).toHaveBeenCalledWith("detect_all_agent_binaries", {
          binaries: expect.arrayContaining(["claude", "gemini", "opencode"]),
        });

        const map = detections();
        expect(map.size).toBe(12);
        expect(map.get("claude")?.available).toBe(true);
        expect(map.get("gemini")?.available).toBe(false);
        expect(map.get("opencode")?.available).toBe(true);
        expect(map.get("aider")?.available).toBe(false);
        expect(map.get("codex")?.available).toBe(true);

      });
    });
  });

  describe("detectVersion()", () => {
    it("fetches version for an available agent", async () => {
      await testInScopeAsync(async () => {
        // First call: batch detection
        mockInvoke.mockResolvedValueOnce(batchResponse({ claude: "/bin/claude" }));
        // Second call: single detect_agent_binary for version
        mockInvoke.mockResolvedValueOnce({ path: "/bin/claude", version: "1.2.3" });

        const { detectAll, detectVersion, getDetection } = useAgentDetection();
        await detectAll();

        expect(getDetection("claude")?.version).toBeNull();

        await detectVersion("claude");

        expect(getDetection("claude")?.version).toBe("1.2.3");
        expect(mockInvoke).toHaveBeenCalledWith("detect_agent_binary", { binary: "claude" });

      });
    });

    it("skips version fetch for unavailable agents", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockResolvedValueOnce(batchResponse());

        const { detectAll, detectVersion } = useAgentDetection();
        await detectAll();
        await detectVersion("claude");

        // Only the batch call, no individual detect
        expect(mockInvoke).toHaveBeenCalledTimes(1);

      });
    });
  });

  describe("getDetection()", () => {
    it("returns detection for a known type after detectAll", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockResolvedValueOnce(batchResponse({ claude: "/bin/claude" }));

        const { detectAll, getDetection } = useAgentDetection();
        await detectAll();

        const detection = getDetection("claude");
        expect(detection).toBeDefined();
        expect(detection!.available).toBe(true);
        expect(detection!.path).toBe("/bin/claude");

      });
    });

    it("returns undefined for unknown type before detection", () => {
      testInScope(() => {
        const { getDetection } = useAgentDetection();
        expect(getDetection("claude")).toBeUndefined();
      });
    });
  });

  describe("isAvailable()", () => {
    it("returns true when agent is detected as available", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockResolvedValueOnce(batchResponse({ claude: "/bin/claude" }));

        const { detectAll, isAvailable } = useAgentDetection();
        await detectAll();

        expect(isAvailable("claude")).toBe(true);

      });
    });

    it("returns false when agent is not available", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockResolvedValueOnce(batchResponse());

        const { detectAll, isAvailable } = useAgentDetection();
        await detectAll();

        expect(isAvailable("claude")).toBe(false);

      });
    });

    it("returns false when no detection has been run", () => {
      testInScope(() => {
        const { isAvailable } = useAgentDetection();
        expect(isAvailable("claude")).toBe(false);
      });
    });
  });

  describe("getAvailable()", () => {
    it("returns only available agents", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockResolvedValueOnce(
          batchResponse({ claude: "/bin/claude", opencode: "/bin/opencode" }),
        );

        const { detectAll, getAvailable } = useAgentDetection();
        await detectAll();

        const available = getAvailable();
        expect(available).toHaveLength(2);
        expect(available.map((a) => a.type)).toContain("claude");
        expect(available.map((a) => a.type)).toContain("opencode");

      });
    });

    it("returns empty array when no agents available", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockResolvedValueOnce(batchResponse());

        const { detectAll, getAvailable } = useAgentDetection();
        await detectAll();

        expect(getAvailable()).toHaveLength(0);

      });
    });
  });

  describe("loading()", () => {
    it("is false initially", () => {
      testInScope(() => {
        const { loading } = useAgentDetection();
        expect(loading()).toBe(false);
      });
    });

    it("is false after detectAll completes", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockResolvedValueOnce(batchResponse());

        const { detectAll, loading } = useAgentDetection();
        await detectAll();

        expect(loading()).toBe(false);

      });
    });
  });
});
