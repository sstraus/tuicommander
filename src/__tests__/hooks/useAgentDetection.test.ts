import { beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { AGENT_TYPES } from "../../agents";
import { useAgentDetection } from "../../hooks/useAgentDetection";
import { testInScope, testInScopeAsync } from "../helpers/store";
import { mockInvoke } from "../mocks/tauri";

describe("useAgentDetection", () => {
	beforeEach(() => {
		mockInvoke.mockReset();
	});

	/** Helper: build a batch detection response */
	function batchResponse(available: Record<string, string | null> = {}) {
		const allBinaries = [
			"claude",
			"agy",
			"opencode",
			"codex",
			"amp",
			"cursor-agent",
			"oz",
			"droid",
			"git",
			"grok",
			"pi",
			"ego",
		];
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
					binaries: expect.arrayContaining(["claude", "agy", "opencode"]),
				});

				const map = detections();
				// Every registered agent type must get a detection entry — derived from the
				// registry so adding an agent doesn't silently leave one undetected.
				expect(map.size).toBe(AGENT_TYPES.length);
				expect(map.get("claude")?.available).toBe(true);
				expect(map.get("agy")?.available).toBe(false);
				expect(map.get("opencode")?.available).toBe(true);
				expect(map.get("codex")?.available).toBe(true);
				expect(map.get("grok")?.available).toBe(false);
				expect(map.get("pi")?.available).toBe(false);
				expect(map.get("ego")?.available).toBe(false);
			});
		});

		it("does not send the API agent's empty binary name to detection", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockResolvedValueOnce(batchResponse());
				const { detectAll, getDetection } = useAgentDetection();

				await detectAll();

				expect(mockInvoke).toHaveBeenCalledWith("detect_all_agent_binaries", {
					binaries: expect.not.arrayContaining([""]),
				});
				expect(getDetection("api")?.available).toBe(false);
			});
		});

		// Catches: detection skipped outside the Tauri webview, which left every "+" long press
		// (agent list) empty in a browser or tablet session although /agents/detect-all exists.
		it("detects agents in browser mode too", async () => {
			const realFetch = globalThis.fetch;
			(globalThis as Record<string, unknown>).__TAURI_SHIM__ = true;
			const fetchMock = vi.fn(
				async (_url: string) =>
					new Response(JSON.stringify(batchResponse({ claude: "/bin/claude" })), {
						status: 200,
						headers: { "content-type": "application/json" },
					}),
			);
			globalThis.fetch = fetchMock as unknown as typeof fetch;
			try {
				await testInScopeAsync(async () => {
					const { detectAll, getAvailable } = useAgentDetection();

					await detectAll();

					expect(mockInvoke).not.toHaveBeenCalled();
					expect(String(fetchMock.mock.calls[0]?.[0])).toContain("/agents/detect-all");
					expect(getAvailable().map((a) => a.type)).toEqual(["claude"]);
				});
			} finally {
				delete (globalThis as Record<string, unknown>).__TAURI_SHIM__;
				globalThis.fetch = realFetch;
			}
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
				mockInvoke.mockResolvedValueOnce(batchResponse({ claude: "/bin/claude", opencode: "/bin/opencode" }));

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
