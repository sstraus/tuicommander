import { Component, For, Show, createSignal, onMount } from "solid-js";
import { dictationStore, WHISPER_LANGUAGES } from "../../stores/dictation";
import type { ModelInfo } from "../../stores/dictation";
import { t } from "../../i18n";
import { cx } from "../../utils";
import s from "./Settings.module.css";
import d from "./DictationSettings.module.css";

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
    <div class={cx(d.modelRow, isSelected() && d.active)}>
      <div class={d.modelInfo}>
        <span class={d.modelName}>{props.model.display_name}</span>
        <span class={d.modelSize}>{sizeLabel()}</span>
      </div>
      <Show when={!isDownloading()}>
        <span class={cx(d.modelBadge, props.model.downloaded && d.downloaded)}>
          {props.model.downloaded ? t("dictation.downloaded", "Downloaded") : t("dictation.notDownloaded", "Not Downloaded")}
        </span>
      </Show>
      <div class={d.modelActions}>
        <Show when={props.model.downloaded && !isSelected()}>
          <button
            class={d.modelSelect}
            onClick={() => dictationStore.setModel(props.model.name)}
          >
            {t("dictation.use", "Use")}
          </button>
        </Show>
        <Show when={props.model.downloaded && isSelected()}>
          <span class={d.modelActiveLabel}>{t("dictation.active", "Active")}</span>
        </Show>
        <Show when={!props.model.downloaded && !isDownloading()}>
          <button
            class={d.modelDownload}
            onClick={() => dictationStore.downloadModel(props.model.name)}
          >
            {t("dictation.download", "Download")}
          </button>
        </Show>
        <Show when={isDownloading()}>
          <div class={d.downloadProgress}>
            <div class={d.progressBar}>
              <div
                class={d.progressFill}
                style={{ width: `${dictationStore.state.downloadPercent}%` }}
              />
            </div>
            <span class={d.progressText}>
              {dictationStore.state.downloadPercent}%
            </span>
          </div>
        </Show>
        <Show when={props.model.downloaded}>
          <button
            class={d.modelDelete}
            onClick={() => dictationStore.deleteModel(props.model.name)}
            title={t("dictation.deleteModel", "Delete model")}
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
    <div class={s.section}>
      <h3>{t("dictation.title", "Dictation Settings")}</h3>

      {/* Enable toggle */}
      <div class={s.group}>
        <label>{t("dictation.enableLabel", "Enable Dictation")}</label>
        <div class={s.toggle}>
          <input
            type="checkbox"
            checked={dictationStore.state.enabled}
            onChange={(e) => dictationStore.setEnabled(e.currentTarget.checked)}
          />
          <span>{t("dictation.enableHint", "Enable voice-to-text dictation")}</span>
        </div>
      </div>

      {/* Model selector */}
      <div class={s.group}>
        <label>{t("dictation.modelLabel", "Whisper Model")}</label>
        <p class={s.hint} style={{ "margin-bottom": "8px" }}>
          {t("dictation.modelHint", "Choose a model. Larger models are more accurate but slower.")}
        </p>
        <div class={d.modelList}>
          <For each={dictationStore.state.models}>
            {(model: ModelInfo) => (
              <ModelRow model={model} />
            )}
          </For>
        </div>
      </div>

      {/* Hotkey */}
      <div class={s.group}>
        <label>{t("dictation.hotkeyLabel", "Hotkey")}</label>
        <div class={d.hotkeyRow}>
          <Show
            when={dictationStore.state.capturingHotkey}
            fallback={
              <button
                class={d.hotkeyDisplay}
                onClick={() => dictationStore.setCapturingHotkey(true)}
                title={t("dictation.hotkeyChangeTitle", "Click to change hotkey")}
              >
                {dictationStore.state.hotkey}
              </button>
            }
          >
            <input
              class={d.hotkeyInput}
              placeholder={t("dictation.hotkeyPlaceholder", "Press a key combination...")}
              onKeyDown={handleHotkeyCapture}
              onBlur={() => dictationStore.setCapturingHotkey(false)}
              ref={(el) => requestAnimationFrame(() => el.focus())}
              readonly
            />
          </Show>
        </div>
        <p class={s.hint}>
          {t("dictation.hotkeyHint", "Press to start/stop recording. Works globally.")}
        </p>
      </div>

      {/* Language */}
      <div class={s.group}>
        <label>{t("dictation.languageLabel", "Language")}</label>
        <select
          value={dictationStore.state.language}
          onChange={(e) => dictationStore.setLanguage(e.currentTarget.value)}
        >
          <For each={Object.entries(WHISPER_LANGUAGES)}>
            {([value, label]) => <option value={value}>{label}</option>}
          </For>
        </select>
        <p class={s.hint}>
          {t("dictation.languageHint", "Auto-detect works well for most languages.")}
        </p>
      </div>

      {/* Audio devices */}
      <div class={s.group}>
        <label>{t("dictation.microphoneLabel", "Microphone")}</label>
        <Show
          when={dictationStore.state.devices.length > 0}
          fallback={
            <div>
              <button
                class={s.downloadBtn}
                onClick={() => dictationStore.refreshDevices()}
                style={{ background: "var(--bg-tertiary)", color: "var(--fg-secondary)", border: "1px solid var(--border)" }}
              >
                {t("dictation.detectMicrophones", "Detect Microphones")}
              </button>
              <p class={s.hint}>
                {t("dictation.detectMicrophonesHint", "Triggers macOS microphone permission dialog.")}
              </p>
            </div>
          }
        >
          <select disabled>
            <For each={dictationStore.state.devices}>
              {(device) => (
                <option selected={device.is_default}>
                  {device.name}
                  {device.is_default ? ` ${t("dictation.defaultDevice", "(Default)")}` : ""}
                </option>
              )}
            </For>
          </select>
          <p class={s.hint}>
            {t("dictation.microphoneHint", "The default input device is used.")}
          </p>
        </Show>
      </div>

      {/* Correction map */}
      <div class={s.group}>
        <label>{t("dictation.correctionsLabel", "Auto-Corrections")}</label>
        <p class={s.hint} style={{ "margin-bottom": "8px" }}>
          {t("dictation.correctionsHint", "Automatically replace dictation output. Useful for technical terms.")}
        </p>

        {/* Existing corrections */}
        <Show when={Object.keys(dictationStore.state.corrections).length > 0}>
          <div class={d.correctionsTable}>
            <div class={d.correctionsHeader}>
              <span>{t("dictation.correctionsFrom", "From")}</span>
              <span>{t("dictation.correctionsTo", "To")}</span>
              <span />
            </div>
            <For each={Object.entries(dictationStore.state.corrections)}>
              {([from, to]) => (
                <div class={d.correctionsRow}>
                  <span class={d.correctionText}>{from}</span>
                  <span class={d.correctionText}>{to}</span>
                  <button
                    class={d.correctionDelete}
                    onClick={() => handleRemoveCorrection(from)}
                    title={t("dictation.removeCorrection", "Remove correction")}
                  >
                    &times;
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>

        {/* Add new correction */}
        <div class={d.correctionAdd}>
          <input
            type="text"
            placeholder={t("dictation.correctionFromPlaceholder", "Heard text...")}
            value={newFrom()}
            onInput={(e) => setNewFrom(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAddCorrection()}
          />
          <span class={d.correctionArrow}>&rarr;</span>
          <input
            type="text"
            placeholder={t("dictation.correctionToPlaceholder", "Replace with...")}
            value={newTo()}
            onInput={(e) => setNewTo(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAddCorrection()}
          />
          <button
            class={d.correctionAddBtn}
            onClick={handleAddCorrection}
            disabled={!newFrom().trim() || !newTo().trim()}
          >
            {t("dictation.addCorrection", "Add")}
          </button>
        </div>

        {/* Import/Export */}
        <div class={s.actions} style={{ "margin-top": "8px" }}>
          <button onClick={handleImportCorrections}>{t("dictation.import", "Import")}</button>
          <button onClick={handleExportCorrections}>{t("dictation.export", "Export")}</button>
        </div>
      </div>
    </div>
  );
};
