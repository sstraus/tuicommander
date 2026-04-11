import { describe, it, expect, vi, beforeEach } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import { testInScope, testInScopeAsync } from "../helpers/store";

describe("dictationStore", () => {
  let store: typeof import("../../stores/dictation").dictationStore;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(undefined);
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
  });

  describe("refreshModels()", () => {
    it("fetches model info from backend", async () => {
      const mockModels = [
        { name: "small", display_name: "Whisper Small", size_hint_mb: 488, downloaded: false, actual_size_mb: 0 },
        { name: "large-v3-turbo", display_name: "Whisper Large V3 Turbo", size_hint_mb: 1620, downloaded: true, actual_size_mb: 1620 },
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
      // First call: get_dictation_config returns current config
      mockInvoke.mockResolvedValueOnce(undefined); // set_dictation_config

      await testInScopeAsync(async () => {
        await store.setModel("small");
        expect(store.state.selectedModel).toBe("small");
        // saveConfig is called with model included
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config", {
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
        .mockResolvedValueOnce({ // get_dictation_status (from refreshStatus)
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
      const downloadPromise = new Promise<string>((r) => { resolveDownload = r; });
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
      const startPromise = new Promise<void>((r) => { resolveStart = r; });
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
  });

  describe("saveConfig()", () => {
    it("includes model and device in config when saving", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        await store.saveConfig({ language: "en" });
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config", {
          config: expect.objectContaining({
            model: "large-v3-turbo",
            language: "en",
            device: null,
          }),
        });
      });
    });

    it("handles save failure gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("disk full"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await testInScopeAsync(async () => {
        await store.saveConfig({ enabled: true });
        expect(consoleSpy).toHaveBeenCalledWith("[dictation]", expect.stringContaining("Failed to save"), expect.anything());
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
        .mockResolvedValueOnce(undefined)  // start_dictation
        .mockResolvedValueOnce({           // stop_dictation_and_transcribe
          text: "Hello world",
          skip_reason: null,
          duration_s: 2.5,
        });

      await testInScopeAsync(async () => {
        await store.startRecording();
        expect(store.state.recording).toBe(true);

        const result = await store.stopRecording();
        expect(result).toEqual({
          text: "Hello world",
          skip_reason: null,
          duration_s: 2.5,
        });
        expect(store.state.recording).toBe(false);
        expect(store.state.processing).toBe(false);
        expect(store.state.partialText).toBe("");
        expect(mockInvoke).toHaveBeenCalledWith("stop_dictation_and_transcribe");
      });
    });

    it("returns null and resets state on failure", async () => {
      mockInvoke
        .mockResolvedValueOnce(undefined)  // start_dictation
        .mockRejectedValueOnce(new Error("transcription failed"));  // stop
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
      mockInvoke.mockResolvedValueOnce(undefined);

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

    it("handles error gracefully", async () => {
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
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        store.setEnabled(true);
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config",
          expect.objectContaining({ config: expect.objectContaining({ enabled: true }) }),
        );
      });
    });
  });

  describe("setHotkey()", () => {
    it("saves config with new hotkey", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        store.setHotkey("F8");
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config",
          expect.objectContaining({ config: expect.objectContaining({ hotkey: "F8" }) }),
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
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        store.setLanguage("fr");
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config",
          expect.objectContaining({ config: expect.objectContaining({ language: "fr" }) }),
        );
      });
    });
  });

  describe("setDevice()", () => {
    it("saves config with specific device", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        store.setDevice("USB Microphone");
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config",
          expect.objectContaining({ config: expect.objectContaining({ device: "USB Microphone" }) }),
        );
      });
    });

    it("saves null device to use system default", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      await testInScopeAsync(async () => {
        store.setDevice(null);
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config",
          expect.objectContaining({ config: expect.objectContaining({ device: null }) }),
        );
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
});
