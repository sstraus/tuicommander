import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HandsFreeStatus } from "../../stores/dictation";
import { testInScope, testInScopeAsync } from "../helpers/store";
import { mockInvoke } from "../mocks/tauri";

/**
 * The browser audio transport, stubbed. Its own behaviour is
 * `browserVoice.test.ts`; what matters here is *whether* the store opens it,
 * which is the difference between using this tab's microphone and using the
 * one attached to the machine running TUICommander.
 */
const stopBrowserVoice = vi.fn();
const connectBrowserVoice = vi.fn(async (_owner: string) => ({ stop: stopBrowserVoice }));
vi.mock("../../utils/browserVoice", () => ({
	connectBrowserVoice: (owner: string) => connectBrowserVoice(owner),
}));

/** The earcon player, stubbed: jsdom has no Web Audio, and what the store owns is *when* it plays. */
const playEarcon = vi.fn();
const primeEarcons = vi.fn();
vi.mock("../../utils/earcon", () => ({
	playEarcon: (kind: string) => playEarcon(kind),
	primeEarcons: () => primeEarcons(),
}));

const addToast = vi.fn();
vi.mock("../../stores/toasts", () => ({
	toastsStore: { add: addToast },
}));

/**
 * Load the store the way a browser tab loads it: no Tauri internals, so
 * `src/invoke.ts` routes every command over HTTP.
 *
 * The HTTP transport is the point rather than an obstacle — the browser owner
 * has to survive the trip to the server, and a test that stubbed `invoke`
 * itself would prove only that the store said the right word to itself.
 */
async function browserMode(
	options: {
		armFails?: boolean;
		disarmFails?: boolean;
		config?: unknown;
		handsFree?: (owner: string) => unknown[];
	} = {},
) {
	const internals = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
	const realFetch = globalThis.fetch;
	delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;

	const calls: { path: string; body: unknown }[] = [];
	let handsFree: unknown[] = [];
	const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
		const path = new URL(String(input), "http://localhost").pathname;
		calls.push({
			path,
			body: init?.body ? JSON.parse(String(init.body)) : undefined,
		});
		if (path.endsWith("/hands-free/arm") && options.armFails) {
			return new Response("the terminal is gone", { status: 400 });
		}
		if (path.endsWith("/hands-free/disarm") && options.disarmFails) {
			return new Response("disarm refused", { status: 500 });
		}
		if (path.endsWith("/hands-free/disarm")) {
			return new Response(JSON.stringify({ status: { armed: false, phase: "disarmed" } }), {
				headers: { "content-type": "application/json" },
			});
		}
		if (/\/speech\/(pause|resume|status)$/.test(path)) {
			return new Response(JSON.stringify({ paused: path.endsWith("/pause") }), {
				headers: { "content-type": "application/json" },
			});
		}
		const body =
			path.endsWith("/dictation/hands-free") && handsFree.length > 0
				? handsFree.shift()
				: path.endsWith("/dictation/config") && options.config
					? options.config
					: { armed: true, phase: "waiting" };
		return new Response(JSON.stringify(body), {
			status: 200,
			headers: { "content-type": "application/json" },
		});
	});
	globalThis.fetch = fetchMock as unknown as typeof fetch;

	vi.resetModules();
	const module = await import("../../stores/dictation");
	// Built after the import: the statuses name this tab's owner, which only
	// the freshly imported module knows.
	handsFree = options.handsFree?.(module.browserAudioOwner) ?? [];
	return {
		store: module.dictationStore,
		owner: module.browserAudioOwner,
		fetched: (path: string) => calls.find((call) => call.path === path)?.body,
		requested: (path: string) => calls.some((call) => call.path === path),
		// Vitest numbers every mock call on one global counter, so these are
		// comparable across the two mocks.
		armCallOrder: () =>
			fetchMock.mock.invocationCallOrder[calls.findIndex((call) => call.path.endsWith("/hands-free/arm"))],
		restore() {
			globalThis.fetch = realFetch;
			(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = internals;
		},
	};
}

describe("dictationStore", () => {
	let store: typeof import("../../stores/dictation").dictationStore;
	// An armed conversation polls on an interval, and every test imports a
	// fresh store, so no test can reach the previous one's timer to disarm it.
	// Clearing what the test started keeps one test's poll out of the next.
	let intervals: ReturnType<typeof setInterval>[] = [];

	afterEach(() => {
		for (const interval of intervals) clearInterval(interval);
		intervals = [];
		vi.mocked(globalThis.setInterval).mockRestore();
	});

	beforeEach(async () => {
		const realSetInterval = globalThis.setInterval;
		vi.spyOn(globalThis, "setInterval").mockImplementation(((...args: Parameters<typeof setInterval>) => {
			const interval = realSetInterval(...args);
			intervals.push(interval);
			return interval;
		}) as typeof setInterval);
		vi.resetModules();
		mockInvoke.mockReset();
		connectBrowserVoice.mockClear();
		stopBrowserVoice.mockClear();
		playEarcon.mockClear();
		primeEarcons.mockClear();
		addToast.mockClear();
		// Rust always returns required fields plus defaults for old documents.
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "get_dictation_config"
					? {
							enabled: false,
							hotkey: "F5",
							language: "auto",
							model: "large-v3-turbo",
							device: null,
							rms_threshold: 0.001,
						}
					: undefined,
			),
		);
		store = (await import("../../stores/dictation")).dictationStore;
	});

	describe("defaults", () => {
		it("has correct default state", () => {
			testInScope(() => {
				expect(store.state.enabled).toBe(false);
				expect(store.state.hotkey).toBe("F5");
				expect(store.state.language).toBe("auto");
				expect(store.state.selectedModel).toBe("large-v3-turbo");
				expect(store.state.selectedDevice).toBeNull();
				expect(store.state.models).toEqual([]);
				expect(store.state.modelStatus).toBe("not_downloaded");
				expect(store.state.recording).toBe(false);
				expect(store.state.processing).toBe(false);
				expect(store.state.loading).toBe(false);
				expect(store.state.downloading).toBe(false);
				expect(store.state.downloadPercent).toBe(0);
			});
		});
	});

	describe("spoken replies preference", () => {
		it("keeps persisted mute and saves it without overwriting unrelated voice settings", async () => {
			// catches: refresh re-enables mute, or saving it clobbers the stored voice.
			mockInvoke.mockResolvedValueOnce({ hands_free_spoken_replies: false });
			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.spokenReplies).toBe(false);
				const stored = { hands_free_spoken_replies: false, speech_edge_voice: "it-IT-IsabellaNeural" };
				mockInvoke.mockResolvedValueOnce(stored).mockResolvedValueOnce(undefined);
				await store.saveConfig({ hands_free_spoken_replies: true });
				expect(mockInvoke).toHaveBeenLastCalledWith("set_dictation_config", {
					base: stored,
					config: { ...stored, hands_free_spoken_replies: true },
				});
				expect(store.state.spokenReplies).toBe(true);
			});
		});
	});

	describe("refreshConfig()", () => {
		it("loads config including model and device fields from backend", async () => {
			mockInvoke.mockResolvedValueOnce({
				enabled: true,
				hotkey: "F6",
				language: "en",
				model: "small",
				device: "USB Microphone",
			});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(mockInvoke).toHaveBeenCalledWith("get_dictation_config");
				expect(store.state.enabled).toBe(true);
				expect(store.state.hotkey).toBe("F6");
				expect(store.state.language).toBe("en");
				expect(store.state.selectedModel).toBe("small");
				expect(store.state.selectedDevice).toBe("USB Microphone");
			});
		});

		it("keeps default model when config has no model field", async () => {
			mockInvoke.mockResolvedValueOnce({
				enabled: false,
				hotkey: "F5",
				language: "auto",
			});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.selectedModel).toBe("large-v3-turbo");
			});
		});

		it("starts with auto-send on, like the Rust default, before any config loads", () => {
			testInScope(() => {
				expect(store.state.autoSend).toBe(true);
			});
		});

		it("turns auto-send on for a config without the field, and keeps an explicit off", async () => {
			mockInvoke.mockResolvedValueOnce({ enabled: false, hotkey: "F5", language: "auto" });
			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.autoSend).toBe(true);
			});

			mockInvoke.mockResolvedValueOnce({ enabled: false, hotkey: "F5", language: "auto", auto_send: false });
			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.autoSend).toBe(false);
			});
		});

		it("defaults device to null when config has no device field", async () => {
			mockInvoke.mockResolvedValueOnce({
				enabled: false,
				hotkey: "F5",
				language: "auto",
				model: "large-v3-turbo",
			});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.selectedDevice).toBeNull();
			});
		});

		it("warns when the backend recovered valid settings from corruption", async () => {
			mockInvoke.mockResolvedValueOnce({
				enabled: true,
				hotkey: "F8",
				language: "it",
				recovered_from_corruption: true,
			});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.hotkey).toBe("F8");
				expect(store.state.language).toBe("it");
				expect(addToast).toHaveBeenCalledWith(
					"Dictation settings recovered",
					"Some invalid settings were reset; valid settings were kept.",
					"warn",
				);
			});
		});
	});

	describe("refreshModels()", () => {
		it("fetches model info from backend", async () => {
			const mockModels = [
				{ name: "small", display_name: "Whisper Small", size_hint_mb: 488, downloaded: false, actual_size_mb: 0 },
				{
					name: "large-v3-turbo",
					display_name: "Whisper Large V3 Turbo",
					size_hint_mb: 1620,
					downloaded: true,
					actual_size_mb: 1620,
				},
			];
			mockInvoke.mockResolvedValueOnce(mockModels);

			await testInScopeAsync(async () => {
				await store.refreshModels();
				expect(mockInvoke).toHaveBeenCalledWith("get_model_info");
				expect(store.state.models).toEqual(mockModels);
			});
		});

		it("handles backend errors gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("backend down"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshModels();
				expect(store.state.models).toEqual([]);
				expect(consoleSpy).toHaveBeenCalled();
				consoleSpy.mockRestore();
			});
		});
	});

	describe("setModel()", () => {
		it("saves model to config and updates selectedModel", async () => {
			await testInScopeAsync(async () => {
				await store.setModel("small");
				expect(store.state.selectedModel).toBe("small");
				// saveConfig is called with model included
				expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config", {
					base: expect.anything(),
					config: expect.objectContaining({ model: "small" }),
				});
			});
		});
	});

	describe("deleteModel()", () => {
		it("calls delete_whisper_model and refreshes models", async () => {
			const mockModels = [
				{ name: "small", display_name: "Whisper Small", size_hint_mb: 488, downloaded: false, actual_size_mb: 0 },
			];
			// First call: delete_whisper_model
			mockInvoke.mockResolvedValueOnce("Deleted Whisper Small");
			// Second call: get_model_info (from refreshModels)
			mockInvoke.mockResolvedValueOnce(mockModels);

			await testInScopeAsync(async () => {
				await store.deleteModel("small");
				expect(mockInvoke).toHaveBeenCalledWith("delete_whisper_model", { modelName: "small" });
				expect(mockInvoke).toHaveBeenCalledWith("get_model_info");
				expect(store.state.models).toEqual(mockModels);
			});
		});

		it("handles deletion errors gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("file locked"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.deleteModel("small");
				expect(consoleSpy).toHaveBeenCalled();
				consoleSpy.mockRestore();
			});
		});
	});

	describe("downloadModel()", () => {
		it("accepts model name parameter", async () => {
			mockInvoke
				.mockResolvedValueOnce("Downloaded") // download_whisper_model
				.mockResolvedValueOnce({
					// get_dictation_status (from refreshStatus)
					model_status: "downloaded",
					model_name: "small",
					model_size_mb: 488,
					recording: false,
					processing: false,
				})
				.mockResolvedValueOnce([]); // get_model_info (from refreshModels)

			await testInScopeAsync(async () => {
				await store.downloadModel("small");
				expect(mockInvoke).toHaveBeenCalledWith("download_whisper_model", { modelName: "small" });
				expect(store.state.downloading).toBe(false);
			});
		});

		it("uses selectedModel when no name provided", async () => {
			mockInvoke
				.mockResolvedValueOnce("Downloaded")
				.mockResolvedValueOnce({
					model_status: "downloaded",
					model_name: "large-v3-turbo",
					model_size_mb: 1620,
					recording: false,
					processing: false,
				})
				.mockResolvedValueOnce([]);

			await testInScopeAsync(async () => {
				await store.downloadModel();
				expect(mockInvoke).toHaveBeenCalledWith("download_whisper_model", { modelName: "large-v3-turbo" });
			});
		});

		it("sets downloading state during download", async () => {
			let resolveDownload: (v: string) => void;
			const downloadPromise = new Promise<string>((r) => {
				resolveDownload = r;
			});
			mockInvoke.mockReturnValueOnce(downloadPromise);

			await testInScopeAsync(async () => {
				const downloadTask = store.downloadModel("small");
				// downloading should be true while in progress
				expect(store.state.downloading).toBe(true);
				expect(store.state.downloadPercent).toBe(0);

				// Resolve the download
				mockInvoke.mockResolvedValueOnce({
					model_status: "downloaded",
					model_name: "small",
					model_size_mb: 488,
					recording: false,
					processing: false,
				});
				mockInvoke.mockResolvedValueOnce([]);
				resolveDownload!("Downloaded");
				await downloadTask;

				expect(store.state.downloading).toBe(false);
			});
		});
	});

	describe("startRecording()", () => {
		it("sets loading=true while invoke is pending and clears it after", async () => {
			let resolveStart: () => void;
			const startPromise = new Promise<void>((r) => {
				resolveStart = r;
			});
			mockInvoke.mockReturnValueOnce(startPromise);

			await testInScopeAsync(async () => {
				const task = store.startRecording();
				expect(store.state.loading).toBe(true);
				expect(store.state.recording).toBe(false);

				resolveStart!();
				await task;

				expect(store.state.loading).toBe(false);
				expect(store.state.recording).toBe(true);
			});
		});

		it("clears loading on failure and rethrows", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("mic busy"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await expect(store.startRecording()).rejects.toThrow("mic busy");
				expect(store.state.loading).toBe(false);
				expect(store.state.recording).toBe(false);
				consoleSpy.mockRestore();
			});
		});

		// Bug caught: a secondary instance's refused start looks like a broken
		// microphone instead of "the other instance owns dictation".
		it("flags ownedElsewhere when another instance owns dictation", async () => {
			mockInvoke.mockRejectedValueOnce("Dictation is owned by another TUICommander instance");
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await expect(store.startRecording()).rejects.toBeDefined();
				expect(store.state.ownedElsewhere).toBe(true);
				consoleSpy.mockRestore();
			});
		});
	});

	describe("saveConfig()", () => {
		it("includes model and device in config when saving", async () => {
			await testInScopeAsync(async () => {
				await store.saveConfig({ language: "en" });
				expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config", {
					base: expect.anything(),
					config: expect.objectContaining({
						model: "large-v3-turbo",
						language: "en",
						device: null,
					}),
				});
			});
		});

		it("handles save failure gracefully", async () => {
			mockInvoke.mockResolvedValueOnce({}).mockRejectedValueOnce(new Error("disk full"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
			const { appLogger } = await import("../../stores/appLogger");

			await testInScopeAsync(async () => {
				await store.saveConfig({ enabled: true });
				expect(store.state.enabled).toBe(false);
				expect(appLogger.getEntries()).toEqual(
					expect.arrayContaining([
						expect.objectContaining({
							level: "error",
							source: "dictation",
							message: "Failed to save dictation config",
							data: expect.objectContaining({ message: "disk full" }),
						}),
					]),
				);
				consoleSpy.mockRestore();
			});
		});
	});

	describe("refreshConfig() error handling", () => {
		it("handles backend error gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("backend down"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.enabled).toBe(false); // unchanged
				expect(consoleSpy).toHaveBeenCalled();
				consoleSpy.mockRestore();
			});
		});
	});

	describe("stopRecording()", () => {
		it("returns TranscribeResponse on success", async () => {
			// startRecording sets recording=true; mock both start and stop invoke calls
			mockInvoke
				.mockResolvedValueOnce(undefined) // start_dictation
				.mockResolvedValueOnce({
					// stop_dictation_and_transcribe
					text: "Hello world",
					skip_reason: null,
					duration_s: 2.5,
					truncated_s: 0,
				});

			await testInScopeAsync(async () => {
				await store.startRecording();
				expect(store.state.recording).toBe(true);

				const result = await store.stopRecording();
				expect(result).toEqual({
					text: "Hello world",
					skip_reason: null,
					duration_s: 2.5,
					truncated_s: 0,
				});
				expect(store.state.recording).toBe(false);
				expect(store.state.processing).toBe(false);
				expect(store.state.partialText).toBe("");
				expect(mockInvoke).toHaveBeenCalledWith("stop_dictation_and_transcribe");
			});
		});

		it("returns null and resets state on failure", async () => {
			mockInvoke
				.mockResolvedValueOnce(undefined) // start_dictation
				.mockRejectedValueOnce(new Error("transcription failed")); // stop
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.startRecording();
				const result = await store.stopRecording();
				expect(result).toBeNull();
				expect(store.state.recording).toBe(false);
				expect(store.state.processing).toBe(false);
				consoleSpy.mockRestore();
			});
		});

		it("returns null immediately when not recording", async () => {
			await testInScopeAsync(async () => {
				const result = await store.stopRecording();
				expect(result).toBeNull();
				expect(mockInvoke).not.toHaveBeenCalled();
			});
		});

		it("records why a recording produced no text, and clears it on the next success", async () => {
			// The tuning panel has nothing else to show: a gate that rejected
			// speech and a dead microphone both produce an empty transcript.
			mockInvoke
				.mockResolvedValueOnce(undefined) // start_dictation
				.mockResolvedValueOnce({
					text: "",
					skip_reason: "no speech detected (no_speech 0.91 > 0.60)",
					duration_s: 3.1,
					truncated_s: 0,
				})
				.mockResolvedValueOnce(undefined) // start_dictation
				.mockResolvedValueOnce({
					text: "run the tests",
					skip_reason: null,
					duration_s: 1.4,
					truncated_s: 0,
				});

			await testInScopeAsync(async () => {
				await store.startRecording();
				await store.stopRecording();
				expect(store.state.lastSkipReason).toBe("no speech detected (no_speech 0.91 > 0.60)");

				await store.startRecording();
				await store.stopRecording();
				expect(store.state.lastSkipReason).toBeNull();
			});
		});
	});

	describe("voice gates", () => {
		it("defaults to the thresholds the Rust side uses", () => {
			testInScope(() => {
				expect(store.state.rmsThreshold).toBe(0.001);
				expect(store.state.noSpeechThreshold).toBe(0.6);
			});
		});

		it("loads tuned thresholds from the backend config", async () => {
			mockInvoke.mockResolvedValueOnce({
				enabled: true,
				hotkey: "F5",
				language: "auto",
				rms_threshold: 0.004,
				no_speech_threshold: 0.35,
			});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.rmsThreshold).toBe(0.004);
				expect(store.state.noSpeechThreshold).toBe(0.35);
			});
		});

		it("falls back to the defaults when the stored config predates the gates", async () => {
			// A `no_speech_threshold` read as undefined would reach the slider as
			// 0 and reject every transcription.
			mockInvoke.mockResolvedValueOnce({
				enabled: true,
				hotkey: "F5",
				language: "auto",
			});

			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.rmsThreshold).toBe(0.001);
				expect(store.state.noSpeechThreshold).toBe(0.6);
			});
		});

		it("persists a tuned threshold without dropping the other one", async () => {
			await testInScopeAsync(async () => {
				await store.saveConfig({ no_speech_threshold: 0.4 });

				expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config", {
					base: expect.anything(),
					config: expect.objectContaining({
						no_speech_threshold: 0.4,
						rms_threshold: 0.001,
					}),
				});
				expect(store.state.noSpeechThreshold).toBe(0.4);
			});
		});
	});

	describe("injectText()", () => {
		it("returns injected text on success", async () => {
			mockInvoke.mockResolvedValueOnce("corrected text");

			await testInScopeAsync(async () => {
				const result = await store.injectText("raw text");
				expect(result).toBe("corrected text");
				expect(mockInvoke).toHaveBeenCalledWith("inject_text", { text: "raw text" });
			});
		});

		it("returns null on failure", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("inject failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				const result = await store.injectText("raw text");
				expect(result).toBeNull();
				consoleSpy.mockRestore();
			});
		});
	});

	describe("refreshStatus()", () => {
		it("loads status from backend", async () => {
			mockInvoke.mockResolvedValueOnce({
				model_status: "ready",
				model_name: "large-v3-turbo",
				model_size_mb: 1620,
				recording: true,
				processing: false,
			});

			await testInScopeAsync(async () => {
				await store.refreshStatus();
				expect(store.state.modelStatus).toBe("ready");
				expect(store.state.modelName).toBe("large-v3-turbo");
				expect(store.state.modelSizeMb).toBe(1620);
				expect(store.state.recording).toBe(true);
			});
		});

		it("handles error gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshStatus();
				expect(store.state.modelStatus).toBe("not_downloaded"); // unchanged
				consoleSpy.mockRestore();
			});
		});
	});

	describe("refreshCorrections()", () => {
		it("loads correction map from backend", async () => {
			mockInvoke.mockResolvedValueOnce({ hello: "hi", teh: "the" });

			await testInScopeAsync(async () => {
				await store.refreshCorrections();
				expect(store.state.corrections).toEqual({ hello: "hi", teh: "the" });
			});
		});

		it("handles error gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshCorrections();
				expect(store.state.corrections).toEqual({}); // unchanged
				consoleSpy.mockRestore();
			});
		});
	});

	describe("saveCorrections()", () => {
		it("saves corrections to backend", async () => {
			await testInScopeAsync(async () => {
				await store.saveCorrections({ foo: "bar" });
				expect(mockInvoke).toHaveBeenCalledWith("set_correction_map", { map: { foo: "bar" } });
				expect(store.state.corrections).toEqual({ foo: "bar" });
			});
		});

		it("handles error gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.saveCorrections({ foo: "bar" });
				expect(store.state.corrections).toEqual({}); // unchanged
				consoleSpy.mockRestore();
			});
		});
	});

	describe("refreshDevices()", () => {
		it("loads devices from backend", async () => {
			const devices = [{ name: "Default", is_default: true }];
			mockInvoke.mockResolvedValueOnce(devices);

			await testInScopeAsync(async () => {
				await store.refreshDevices();
				expect(store.state.devices).toEqual(devices);
			});
		});

		it("a timeout empties the device list", async () => {
			// Bug: the first enumeration times out (macOS mic prompt still open);
			// the list must keep its previous content and the retry must pick up
			// the late answer.
			const before = [{ name: "Old", is_default: true }];
			const after = [{ name: "New", is_default: true }];
			mockInvoke.mockResolvedValueOnce(before);
			mockInvoke.mockRejectedValueOnce(new Error("listing audio input devices timed out after 30s"));
			mockInvoke.mockResolvedValueOnce(after);
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshDevices();
				expect(store.state.devices).toEqual(before);
				await store.refreshDevices();
				expect(store.state.devices).toEqual(after);
				consoleSpy.mockRestore();
			});
		});

		it("keeps the previous list when the retry also fails", async () => {
			const before = [{ name: "Old", is_default: true }];
			mockInvoke.mockResolvedValueOnce(before);
			mockInvoke.mockRejectedValueOnce(new Error("timed out"));
			mockInvoke.mockRejectedValueOnce(new Error("timed out"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshDevices();
				await store.refreshDevices();
				expect(store.state.devices).toEqual(before);
				consoleSpy.mockRestore();
			});
		});

		it("handles error gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("failed"));
			mockInvoke.mockRejectedValueOnce(new Error("failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.refreshDevices();
				expect(store.state.devices).toEqual([]); // unchanged
				consoleSpy.mockRestore();
			});
		});
	});

	describe("setEnabled()", () => {
		it("saves config with enabled flag", async () => {
			await testInScopeAsync(async () => {
				// The setters are fire-and-forget by design, and `saveConfig`
				// now reads the stored config before writing it, so the save
				// lands a microtask later rather than inside the call.
				store.setEnabled(true);
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({ config: expect.objectContaining({ enabled: true }) }),
					),
				);
			});
		});
	});

	describe("setHotkey()", () => {
		it("saves config with new hotkey", async () => {
			await testInScopeAsync(async () => {
				store.setHotkey("F8");
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({ config: expect.objectContaining({ hotkey: "F8" }) }),
					),
				);
			});
		});
	});

	describe("setCapturingHotkey()", () => {
		it("sets capturing state", () => {
			testInScope(() => {
				store.setCapturingHotkey(true);
				expect(store.state.capturingHotkey).toBe(true);
				store.setCapturingHotkey(false);
				expect(store.state.capturingHotkey).toBe(false);
			});
		});
	});

	describe("setLanguage()", () => {
		it("saves config with new language", async () => {
			await testInScopeAsync(async () => {
				store.setLanguage("fr");
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({ config: expect.objectContaining({ language: "fr" }) }),
					),
				);
			});
		});
	});

	describe("setDevice()", () => {
		it("saves config with specific device", async () => {
			await testInScopeAsync(async () => {
				store.setDevice("USB Microphone");
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({
							config: expect.objectContaining({ device: "USB Microphone" }),
						}),
					),
				);
			});
		});

		it("saves null device to use system default", async () => {
			await testInScopeAsync(async () => {
				store.setDevice(null);
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({ config: expect.objectContaining({ device: null }) }),
					),
				);
			});
		});
	});

	describe("setNotifyModelOnHandsFree()", () => {
		/**
		 * The one setting whose default is `true`, so the assertion that
		 * matters is the one that turns it off: a default-on flag that cannot
		 * be written false is indistinguishable from a flag nobody reads.
		 */
		it("writes the flag off and remembers it", async () => {
			await testInScopeAsync(async () => {
				expect(store.state.notifyModelOnHandsFree, "on by default, as in Rust").toBe(true);
				store.setNotifyModelOnHandsFree(false);
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({
							config: expect.objectContaining({ hands_free_notify_model: false }),
						}),
					),
				);
				await vi.waitFor(() => expect(store.state.notifyModelOnHandsFree).toBe(false));
			});
		});
	});

	describe("downloadModel() error", () => {
		it("clears downloading state on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("download failed"));
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.downloadModel("small");
				expect(store.state.downloading).toBe(false);
				consoleSpy.mockRestore();
			});
		});
	});

	/**
	 * The save request carries a loaded base and an edited document. If the
	 * caller rebuilds the edited document from a hand-written field list,
	 * every omitted field becomes an unintended deletion or default. The
	 * frontend therefore starts with the complete loaded document and edits
	 * only the fields owned by this surface.
	 *
	 * It has already happened twice. `hands_free_hold_back_ms` (814-6d13) has
	 * been snapping back to its default since it was added, and
	 * `hands_free_activation_phrase` (815-7c76) would have done the same, which
	 * is worse: a gate the user configured and the UI quietly disarmed.
	 *
	 * Both have controls since 818-2a29. A save from another surface still
	 * runs with store state that was never loaded from disk; `speech_command`
	 * has no control at all.
	 *
	 * The fix is the load-modify-save rule already recorded for `save_config`:
	 * read the stored config, change only what this surface owns, and submit
	 * both versions. The assertion is "a field the UI does not model survives
	 * a save", driven off the Rust struct so newly added fields remain covered.
	 */
	describe("saveConfig() payload", () => {
		/** Field names declared by `DictationConfig` in the Rust source. */
		function rustConfigFields(): string[] {
			const source = readFileSync(join(process.cwd(), "src-tauri/src/config.rs"), "utf8");
			const struct = source.match(/pub struct DictationConfig \{([\s\S]*?)\n {4}\}/);
			if (!struct) throw new Error("DictationConfig not found in config.rs");
			const fields = [...struct[1].matchAll(/^\s*pub ([a-z0-9_]+):/gm)].map((match) => match[1]);
			if (fields.length === 0) throw new Error("DictationConfig parsed to zero fields");
			return fields;
		}

		it("carries every field the Rust struct declares, including ones the UI never models", async () => {
			const fields = rustConfigFields();
			// The stored config as Rust would hand it back: every declared
			// field present, each with a value this test can recognise again.
			const stored = Object.fromEntries(fields.map((field) => [field, `stored:${field}`]));
			mockInvoke.mockReset();
			mockInvoke.mockImplementation((command: string) =>
				Promise.resolve(command === "get_dictation_config" ? stored : undefined),
			);

			await testInScopeAsync(async () => {
				await store.saveConfig({ auto_send: true });

				const call = mockInvoke.mock.calls.find(([name]) => name === "set_dictation_config");
				if (!call) throw new Error("saveConfig must reach set_dictation_config");
				const sent = (call[1] as { config: Record<string, unknown> }).config;

				expect(Object.keys(sent).sort()).toEqual([...fields].sort());
				expect(sent.auto_send, "the caller's own change must win").toBe(true);
				expect(sent.speech_command, "a field no UI control models must survive untouched").toBe(
					"stored:speech_command",
				);
				// These three have controls now (818-2a29), all of them in one
				// panel — so a save from anywhere else must still leave them
				// alone rather than writing this session's defaults over them.
				expect(sent.hands_free_activation_phrase).toBe("stored:hands_free_activation_phrase");
				expect(sent.hands_free_hold_back_ms).toBe("stored:hands_free_hold_back_ms");
				expect(sent.speech_voice).toBe("stored:speech_voice");
				expect(sent.hands_free_earcons).toBe("stored:hands_free_earcons");
				expect(sent.hands_free_start_notice).toBe("stored:hands_free_start_notice");
			});
		});

		it("saves a custom start notice and resets it to empty, which Rust reads as the built-in text", async () => {
			let stored: Record<string, unknown> = { hands_free_start_notice: "" };
			mockInvoke.mockReset();
			mockInvoke.mockImplementation((command: string, args?: { config: Record<string, unknown> }) => {
				if (command === "get_dictation_config") return Promise.resolve(stored);
				if (command === "set_dictation_config" && args) stored = args.config;
				if (command === "get_hands_free_default_notice") return Promise.resolve("built-in notice");
				return Promise.resolve(undefined);
			});

			await testInScopeAsync(async () => {
				store.setHandsFreeStartNotice("  Answer in Italian.\n");
				await vi.waitFor(() => expect(store.state.handsFreeStartNotice).toBe("Answer in Italian."));
				expect(stored.hands_free_start_notice).toBe("Answer in Italian.");

				store.resetHandsFreeStartNotice();
				await vi.waitFor(() => expect(store.state.handsFreeStartNotice).toBe(""));
				expect(stored.hands_free_start_notice, "empty is the reset; the default text is never copied").toBe("");

				expect(await store.getDefaultHandsFreeStartNotice()).toBe("built-in notice");
			});
		});

		it("saves a volume change and keeps the stored levelling", async () => {
			// The two loudness sliders save one field each; the other must come
			// from disk, not from a default that would undo the user's choice.
			const stored = { speech_volume_db: -18, speech_levelling: 0.2 };
			mockInvoke.mockReset();
			mockInvoke.mockImplementation((command: string) =>
				Promise.resolve(command === "get_dictation_config" ? stored : undefined),
			);

			await testInScopeAsync(async () => {
				await store.saveConfig({ speech_volume_db: -24 });

				const call = mockInvoke.mock.calls.find(([name]) => name === "set_dictation_config");
				if (!call) throw new Error("saveConfig must reach set_dictation_config");
				const sent = (call[1] as { config: Record<string, unknown> }).config;
				expect(sent.speech_volume_db, "the caller's own change must win").toBe(-24);
				expect(sent.speech_levelling).toBe(0.2);
			});
		});

		it("does not write a config it could not read first", async () => {
			mockInvoke.mockReset();
			mockInvoke.mockImplementation((command: string) =>
				command === "get_dictation_config" ? Promise.reject(new Error("backend down")) : Promise.resolve(undefined),
			);
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.saveConfig({ auto_send: true });

				expect(
					mockInvoke.mock.calls.some(([name]) => name === "set_dictation_config"),
					"a failed load must abort the save, not write a config built from defaults",
				).toBe(false);
				consoleSpy.mockRestore();
			});
		});

		it("does not write a config that came back as something other than one", async () => {
			mockInvoke.mockReset();
			mockInvoke.mockResolvedValue(undefined);
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.saveConfig({ auto_send: true });

				expect(
					mockInvoke.mock.calls.some(([name]) => name === "set_dictation_config"),
					"nothing is not a config, and a save built on it writes defaults over real settings",
				).toBe(false);
				consoleSpy.mockRestore();
			});
		});
	});

	// --- Spoken replies and the conversation (818-2a29) ---------------------

	describe("speech assets", () => {
		it("loads the catalogue", async () => {
			const catalogue = [
				{
					id: "onnxruntime",
					display_name: "Speech runtime",
					kind: "runtime",
					language: null,
					voices: [],
					download_bytes: 20_000_000,
					state: "absent",
					missing: [],
				},
			];
			mockInvoke.mockResolvedValueOnce(catalogue);

			await testInScopeAsync(async () => {
				await store.refreshSpeechAssets();
				expect(mockInvoke).toHaveBeenCalledWith("get_speech_assets");
				expect(store.state.speechAssets).toEqual(catalogue);
			});
		});

		/**
		 * A bar left at its last percent reads as a download still running, and
		 * the row then offers Cancel for something that already gave up.
		 */
		it("clears the progress bar when a download fails", async () => {
			mockInvoke.mockImplementation((command: string) =>
				command === "download_speech_asset"
					? Promise.reject(new Error("network down"))
					: Promise.resolve(command === "get_speech_assets" ? [] : undefined),
			);
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.downloadSpeechAsset("italian");
				expect(store.state.speechDownloads.italian).toBeUndefined();
				expect(consoleSpy).toHaveBeenCalled();
				consoleSpy.mockRestore();
			});
		});

		it("persists the chosen voice", async () => {
			await testInScopeAsync(async () => {
				store.setSpeechVoice("giovanni");
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith(
						"set_dictation_config",
						expect.objectContaining({ config: expect.objectContaining({ speech_voice: "giovanni" }) }),
					),
				);
			});
		});
	});

	describe("hands-free conversation", () => {
		/**
		 * The owner is the whole point of the binding: Rust refuses anything
		 * that is not the desktop endpoint rather than opening the microphone
		 * on the machine running TUICommander. Sending the session id as the
		 * owner, or a browser client id, would be how that guard gets bypassed.
		 */
		it("arms the desktop endpoint against the chosen terminal", async () => {
			mockInvoke.mockImplementation((command: string) =>
				Promise.resolve(
					command === "arm_hands_free_dictation"
						? { armed: true, phase: "waiting", sessionId: "sess-1", owner: "desktop" }
						: undefined,
				),
			);

			await testInScopeAsync(async () => {
				expect(await store.armHandsFree("sess-1")).toBe(true);
				expect(mockInvoke).toHaveBeenCalledWith("arm_hands_free_dictation", {
					sessionId: "sess-1",
					owner: "desktop",
				});
				expect(store.state.handsFree?.armed).toBe(true);
				expect(store.state.handsFreeError).toBeNull();
			});
		});

		it("keeps a refused arm visible instead of showing a conversation that never started", async () => {
			mockInvoke.mockImplementation((command: string) =>
				command === "arm_hands_free_dictation"
					? Promise.reject("Audio endpoint 'browser-42' is not available on this build")
					: Promise.resolve(undefined),
			);
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				expect(await store.armHandsFree("sess-1")).toBe(false);
				expect(store.state.handsFree).toBeNull();
				expect(store.state.handsFreeError).toContain("not available on this build");
				consoleSpy.mockRestore();
			});
		});

		// Bug caught: a refused hands-free arm on a non-owner instance is shown as
		// a generic failure and the Settings notice never appears.
		it("flags ownedElsewhere when hands-free is refused by a non-owner", async () => {
			mockInvoke.mockImplementation((command: string) =>
				command === "arm_hands_free_dictation"
					? Promise.reject("Dictation is owned by another TUICommander instance. Restart this instance.")
					: Promise.resolve(undefined),
			);
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				expect(await store.armHandsFree("sess-1")).toBe(false);
				expect(store.state.ownedElsewhere).toBe(true);
				expect(store.state.handsFreeError).toContain("owned by another TUICommander instance");
				consoleSpy.mockRestore();
			});
		});

		// Bug caught: status.owned_elsewhere is never mapped, so opening Settings
		// on a non-owner shows no notice.
		it("maps status.owned_elsewhere into the store", async () => {
			mockInvoke.mockResolvedValue({
				model_status: "ready",
				model_name: "m",
				model_size_mb: 1,
				recording: false,
				processing: false,
				owned_elsewhere: true,
			});
			await testInScopeAsync(async () => {
				await store.refreshStatus();
				expect(store.state.ownedElsewhere).toBe(true);
			});
		});

		/**
		 * Criterion 2 of 832-e730, from the side that decides it.
		 *
		 * The owner name is the whole binding: Rust opens this machine's
		 * devices for `desktop` and the named client's socket for anything
		 * else, with no fallback. A browser tab that sent `desktop` would be
		 * listening through a microphone in another building, and nothing
		 * downstream could tell.
		 */
		it("arms a browser tab under its own owner, with its own socket opened first", async () => {
			const browser = await browserMode();
			const owner = browser.owner;

			try {
				await testInScopeAsync(async () => {
					expect(await browser.store.armHandsFree("sess-1")).toBe(true);
					expect(connectBrowserVoice).toHaveBeenCalledWith(owner);
					expect(owner).not.toBe("desktop");
					const armed = browser.fetched("/dictation/hands-free/arm");
					expect(armed).toEqual({ sessionId: "sess-1", owner });
					// Opened before the arm, because Rust refuses an owner with
					// no socket behind it rather than falling back to the
					// server's microphone.
					expect(connectBrowserVoice.mock.invocationCallOrder[0]).toBeLessThan(browser.armCallOrder());
				});
			} finally {
				browser.restore();
			}
		});

		/**
		 * A refused arm leaves no conversation, so it must leave no open
		 * microphone either — a device light that stays on for a conversation
		 * that never started is the failure a user notices and cannot explain.
		 */
		it("closes the browser microphone when the arm is refused", async () => {
			const browser = await browserMode({ armFails: true });
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			try {
				await testInScopeAsync(async () => {
					expect(await browser.store.armHandsFree("sess-1")).toBe(false);
					expect(stopBrowserVoice).toHaveBeenCalled();
				});
			} finally {
				consoleSpy.mockRestore();
				browser.restore();
			}
		});

		// Catches: iOS controls have no identity or cannot reach reply controls / conversation stop.
		it("gives Now Playing an identity and routes its controls through browser dictation", async () => {
			const handlers = new Map<MediaSessionAction, MediaSessionActionHandler | null>();
			const media = {
				metadata: null as MediaMetadata | null,
				playbackState: "none" as MediaSessionPlaybackState,
				setActionHandler: (action: MediaSessionAction, handler: MediaSessionActionHandler | null) =>
					handlers.set(action, handler),
			};
			vi.stubGlobal(
				"MediaMetadata",
				class {
					constructor(public fields: MediaMetadataInit) {}
					get title() {
						return this.fields.title;
					}
				},
			);
			Object.defineProperty(navigator, "mediaSession", { configurable: true, value: media });
			const browser = await browserMode();
			try {
				await browser.store.armHandsFree("sess-1");
				expect(media.metadata?.title).toBe("TUICommander hands-free");
				expect(media.playbackState).toBe("playing");
				for (const [action, path] of [
					["pause", "/dictation/speech/pause"],
					["play", "/dictation/speech/resume"],
				] as const) {
					expect(handlers.get(action)).toBeTypeOf("function");
					await handlers.get(action)?.({ action });
					expect(browser.requested(path)).toBe(true);
					expect(media.playbackState).toBe(action === "pause" ? "paused" : "playing");
					expect(stopBrowserVoice).not.toHaveBeenCalled();
				}
				await handlers.get("stop")?.({ action: "stop" });
				expect(browser.requested("/dictation/hands-free/disarm")).toBe(true);
				expect(stopBrowserVoice).toHaveBeenCalled();
				expect(media.metadata).toBeNull();
				expect(media.playbackState).toBe("none");
				for (const action of ["play", "pause", "stop"] as const) expect(handlers.get(action)).toBeNull();
			} finally {
				await browser.store.disarmHandsFree();
				browser.restore();
				Reflect.deleteProperty(navigator, "mediaSession");
				vi.unstubAllGlobals();
			}
		});

		// Catches: a backend disarm leaves stale lock-screen handlers and this tab's mic open.
		it("releases Now Playing and browser audio when a polled conversation ends", async () => {
			const handlers = new Map<MediaSessionAction, MediaSessionActionHandler | null>();
			const media = {
				metadata: null,
				playbackState: "none",
				setActionHandler: (action: MediaSessionAction, handler: MediaSessionActionHandler | null) =>
					handlers.set(action, handler),
			};
			Object.defineProperty(navigator, "mediaSession", { configurable: true, value: media });
			const browser = await browserMode({ handsFree: () => [{ armed: false, phase: "disarmed" }] });
			try {
				await browser.store.armHandsFree("sess-1");
				expect(handlers.get("stop")).toBeTypeOf("function");
				await browser.store.refreshHandsFree();
				expect(stopBrowserVoice).toHaveBeenCalledOnce();
				expect(media.playbackState).toBe("none");
				expect(media.metadata).toBeNull();
				for (const action of ["play", "pause", "stop"] as const) expect(handlers.get(action)).toBeNull();
			} finally {
				await browser.store.disarmHandsFree();
				browser.restore();
				Reflect.deleteProperty(navigator, "mediaSession");
			}
		});

		// Catches: a failed disarm keeps actionable lock-screen controls and the local microphone.
		it("releases local Now Playing even when the disarm request fails", async () => {
			const handlers = new Map<MediaSessionAction, MediaSessionActionHandler | null>();
			const media = {
				metadata: null,
				playbackState: "none",
				setActionHandler: (action: MediaSessionAction, handler: MediaSessionActionHandler | null) =>
					handlers.set(action, handler),
			};
			Object.defineProperty(navigator, "mediaSession", { configurable: true, value: media });
			const browser = await browserMode({ disarmFails: true });
			const errors = vi.spyOn(console, "error").mockImplementation(() => {});
			try {
				await browser.store.armHandsFree("sess-1");
				await handlers.get("stop")?.({ action: "stop" });
				expect(browser.store.state.handsFreeError).toContain("disarm refused");
				expect(errors).toHaveBeenCalled();
				expect(stopBrowserVoice).toHaveBeenCalledOnce();
				expect(media.playbackState).toBe("none");
				for (const action of ["play", "pause", "stop"] as const) expect(handlers.get(action)).toBeNull();
			} finally {
				await browser.store.disarmHandsFree();
				errors.mockRestore();
				browser.restore();
				Reflect.deleteProperty(navigator, "mediaSession");
			}
		});

		/**
		 * The control for the pair above: the desktop never opens a socket, so
		 * a browser transport that leaked into the desktop path — the way a
		 * fallback would — is visible here rather than only on real hardware.
		 */
		it("opens no audio socket on the desktop", async () => {
			mockInvoke.mockImplementation((command: string) =>
				Promise.resolve(
					command === "arm_hands_free_dictation"
						? { armed: true, phase: "waiting", sessionId: "sess-1", owner: "desktop" }
						: undefined,
				),
			);

			await testInScopeAsync(async () => {
				await store.armHandsFree("sess-1");
				expect(connectBrowserVoice).not.toHaveBeenCalled();
			});
		});

		// Catches: a late pre-disarm poll restores the ended conversation and its monitor.
		it("ignores an armed status read started before disarming", async () => {
			let answerOldPoll!: (status: HandsFreeStatus) => void;
			const oldPoll = new Promise<HandsFreeStatus>((resolve) => {
				answerOldPoll = resolve;
			});
			mockInvoke.mockImplementation((command: string) =>
				command === "get_hands_free_status"
					? oldPoll
					: Promise.resolve(
							command === "disarm_hands_free_dictation" ? { status: { armed: false, phase: "disarmed" } } : undefined,
						),
			);
			await testInScopeAsync(async () => {
				const refresh = store.refreshHandsFree();
				await store.disarmHandsFree();
				answerOldPoll({ armed: true, phase: "waiting" } as HandsFreeStatus);
				await refresh;
				expect(store.state.handsFree?.armed).toBe(false);
				expect(store.state.handsFree?.phase).toBe("disarmed");
			});
		});

		/**
		 * A stop takes the state the backend reports after it, so the UI
		 * never shows a conversation that has ended as armed.
		 */
		it("applies the status a stop returns", async () => {
			mockInvoke.mockImplementation((command: string) =>
				Promise.resolve(
					command === "disarm_hands_free_dictation"
						? {
								wasArmed: true,
								discardedPending: false,
								status: { armed: false, phase: "disarmed" },
							}
						: undefined,
				),
			);

			await testInScopeAsync(async () => {
				await store.disarmHandsFree();
				expect(store.state.handsFree?.armed).toBe(false);
			});
		});
	});

	/**
	 * The hands-free meter runs for the whole conversation, so it must show the
	 * voice and ignore the room: a fan that kept the needle twitching would read
	 * as "it hears something" when nothing will be sent.
	 */
	describe("easeMeterLevel()", () => {
		let easeMeterLevel: typeof import("../../stores/dictation").easeMeterLevel;

		beforeEach(async () => {
			({ easeMeterLevel } = await import("../../stores/dictation"));
		});

		it("reads room noise below the floor as silence", () => {
			expect(easeMeterLevel(0, 0.2)).toBe(0);
			expect(easeMeterLevel(0, 0.35)).toBe(0);
		});

		it("rises to a speech level on the first poll", () => {
			expect(easeMeterLevel(0, 1)).toBe(1);
			const speech = easeMeterLevel(0, 0.8);
			expect(speech).toBeGreaterThan(0.5);
			// A louder poll right after is not held back by the previous value.
			expect(easeMeterLevel(speech, 1)).toBe(1);
		});

		it("falls back to silence gradually and ends at exactly zero", () => {
			let level = easeMeterLevel(0, 1);
			let polls = 0;
			while (level > 0 && polls < 100) {
				const next = easeMeterLevel(level, 0);
				expect(next).toBeLessThan(level);
				level = next;
				polls += 1;
			}
			// One sentence must not drop out between two 75 ms polls.
			expect(polls).toBeGreaterThan(5);
			expect(level).toBe(0);
		});

		it("treats a missing or non-numeric level as silence", () => {
			expect(easeMeterLevel(0, undefined)).toBe(0);
			expect(easeMeterLevel(0, Number.NaN)).toBe(0);
			// And it releases rather than jumping to zero.
			expect(easeMeterLevel(0.5, undefined)).toBeCloseTo(0.4);
		});
	});

	/**
	 * The push half of the speech contract (833-6fd4).
	 *
	 * `listen` in `src/invoke.ts` is one function over two transports: Tauri
	 * events on the desktop, the shared SSE stream in a browser. A store that
	 * subscribes therefore works on both, and a store that polls works on
	 * neither without a timer. These tests hold the subscription and what it
	 * does with what arrives.
	 */
	describe("events the backend pushes", () => {
		/** The handler the store registered for `name`, or undefined. */
		const subscribed = async (name: string) => {
			const { listen } = await import("@tauri-apps/api/event");
			const call = vi
				.mocked(listen)
				.mock.calls.filter(([event]) => event === name)
				.pop();
			return call?.[1] as ((event: { payload: unknown }) => void) | undefined;
		};

		it("renders a speech download percent from the event rather than a poll", async () => {
			const handler = await subscribed("speech-download-progress");
			expect(handler, "no subscription means a browser sees no progress at all").toBeDefined();

			testInScope(() => {
				handler?.({ payload: { asset: "italian", percent: 42 } });
				expect(store.state.speechDownloads.italian).toBe(42);
			});
		});

		it("clears a finished download this client never started", async () => {
			// A download started over HTTP by another client, or by curl, has
			// no `downloadSpeechAsset` here whose `finally` would clear it: only
			// the backend's done event can end the bar, or it sits at 100%.
			const handler = await subscribed("speech-download-progress");

			await testInScopeAsync(async () => {
				handler?.({ payload: { asset: "italian", percent: 100 } });
				expect(store.state.speechDownloads.italian).toBe(100);

				mockInvoke.mockClear();
				mockInvoke.mockResolvedValueOnce([]);
				handler?.({ payload: { asset: "italian", percent: 100, done: true } });
				expect(store.state.speechDownloads.italian).toBeUndefined();
				await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_speech_assets"));
			});
		});

		it("applies a pushed utterance without asking the backend anything", async () => {
			const handler = await subscribed("speech-utterance");
			expect(handler, "finished and interrupted have no call to return from").toBeDefined();

			await testInScopeAsync(async () => {
				mockInvoke.mockClear();
				handler?.({
					payload: { utteranceId: "3", state: "speaking", error: null, turn: 9 },
				});
				expect(store.state.utterance?.utteranceId).toBe("3");
				expect(store.state.utterance?.state).toBe("speaking");
				expect(mockInvoke).not.toHaveBeenCalled();
			});
		});

		/**
		 * One reply says nothing about how many are waiting behind it. Letting
		 * it overwrite the whole snapshot would have the panel report an empty
		 * queue every time a reply started playing.
		 */
		it("leaves the queue depth alone while settling what the speaker is doing", async () => {
			const handler = await subscribed("speech-utterance");

			await testInScopeAsync(async () => {
				mockInvoke.mockImplementation((command: string) =>
					Promise.resolve(
						command === "get_speech_status"
							? { available: true, queued: 3, rendering: true, speaking: false, turn: 9 }
							: undefined,
					),
				);
				await store.refreshSpeechStatus();

				handler?.({
					payload: { utteranceId: "4", state: "speaking", error: null, turn: 9 },
				});

				expect(store.state.speech?.speaking).toBe(true);
				expect(store.state.speech?.rendering).toBe(false);
				expect(store.state.speech?.queued).toBe(3);
			});
		});

		/** Bug caught: Play stays on the pill after the held reply was dropped, and resumes nothing. */
		it("clears a held pause when the reply ends", async () => {
			const handler = await subscribed("speech-utterance");
			const { playbackState } = await import("../../stores/dictation");

			await testInScopeAsync(async () => {
				mockInvoke.mockImplementation((command: string) =>
					Promise.resolve(
						command === "get_speech_status" ? { available: true, queued: 0, speaking: false, paused: true } : undefined,
					),
				);
				await store.refreshSpeechStatus();
				expect(playbackState(store.state.speech)).toBe("paused");

				handler?.({ payload: { utteranceId: "4", state: "rendering", error: null, turn: 9 } });
				expect(store.state.speech?.paused, "a queued reply says nothing about the held one").toBe(true);

				handler?.({ payload: { utteranceId: "4", state: "interrupted", error: null, turn: 9 } });
				expect(store.state.speech?.paused).toBe(false);
				expect(playbackState(store.state.speech)).toBe("idle");
			});
		});

		/** Bug caught: a pause the backend refused leaves the pill showing Play for audio still playing. */
		it("re-reads the speaker when pause is refused", async () => {
			const { playbackState } = await import("../../stores/dictation");
			await testInScopeAsync(async () => {
				mockInvoke.mockImplementation((command: string) => {
					if (command === "pause_speech") return Promise.reject("nothing is playing");
					if (command === "get_speech_status")
						return Promise.resolve({ available: true, speaking: false, paused: false });
					return Promise.resolve(undefined);
				});
				await store.pauseSpeech();
				expect(mockInvoke).toHaveBeenCalledWith("get_speech_status");
				expect(playbackState(store.state.speech)).toBe("idle");
			});
		});
	});

	describe("hands-free earcons", () => {
		const status = (overrides: Partial<HandsFreeStatus> = {}): HandsFreeStatus => ({
			armed: true,
			phase: "waiting",
			sessionId: "sess-1",
			owner: "desktop",
			generation: 1,
			pendingText: null,
			holdBackMs: 1500,
			error: null,
			deliveredTurns: 0,
			droppedTurns: 0,
			...overrides,
		});

		/** Answer each `get_hands_free_status` poll with the next status. */
		const polls = (...statuses: HandsFreeStatus[]) => {
			mockInvoke.mockImplementation((command: string) =>
				Promise.resolve(command === "get_hands_free_status" ? statuses.shift() : {}),
			);
		};

		it("plays the delivered earcon when a turn reaches the agent", async () => {
			polls(status(), status({ phase: "delivered", deliveredTurns: 1 }));
			await store.refreshHandsFree();
			await store.refreshHandsFree();
			expect(playEarcon.mock.calls).toEqual([["delivered"]]);
		});

		/**
		 * The phase alone would miss this: the next utterance overwrote
		 * `delivered` before the poll saw it. The counter did not forget.
		 */
		it("still plays when the delivered phase was overwritten between two polls", async () => {
			polls(status({ phase: "holding_back" }), status({ phase: "capturing", deliveredTurns: 1 }));
			await store.refreshHandsFree();
			await store.refreshHandsFree();
			expect(playEarcon.mock.calls).toEqual([["delivered"]]);
		});

		it("plays the dropped earcon when the activation gate drops a turn", async () => {
			polls(status(), status({ droppedTurns: 1 }));
			await store.refreshHandsFree();
			await store.refreshHandsFree();
			expect(playEarcon.mock.calls).toEqual([["dropped"]]);
		});

		it("stays silent while nothing moved, and on the first status it reads", async () => {
			polls(status({ deliveredTurns: 4, droppedTurns: 2 }), status({ deliveredTurns: 4, droppedTurns: 2 }));
			await store.refreshHandsFree();
			await store.refreshHandsFree();
			expect(playEarcon).not.toHaveBeenCalled();
		});

		/** A phone holding the conversation must not make the desktop beep. */
		it("stays silent on a client whose hardware the conversation does not use", async () => {
			polls(status({ owner: "browser-phone" }), status({ owner: "browser-phone", deliveredTurns: 1 }));
			await store.refreshHandsFree();
			await store.refreshHandsFree();
			expect(playEarcon).not.toHaveBeenCalled();
		});

		it("stays silent on the status a disarm returns", async () => {
			polls(status(), status({ armed: false, owner: null, phase: "disarmed", deliveredTurns: 1 }));
			await store.refreshHandsFree();
			await store.refreshHandsFree();
			expect(playEarcon).not.toHaveBeenCalled();
		});

		it("prefers the delivered earcon when both counters moved in one poll", async () => {
			const { turnEarcon } = await import("../../stores/dictation");
			expect(turnEarcon(status(), status({ deliveredTurns: 1, droppedTurns: 1 }), "desktop")).toBe("delivered");
		});

		it("plays on the browser tab that owns the conversation", async () => {
			const browser = await browserMode({
				handsFree: (owner) => [status({ owner }), status({ owner, droppedTurns: 1 })],
			});
			try {
				await browser.store.refreshHandsFree();
				await browser.store.refreshHandsFree();
				expect(playEarcon.mock.calls).toEqual([["dropped"]]);
			} finally {
				browser.restore();
			}
		});

		describe("turned off", () => {
			it("plays nothing for a delivered or a dropped turn", async () => {
				mockInvoke.mockImplementationOnce(() => Promise.resolve({ hands_free_earcons: false }));
				await store.refreshConfig();
				expect(store.state.handsFreeEarcons).toBe(false);
				polls(status(), status({ deliveredTurns: 1 }), status({ deliveredTurns: 1, droppedTurns: 1 }));
				await store.refreshHandsFree();
				await store.refreshHandsFree();
				await store.refreshHandsFree();
				expect(playEarcon).not.toHaveBeenCalled();
			});

			/**
			 * `refreshConfig` is desktop-only, so a browser tab learns the
			 * setting when it arms — the moment the earcons start to matter.
			 */
			it("is read when a browser tab arms", async () => {
				const browser = await browserMode({
					config: { hands_free_earcons: false },
					handsFree: (owner) => [status({ owner }), status({ owner, deliveredTurns: 1 })],
				});
				try {
					expect(await browser.store.armHandsFree("sess-1")).toBe(true);
					expect(browser.store.state.handsFreeEarcons).toBe(false);
					await browser.store.refreshHandsFree();
					await browser.store.refreshHandsFree();
					expect(playEarcon).not.toHaveBeenCalled();
				} finally {
					browser.restore();
				}
			});

			it("saves the choice under hands_free_earcons and plays again once back on", async () => {
				await testInScopeAsync(async () => {
					store.setHandsFreeEarcons(false);
					await vi.waitFor(() => expect(store.state.handsFreeEarcons).toBe(false));
					expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config", {
						base: expect.anything(),
						config: expect.objectContaining({ hands_free_earcons: false }),
					});
					store.setHandsFreeEarcons(true);
					await vi.waitFor(() => expect(store.state.handsFreeEarcons).toBe(true));
				});
				polls(status(), status({ droppedTurns: 1 }));
				await store.refreshHandsFree();
				await store.refreshHandsFree();
				expect(playEarcon.mock.calls).toEqual([["dropped"]]);
			});
		});

		it("primes the audio context inside the arming gesture", async () => {
			mockInvoke.mockImplementation((command: string) =>
				Promise.resolve(command === "arm_hands_free_dictation" ? status() : undefined),
			);
			await testInScopeAsync(async () => {
				expect(await store.armHandsFree("sess-1")).toBe(true);
			});
			expect(primeEarcons).toHaveBeenCalled();
		});
	});

	describe("voice library (855-0948)", () => {
		it("loads the speech volume and levelling into state", async () => {
			mockInvoke.mockImplementation((command: string) =>
				Promise.resolve(
					command === "get_dictation_config" ? { speech_volume_db: -24, speech_levelling: 0.3 } : undefined,
				),
			);
			await testInScopeAsync(async () => {
				await store.refreshConfig();
				expect(store.state.speechVolumeDb).toBe(-24);
				expect(store.state.speechLevelling).toBe(0.3);
			});
		});

		it("saves each loudness setting on its own and reflects it in state", async () => {
			await testInScopeAsync(async () => {
				await store.setSpeechVolumeDb(-21);
				await store.setSpeechLevelling(0.5);
				const sent = mockInvoke.mock.calls
					.filter(([name]) => name === "set_dictation_config")
					.map((call) => (call[1] as { config: Record<string, unknown> }).config);
				expect(sent[0].speech_volume_db).toBe(-21);
				expect(sent[1].speech_levelling).toBe(0.5);
				expect(store.state.speechVolumeDb).toBe(-21);
				expect(store.state.speechLevelling).toBe(0.5);
			});
		});

		// The file travels as base64 so the payload is the same JSON on IPC and
		// HTTP; the name is the file name without its extension.
		it("imports a voice file as base64 under its file name, then re-reads the catalogue", async () => {
			const file = new File([new Uint8Array([1, 2, 3, 250])], "my_voice.safetensors");
			await testInScopeAsync(async () => {
				await store.importSpeechVoice("it", file);
				expect(mockInvoke).toHaveBeenCalledWith("import_speech_voice", {
					language: "it",
					name: "my_voice",
					dataBase64: "AQID+g==",
				});
				expect(mockInvoke).toHaveBeenCalledWith("get_speech_assets");
			});
		});

		it("reports why an import was refused instead of throwing", async () => {
			mockInvoke.mockImplementation((command: string) =>
				command === "import_speech_voice"
					? Promise.reject("the voice file is over the 64 MB limit")
					: Promise.resolve(command === "get_dictation_config" ? {} : undefined),
			);
			await testInScopeAsync(async () => {
				const error = await store.importSpeechVoice("it", new File([new Uint8Array([1])], "big.safetensors"));
				expect(error).toBe("the voice file is over the 64 MB limit");
			});
		});

		it("deletes a user voice by language and name", async () => {
			await testInScopeAsync(async () => {
				await store.deleteSpeechVoice("it", "my_voice");
				expect(mockInvoke).toHaveBeenCalledWith("delete_speech_voice", { language: "it", name: "my_voice" });
			});
		});

		it("lists a language's voices with where each one comes from", async () => {
			const voices = [
				{ id: "giovanni", source: "default" },
				{ id: "jean", source: "downloaded" },
				{ id: "my_voice", source: "user" },
			];
			mockInvoke.mockImplementation((command: string) =>
				Promise.resolve(command === "get_speech_voices" ? voices : command === "get_dictation_config" ? {} : undefined),
			);
			await testInScopeAsync(async () => {
				await store.refreshSpeechVoices("it");
				expect(mockInvoke).toHaveBeenCalledWith("get_speech_voices", { language: "it" });
				expect(store.state.speechVoices).toEqual(voices);
			});
		});

		it("keeps voices for the current language when an older refresh resolves last", async () => {
			let resolveItalian: (voices: { id: string; source: "default" }[]) => void;
			let resolveGerman: (voices: { id: string; source: "default" }[]) => void;
			const italian = new Promise<{ id: string; source: "default" }[]>((resolve) => {
				resolveItalian = resolve;
			});
			const german = new Promise<{ id: string; source: "default" }[]>((resolve) => {
				resolveGerman = resolve;
			});
			mockInvoke.mockImplementation((command: string, args?: { language?: string }) => {
				if (command === "get_speech_voices") return args?.language === "it" ? italian : german;
				return Promise.resolve(command === "get_dictation_config" ? {} : undefined);
			});

			await testInScopeAsync(async () => {
				const first = store.refreshSpeechVoices("it");
				const second = store.refreshSpeechVoices("de");
				resolveGerman!([{ id: "vera", source: "default" }]);
				await second;
				resolveItalian!([{ id: "giovanni", source: "default" }]);
				await first;
				expect(store.state.speechVoices).toEqual([{ id: "vera", source: "default" }]);
			});
		});

		// Listen needs no hands-free conversation: the backend renders the
		// sample with the voice it is given, without changing the saved voice.
		it("previews a voice without saving it", async () => {
			await testInScopeAsync(async () => {
				const error = await store.previewSpeechVoice("it", "jean");
				expect(error).toBeNull();
				expect(mockInvoke).toHaveBeenCalledWith("preview_speech_voice", {
					language: "it",
					voice: "jean",
					text: expect.any(String),
				});
				expect(mockInvoke.mock.calls.some(([name]) => name === "set_dictation_config")).toBe(false);
			});
		});

		it("previews an Italian voice with an Italian sample", async () => {
			await testInScopeAsync(async () => {
				await store.previewSpeechVoice("it", "giovanni");
				expect(mockInvoke).toHaveBeenCalledWith("preview_speech_voice", {
					language: "it",
					voice: "giovanni",
					text: "Questa è la voce delle risposte.",
				});
			});
		});

		it("returns why a preview was refused", async () => {
			mockInvoke.mockImplementation((command: string) =>
				command === "preview_speech_voice"
					? Promise.reject("a reply is being spoken; try again when it ends")
					: Promise.resolve(command === "get_dictation_config" ? {} : undefined),
			);
			await testInScopeAsync(async () => {
				expect(await store.previewSpeechVoice("it", "jean")).toBe("a reply is being spoken; try again when it ends");
			});
		});
	});

	// Edge voices (1357-7d37)
	describe("Edge voices", () => {
		it("keeps the reason when the service cannot be reached instead of an empty list that means nothing", async () => {
			mockInvoke.mockImplementation((command: string) =>
				command === "get_edge_voices"
					? Promise.reject("Cannot load the Microsoft Edge voice list; it needs an internet connection")
					: Promise.resolve(command === "get_dictation_config" ? {} : undefined),
			);
			await testInScopeAsync(async () => {
				await store.refreshEdgeVoices("it");
				expect(store.state.edgeVoices).toEqual([]);
				expect(store.state.edgeVoicesError).toContain("internet connection");
			});
		});

		it("clears an earlier error once the list loads", async () => {
			const voices = [{ id: "it-IT-IsabellaNeural", locale: "it-IT", gender: "Female", label: "Isabella" }];
			let online = false;
			mockInvoke.mockImplementation((command: string) => {
				if (command === "get_edge_voices") return online ? Promise.resolve(voices) : Promise.reject("offline");
				return Promise.resolve(command === "get_dictation_config" ? {} : undefined);
			});
			await testInScopeAsync(async () => {
				await store.refreshEdgeVoices("it");
				online = true;
				await store.refreshEdgeVoices("it");
				expect(store.state.edgeVoices).toEqual(voices);
				expect(store.state.edgeVoicesError).toBeNull();
			});
		});

		it("lets only the latest language request update the picker", async () => {
			let resolveItalian: ((v: unknown) => void) | undefined;
			mockInvoke.mockImplementation((command: string, args?: { language?: string }) => {
				if (command === "get_edge_voices") {
					return args?.language === "it"
						? new Promise((resolve) => {
								resolveItalian = resolve;
							})
						: Promise.resolve([{ id: "de-DE-KatjaNeural", locale: "de-DE", gender: "Female", label: "Katja" }]);
				}
				return Promise.resolve(command === "get_dictation_config" ? {} : undefined);
			});
			await testInScopeAsync(async () => {
				const first = store.refreshEdgeVoices("it");
				await store.refreshEdgeVoices("de");
				resolveItalian?.([{ id: "it-IT-ElsaNeural", locale: "it-IT", gender: "Female", label: "Elsa" }]);
				await first;
				expect(store.state.edgeVoices.map((v) => v.id)).toEqual(["de-DE-KatjaNeural"]);
			});
		});

		it("saves the engine and the Edge voice under their own config keys", async () => {
			let stored: Record<string, unknown> = {};
			mockInvoke.mockImplementation((command: string, args?: { config: Record<string, unknown> }) => {
				if (command === "get_dictation_config") return Promise.resolve(stored);
				if (command === "set_dictation_config" && args) stored = args.config;
				return Promise.resolve(undefined);
			});
			await testInScopeAsync(async () => {
				await store.setSpeechEngine("pocket");
				await store.saveConfig({ speech_edge_voice: "it-IT-ElsaNeural" });
				expect(stored.speech_engine).toBe("pocket");
				expect(stored.speech_edge_voice).toBe("it-IT-ElsaNeural");
				expect(stored.speech_voice).toBeUndefined();
				expect(store.state.speechEngine).toBe("pocket");
				expect(store.state.speechEdgeVoice).toBe("it-IT-ElsaNeural");
			});
		});
	});
});
