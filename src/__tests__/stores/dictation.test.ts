import { describe, it, expect, vi, beforeEach } from "vitest";
import { mockInvoke } from "../mocks/tauri";
import { createRoot } from "solid-js";

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
      createRoot((dispose) => {
        expect(store.state.enabled).toBe(false);
        expect(store.state.hotkey).toBe("F5");
        expect(store.state.language).toBe("auto");
        expect(store.state.selectedModel).toBe("large-v3-turbo");
        expect(store.state.models).toEqual([]);
        expect(store.state.modelStatus).toBe("not_downloaded");
        expect(store.state.recording).toBe(false);
        expect(store.state.processing).toBe(false);
        expect(store.state.loading).toBe(false);
        expect(store.state.downloading).toBe(false);
        expect(store.state.downloadPercent).toBe(0);
        dispose();
      });
    });
  });

  describe("refreshConfig()", () => {
    it("loads config including model field from backend", async () => {
      mockInvoke.mockResolvedValueOnce({
        enabled: true,
        hotkey: "F6",
        language: "en",
        model: "small",
      });

      await createRoot(async (dispose) => {
        await store.refreshConfig();
        expect(mockInvoke).toHaveBeenCalledWith("get_dictation_config");
        expect(store.state.enabled).toBe(true);
        expect(store.state.hotkey).toBe("F6");
        expect(store.state.language).toBe("en");
        expect(store.state.selectedModel).toBe("small");
        dispose();
      });
    });

    it("keeps default model when config has no model field", async () => {
      mockInvoke.mockResolvedValueOnce({
        enabled: false,
        hotkey: "F5",
        language: "auto",
      });

      await createRoot(async (dispose) => {
        await store.refreshConfig();
        expect(store.state.selectedModel).toBe("large-v3-turbo");
        dispose();
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

      await createRoot(async (dispose) => {
        await store.refreshModels();
        expect(mockInvoke).toHaveBeenCalledWith("get_model_info");
        expect(store.state.models).toEqual(mockModels);
        dispose();
      });
    });

    it("handles backend errors gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("backend down"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.refreshModels();
        expect(store.state.models).toEqual([]);
        expect(consoleSpy).toHaveBeenCalled();
        consoleSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("setModel()", () => {
    it("saves model to config and updates selectedModel", async () => {
      // First call: get_dictation_config returns current config
      mockInvoke.mockResolvedValueOnce(undefined); // set_dictation_config

      await createRoot(async (dispose) => {
        await store.setModel("small");
        expect(store.state.selectedModel).toBe("small");
        // saveConfig is called with model included
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config", {
          config: expect.objectContaining({ model: "small" }),
        });
        dispose();
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

      await createRoot(async (dispose) => {
        await store.deleteModel("small");
        expect(mockInvoke).toHaveBeenCalledWith("delete_whisper_model", { modelName: "small" });
        expect(mockInvoke).toHaveBeenCalledWith("get_model_info");
        expect(store.state.models).toEqual(mockModels);
        dispose();
      });
    });

    it("handles deletion errors gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("file locked"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.deleteModel("small");
        expect(consoleSpy).toHaveBeenCalled();
        consoleSpy.mockRestore();
        dispose();
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

      await createRoot(async (dispose) => {
        await store.downloadModel("small");
        expect(mockInvoke).toHaveBeenCalledWith("download_whisper_model", { modelName: "small" });
        expect(store.state.downloading).toBe(false);
        dispose();
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

      await createRoot(async (dispose) => {
        await store.downloadModel();
        expect(mockInvoke).toHaveBeenCalledWith("download_whisper_model", { modelName: "large-v3-turbo" });
        dispose();
      });
    });

    it("sets downloading state during download", async () => {
      let resolveDownload: (v: string) => void;
      const downloadPromise = new Promise<string>((r) => { resolveDownload = r; });
      mockInvoke.mockReturnValueOnce(downloadPromise);

      await createRoot(async (dispose) => {
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
        dispose();
      });
    });
  });

  describe("startRecording()", () => {
    it("sets loading=true while invoke is pending and clears it after", async () => {
      let resolveStart: () => void;
      const startPromise = new Promise<void>((r) => { resolveStart = r; });
      mockInvoke.mockReturnValueOnce(startPromise);

      await createRoot(async (dispose) => {
        const task = store.startRecording();
        expect(store.state.loading).toBe(true);
        expect(store.state.recording).toBe(false);

        resolveStart!();
        await task;

        expect(store.state.loading).toBe(false);
        expect(store.state.recording).toBe(true);
        dispose();
      });
    });

    it("clears loading on failure and rethrows", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("mic busy"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await expect(store.startRecording()).rejects.toThrow("mic busy");
        expect(store.state.loading).toBe(false);
        expect(store.state.recording).toBe(false);
        consoleSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("saveConfig()", () => {
    it("includes model in config when saving", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        await store.saveConfig({ language: "en" });
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config", {
          config: expect.objectContaining({
            model: "large-v3-turbo",
            language: "en",
          }),
        });
        dispose();
      });
    });

    it("handles save failure gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("disk full"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.saveConfig({ enabled: true });
        expect(consoleSpy).toHaveBeenCalledWith("[dictation]", expect.stringContaining("Failed to save"), expect.anything());
        consoleSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("refreshConfig() error handling", () => {
    it("handles backend error gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("backend down"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.refreshConfig();
        expect(store.state.enabled).toBe(false); // unchanged
        expect(consoleSpy).toHaveBeenCalled();
        consoleSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("stopRecording()", () => {
    it("returns transcribed text on success", async () => {
      mockInvoke.mockResolvedValueOnce("Hello world");

      await createRoot(async (dispose) => {
        const result = await store.stopRecording();
        expect(result).toBe("Hello world");
        expect(store.state.recording).toBe(false);
        expect(store.state.processing).toBe(false);
        expect(mockInvoke).toHaveBeenCalledWith("stop_dictation_and_transcribe");
        dispose();
      });
    });

    it("returns null and resets state on failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("transcription failed"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        const result = await store.stopRecording();
        expect(result).toBeNull();
        expect(store.state.recording).toBe(false);
        expect(store.state.processing).toBe(false);
        consoleSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("injectText()", () => {
    it("returns injected text on success", async () => {
      mockInvoke.mockResolvedValueOnce("corrected text");

      await createRoot(async (dispose) => {
        const result = await store.injectText("raw text");
        expect(result).toBe("corrected text");
        expect(mockInvoke).toHaveBeenCalledWith("inject_text", { text: "raw text" });
        dispose();
      });
    });

    it("returns null on failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("inject failed"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        const result = await store.injectText("raw text");
        expect(result).toBeNull();
        consoleSpy.mockRestore();
        dispose();
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

      await createRoot(async (dispose) => {
        await store.refreshStatus();
        expect(store.state.modelStatus).toBe("ready");
        expect(store.state.modelName).toBe("large-v3-turbo");
        expect(store.state.modelSizeMb).toBe(1620);
        expect(store.state.recording).toBe(true);
        dispose();
      });
    });

    it("handles error gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("failed"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.refreshStatus();
        expect(store.state.modelStatus).toBe("not_downloaded"); // unchanged
        consoleSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("refreshCorrections()", () => {
    it("loads correction map from backend", async () => {
      mockInvoke.mockResolvedValueOnce({ hello: "hi", teh: "the" });

      await createRoot(async (dispose) => {
        await store.refreshCorrections();
        expect(store.state.corrections).toEqual({ hello: "hi", teh: "the" });
        dispose();
      });
    });

    it("handles error gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("failed"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.refreshCorrections();
        expect(store.state.corrections).toEqual({}); // unchanged
        consoleSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("saveCorrections()", () => {
    it("saves corrections to backend", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        await store.saveCorrections({ foo: "bar" });
        expect(mockInvoke).toHaveBeenCalledWith("set_correction_map", { map: { foo: "bar" } });
        expect(store.state.corrections).toEqual({ foo: "bar" });
        dispose();
      });
    });

    it("handles error gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("failed"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.saveCorrections({ foo: "bar" });
        expect(store.state.corrections).toEqual({}); // unchanged
        consoleSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("refreshDevices()", () => {
    it("loads devices from backend", async () => {
      const devices = [{ name: "Default", is_default: true }];
      mockInvoke.mockResolvedValueOnce(devices);

      await createRoot(async (dispose) => {
        await store.refreshDevices();
        expect(store.state.devices).toEqual(devices);
        dispose();
      });
    });

    it("handles error gracefully", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("failed"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.refreshDevices();
        expect(store.state.devices).toEqual([]); // unchanged
        consoleSpy.mockRestore();
        dispose();
      });
    });
  });

  describe("setEnabled()", () => {
    it("saves config with enabled flag", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        store.setEnabled(true);
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config",
          expect.objectContaining({ config: expect.objectContaining({ enabled: true }) }),
        );
        dispose();
      });
    });
  });

  describe("setHotkey()", () => {
    it("saves config with new hotkey", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        store.setHotkey("F8");
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config",
          expect.objectContaining({ config: expect.objectContaining({ hotkey: "F8" }) }),
        );
        dispose();
      });
    });
  });

  describe("setCapturingHotkey()", () => {
    it("sets capturing state", () => {
      createRoot((dispose) => {
        store.setCapturingHotkey(true);
        expect(store.state.capturingHotkey).toBe(true);
        store.setCapturingHotkey(false);
        expect(store.state.capturingHotkey).toBe(false);
        dispose();
      });
    });
  });

  describe("setLanguage()", () => {
    it("saves config with new language", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      await createRoot(async (dispose) => {
        store.setLanguage("fr");
        expect(mockInvoke).toHaveBeenCalledWith("set_dictation_config",
          expect.objectContaining({ config: expect.objectContaining({ language: "fr" }) }),
        );
        dispose();
      });
    });
  });

  describe("downloadModel() error", () => {
    it("clears downloading state on error", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("download failed"));
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      await createRoot(async (dispose) => {
        await store.downloadModel("small");
        expect(store.state.downloading).toBe(false);
        consoleSpy.mockRestore();
        dispose();
      });
    });
  });
});
