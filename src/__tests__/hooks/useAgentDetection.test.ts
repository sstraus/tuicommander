import { describe, it, expect, beforeEach } from "vitest";
import { createRoot } from "solid-js";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { useAgentDetection } from "../../hooks/useAgentDetection";

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
    it("detects all 12 agents via single batch call", async () => {
      await createRoot(async (dispose) => {
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
        expect(map.size).toBe(10);
        expect(map.get("claude")?.available).toBe(true);
        expect(map.get("gemini")?.available).toBe(false);
        expect(map.get("opencode")?.available).toBe(true);
        expect(map.get("aider")?.available).toBe(false);
        expect(map.get("codex")?.available).toBe(true);

        dispose();
      });
    });
  });

  describe("detectVersion()", () => {
    it("fetches version for an available agent", async () => {
      await createRoot(async (dispose) => {
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

        dispose();
      });
    });

    it("skips version fetch for unavailable agents", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValueOnce(batchResponse());

        const { detectAll, detectVersion } = useAgentDetection();
        await detectAll();
        await detectVersion("claude");

        // Only the batch call, no individual detect
        expect(mockInvoke).toHaveBeenCalledTimes(1);

        dispose();
      });
    });
  });

  describe("getDetection()", () => {
    it("returns detection for a known type after detectAll", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValueOnce(batchResponse({ claude: "/bin/claude" }));

        const { detectAll, getDetection } = useAgentDetection();
        await detectAll();

        const detection = getDetection("claude");
        expect(detection).toBeDefined();
        expect(detection!.available).toBe(true);
        expect(detection!.path).toBe("/bin/claude");

        dispose();
      });
    });

    it("returns undefined for unknown type before detection", () => {
      createRoot((dispose) => {
        const { getDetection } = useAgentDetection();
        expect(getDetection("claude")).toBeUndefined();
        dispose();
      });
    });
  });

  describe("isAvailable()", () => {
    it("returns true when agent is detected as available", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValueOnce(batchResponse({ claude: "/bin/claude" }));

        const { detectAll, isAvailable } = useAgentDetection();
        await detectAll();

        expect(isAvailable("claude")).toBe(true);

        dispose();
      });
    });

    it("returns false when agent is not available", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValueOnce(batchResponse());

        const { detectAll, isAvailable } = useAgentDetection();
        await detectAll();

        expect(isAvailable("claude")).toBe(false);

        dispose();
      });
    });

    it("returns false when no detection has been run", () => {
      createRoot((dispose) => {
        const { isAvailable } = useAgentDetection();
        expect(isAvailable("claude")).toBe(false);
        dispose();
      });
    });
  });

  describe("getAvailable()", () => {
    it("returns only available agents", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValueOnce(
          batchResponse({ claude: "/bin/claude", opencode: "/bin/opencode" }),
        );

        const { detectAll, getAvailable } = useAgentDetection();
        await detectAll();

        const available = getAvailable();
        expect(available).toHaveLength(2);
        expect(available.map((a) => a.type)).toContain("claude");
        expect(available.map((a) => a.type)).toContain("opencode");

        dispose();
      });
    });

    it("returns empty array when no agents available", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValueOnce(batchResponse());

        const { detectAll, getAvailable } = useAgentDetection();
        await detectAll();

        expect(getAvailable()).toHaveLength(0);

        dispose();
      });
    });
  });

  describe("loading()", () => {
    it("is false initially", () => {
      createRoot((dispose) => {
        const { loading } = useAgentDetection();
        expect(loading()).toBe(false);
        dispose();
      });
    });

    it("is false after detectAll completes", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValueOnce(batchResponse());

        const { detectAll, loading } = useAgentDetection();
        await detectAll();

        expect(loading()).toBe(false);

        dispose();
      });
    });
  });
});
