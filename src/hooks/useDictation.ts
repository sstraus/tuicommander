import { terminalsStore } from "../stores/terminals";
/** Dependencies injected into useDictation */
export interface DictationDeps {
  pty: {
    write: (sessionId: string, data: string) => Promise<void>;
  };
  dictation: {
    state: {
      enabled: boolean;
      recording: boolean;
      processing: boolean;
      loading: boolean;
      modelStatus: string;
    };
    refreshStatus: () => Promise<void>;
    startRecording: () => Promise<void>;
    stopRecording: () => Promise<string | null>;
  };
  setStatusInfo: (msg: string) => void;
  openSettings: (tab?: string) => void;
}

/** Dictation: start/stop recording, transcription, and terminal injection */
export function useDictation(deps: DictationDeps) {
  const handleDictationStart = async () => {
    if (!deps.dictation.state.enabled) return;
    if (deps.dictation.state.recording || deps.dictation.state.processing || deps.dictation.state.loading) return;

    await deps.dictation.refreshStatus();
    if (deps.dictation.state.modelStatus === "not_downloaded") {
      deps.setStatusInfo("Dictation: model not downloaded — open Settings > Dictation");
      deps.openSettings("dictation");
      return;
    }

    if (deps.dictation.state.modelStatus !== "ready") {
      deps.setStatusInfo("Dictation: loading model into memory (first use takes a moment)…");
    }

    try {
      await deps.dictation.startRecording();
      deps.setStatusInfo("Dictation: recording…");
    } catch (err) {
      console.error("Dictation start failed:", err);
      deps.setStatusInfo("Dictation: failed to start recording");
    }
  };

  const handleDictationStop = async () => {
    if (!deps.dictation.state.recording) return;
    deps.setStatusInfo("Dictation: transcribing…");
    const text = await deps.dictation.stopRecording();
    if (text && text.trim()) {
      const active = terminalsStore.getActive();
      if (active?.sessionId) {
        try {
          await deps.pty.write(active.sessionId, text.trim());
          deps.setStatusInfo("Ready");
          requestAnimationFrame(() => active.ref?.focus());
        } catch (err) {
          console.error("[Dictation] Failed to write to terminal:", err);
          deps.setStatusInfo("Dictation: failed to write to terminal");
        }
      } else {
        deps.setStatusInfo("Dictation: no active terminal");
      }
    } else {
      deps.setStatusInfo("Ready");
    }
  };

  return {
    handleDictationStart,
    handleDictationStop,
  };
}
