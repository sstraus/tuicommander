import { describe, it, expect, beforeEach, vi } from "vitest";
import "../mocks/tauri";
import { terminalsStore } from "../../stores/terminals";
import { useDictation } from "../../hooks/useDictation";

function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
}

describe("useDictation", () => {
  const mockPty = {
    write: vi.fn().mockResolvedValue(undefined),
  };

  const mockDictationStore = {
    state: {
      enabled: true,
      recording: false,
      processing: false,
      loading: false,
      modelStatus: "ready" as string,
    },
    refreshStatus: vi.fn().mockResolvedValue(undefined),
    startRecording: vi.fn().mockResolvedValue(undefined),
    stopRecording: vi.fn().mockResolvedValue("hello world"),
  };

  const mockSetStatusInfo = vi.fn();
  const mockOpenSettings = vi.fn();

  let dictation: ReturnType<typeof useDictation>;

  beforeEach(() => {
    resetStores();
    vi.clearAllMocks();

    // Reset mock state
    mockDictationStore.state = {
      enabled: true,
      recording: false,
      processing: false,
      loading: false,
      modelStatus: "ready",
    };

    dictation = useDictation({
      pty: mockPty,
      dictation: mockDictationStore,
      setStatusInfo: mockSetStatusInfo,
      openSettings: mockOpenSettings,
    });
  });

  describe("handleDictationStart", () => {
    it("starts recording when enabled and ready", async () => {
      await dictation.handleDictationStart();

      expect(mockDictationStore.refreshStatus).toHaveBeenCalled();
      expect(mockDictationStore.startRecording).toHaveBeenCalled();
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Dictation: recording…");
    });

    it("does nothing when disabled", async () => {
      mockDictationStore.state.enabled = false;

      await dictation.handleDictationStart();

      expect(mockDictationStore.refreshStatus).not.toHaveBeenCalled();
      expect(mockDictationStore.startRecording).not.toHaveBeenCalled();
    });

    it("does nothing when already recording", async () => {
      mockDictationStore.state.recording = true;

      await dictation.handleDictationStart();

      expect(mockDictationStore.startRecording).not.toHaveBeenCalled();
    });

    it("does nothing when processing", async () => {
      mockDictationStore.state.processing = true;

      await dictation.handleDictationStart();

      expect(mockDictationStore.startRecording).not.toHaveBeenCalled();
    });

    it("does nothing when loading", async () => {
      mockDictationStore.state.loading = true;

      await dictation.handleDictationStart();

      expect(mockDictationStore.startRecording).not.toHaveBeenCalled();
    });

    it("opens settings when model not downloaded", async () => {
      mockDictationStore.state.modelStatus = "not_downloaded";

      await dictation.handleDictationStart();

      expect(mockSetStatusInfo).toHaveBeenCalledWith(
        "Dictation: model not downloaded — open Settings > Dictation",
      );
      expect(mockOpenSettings).toHaveBeenCalledWith("dictation");
      expect(mockDictationStore.startRecording).not.toHaveBeenCalled();
    });

    it("shows loading message when model not ready", async () => {
      mockDictationStore.state.modelStatus = "downloading";

      await dictation.handleDictationStart();

      expect(mockSetStatusInfo).toHaveBeenCalledWith(
        "Dictation: loading model into memory (first use takes a moment)…",
      );
      expect(mockDictationStore.startRecording).toHaveBeenCalled();
    });

    it("sets error status when start fails", async () => {
      mockDictationStore.startRecording.mockRejectedValueOnce(new Error("mic denied"));

      await dictation.handleDictationStart();

      expect(mockSetStatusInfo).toHaveBeenCalledWith("Dictation: failed to start recording");
    });
  });

  describe("handleDictationStop", () => {
    it("transcribes and writes to active terminal", async () => {
      mockDictationStore.state.recording = true;

      const id = terminalsStore.add({
        sessionId: "sess-1",
        fontSize: 14,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      await dictation.handleDictationStop();

      expect(mockSetStatusInfo).toHaveBeenCalledWith("Dictation: transcribing…");
      expect(mockDictationStore.stopRecording).toHaveBeenCalled();
      expect(mockPty.write).toHaveBeenCalledWith("sess-1", "hello world");
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Ready");
    });

    it("does nothing when not recording", async () => {
      mockDictationStore.state.recording = false;

      await dictation.handleDictationStop();

      expect(mockDictationStore.stopRecording).not.toHaveBeenCalled();
    });

    it("sets status when no active terminal", async () => {
      mockDictationStore.state.recording = true;

      await dictation.handleDictationStop();

      expect(mockSetStatusInfo).toHaveBeenCalledWith("Dictation: no active terminal");
    });

    it("resets status when transcription is empty", async () => {
      mockDictationStore.state.recording = true;
      mockDictationStore.stopRecording.mockResolvedValueOnce("   ");

      await dictation.handleDictationStop();

      expect(mockPty.write).not.toHaveBeenCalled();
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Ready");
    });

    it("resets status when transcription is null", async () => {
      mockDictationStore.state.recording = true;
      mockDictationStore.stopRecording.mockResolvedValueOnce(null);

      await dictation.handleDictationStop();

      expect(mockPty.write).not.toHaveBeenCalled();
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Ready");
    });

    it("inserts into focused textarea instead of terminal", async () => {
      mockDictationStore.state.recording = true;

      const textarea = document.createElement("textarea");
      textarea.value = "existing ";
      textarea.selectionStart = 9;
      textarea.selectionEnd = 9;
      document.body.appendChild(textarea);
      textarea.focus();

      try {
        await dictation.handleDictationStop();

        expect(textarea.value).toBe("existing hello world");
        expect(mockPty.write).not.toHaveBeenCalled();
        expect(mockSetStatusInfo).toHaveBeenCalledWith("Ready");
      } finally {
        document.body.removeChild(textarea);
      }
    });

    it("inserts into focused input instead of terminal", async () => {
      mockDictationStore.state.recording = true;

      const input = document.createElement("input");
      input.type = "text";
      input.value = "";
      document.body.appendChild(input);
      input.focus();

      try {
        await dictation.handleDictationStop();

        expect(input.value).toBe("hello world");
        expect(mockPty.write).not.toHaveBeenCalled();
        expect(mockSetStatusInfo).toHaveBeenCalledWith("Ready");
      } finally {
        document.body.removeChild(input);
      }
    });

    it("replaces selected text in focused textarea", async () => {
      mockDictationStore.state.recording = true;

      const textarea = document.createElement("textarea");
      textarea.value = "replace THIS please";
      textarea.selectionStart = 8;
      textarea.selectionEnd = 12;
      document.body.appendChild(textarea);
      textarea.focus();

      try {
        await dictation.handleDictationStop();

        expect(textarea.value).toBe("replace hello world please");
        expect(mockPty.write).not.toHaveBeenCalled();
      } finally {
        document.body.removeChild(textarea);
      }
    });
  });
});
