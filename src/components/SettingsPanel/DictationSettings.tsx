import { type Component, createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import type { ModelInfo, SpeechAsset, SpeechEngineId } from "../../stores/dictation";
import { dictationStore, WHISPER_LANGUAGES } from "../../stores/dictation";
import { settingsExpertStore } from "../../stores/settingsExpert";
import { terminalsStore } from "../../stores/terminals";
import { isTauri } from "../../transport";
import { cx } from "../../utils";
import { handsFreePhaseLabel } from "../DictationToast/handsFreePhaseLabel";
import { KeyComboCapture } from "../shared/KeyComboCapture";
import d from "./DictationSettings.module.css";
import { ExpertSetting } from "./ExpertSetting";
import { SettingSlider } from "./SettingFields";
import s from "./Settings.module.css";

/**
 * Mirrors `audio.rs`: `meter_level = sqrt(rms * 20)`.
 *
 * The gate is a raw RMS and the meter is that curve, so a threshold shown in raw
 * units cannot be compared against the bar the user is watching. Both are drawn
 * on the meter's scale instead, and only converted back on save.
 */
const RMS_METER_SCALE = 20;

function rmsToMeter(rms: number): number {
	return Math.min(1, Math.sqrt(Math.max(0, rms) * RMS_METER_SCALE));
}

function meterToRms(meter: number): number {
	return (meter * meter) / RMS_METER_SCALE;
}

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
					{props.model.downloaded
						? t("dictation.downloaded", "Downloaded")
						: t("dictation.notDownloaded", "Not Downloaded")}
				</span>
			</Show>
			<div class={d.modelActions}>
				<Show when={props.model.downloaded && !isSelected()}>
					<button class={d.modelSelect} onClick={() => dictationStore.setModel(props.model.name)}>
						{t("dictation.use", "Use")}
					</button>
				</Show>
				<Show when={props.model.downloaded && isSelected()}>
					<span class={d.modelActiveLabel}>{t("dictation.active", "Active")}</span>
				</Show>
				<Show when={!props.model.downloaded && !isDownloading()}>
					<button class={d.modelDownload} onClick={() => dictationStore.downloadModel(props.model.name)}>
						{t("dictation.download", "Download")}
					</button>
				</Show>
				<Show when={isDownloading()}>
					<div class={d.downloadProgress}>
						<div class={d.progressBar}>
							<div
								class={d.progressFill}
								style={{ transform: `scaleX(${dictationStore.state.downloadPercent / 100})` }}
							/>
						</div>
						<span class={d.progressText}>{dictationStore.state.downloadPercent}%</span>
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

	// Load data on mount. Auto-detect devices only if mic is already authorized
	// (avoids triggering the macOS TCC permission dialog unexpectedly).
	onMount(async () => {
		dictationStore.refreshConfig();
		dictationStore.refreshStatus();
		dictationStore.refreshCorrections();
		dictationStore.refreshModels();
		try {
			const perm = await invoke<string>("check_microphone_permission");
			if (perm === "authorized") {
				dictationStore.refreshDevices();
			} else if (perm === "denied" || perm === "restricted") {
				appLogger.warn(
					"dictation",
					`Microphone access ${perm} — grant permission in System Settings > Privacy > Microphone`,
				);
			}
		} catch {
			appLogger.warn("dictation", "Failed to check microphone permission");
		}
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
				appLogger.error("dictation", "Failed to import corrections file");
			}
		};
		input.click();
	};

	// Speech-to-text first, text-to-speech last, never mixed: each section
	// keeps its own advanced controls at its bottom instead of pooling them in
	// a shared "Advanced" section. Every section component opens with its own
	// h3 heading — see `SpeechRecognition` for why that matters.
	return (
		<div class={s.section}>
			<h3>{t("dictation.heading.dictation", "Dictation")}</h3>

			<Show when={dictationStore.state.ownedElsewhere}>
				<p class={s.hint}>
					{t(
						"dictation.ownedElsewhere",
						"Dictation is owned by another TUICommander instance on this configuration. Use it there; this instance does not register the global hotkey, watch the Fn key or open the microphone. Restart this instance after the other one quits to take dictation over.",
					)}
				</p>
			</Show>

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

			{/* Hotkey — a global hotkey belongs to the machine running
			    TUICommander, so a browser tab has none to configure. */}
			<Show when={isTauri()}>
				<div class={s.group}>
					<label>{t("dictation.hotkeyLabel", "Hotkey")}</label>
					<div class={d.hotkeyRow}>
						<KeyComboCapture
							value={dictationStore.state.hotkey}
							onChange={(combo) => dictationStore.setHotkey(combo)}
							placeholder={t("dictation.hotkeyPlaceholder", "Press a key combination...")}
							onCapturingChange={(capturing) => dictationStore.setCapturingHotkey(capturing)}
						/>
					</div>
					<p class={s.hint}>
						{t(
							"dictation.hotkeyHint",
							"Hold the hotkey to start recording, release to stop. Short presses pass through as normal input.",
						)}
					</p>
				</div>

				<ExpertSetting configKey="dictation.long_press_ms" value={dictationStore.state.longPressMs}>
					<SettingSlider
						label={t("dictation.longPressLabel", "Long-press threshold")}
						value={dictationStore.state.longPressMs}
						onChange={(v) => dictationStore.setLongPressMs(v)}
						min={0}
						max={1000}
						step={50}
						formatValue={(v) => (v === 0 ? t("dictation.instant", "Instant") : `${v}ms`)}
						hint={t(
							"dictation.longPressHint",
							"How long to hold the key before dictation starts. 0 = instant (no short-press pass-through), higher = fewer accidental triggers.",
						)}
					/>
				</ExpertSetting>
			</Show>

			<ExpertSetting configKey="dictation.auto_send" value={dictationStore.state.autoSend}>
				<div class={s.group}>
					<label>{t("dictation.autoSendLabel", "Auto-send")}</label>
					<div class={s.toggle}>
						<input
							type="checkbox"
							checked={dictationStore.state.autoSend}
							onChange={(e) => dictationStore.setAutoSend(e.currentTarget.checked)}
						/>
						<span>{t("dictation.autoSendHint", "Automatically press Enter after inserting transcribed text")}</span>
					</div>
				</div>
			</ExpertSetting>

			<SpeechRecognition />

			<h3>{t("dictation.heading.corrections", "Auto-Corrections")}</h3>
			<p class={cx(s.hint, d.intro)}>
				{t("dictation.correctionsHint", "Automatically replace dictation output. Useful for technical terms.")}
			</p>

			<div class={s.group}>
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

				<div class={d.controlRow}>
					<button class={s.testBtn} onClick={handleImportCorrections}>
						{t("dictation.import", "Import")}
					</button>
					<button class={s.testBtn} onClick={handleExportCorrections}>
						{t("dictation.export", "Export")}
					</button>
				</div>
			</div>

			<HandsFreeControls />

			<SpeechSetup />
		</div>
	);
};

/**
 * Speech-to-text: the microphone, the Whisper model and language, and — last,
 * as this section's advanced part — the live harness for the two speech gates.
 *
 * Opens with its own h3 heading, as do `HandsFreeControls` and `SpeechSetup`.
 * `extractSettings` builds the settings search index from source order and
 * assigns each label to the nearest preceding h3 heading; it does not follow the
 * render tree. A sub-component without a heading of its own would have its
 * labels filed under whichever heading happens to precede its definition —
 * or, defined above the panel, under none, which makes them unreachable from
 * settings search.
 *
 * The test recording reports the transcript back into the panel instead of
 * typing it into a terminal: tuning a gate means seeing what it rejected, and a
 * threshold that swallows speech is indistinguishable from a dead microphone
 * until the skip reason is on screen.
 */
const SpeechRecognition: Component = () => {
	const [testText, setTestText] = createSignal<string | null>(null);

	const recording = () => dictationStore.state.recording;
	const thresholdPercent = () => rmsToMeter(dictationStore.state.rmsThreshold) * 100;
	const levelPercent = () => dictationStore.state.audioLevel * 100;

	/** Replies follow this language, so one with no speech bundle stays silent
	 *  under Pocket TTS. Edge covers about seventy languages and the external
	 *  command picks its own, so only Pocket is limited by the bundles.
	 *  Auto has no fixed language, and an empty catalogue has not loaded yet. */
	const isVoiceless = (code: string): boolean => {
		const assets = dictationStore.state.speechAssets;
		return (
			dictationStore.state.speechEngine === "pocket" &&
			code !== "auto" &&
			assets.length > 0 &&
			!assets.some((asset) => asset.language === code)
		);
	};

	const toggleTest = async () => {
		if (recording()) {
			const result = await dictationStore.stopRecording();
			setTestText(result?.text.trim() || null);
			return;
		}
		setTestText(null);
		try {
			await dictationStore.startRecording();
		} catch {
			// startRecording logs the failure; lastSkipReason covers the rest.
		}
	};

	return (
		<>
			<h3>{t("dictation.heading.recognition", "Speech recognition")}</h3>

			{/* Audio devices — this list is the *server's* hardware. A browser
			    captures from its own device, chosen by the browser's own
			    permission prompt, so offering these names there would let a
			    user pick a microphone in another building. */}
			<Show when={isTauri()}>
				<ExpertSetting configKey="dictation.device" value={dictationStore.state.selectedDevice}>
					<div class={s.group}>
						<label>{t("dictation.inputDeviceLabel", "Input device")}</label>
						<Show
							when={dictationStore.state.devices.length > 0}
							fallback={
								<div>
									<button class={s.testBtn} onClick={() => dictationStore.refreshDevices()}>
										{t("dictation.detectMicrophones", "Detect Microphones")}
									</button>
									<p class={s.hint}>
										{t("dictation.detectMicrophonesHint", "Triggers macOS microphone permission dialog.")}
									</p>
								</div>
							}
						>
							<select
								value={dictationStore.state.selectedDevice ?? ""}
								onChange={(e) => {
									const val = e.currentTarget.value;
									dictationStore.setDevice(val === "" ? null : val);
								}}
							>
								<option value="">{t("dictation.systemDefault", "System Default")}</option>
								<For each={dictationStore.state.devices}>
									{(device) => <option value={device.name}>{device.name}</option>}
								</For>
							</select>
							<p class={s.hint}>{t("dictation.microphoneHint", "Select the input device to use for dictation.")}</p>
						</Show>
					</div>
				</ExpertSetting>
			</Show>

			<div class={s.group}>
				<label>{t("dictation.modelLabel", "Whisper Model")}</label>
				<p class={cx(s.hint, d.intro)}>
					{t("dictation.modelHint", "Choose a model. Larger models are more accurate but slower.")}
				</p>
				<div class={d.modelList}>
					<For each={dictationStore.state.models}>{(model: ModelInfo) => <ModelRow model={model} />}</For>
				</div>
			</div>

			<div class={s.group}>
				<label>{t("dictation.languageLabel", "Language")}</label>
				<select
					value={dictationStore.state.language}
					onChange={(e) => dictationStore.setLanguage(e.currentTarget.value)}
				>
					<For each={Object.entries(WHISPER_LANGUAGES)}>
						{([value, label]) => (
							<option value={value}>
								{isVoiceless(value)
									? t("dictation.languageNoVoice", "{lang} — no spoken replies").replace("{lang}", label)
									: label}
							</option>
						)}
					</For>
				</select>
				<p class={s.hint}>{t("dictation.languageHint", "Auto-detect works well for most languages.")}</p>
				<Show when={isVoiceless(dictationStore.state.language)}>
					<p class={s.hint}>
						{t(
							"dictation.languageNoVoiceHint",
							"Replies in this language will not be spoken. Whisper also expects you to speak it — if you talk in another language, choose it or Auto-detect.",
						)}
					</p>
				</Show>
			</div>

			{/* This section's advanced part: only needed once speech is cut or
			    noise gets through. */}
			<div class={s.group}>
				<label>{t("dictation.tuningLabel", "Voice tuning")}</label>
				<p class={cx(s.hint, d.intro)}>
					{t(
						"dictation.tuningHint",
						"Record a test phrase and watch where your voice sits against the gates. Text stays in this panel — nothing is sent to a terminal.",
					)}
				</p>
				<div class={d.tuningMeter}>
					<div class={d.tuningLevel} style={{ transform: `scaleX(${dictationStore.state.audioLevel})` }} />
					<div
						class={d.tuningThreshold}
						style={{ left: `${thresholdPercent()}%` }}
						title={t("dictation.tuningThresholdMarker", "Level gate")}
					/>
				</div>
				<div class={d.tuningReadout}>
					<span>
						{t("dictation.tuningLevelReadout", "Level")}: {Math.round(levelPercent())}%
					</span>
					<span>
						{t("dictation.tuningGateReadout", "Gate")}: {Math.round(thresholdPercent())}%
					</span>
				</div>

				<div class={d.controlRow}>
					<button class={s.testBtn} onClick={toggleTest} disabled={dictationStore.state.processing}>
						{recording() ? t("dictation.tuningStop", "Stop test") : t("dictation.tuningStart", "Start test recording")}
					</button>
				</div>

				<Show when={dictationStore.state.partialText}>
					<p class={d.tuningPartial}>{dictationStore.state.partialText}</p>
				</Show>
				<Show when={testText()}>
					<p class={d.tuningResult}>{testText()}</p>
				</Show>
				<Show when={dictationStore.state.lastSkipReason}>
					<p class={d.tuningSkip}>
						{t("dictation.tuningSkipped", "Rejected")}: {dictationStore.state.lastSkipReason}
					</p>
				</Show>
			</div>

			<ExpertSetting configKey="dictation.rms_threshold" value={dictationStore.state.rmsThreshold}>
				<SettingSlider
					label={t("dictation.rmsLabel", "Level gate")}
					value={Math.round(thresholdPercent())}
					onChange={(v) => dictationStore.setRmsThreshold(meterToRms(v / 100))}
					min={0}
					max={50}
					step={1}
					formatValue={(v) => `${v}%`}
					hint={t(
						"dictation.rmsHint",
						"Audio quieter than this never reaches Whisper. Raise it until room noise stays below the marker; lower it if quiet speech is rejected.",
					)}
				/>
			</ExpertSetting>

			<ExpertSetting configKey="dictation.no_speech_threshold" value={dictationStore.state.noSpeechThreshold}>
				<SettingSlider
					label={t("dictation.noSpeechLabel", "Speech confidence gate")}
					value={Math.round(dictationStore.state.noSpeechThreshold * 100)}
					onChange={(v) => dictationStore.setNoSpeechThreshold(v / 100)}
					min={10}
					max={100}
					step={5}
					formatValue={(v) => (v === 100 ? t("dictation.off", "Off") : `${v}%`)}
					hint={t(
						"dictation.noSpeechHint",
						"Discards a transcript when Whisper itself reports it probably heard no speech. Lower is stricter; 100% turns the gate off.",
					)}
				/>
			</ExpertSetting>
		</>
	);
};

/** Bytes as the megabytes a download dialog would quote. */
function megabytes(bytes: number): string {
	return `${Math.round(bytes / 1_000_000)} MB`;
}

/**
 * Setting up the voice that speaks replies back.
 *
 * Carries its own section heading for the same reason as `SpeechRecognition`: the settings
 * search index is built from source order and assigns each label to the
 * nearest preceding h3 heading.
 *
 * There is no language control here on purpose. A conversation is held in one
 * language, and that is the Whisper language above — picking a second one is
 * how you get a reply in English to a question asked in Italian. What the user
 * chooses here is which of that language's voices speaks it.
 */
const SpeechSetup: Component = () => {
	onMount(() => {
		dictationStore.refreshSpeechAssets();
	});

	const languageAsset = (): SpeechAsset | undefined =>
		dictationStore.state.speechAssets.find(
			(asset) => asset.kind === "language" && asset.language === dictationStore.state.language,
		);

	// A downloadable voice (`kind: "voice"`) belongs in the voice list below, not
	// in this list of downloads.
	const downloadRows = (): SpeechAsset[] => dictationStore.state.speechAssets.filter((asset) => asset.kind !== "voice");

	const engine = (): SpeechEngineId => dictationStore.state.speechEngine;
	const isEdge = () => engine() === "edge";

	// Re-read the language's voices whenever the catalogue moves (a voice was
	// downloaded, repaired or deleted) or the language changes.
	createEffect(() => {
		const code = dictationStore.state.language;
		void dictationStore.state.speechAssets;
		if (code !== "auto" && engine() === "pocket") void dictationStore.refreshSpeechVoices(code);
	});

	// The Edge list comes from the service, so it is asked only while Edge is
	// the engine and a language is fixed.
	createEffect(() => {
		const code = dictationStore.state.language;
		if (code !== "auto" && isEdge()) void dictationStore.refreshEdgeVoices(code);
	});

	/** Whether there is a voice to choose: Edge under a fixed language, or a
	 * Pocket language that ships voices. The external command names its own. */
	const hasVoicePicker = (): boolean =>
		isEdge()
			? dictationStore.state.language !== "auto"
			: engine() === "pocket" && (languageAsset()?.voices.length ?? 0) > 0;

	/** Voice ids the picker offers: only voices that can speak now. Before the
	 * list loads, the language's shipped voices — but only once the language is
	 * downloaded, because without it no voice speaks. */
	const voiceChoices = (): string[] => {
		const loaded = dictationStore.state.speechVoices.map((voice) => voice.id);
		if (loaded.length > 0) return loaded;
		const asset = languageAsset();
		return asset?.state === "ready" ? asset.voices : [];
	};

	const [previewError, setPreviewError] = createSignal<string | null>(null);
	const listen = async () => {
		const code = isEdge() ? dictationStore.state.language : languageAsset()?.language;
		if (!code) return;
		const voice = isEdge() ? dictationStore.state.speechEdgeVoice : dictationStore.state.speechVoice;
		setPreviewError(await dictationStore.previewSpeechVoice(code, voice));
	};

	// The drag shows its value at once; the config is written on release.
	const [volumeDrag, setVolumeDrag] = createSignal<number | undefined>();
	const [levellingDrag, setLevellingDrag] = createSignal<number | undefined>();

	/** Which language replies are spoken in, in the user's terms. */
	const spokenLanguage = (): string => {
		const code = dictationStore.state.language;
		if (engine() === "external") return t("dictation.speechLanguageExternal", "Chosen by your command");
		if (code === "auto") {
			return t("dictation.speechLanguageAuto", "Whatever Whisper hears — nothing is spoken until somebody speaks");
		}
		if (isEdge()) return WHISPER_LANGUAGES[code] ?? code;
		const asset = languageAsset();
		return asset
			? asset.display_name
			: t("dictation.speechLanguageMissing", "{lang} — no speech bundle ships for it").replace(
					"{lang}",
					WHISPER_LANGUAGES[code] ?? code,
				);
	};

	return (
		<>
			<h3>{t("dictation.heading.spokenReplies", "Spoken replies")}</h3>
			<p class={cx(s.hint, d.intro)}>
				{isEdge()
					? t(
							"dictation.speechHintEdge",
							"Replies are spoken with Microsoft Edge neural voices: nothing to download, but each reply's text is sent to Microsoft's online speech service, so it needs an internet connection.",
						)
					: engine() === "pocket"
						? t(
								"dictation.speechHint",
								"Downloads needed to let an agent answer out loud. The runtime library is shared; each language is a separate bundle and brings its own voices.",
							)
						: t(
								"dictation.speechHintExternal",
								"Replies are spoken by the command below, which runs on this machine as you.",
							)}
			</p>

			<div class={s.group}>
				<Show when={engine() === "pocket"}>
					<div class={d.modelList} data-speech-downloads>
						<For each={downloadRows()}>{(asset) => <SpeechAssetRow asset={asset} />}</For>
					</div>
				</Show>

				<div class={d.conversation}>
					<div class={d.conversationRow}>
						<span>{t("dictation.speechLanguageLabel", "Replies are spoken in")}</span>
						<span class={d.conversationValue}>{spokenLanguage()}</span>
					</div>
				</div>
			</div>

			<ExpertSetting configKey="dictation.speech_engine" value={engine()}>
				<div class={s.group}>
					<label>{t("dictation.speechEngineLabel", "Speech engine")}</label>
					<select
						value={engine()}
						onChange={(e) => void dictationStore.setSpeechEngine(e.currentTarget.value as SpeechEngineId)}
					>
						<option value="edge">{t("dictation.engineEdge", "Microsoft Edge voices (online)")}</option>
						<option value="pocket">{t("dictation.enginePocket", "Pocket TTS (local, downloads a model)")}</option>
						<option value="external">{t("dictation.engineExternal", "External command")}</option>
					</select>
					<p class={s.hint}>
						{t(
							"dictation.speechEngineHint",
							"Edge is the default. Pocket TTS runs entirely on this machine after a download. An external command is your own engine.",
						)}
					</p>
				</div>
			</ExpertSetting>

			<Show when={engine() === "external"}>
				<ExpertSetting configKey="dictation.speech_command" value={dictationStore.state.speechCommand}>
					<div class={s.group}>
						<label>{t("dictation.speechCommandLabel", "Speech command")}</label>
						<textarea
							rows={4}
							value={dictationStore.state.speechCommand.join("\n")}
							onChange={(e) =>
								void dictationStore.setSpeechCommand(
									e.currentTarget.value
										.split("\n")
										.map((line) => line.trim())
										.filter((line) => line !== ""),
								)
							}
						/>
						<p class={s.hint}>
							{t(
								"dictation.speechCommandHint",
								"One argument per line, no shell. {out} is the WAV file the command must write; {text} and {voice} are optional (without {text}, the text arrives on stdin).",
							)}
						</p>
					</div>
				</ExpertSetting>
			</Show>

			<Show when={hasVoicePicker()}>
				<div class={s.group}>
					<label>{t("dictation.voiceLabel", "Voice")}</label>
					<div class={d.controlRow}>
						<Show
							when={isEdge()}
							fallback={
								<select
									value={dictationStore.state.speechVoice}
									onChange={(e) => dictationStore.setSpeechVoice(e.currentTarget.value)}
								>
									<option value="">{t("dictation.voiceDefault", "Default for this language")}</option>
									<For each={voiceChoices()}>{(voice) => <option value={voice}>{voice}</option>}</For>
								</select>
							}
						>
							<select
								value={dictationStore.state.speechEdgeVoice}
								onChange={(e) => dictationStore.setSpeechEdgeVoice(e.currentTarget.value)}
							>
								<option value="">{t("dictation.voiceDefault", "Default for this language")}</option>
								<For each={dictationStore.state.edgeVoices}>
									{(voice) => <option value={voice.id}>{voice.label}</option>}
								</For>
							</select>
						</Show>
						<button class={d.modelDownload} onClick={listen}>
							{t("dictation.listen", "Listen")}
						</button>
					</div>
					<Show when={isEdge() && dictationStore.state.edgeVoicesError}>
						<p class={cx(s.hint, d.conversationError)}>{dictationStore.state.edgeVoicesError}</p>
					</Show>
					<Show when={previewError()}>
						<p class={cx(s.hint, d.conversationError)}>{previewError()}</p>
					</Show>
					<p class={s.hint}>
						{t(
							"dictation.voiceHint",
							"Changing the voice stops any reply already being spoken — a sentence half said in one voice does not finish in another.",
						)}
					</p>
				</div>
			</Show>

			<Show when={engine() === "pocket" && (languageAsset()?.voices.length ?? 0) > 0}>
				<VoiceLibrary language={languageAsset()?.language ?? ""} />
			</Show>

			<ExpertSetting configKey="dictation.speech_volume_db" value={dictationStore.state.speechVolumeDb}>
				<SettingSlider
					label={t("dictation.voiceVolumeLabel", "Voice volume")}
					value={volumeDrag() ?? dictationStore.state.speechVolumeDb}
					onChange={setVolumeDrag}
					onCommit={(v) => {
						void dictationStore.setSpeechVolumeDb(v);
						setVolumeDrag(undefined);
					}}
					min={-30}
					max={-12}
					step={1}
					formatValue={(v) => `${v} dB`}
					hint={t(
						"dictation.voiceVolumeHint",
						"How loud every reply is spoken. Peaks are limited, so a high level never clips. Applies to the next reply.",
					)}
				/>
			</ExpertSetting>

			<ExpertSetting configKey="dictation.speech_levelling" value={dictationStore.state.speechLevelling}>
				<SettingSlider
					label={t("dictation.levellingLabel", "Levelling")}
					value={levellingDrag() ?? Math.round(dictationStore.state.speechLevelling * 100)}
					onChange={setLevellingDrag}
					onCommit={(v) => {
						void dictationStore.setSpeechLevelling(v / 100);
						setLevellingDrag(undefined);
					}}
					min={0}
					max={100}
					step={1}
					formatValue={(v) =>
						v === 0
							? t("dictation.levellingOff", "Off")
							: v === 100
								? t("dictation.levellingStrong", "Strong")
								: `${v}%`
					}
					hint={t(
						"dictation.levellingHint",
						"Evens out quiet and loud words within a reply. Off keeps the voice as recorded.",
					)}
				/>
			</ExpertSetting>
		</>
	);
};

/**
 * The voices of one language: catalogue voices already installed, catalogue
 * voices to download, and voice files the user imported. TUICommander does not
 * make voices; a user voice is a `.safetensors` file made elsewhere.
 */
const VoiceLibrary: Component<{ language: string }> = (props) => {
	const voiceAssets = (): SpeechAsset[] =>
		dictationStore.state.speechAssets.filter((asset) => asset.kind === "voice" && asset.language === props.language);
	const installed = () => voiceAssets().filter((asset) => asset.state === "ready");
	const downloadable = () => voiceAssets().filter((asset) => asset.state !== "ready");
	const userVoices = () => dictationStore.state.speechVoices.filter((voice) => voice.source === "user");

	const [importError, setImportError] = createSignal<string | null>(null);
	let fileInput: HTMLInputElement | undefined;
	const importFile = async (file: File | undefined) => {
		if (!file) return;
		setImportError(await dictationStore.importSpeechVoice(props.language, file));
		if (fileInput) fileInput.value = "";
	};

	return (
		<div class={cx(s.group, d.voiceLibrary)}>
			<label>{t("dictation.voicesLabel", "Voices")}</label>
			<Show when={installed().length > 0}>
				<div class={d.voiceGroup} data-voice-group="installed">
					<span class={d.voiceGroupTitle}>{t("dictation.voiceGroupInstalled", "Installed")}</span>
					<div class={d.modelList}>
						<For each={installed()}>{(asset) => <SpeechAssetRow asset={asset} />}</For>
					</div>
				</div>
			</Show>
			<Show when={downloadable().length > 0}>
				{/* Collapsed: a language offers two dozen voices, and listed open they
				    push the volume sliders out of sight. A native <summary> is
				    focusable and toggles on Enter and Space. */}
				<details class={d.voiceGroup} data-voice-group="downloadable">
					<summary class={cx(d.voiceGroupTitle, d.voiceGroupToggle)}>
						{t("dictation.voiceGroupDownloadable", "Downloadable")} ({downloadable().length})
					</summary>
					<div class={d.modelList}>
						<For each={downloadable()}>{(asset) => <SpeechAssetRow asset={asset} />}</For>
					</div>
				</details>
			</Show>
			<div class={d.voiceGroup} data-voice-group="yours">
				<span class={d.voiceGroupTitle}>{t("dictation.voiceGroupYours", "Yours")}</span>
				<Show when={userVoices().length > 0}>
					<div class={d.modelList}>
						<For each={userVoices()}>
							{(voice) => (
								<div class={cx(d.modelRow, voice.id === dictationStore.state.speechVoice && d.active)}>
									<div class={d.modelInfo}>
										<span class={d.modelName}>{voice.id}</span>
									</div>
									<div class={d.modelActions}>
										<button
											class={d.modelDelete}
											onClick={() => dictationStore.deleteSpeechVoice(props.language, voice.id)}
											title={t("dictation.voiceDeleteUser", "Delete this voice file")}
										>
											&times;
										</button>
									</div>
								</div>
							)}
						</For>
					</div>
				</Show>
				<div class={d.controlRow}>
					<button class={d.modelDownload} onClick={() => fileInput?.click()}>
						{t("dictation.addVoiceFile", "Add voice file…")}
					</button>
					<input
						ref={fileInput}
						type="file"
						accept=".safetensors"
						hidden
						onChange={(e) => void importFile(e.currentTarget.files?.[0])}
					/>
				</div>
				<Show when={importError()}>
					<p class={cx(s.hint, d.conversationError)}>{importError()}</p>
				</Show>
			</div>
			<p class={s.hint}>
				{t(
					"dictation.voicesHint",
					"Each voice is a separate download. A voice file you add must be made for this language's model (.safetensors); TUICommander does not create voices.",
				)}
			</p>
		</div>
	);
};

/** One catalogue entry: what it is, what state it is in, and what to do next. */
const SpeechAssetRow: Component<{ asset: SpeechAsset }> = (props) => {
	const percent = () => dictationStore.state.speechDownloads[props.asset.id];
	const downloading = () => props.asset.state === "downloading" || percent() !== undefined;
	// The bundle replies are spoken with — the counterpart of the selected
	// Whisper model, and highlighted the same way.
	const speaking = () =>
		props.asset.state === "ready" &&
		(props.asset.kind === "voice"
			? props.asset.voice === dictationStore.state.speechVoice
			: props.asset.language !== null && props.asset.language === dictationStore.state.language);

	return (
		<div class={cx(d.modelRow, speaking() && d.active)}>
			<div class={d.modelInfo}>
				<span class={d.modelName}>{props.asset.display_name}</span>
				<span class={d.modelSize}>{megabytes(props.asset.download_bytes)}</span>
			</div>
			<Show when={!downloading()}>
				<span class={cx(d.modelBadge, props.asset.state === "ready" && d.downloaded)}>
					{props.asset.state === "ready"
						? t("dictation.downloaded", "Downloaded")
						: props.asset.state === "incomplete"
							? t("dictation.speechIncomplete", "Incomplete")
							: t("dictation.notDownloaded", "Not Downloaded")}
				</span>
			</Show>
			<div class={d.modelActions}>
				<Show when={downloading()}>
					<div class={d.downloadProgress}>
						<div class={d.progressBar}>
							<div class={d.progressFill} style={{ transform: `scaleX(${(percent() ?? 0) / 100})` }} />
						</div>
						<span class={d.progressText}>{percent() ?? 0}%</span>
					</div>
					<button
						class={d.modelDelete}
						onClick={() => dictationStore.cancelSpeechDownload(props.asset.id)}
						title={t("dictation.cancel", "Cancel")}
					>
						&times;
					</button>
				</Show>
				<Show when={!downloading() && speaking()}>
					<span class={d.modelActiveLabel}>{t("dictation.active", "Active")}</span>
				</Show>
				<Show when={!downloading() && props.asset.state !== "ready"}>
					<button class={d.modelDownload} onClick={() => dictationStore.downloadSpeechAsset(props.asset.id)}>
						{props.asset.state === "incomplete"
							? t("dictation.speechRepair", "Repair")
							: t("dictation.download", "Download")}
					</button>
				</Show>
				<Show when={!downloading() && props.asset.state !== "absent"}>
					<button
						class={d.modelDelete}
						onClick={() => dictationStore.deleteSpeechAsset(props.asset.id)}
						title={t("dictation.speechDelete", "Delete this download")}
					>
						&times;
					</button>
				</Show>
			</div>
		</div>
	);
};

/**
 * Starting, watching and stopping a hands-free conversation.
 *
 * Nothing here arms on mount. Opening this panel must never open the
 * microphone, and neither must starting the app: a conversation begins because
 * somebody pressed Start, and ends because somebody pressed Stop, pressed the
 * dictation hotkey, or closed the terminal it was bound to.
 *
 * Every decision below belongs to Rust — when an utterance ends, whether the
 * activation phrase opened it, when the hold-back expires, what may be spoken.
 * This renders the answers and offers the two buttons.
 *
 * There is deliberately **no** browser branch here, and since 832-e730 that is
 * because none is needed: a browser tab arms under its own owner name and holds
 * the conversation through its own microphone and speaker, so the same two
 * buttons do the same thing on both transports. What differs is which hardware
 * the store opens before arming, and that decision lives in
 * `dictation.ts armHandsFree` rather than in a control.
 */
const HandsFreeControls: Component = () => {
	const [target, setTarget] = createSignal(terminalsStore.getActive()?.sessionId ?? "");
	// Rust owns the built-in notice; it is only shown here as the placeholder.
	const [defaultNotice, setDefaultNotice] = createSignal("");

	// Polled rather than pushed: hands-free state has no SSE arm yet, and a
	// panel that shows a stale phase is worse than one that lags a beat. Only
	// while this panel is open — see `onCleanup`.
	let timer: ReturnType<typeof setInterval> | null = null;
	onMount(() => {
		dictationStore
			.getDefaultHandsFreeStartNotice()
			.then(setDefaultNotice)
			.catch(() => appLogger.warn("dictation", "Failed to load the default hands-free start notice"));
		dictationStore.refreshHandsFree();
		dictationStore.refreshSpeechStatus();
		timer = setInterval(() => {
			dictationStore.refreshHandsFree();
			dictationStore.refreshSpeechStatus();
		}, 500);
	});
	onCleanup(() => {
		if (timer) clearInterval(timer);
	});

	const status = () => dictationStore.state.handsFree;
	const speech = () => dictationStore.state.speech;
	const armed = () => status()?.armed === true;

	/** Terminals that have a live PTY session, which is what can be bound. */
	const targets = () =>
		terminalsStore
			.getIds()
			.map((id) => terminalsStore.get(id))
			.filter((term): term is NonNullable<typeof term> => !!term?.sessionId);

	const phaseLabel = (): string => handsFreePhaseLabel(status()?.phase);

	/** What the speaker is doing, or empty when it is doing nothing. */
	const speakingLabel = (): string => {
		const current = speech();
		if (!current) return "";
		if (current.speaking) return t("dictation.speechPlaying", "Playing a reply");
		if (current.rendering) return t("dictation.speechRendering", "Synthesising a reply");
		if (current.queued > 0)
			return t("dictation.speechQueued", "{n} replies waiting").replace("{n}", String(current.queued));
		return "";
	};

	const start = async () => {
		const sessionId = target();
		if (!sessionId) return;
		await dictationStore.armHandsFree(sessionId);
	};

	return (
		<>
			<h3>{t("dictation.heading.handsFree", "Hands-free conversation")}</h3>
			<p class={cx(s.hint, d.intro)}>
				{t(
					"dictation.handsFreeHint",
					"Push-to-talk is the hotkey above: hold it, speak, release. Hands-free is the other mode — it binds one terminal, keeps the microphone open and sends each utterance by itself. The hotkey stops it.",
				)}
			</p>

			<div class={s.group}>
				<div class={d.controlRow}>
					<Show
						when={armed()}
						fallback={
							<>
								<select value={target()} onChange={(e) => setTarget(e.currentTarget.value)}>
									<option value="">{t("dictation.handsFreeNoTarget", "Choose a terminal…")}</option>
									<For each={targets()}>{(term) => <option value={term.sessionId ?? ""}>{term.name}</option>}</For>
								</select>
								<button class={cx(s.testBtn, d.startConversation)} onClick={start} disabled={!target()}>
									{t("dictation.handsFreeStart", "Start conversation")}
								</button>
							</>
						}
					>
						<button class={cx(s.testBtn, d.stopConversation)} onClick={() => dictationStore.disarmHandsFree()}>
							{t("dictation.handsFreeStop", "Stop conversation")}
						</button>
					</Show>
				</div>

				<div class={cx(d.conversation, d.conversationState)} role="status">
					<div class={d.conversationRow}>
						<span>{t("dictation.handsFreeState", "State")}</span>
						<span class={cx(d.phase, armed() && d.live, status()?.phase === "error" && d.failed)}>
							<span class={d.stateIndicator} aria-hidden="true" />
							{armed() ? t("dictation.handsFreeRunning", "Running") : t("dictation.handsFreeStopped", "Stopped")}
							<Show when={status()}> · {phaseLabel()}</Show>
						</span>
					</div>
				</div>

				<Show when={status()}>
					{(current) => (
						<div class={d.conversation}>
							<Show when={current().sessionId}>
								<div class={d.conversationRow}>
									<span>{t("dictation.handsFreeTarget", "Bound terminal")}</span>
									<span class={d.conversationValue}>
										{terminalsStore.get(terminalsStore.getTerminalForSession(current().sessionId ?? "") ?? "")?.name ??
											current().sessionId}
									</span>
								</div>
							</Show>
							<Show when={current().owner}>
								<div class={d.conversationRow}>
									<span>{t("dictation.handsFreeOwner", "Audio from")}</span>
									<span class={d.conversationValue}>{current().owner}</span>
								</div>
							</Show>
							<Show when={speakingLabel()}>
								<div class={d.conversationRow}>
									<span>{t("dictation.handsFreeSpeaker", "Speaker")}</span>
									<span class={d.conversationValue}>{speakingLabel()}</span>
								</div>
							</Show>
							<Show when={current().armed && speech() && !speech()?.available}>
								<div class={d.conversationRow}>
									<span>{t("dictation.handsFreeNoVoice", "Cannot speak")}</span>
									<span class={d.conversationValue}>{speech()?.unavailableReason}</span>
								</div>
							</Show>
							<Show when={current().pendingText}>
								<p class={d.conversationPending}>
									{t("dictation.handsFreePending", "About to send")}: {current().pendingText}
								</p>
							</Show>
							<Show when={current().error ?? dictationStore.state.handsFreeError}>
								<p class={d.conversationError}>{current().error ?? dictationStore.state.handsFreeError}</p>
							</Show>
						</div>
					)}
				</Show>
			</div>

			<div class={s.group}>
				<label>{t("dictation.activationPhraseLabel", "Activation phrase")}</label>
				<input
					type="text"
					value={dictationStore.state.handsFreeActivationPhrase}
					placeholder={t("dictation.activationPhrasePlaceholder", "computer")}
					onChange={(e) => dictationStore.setHandsFreeActivationPhrase(e.currentTarget.value)}
				/>
				<p class={s.hint}>
					{t(
						"dictation.activationPhraseHint",
						"When set, only speech that opens with this phrase is sent, and the phrase itself is removed first. The match runs on this machine, so unrelated speech never leaves it.",
					)}
				</p>
				<p class={s.hint}>
					{t(
						"dictation.activationPhraseSuggestion",
						"Try “computer”: it is distinctive, Whisper transcribes it reliably, and ordinary speech rarely contains it. Leave the field empty to send every utterance.",
					)}
				</p>
			</div>

			<ExpertSetting configKey="dictation.hands_free_hold_back_ms" value={dictationStore.state.handsFreeHoldBackMs}>
				<SettingSlider
					label={t("dictation.holdBackLabel", "Hold-back before sending")}
					value={dictationStore.state.handsFreeHoldBackMs}
					onChange={(v) => dictationStore.setHandsFreeHoldBackMs(v)}
					min={0}
					max={5000}
					step={250}
					formatValue={(v) => (v === 0 ? t("dictation.instant", "Instant") : `${v}ms`)}
					hint={t(
						"dictation.holdBackHint",
						"How long a finished utterance is shown before it is sent, so you can stop one you did not mean. Applies to the next conversation, not the one already running.",
					)}
				/>
			</ExpertSetting>

			<div class={s.group}>
				<label>{t("dictation.spokenRepliesLabel", "Spoken replies")}</label>
				<div class={s.toggle}>
					<input
						type="checkbox"
						checked={dictationStore.state.spokenReplies}
						onChange={(e) => dictationStore.setSpokenReplies(e.currentTarget.checked)}
					/>
					<span>
						{t(
							"dictation.spokenRepliesHint",
							"Let the agent answer out loud. Turn off to keep dictation and receive text replies.",
						)}
					</span>
				</div>
			</div>

			<div class={s.group}>
				<label>{t("dictation.earconsLabel", "Earcons")}</label>
				<div class={s.toggle}>
					<input
						type="checkbox"
						checked={dictationStore.state.handsFreeEarcons}
						onChange={(e) => dictationStore.setHandsFreeEarcons(e.currentTarget.checked)}
					/>
					<span>
						{t(
							"dictation.earconsHint",
							"Play a short sound when a turn is sent to the agent, and a lower one when a turn without the activation phrase is dropped. Only the device you talk into plays them.",
						)}
					</span>
				</div>
			</div>

			<ExpertSetting configKey="dictation.hands_free_notify_model" value={dictationStore.state.notifyModelOnHandsFree}>
				<div class={s.group}>
					<label>{t("dictation.notifyModelLabel", "Notify model when hands-free changes")}</label>
					<div class={s.toggle}>
						<input
							type="checkbox"
							checked={dictationStore.state.notifyModelOnHandsFree}
							onChange={(e) => dictationStore.setNotifyModelOnHandsFree(e.currentTarget.checked)}
						/>
						<span>
							{t(
								"dictation.notifyModelHint",
								"Tell the agent when a hands-free conversation starts, so it answers out loud, and when it ends, so it goes back to text. Turning this off never leaves speech running: disarming always stops it.",
							)}
						</span>
					</div>
				</div>
			</ExpertSetting>

			{/* The notice is only sent while the toggle above is on. */}
			<Show when={dictationStore.state.notifyModelOnHandsFree}>
				<ExpertSetting configKey="dictation.hands_free_start_notice" value={dictationStore.state.handsFreeStartNotice}>
					<div class={s.group}>
						<label>{t("dictation.startNoticeLabel", "Start notice")}</label>
						<textarea
							value={dictationStore.state.handsFreeStartNotice}
							placeholder={defaultNotice()}
							onChange={(e) => dictationStore.setHandsFreeStartNotice(e.currentTarget.value)}
						/>
						<div class={d.controlRow}>
							<button
								class={s.testBtn}
								onClick={() => {
									// A click is not an input/change event: pin here so the reset does not hide the control.
									settingsExpertStore.pin("dictation.hands_free_start_notice");
									dictationStore.resetHandsFreeStartNotice();
								}}
								disabled={!dictationStore.state.handsFreeStartNotice}
							>
								{t("dictation.startNoticeReset", "Reset to default")}
							</button>
						</div>
						<p class={s.hint}>
							{t(
								"dictation.startNoticeHint",
								"What the agent reads when a conversation starts. Leave it empty to send the built-in text shown in grey.",
							)}
						</p>
					</div>
				</ExpertSetting>
			</Show>
		</>
	);
};
