import { createStore } from "solid-js/store";
import { invoke, listen } from "../invoke";

/** Dictation config persisted to ~/.tuicommander/dictation-config.json */
interface DictationConfig {
  enabled: boolean;
  hotkey: string;
  language: string;
  model: string;
}

/** Model info from Rust backend */
export interface ModelInfo {
  name: string;
  display_name: string;
  size_hint_mb: number;
  downloaded: boolean;
  actual_size_mb: number;
}

/** Model status from Rust backend */
interface DictationStatus {
  model_status: string; // "not_downloaded", "downloaded", "ready"
  model_name: string;
  model_size_mb: number;
  recording: boolean;
  processing: boolean;
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
  models: ModelInfo[];
  modelStatus: string;
  modelName: string;
  modelSizeMb: number;
  recording: boolean;
  processing: boolean;
  loading: boolean; // Model is being loaded into memory on first use
  downloading: boolean;
  downloadPercent: number;
  corrections: Record<string, string>;
  devices: AudioDevice[];
  capturingHotkey: boolean;
}

function createDictationStore() {
  const [state, setState] = createStore<DictationStoreState>({
    enabled: false,
    hotkey: "F5",
    language: "auto",
    selectedModel: "large-v3-turbo",
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
    capturingHotkey: false,
  });

  // Listen for download progress events from Rust
  listen<DownloadProgress>("dictation-download-progress", (event) => {
    setState("downloadPercent", event.payload.percent);
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
        });
      } catch (err) {
        console.error("Failed to get dictation config:", err);
      }
    },

    /** Save a single config field to disk via Rust */
    async saveConfig(partial: Partial<DictationConfig>): Promise<void> {
      const config: DictationConfig = {
        enabled: partial.enabled ?? state.enabled,
        hotkey: partial.hotkey ?? state.hotkey,
        language: partial.language ?? state.language,
        model: partial.model ?? state.selectedModel,
      };
      try {
        await invoke("set_dictation_config", { config });
        setState(partial as Partial<DictationStoreState>);
      } catch (err) {
        console.error("Failed to save dictation config:", err);
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

    setLanguage(value: string): void {
      actions.saveConfig({ language: value });
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
        console.error("Failed to get dictation status:", err);
      }
    },

    /** Refresh correction map from Rust backend */
    async refreshCorrections(): Promise<void> {
      try {
        const map = await invoke<Record<string, string>>("get_correction_map");
        setState("corrections", map);
      } catch (err) {
        console.error("Failed to get correction map:", err);
      }
    },

    /** Save correction map to Rust backend */
    async saveCorrections(map: Record<string, string>): Promise<void> {
      try {
        await invoke("set_correction_map", { map });
        setState("corrections", map);
      } catch (err) {
        console.error("Failed to save corrections:", err);
      }
    },

    /** List available audio devices */
    async refreshDevices(): Promise<void> {
      try {
        const devices = await invoke<AudioDevice[]>("list_audio_devices");
        setState("devices", devices);
      } catch (err) {
        console.error("Failed to list audio devices:", err);
      }
    },

    /** Fetch available model info from Rust backend */
    async refreshModels(): Promise<void> {
      try {
        const models = await invoke<ModelInfo[]>("get_model_info");
        setState("models", models);
      } catch (err) {
        console.error("Failed to get model info:", err);
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
        console.error("Failed to delete model:", err);
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
        console.error("Model download failed:", err);
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
        console.error("Failed to start recording:", err);
      } finally {
        setState("loading", false);
      }
    },

    /** Stop recording and get transcribed text */
    async stopRecording(): Promise<string | null> {
      try {
        const text = await invoke<string>("stop_dictation_and_transcribe");
        setState("recording", false);
        setState("processing", false);
        return text;
      } catch (err) {
        console.error("Failed to stop recording:", err);
        setState("recording", false);
        setState("processing", false);
        return null;
      }
    },

    /** Inject text (apply corrections) without recording */
    async injectText(text: string): Promise<string | null> {
      try {
        return await invoke<string>("inject_text", { text });
      } catch (err) {
        console.error("Failed to inject text:", err);
        return null;
      }
    },
  };

  return { state, ...actions };
}

export const dictationStore = createDictationStore();
