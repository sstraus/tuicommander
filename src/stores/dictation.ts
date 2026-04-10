import { createStore } from "solid-js/store";
import { invoke, listen } from "../invoke";
import { appLogger } from "./appLogger";

/** Dictation config persisted to ~/.tuicommander/dictation-config.json */
interface DictationConfig {
  enabled: boolean;
  hotkey: string;
  language: string;
  model: string;
  device: string | null;
  long_press_ms: number;
  auto_send: boolean;
}

/** GPU/CPU backend reported by whisper after model load. */
export type DictationBackend = "cpu" | "gpu";

/** Model info from Rust backend */
export interface ModelInfo {
  name: string;
  display_name: string;
  size_hint_mb: number;
  downloaded: boolean;
  actual_size_mb: number;
}

/** Model status values from Rust backend */
type ModelStatus = "not_downloaded" | "downloaded" | "ready";

/** Model status from Rust backend */
interface DictationStatus {
  model_status: ModelStatus;
  model_name: string;
  model_size_mb: number;
  recording: boolean;
  processing: boolean;
}

/** Transcription response from Rust backend */
interface TranscribeResponse {
  text: string;
  skip_reason: string | null;
  duration_s: number;
}

/** Audio device from Rust backend */
interface AudioDevice {
  name: string;
  is_default: boolean;
}

/** Download progress event payload */
interface DownloadProgress {
  downloaded: number;
  total: number;
  percent: number;
}

/** Supported languages for Whisper */
export const WHISPER_LANGUAGES: Record<string, string> = {
  auto: "Auto-detect",
  en: "English",
  es: "Spanish",
  fr: "French",
  de: "German",
  it: "Italian",
  pt: "Portuguese",
  nl: "Dutch",
  ja: "Japanese",
  zh: "Chinese",
  ko: "Korean",
  ru: "Russian",
};

/** Store state */
interface DictationStoreState {
  enabled: boolean;
  hotkey: string;
  language: string;
  selectedModel: string;
  selectedDevice: string | null;
  models: ModelInfo[];
  modelStatus: ModelStatus;
  modelName: string;
  modelSizeMb: number;
  recording: boolean;
  processing: boolean;
  loading: boolean; // Model is being loaded into memory on first use
  downloading: boolean;
  downloadPercent: number;
  corrections: Record<string, string>;
  devices: AudioDevice[];
  longPressMs: number;
  autoSend: boolean;
  capturingHotkey: boolean;
  partialText: string;
  backendInfo: DictationBackend | null;
}

function createDictationStore() {
  const [state, setState] = createStore<DictationStoreState>({
    enabled: false,
    hotkey: "F5",
    language: "auto",
    selectedModel: "large-v3-turbo",
    selectedDevice: null,
    models: [],
    modelStatus: "not_downloaded",
    modelName: "",
    modelSizeMb: 0,
    recording: false,
    processing: false,
    loading: false,
    downloading: false,
    downloadPercent: 0,
    corrections: {},
    devices: [],
    longPressMs: 400,
    autoSend: false,
    capturingHotkey: false,
    partialText: "",
    backendInfo: null,
  });

  // Listen for download progress events from Rust
  listen<DownloadProgress>("dictation-download-progress", (event) => {
    setState("downloadPercent", event.payload.percent);
  });

  // Listen for streaming partial transcription results
  listen<string>("dictation-partial", (event) => {
    setState("partialText", event.payload);
  });

  // Listen for backend info (gpu/cpu) after model load
  listen<{ backend: DictationBackend }>("dictation-backend-info", (event) => {
    setState("backendInfo", event.payload.backend);
  });

  const actions = {
    /** Load config from Rust backend (file-based) */
    async refreshConfig(): Promise<void> {
      try {
        const config = await invoke<DictationConfig>("get_dictation_config");
        setState({
          enabled: config.enabled,
          hotkey: config.hotkey,
          language: config.language,
          selectedModel: config.model ?? "large-v3-turbo",
          selectedDevice: config.device ?? null,
          longPressMs: config.long_press_ms ?? 400,
          autoSend: config.auto_send ?? false,
        });
      } catch (err) {
        appLogger.error("dictation", "Failed to get dictation config", err);
      }
    },

    /** Save a single config field to disk via Rust */
    async saveConfig(partial: Partial<DictationConfig>): Promise<void> {
      const config: DictationConfig = {
        enabled: partial.enabled ?? state.enabled,
        hotkey: partial.hotkey ?? state.hotkey,
        language: partial.language ?? state.language,
        model: partial.model ?? state.selectedModel,
        device: partial.device !== undefined ? partial.device : state.selectedDevice,
        long_press_ms: partial.long_press_ms ?? state.longPressMs,
        auto_send: partial.auto_send ?? state.autoSend,
      };
      try {
        await invoke("set_dictation_config", { config });
        // Map DictationConfig fields to DictationStoreState fields
        const storeUpdate: Partial<DictationStoreState> = {};
        if (partial.enabled !== undefined) storeUpdate.enabled = partial.enabled;
        if (partial.hotkey !== undefined) storeUpdate.hotkey = partial.hotkey;
        if (partial.language !== undefined) storeUpdate.language = partial.language;
        if (partial.model !== undefined) storeUpdate.selectedModel = partial.model;
        if (partial.device !== undefined) storeUpdate.selectedDevice = partial.device;
        if (partial.long_press_ms !== undefined) storeUpdate.longPressMs = partial.long_press_ms;
        if (partial.auto_send !== undefined) storeUpdate.autoSend = partial.auto_send;
        setState(storeUpdate);
      } catch (err) {
        appLogger.error("dictation", "Failed to save dictation config", err);
      }
    },

    setEnabled(value: boolean): void {
      actions.saveConfig({ enabled: value });
    },

    setHotkey(value: string): void {
      actions.saveConfig({ hotkey: value });
    },

    setCapturingHotkey(value: boolean): void {
      setState("capturingHotkey", value);
    },

    setLongPressMs(value: number): void {
      actions.saveConfig({ long_press_ms: value });
    },

    setAutoSend(value: boolean): void {
      actions.saveConfig({ auto_send: value });
    },

    setLanguage(value: string): void {
      actions.saveConfig({ language: value });
    },

    setDevice(value: string | null): void {
      actions.saveConfig({ device: value });
    },

    /** Refresh status from Rust backend */
    async refreshStatus(): Promise<void> {
      try {
        const status = await invoke<DictationStatus>("get_dictation_status");
        setState({
          modelStatus: status.model_status,
          modelName: status.model_name,
          modelSizeMb: status.model_size_mb,
          recording: status.recording,
          processing: status.processing,
        });
      } catch (err) {
        appLogger.error("dictation", "Failed to get dictation status", err);
      }
    },

    /** Refresh correction map from Rust backend */
    async refreshCorrections(): Promise<void> {
      try {
        const map = await invoke<Record<string, string>>("get_correction_map");
        setState("corrections", map);
      } catch (err) {
        appLogger.error("dictation", "Failed to get correction map", err);
      }
    },

    /** Save correction map to Rust backend */
    async saveCorrections(map: Record<string, string>): Promise<void> {
      try {
        await invoke("set_correction_map", { map });
        setState("corrections", map);
      } catch (err) {
        appLogger.error("dictation", "Failed to save corrections", err);
      }
    },

    /** List available audio devices */
    async refreshDevices(): Promise<void> {
      try {
        const devices = await invoke<AudioDevice[]>("list_audio_devices");
        setState("devices", devices);
      } catch (err) {
        appLogger.error("dictation", "Failed to list audio devices", err);
      }
    },

    /** Fetch available model info from Rust backend */
    async refreshModels(): Promise<void> {
      try {
        const models = await invoke<ModelInfo[]>("get_model_info");
        setState("models", models);
      } catch (err) {
        appLogger.error("dictation", "Failed to get model info", err);
      }
    },

    /** Set the selected model and persist to config */
    async setModel(name: string): Promise<void> {
      await actions.saveConfig({ model: name });
      setState("selectedModel", name);
    },

    /** Delete a downloaded model and refresh the model list */
    async deleteModel(name: string): Promise<void> {
      try {
        await invoke("delete_whisper_model", { modelName: name });
        await actions.refreshModels();
      } catch (err) {
        appLogger.error("dictation", "Failed to delete model", err);
      }
    },

    /** Download a Whisper model (defaults to selectedModel) */
    async downloadModel(modelName?: string): Promise<void> {
      setState("downloading", true);
      setState("downloadPercent", 0);
      try {
        await invoke<string>("download_whisper_model", { modelName: modelName ?? state.selectedModel });
        await actions.refreshStatus();
        await actions.refreshModels();
      } catch (err) {
        appLogger.error("dictation", "Model download failed", err);
      } finally {
        setState("downloading", false);
      }
    },

    /** Start recording (sets loading=true while model initializes on first use) */
    async startRecording(): Promise<void> {
      setState("loading", true);
      try {
        await invoke("start_dictation");
        setState("recording", true);
      } catch (err) {
        const errStr = String(err);
        if (errStr.includes("microphone_denied")) {
          appLogger.error("dictation", "Microphone access denied. Open System Settings > Privacy > Microphone to allow access.");
          invoke("open_microphone_settings").catch(() => {});
        } else if (errStr.includes("microphone_restricted")) {
          appLogger.error("dictation", "Microphone access restricted by system policy");
        } else {
          appLogger.error("dictation", "Failed to start recording", err);
        }
        throw err;
      } finally {
        setState("loading", false);
      }
    },

    /** Stop recording and get transcription result */
    async stopRecording(): Promise<TranscribeResponse | null> {
      try {
        const response = await invoke<TranscribeResponse>("stop_dictation_and_transcribe");
        setState("recording", false);
        setState("processing", false);
        setState("partialText", "");
        return response;
      } catch (err) {
        appLogger.error("dictation", "Failed to stop recording", err);
        setState("recording", false);
        setState("processing", false);
        setState("partialText", "");
        return null;
      }
    },

    /** Inject text (apply corrections) without recording */
    async injectText(text: string): Promise<string | null> {
      try {
        return await invoke<string>("inject_text", { text });
      } catch (err) {
        appLogger.error("dictation", "Failed to inject text", err);
        return null;
      }
    },
  };

  return { state, ...actions };
}

export const dictationStore = createDictationStore();
