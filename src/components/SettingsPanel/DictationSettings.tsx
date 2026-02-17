import { Component, For, Show, createSignal, onMount } from "solid-js";
import { dictationStore, WHISPER_LANGUAGES } from "../../stores/dictation";
import type { ModelInfo } from "../../stores/dictation";

/** Single model row in the model selector list */
const ModelRow: Component<{ model: ModelInfo }> = (props) => {
  const isSelected = () => dictationStore.state.selectedModel === props.model.name;
  const isDownloading = () =>
    dictationStore.state.downloading && dictationStore.state.selectedModel === props.model.name;

  const sizeLabel = () =>
    props.model.downloaded && props.model.actual_size_mb > 0
      ? `${props.model.actual_size_mb} MB`
      : `~${props.model.size_hint_mb} MB`;

  return (
    <div class={`dictation-model-row${isSelected() ? " active" : ""}`}>
      <div class="dictation-model-info">
        <span class="dictation-model-name">{props.model.display_name}</span>
        <span class="dictation-model-size">{sizeLabel()}</span>
      </div>
      <span class={`dictation-model-badge${props.model.downloaded ? " downloaded" : ""}`}>
        {props.model.downloaded ? "Downloaded" : "Not downloaded"}
      </span>
      <div class="dictation-model-actions">
        <Show when={props.model.downloaded && !isSelected()}>
          <button
            class="dictation-model-select"
            onClick={() => dictationStore.setModel(props.model.name)}
          >
            Use
          </button>
        </Show>
        <Show when={props.model.downloaded && isSelected()}>
          <span class="dictation-model-active-label">Active</span>
        </Show>
        <Show when={!props.model.downloaded && !isDownloading()}>
          <button
            class="dictation-model-download"
            onClick={() => dictationStore.downloadModel(props.model.name)}
          >
            Download
          </button>
        </Show>
        <Show when={isDownloading()}>
          <div class="dictation-download-progress">
            <div class="dictation-progress-bar">
              <div
                class="dictation-progress-fill"
                style={{ width: `${dictationStore.state.downloadPercent}%` }}
              />
            </div>
            <span class="dictation-progress-text">
              {dictationStore.state.downloadPercent}%
            </span>
          </div>
        </Show>
        <Show when={props.model.downloaded}>
          <button
            class="dictation-model-delete"
            onClick={() => dictationStore.deleteModel(props.model.name)}
            title="Delete model"
          >
            &times;
          </button>
        </Show>
      </div>
    </div>
  );
};

/** Dictation settings tab for the Settings panel */
export const DictationSettings: Component = () => {
  const [newFrom, setNewFrom] = createSignal("");
  const [newTo, setNewTo] = createSignal("");

  // Load data on mount (skip devices — triggers macOS mic permission dialog)
  onMount(() => {
    dictationStore.refreshConfig();
    dictationStore.refreshStatus();
    dictationStore.refreshCorrections();
    dictationStore.refreshModels();
  });

  const handleAddCorrection = () => {
    const from = newFrom().trim();
    const to = newTo().trim();
    if (!from || !to) return;

    const updated = { ...dictationStore.state.corrections, [from]: to };
    dictationStore.saveCorrections(updated);
    setNewFrom("");
    setNewTo("");
  };

  const handleRemoveCorrection = (key: string) => {
    const updated = { ...dictationStore.state.corrections };
    delete updated[key];
    dictationStore.saveCorrections(updated);
  };

  const handleHotkeyCapture = (e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();

    const key = e.key;

    // Ignore bare modifier presses — wait for a real key
    if (["Meta", "Control", "Alt", "Shift"].includes(key)) return;

    const parts: string[] = [];
    if (e.metaKey) parts.push("Cmd");
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");

    // Normalize key name for Tauri global-shortcut compatibility
    if (key === " ") {
      parts.push("Space");
    } else if (key.length === 1) {
      parts.push(key.toUpperCase());
    } else {
      parts.push(key);
    }

    dictationStore.setHotkey(parts.join("+"));
    dictationStore.setCapturingHotkey(false);
  };

  const handleExportCorrections = () => {
    const json = JSON.stringify(dictationStore.state.corrections, null, 2);
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "dictation-corrections.json";
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleImportCorrections = () => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      try {
        const text = await file.text();
        const map = JSON.parse(text);
        if (typeof map === "object" && map !== null) {
          dictationStore.saveCorrections(map as Record<string, string>);
        }
      } catch {
        console.error("Failed to import corrections file");
      }
    };
    input.click();
  };

  return (
    <div class="settings-section">
      <h3>Voice Dictation</h3>

      {/* Enable toggle */}
      <div class="settings-group">
        <label>Enable Dictation</label>
        <div class="settings-toggle">
          <input
            type="checkbox"
            checked={dictationStore.state.enabled}
            onChange={(e) => dictationStore.setEnabled(e.currentTarget.checked)}
          />
          <span>Enable push-to-talk voice dictation into terminals</span>
        </div>
      </div>

      {/* Model selector */}
      <div class="settings-group">
        <label>Whisper Model</label>
        <p class="settings-hint" style={{ "margin-bottom": "8px" }}>
          Select a model for local transcription. Larger models are more accurate but use more disk space and memory.
        </p>
        <div class="dictation-model-list">
          <For each={dictationStore.state.models}>
            {(model: ModelInfo) => (
              <ModelRow model={model} />
            )}
          </For>
        </div>
      </div>

      {/* Hotkey */}
      <div class="settings-group">
        <label>Push-to-Talk Hotkey</label>
        <div class="dictation-hotkey-row">
          <Show
            when={dictationStore.state.capturingHotkey}
            fallback={
              <button
                class="dictation-hotkey-display"
                onClick={() => dictationStore.setCapturingHotkey(true)}
                title="Click to change hotkey"
              >
                {dictationStore.state.hotkey}
              </button>
            }
          >
            <input
              class="dictation-hotkey-input"
              placeholder="Press a key or combo..."
              onKeyDown={handleHotkeyCapture}
              onBlur={() => dictationStore.setCapturingHotkey(false)}
              ref={(el) => requestAnimationFrame(() => el.focus())}
              readonly
            />
          </Show>
        </div>
        <p class="settings-hint">
          Hold this key to record, release to transcribe and inject into the active terminal
        </p>
      </div>

      {/* Language */}
      <div class="settings-group">
        <label>Language</label>
        <select
          value={dictationStore.state.language}
          onChange={(e) => dictationStore.setLanguage(e.currentTarget.value)}
        >
          <For each={Object.entries(WHISPER_LANGUAGES)}>
            {([value, label]) => <option value={value}>{label}</option>}
          </For>
        </select>
        <p class="settings-hint">
          Language hint for Whisper. Auto-detect works for most cases.
        </p>
      </div>

      {/* Audio devices */}
      <div class="settings-group">
        <label>Microphone</label>
        <Show
          when={dictationStore.state.devices.length > 0}
          fallback={
            <div>
              <button
                class="settings-download-btn"
                onClick={() => dictationStore.refreshDevices()}
                style={{ background: "var(--bg-tertiary)", color: "var(--fg-secondary)", border: "1px solid var(--border)" }}
              >
                Detect Microphones
              </button>
              <p class="settings-hint">
                Click to scan for audio input devices (requires microphone permission)
              </p>
            </div>
          }
        >
          <select disabled>
            <For each={dictationStore.state.devices}>
              {(device) => (
                <option selected={device.is_default}>
                  {device.name}
                  {device.is_default ? " (default)" : ""}
                </option>
              )}
            </For>
          </select>
          <p class="settings-hint">
            Currently using the system default microphone
          </p>
        </Show>
      </div>

      {/* Correction map */}
      <div class="settings-group">
        <label>Text Corrections</label>
        <p class="settings-hint" style={{ "margin-bottom": "8px" }}>
          Automatic replacements applied after transcription (longest match first)
        </p>

        {/* Existing corrections */}
        <Show when={Object.keys(dictationStore.state.corrections).length > 0}>
          <div class="dictation-corrections-table">
            <div class="dictation-corrections-header">
              <span>From</span>
              <span>To</span>
              <span />
            </div>
            <For each={Object.entries(dictationStore.state.corrections)}>
              {([from, to]) => (
                <div class="dictation-corrections-row">
                  <span class="dictation-correction-text">{from}</span>
                  <span class="dictation-correction-text">{to}</span>
                  <button
                    class="dictation-correction-delete"
                    onClick={() => handleRemoveCorrection(from)}
                    title="Remove"
                  >
                    &times;
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>

        {/* Add new correction */}
        <div class="dictation-correction-add">
          <input
            type="text"
            placeholder="From..."
            value={newFrom()}
            onInput={(e) => setNewFrom(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAddCorrection()}
          />
          <span class="dictation-correction-arrow">&rarr;</span>
          <input
            type="text"
            placeholder="To..."
            value={newTo()}
            onInput={(e) => setNewTo(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAddCorrection()}
          />
          <button
            class="dictation-correction-add-btn"
            onClick={handleAddCorrection}
            disabled={!newFrom().trim() || !newTo().trim()}
          >
            Add
          </button>
        </div>

        {/* Import/Export */}
        <div class="settings-actions" style={{ "margin-top": "8px" }}>
          <button onClick={handleImportCorrections}>Import</button>
          <button onClick={handleExportCorrections}>Export</button>
        </div>
      </div>
    </div>
  );
};
