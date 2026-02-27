import { describe, it, expect, beforeEach, vi } from "vitest";
import "../mocks/tauri";
import { terminalsStore } from "../../stores/terminals";
import { useDictation } from "../../hooks/useDictation";

function resetStores() {
  for (const id of terminalsStore.getIds()) {
    terminalsStore.remove(id);
  }
}

/** Helper to create a successful TranscribeResponse */
function transcribeOk(text: string) {
  return { text, skip_reason: null, duration_s: 1.5 };
}

/** Helper to create a skipped TranscribeResponse */
function transcribeSkipped(reason: string) {
  return { text: "", skip_reason: reason, duration_s: 0.3 };
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
    stopRecording: vi.fn().mockResolvedValue(transcribeOk("hello world")),
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

    // Default: startRecording sets recording=true (mimics real store behavior)
    mockDictationStore.startRecording.mockImplementation(async () => {
      mockDictationStore.state.recording = true;
    });

    // Default: stopRecording returns success
    mockDictationStore.stopRecording.mockResolvedValue(transcribeOk("hello world"));

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

    it("blocks when Rust reports still processing after refresh", async () => {
      // refreshStatus updates state to show Rust is still processing
      mockDictationStore.refreshStatus.mockImplementationOnce(async () => {
        mockDictationStore.state.processing = true;
      });

      await dictation.handleDictationStart();

      expect(mockDictationStore.startRecording).not.toHaveBeenCalled();
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Dictation: previous session still active");
    });
  });

  describe("handleDictationStop", () => {
    it("transcribes and writes to active terminal", async () => {
      const id = terminalsStore.add({
        sessionId: "sess-1",
        fontSize: 14,
        name: "Test",
        cwd: null,
        awaitingInput: null,
      });
      terminalsStore.setActive(id);

      // Full push-to-talk cycle: start then stop
      await dictation.handleDictationStart();
      await dictation.handleDictationStop();

      expect(mockSetStatusInfo).toHaveBeenCalledWith("Dictation: transcribing…");
      expect(mockDictationStore.stopRecording).toHaveBeenCalled();
      expect(mockPty.write).toHaveBeenCalledWith("sess-1", "hello world");
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Ready");
    });

    it("does nothing when not recording and no pending start", async () => {
      mockDictationStore.state.recording = false;

      await dictation.handleDictationStop();

      expect(mockDictationStore.stopRecording).not.toHaveBeenCalled();
    });

    it("sets status when no active terminal", async () => {
      // Start then stop with no terminal active
      await dictation.handleDictationStart();
      await dictation.handleDictationStop();

      expect(mockSetStatusInfo).toHaveBeenCalledWith("Dictation: no active terminal");
    });

    it("shows skip reason when transcription is skipped", async () => {
      mockDictationStore.stopRecording.mockResolvedValueOnce(
        transcribeSkipped("too short (0.3s, need 0.5s)"),
      );

      await dictation.handleDictationStart();
      await dictation.handleDictationStop();

      expect(mockPty.write).not.toHaveBeenCalled();
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Dictation: too short (0.3s, need 0.5s)");
    });

    it("shows no-text message when text is empty without skip_reason", async () => {
      mockDictationStore.stopRecording.mockResolvedValueOnce(
        transcribeOk("   "),
      );

      await dictation.handleDictationStart();
      await dictation.handleDictationStop();

      expect(mockPty.write).not.toHaveBeenCalled();
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Dictation: no text recognized");
    });

    it("shows failure message when stopRecording returns null", async () => {
      mockDictationStore.stopRecording.mockResolvedValueOnce(null);

      await dictation.handleDictationStart();
      await dictation.handleDictationStop();

      expect(mockPty.write).not.toHaveBeenCalled();
      expect(mockSetStatusInfo).toHaveBeenCalledWith("Dictation: transcription failed");
    });

    it("inserts into focused textarea instead of terminal", async () => {
      const textarea = document.createElement("textarea");
      textarea.value = "existing ";
      textarea.selectionStart = 9;
      textarea.selectionEnd = 9;
      document.body.appendChild(textarea);
      textarea.focus();

      try {
        // Focus captured at start time
        await dictation.handleDictationStart();
        await dictation.handleDictationStop();

        expect(textarea.value).toBe("existing hello world");
        expect(mockPty.write).not.toHaveBeenCalled();
        expect(mockSetStatusInfo).toHaveBeenCalledWith("Ready");
      } finally {
        document.body.removeChild(textarea);
      }
    });

    it("inserts into focused input instead of terminal", async () => {
      const input = document.createElement("input");
      input.type = "text";
      input.value = "";
      document.body.appendChild(input);
      input.focus();

      try {
        await dictation.handleDictationStart();
        await dictation.handleDictationStop();

        expect(input.value).toBe("hello world");
        expect(mockPty.write).not.toHaveBeenCalled();
        expect(mockSetStatusInfo).toHaveBeenCalledWith("Ready");
      } finally {
        document.body.removeChild(input);
      }
    });

    it("replaces selected text in focused textarea", async () => {
      const textarea = document.createElement("textarea");
      textarea.value = "replace THIS please";
      textarea.selectionStart = 8;
      textarea.selectionEnd = 12;
      document.body.appendChild(textarea);
      textarea.focus();

      try {
        await dictation.handleDictationStart();
        await dictation.handleDictationStop();

        expect(textarea.value).toBe("replace hello world please");
        expect(mockPty.write).not.toHaveBeenCalled();
      } finally {
        document.body.removeChild(textarea);
      }
    });
  });

  describe("push-to-talk race condition", () => {
    it("stop waits for slow start before proceeding", async () => {
      // Simulate slow startRecording (model loading, mic init)
      let resolveStart: (() => void) | null = null;
      mockDictationStore.startRecording.mockImplementationOnce(() =>
        new Promise<void>((resolve) => {
          resolveStart = () => {
            mockDictationStore.state.recording = true;
            resolve();
          };
        }),
      );

      // Start fires (key press) — does NOT await (simulates keyboard handler fire-and-forget)
      const startDone = dictation.handleDictationStart();

      // Flush microtasks so start progresses past refreshStatus into startRecording
      await new Promise((r) => setTimeout(r, 0));

      // Stop fires (key release) before start resolves
      const stopDone = dictation.handleDictationStop();

      // Now let start resolve
      expect(resolveStart).not.toBeNull();
      resolveStart!();

      await startDone;
      await stopDone;

      // Stop should have waited for start and then called stopRecording
      expect(mockDictationStore.stopRecording).toHaveBeenCalled();
    });

    it("stop bails when start failed", async () => {
      mockDictationStore.startRecording.mockRejectedValueOnce(new Error("no mic"));

      await dictation.handleDictationStart();
      await dictation.handleDictationStop();

      // Start failed → stop should not call stopRecording
      expect(mockDictationStore.stopRecording).not.toHaveBeenCalled();
    });

    it("captures focus target at start time, not stop time", async () => {
      // Focus a textarea at start time
      const textarea = document.createElement("textarea");
      textarea.value = "";
      document.body.appendChild(textarea);
      textarea.focus();

      let resolveStart: (() => void) | null = null;
      mockDictationStore.startRecording.mockImplementationOnce(() =>
        new Promise<void>((resolve) => {
          resolveStart = () => {
            mockDictationStore.state.recording = true;
            resolve();
          };
        }),
      );

      const startDone = dictation.handleDictationStart();

      // Flush microtasks so start progresses past refreshStatus into startRecording
      await new Promise((r) => setTimeout(r, 0));

      // Focus shifts away before stop
      textarea.blur();

      const stopDone = dictation.handleDictationStop();

      expect(resolveStart).not.toBeNull();
      resolveStart!();

      await startDone;
      await stopDone;

      try {
        // Text should still be inserted into the textarea (captured at start)
        expect(textarea.value).toBe("hello world");
        expect(mockPty.write).not.toHaveBeenCalled();
      } finally {
        document.body.removeChild(textarea);
      }
    });
  });
});
