import { terminalsStore } from "../stores/terminals";
import { appLogger } from "../stores/appLogger";
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
  // Track the in-flight start promise so stop can await it.
  // Without this, a fast key release can fire handleDictationStop before
  // startRecording resolves, causing it to see recording=false and bail.
  let startPromise: Promise<boolean> | null = null;

  // Capture the focused element at key-press time. During push-to-talk the
  // focus may shift (e.g. to the terminal) by the time the key is released,
  // so we snapshot it here to guarantee dictation targets the right element.
  let focusTarget: Element | null = null;

  const handleDictationStart = async () => {
    if (!deps.dictation.state.enabled) return;
    if (deps.dictation.state.recording || deps.dictation.state.processing || deps.dictation.state.loading) return;

    // Snapshot focus target before any async work
    focusTarget = document.activeElement;

    startPromise = (async () => {
      await deps.dictation.refreshStatus();
      if (deps.dictation.state.modelStatus === "not_downloaded") {
        deps.setStatusInfo("Dictation: model not downloaded — open Settings > Dictation");
        deps.openSettings("dictation");
        return false;
      }

      if (deps.dictation.state.modelStatus !== "ready") {
        deps.setStatusInfo("Dictation: loading model into memory (first use takes a moment)…");
      }

      try {
        await deps.dictation.startRecording();
        deps.setStatusInfo("Dictation: recording…");
        return true;
      } catch (err) {
        appLogger.error("dictation", "Dictation start failed", err);
        deps.setStatusInfo("Dictation: failed to start recording");
        return false;
      }
    })();

    await startPromise;
  };

  const handleDictationStop = async () => {
    // Wait for any in-flight start to finish before checking state
    if (startPromise) {
      const started = await startPromise;
      startPromise = null;
      if (!started) return;
    }

    if (!deps.dictation.state.recording) return;
    deps.setStatusInfo("Dictation: transcribing…");
    const text = await deps.dictation.stopRecording();
    if (text && text.trim()) {
      // Use the focus target captured at key-press time
      const el = focusTarget;
      focusTarget = null;
      if (el && (el instanceof HTMLTextAreaElement || el instanceof HTMLInputElement)) {
        const start = el.selectionStart ?? el.value.length;
        const end = el.selectionEnd ?? start;
        const before = el.value.slice(0, start);
        const after = el.value.slice(end);
        el.value = before + text.trim() + after;
        el.selectionStart = el.selectionEnd = start + text.trim().length;
        el.dispatchEvent(new Event("input", { bubbles: true }));
        deps.setStatusInfo("Ready");
        return;
      }
      if (el && el.getAttribute("contenteditable") === "true") {
        document.execCommand("insertText", false, text.trim());
        deps.setStatusInfo("Ready");
        return;
      }

      const active = terminalsStore.getActive();
      if (active?.sessionId) {
        try {
          await deps.pty.write(active.sessionId, text.trim());
          deps.setStatusInfo("Ready");
          requestAnimationFrame(() => active.ref?.focus());
        } catch (err) {
          appLogger.error("dictation", "Failed to write to terminal", err);
          deps.setStatusInfo("Dictation: failed to write to terminal");
        }
      } else {
        deps.setStatusInfo("Dictation: no active terminal");
      }
    } else {
      focusTarget = null;
      deps.setStatusInfo("Ready");
    }
  };

  return {
    handleDictationStart,
    handleDictationStop,
  };
}
