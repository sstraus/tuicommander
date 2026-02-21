import { describe, it, expect, beforeEach } from "vitest";
import { createRoot } from "solid-js";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { useAgentDetection } from "../../hooks/useAgentDetection";

describe("useAgentDetection", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  describe("detectAgent()", () => {
    it("returns available=true with path and version on success", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValueOnce({
          path: "/usr/local/bin/claude",
          version: "1.2.3",
        });

        const { detectAgent } = useAgentDetection();
        const result = await detectAgent("claude", "claude");

        expect(result).toEqual({
          type: "claude",
          available: true,
          path: "/usr/local/bin/claude",
          version: "1.2.3",
        });
        expect(mockInvoke).toHaveBeenCalledWith("detect_agent_binary", { binary: "claude" });

        dispose();
      });
    });

    it("returns available=false when path is null", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockResolvedValueOnce({ path: null, version: null });

        const { detectAgent } = useAgentDetection();
        const result = await detectAgent("gemini", "gemini");

        expect(result).toEqual({
          type: "gemini",
          available: false,
          path: null,
          version: null,
        });

        dispose();
      });
    });

    it("returns available=false on error", async () => {
      await createRoot(async (dispose) => {
        mockInvoke.mockRejectedValueOnce(new Error("binary not found"));

        const { detectAgent } = useAgentDetection();
        const result = await detectAgent("aider", "aider");

        expect(result).toEqual({
          type: "aider",
          available: false,
          path: null,
          version: null,
        });

        dispose();
      });
    });
  });

  describe("detectAll()", () => {
    it("detects all 10 agents and populates detections signal", async () => {
      await createRoot(async (dispose) => {
        // Mock all 10 invoke calls (claude, gemini, opencode, aider, codex, amp, jules, cursor, warp, ona)
        mockInvoke
          .mockResolvedValueOnce({ path: "/bin/claude", version: "1.0" })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: "/bin/opencode", version: "2.0" })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: "/bin/codex", version: "0.5" })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null });

        const { detectAll, detections } = useAgentDetection();
        await detectAll();

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

  describe("getDetection()", () => {
    it("returns detection for a known type after detectAll", async () => {
      await createRoot(async (dispose) => {
        mockInvoke
          .mockResolvedValueOnce({ path: "/bin/claude", version: "1.0" })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null });

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
        mockInvoke
          .mockResolvedValueOnce({ path: "/bin/claude", version: "1.0" })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null });

        const { detectAll, isAvailable } = useAgentDetection();
        await detectAll();

        expect(isAvailable("claude")).toBe(true);

        dispose();
      });
    });

    it("returns false when agent is not available", async () => {
      await createRoot(async (dispose) => {
        mockInvoke
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null });

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
        mockInvoke
          .mockResolvedValueOnce({ path: "/bin/claude", version: "1.0" })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: "/bin/opencode", version: "2.0" })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null });

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
        mockInvoke
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null });

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
        mockInvoke
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null })
          .mockResolvedValueOnce({ path: null, version: null });

        const { detectAll, loading } = useAgentDetection();
        await detectAll();

        expect(loading()).toBe(false);

        dispose();
      });
    });
  });
});
