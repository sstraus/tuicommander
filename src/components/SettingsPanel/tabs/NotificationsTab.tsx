import { type Component, createSignal, For, Show } from "solid-js";
import { t } from "../../../i18n";
import { invoke } from "../../../invoke";
import type { NotificationSound } from "../../../notifications";
import { notificationsStore } from "../../../stores/notifications";
import { isTauri } from "../../../transport";
import { SettingSlider, SettingToggle } from "../SettingFields";
import s from "../Settings.module.css";

interface AudioOutputDevice {
	name: string;
	is_default: boolean;
}

// ---------------------------------------------------------------------------
// Sound pattern visualizations (inline SVG showing pitch contour)
// ---------------------------------------------------------------------------

/** Mini musical staff showing the note pattern for each sound.
 *  5 staff lines, note heads positioned by pitch, stems going up. */
function SoundPatternSvg(props: { sound: NotificationSound }) {
	const patterns: Record<NotificationSound, { x: number; y: number }[]> = {
		question: [
			{ x: 14, y: 16 },
			{ x: 30, y: 8 },
		],
		completion: [
			{ x: 10, y: 16 },
			{ x: 24, y: 10 },
			{ x: 38, y: 4 },
		],
		error: [
			{ x: 14, y: 8 },
			{ x: 30, y: 16 },
		],
		warning: [
			{ x: 14, y: 12 },
			{ x: 30, y: 12 },
		],
		info: [{ x: 22, y: 4 }],
	};

	const colors: Record<NotificationSound, string> = {
		question: "var(--warning)",
		completion: "var(--success)",
		error: "var(--error)",
		warning: "var(--accent)",
		info: "var(--fg-muted)",
	};

	const notes = patterns[props.sound];
	const color = colors[props.sound];
	const w = props.sound === "completion" ? 36 : props.sound === "info" ? 24 : 32;

	return (
		<svg viewBox={`0 0 ${w} 18`} width={w} height="14" style={{ "vertical-align": "middle", "flex-shrink": "0" }}>
			<For each={[3, 6, 9, 12, 15]}>
				{(ly) => <line x1="1" y1={ly} x2={w - 1} y2={ly} stroke="var(--border)" stroke-width="0.4" />}
			</For>
			<For each={notes}>
				{(note) => {
					const sy = (note.y / 20) * 15;
					const sx = (note.x / 48) * w;
					return (
						<>
							<ellipse cx={sx} cy={sy} rx="2.5" ry="1.8" fill={color} transform={`rotate(-15 ${sx} ${sy})`} />
							<line x1={sx + 2.3} y1={sy - 0.5} x2={sx + 2.3} y2={sy - 7} stroke={color} stroke-width="0.7" />
						</>
					);
				}}
			</For>
		</svg>
	);
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const NotificationsTab: Component = () => {
	const sounds: { key: NotificationSound; label: string }[] = [
		{ key: "question", label: t("notifications.sound.question", "Question") },
		{ key: "error", label: t("notifications.sound.error", "Error") },
		{ key: "completion", label: t("notifications.sound.completion", "Completion") },
		{ key: "warning", label: t("notifications.sound.warning", "Warning") },
		{ key: "info", label: t("notifications.sound.info", "Info") },
	];

	// Device enumeration is LAZY: on macOS, cpal's CoreAudio output-device scan
	// triggers the microphone permission prompt. We must NOT run it on tab mount —
	// only when the user explicitly clicks "Choose output device". `null` = not loaded yet.
	const [devices, setDevices] = createSignal<AudioOutputDevice[] | null>(null);
	const [loadingDevices, setLoadingDevices] = createSignal(false);

	async function loadDevices(): Promise<void> {
		if (!isTauri()) return;
		setLoadingDevices(true);
		try {
			setDevices(await invoke<AudioOutputDevice[]>("list_audio_output_devices"));
		} catch {
			setDevices([]);
		} finally {
			setLoadingDevices(false);
		}
	}

	return (
		<div class={s.section}>
			<h3>{t("notifications.heading.notificationSettings", "Notification Settings")}</h3>

			<Show
				when={notificationsStore.state.isAvailable}
				fallback={
					<p class={s.warning}>
						{t("notifications.warning.notAvailable", "Audio notifications are not available on this platform")}
					</p>
				}
			>
				<SettingToggle
					checked={notificationsStore.state.config.enabled}
					onChange={(v) => notificationsStore.setEnabled(v)}
					label={t("notifications.toggle.enableAudio", "Enable audio notifications")}
				/>

				<SettingSlider
					label={t("notifications.label.masterVolume", "Master Volume")}
					value={Math.round(notificationsStore.state.config.volume * 100)}
					onChange={(v) => notificationsStore.setVolume(v / 100)}
					onCommit={() => notificationsStore.testSound("info")}
					min={0}
					max={100}
					suffix="%"
					hint={t(
						"notifications.hint.masterVolume",
						"Overall volume for all notification sounds — release the slider to hear a preview",
					)}
				/>

				<Show when={isTauri()}>
					<div class={s.group}>
						<label>{t("notifications.label.audioDevice", "Audio Output Device")}</label>
						<p class={s.hint}>
							{t("notifications.hint.audioDevice", "Choose which speaker or output to use for notification sounds")}
						</p>
						<Show
							when={devices() !== null}
							fallback={
								<>
									<p class={s.hint}>
										{t("notifications.hint.audioDeviceCurrent", "Currently: {device}", {
											device:
												notificationsStore.state.config.audio_device ??
												t("notifications.option.systemDefault", "System Default"),
										})}
									</p>
									<button class={s.testBtn} disabled={loadingDevices()} onClick={loadDevices}>
										{loadingDevices()
											? t("notifications.btn.loadingDevices", "Loading…")
											: t("notifications.btn.chooseDevice", "Choose output device…")}
									</button>
									<p class={s.hint} style={{ "margin-top": "6px" }}>
										{t(
											"notifications.hint.deviceMicPrompt",
											"macOS may ask for microphone access — the audio system requires it to enumerate output devices. Notifications never record audio.",
										)}
									</p>
								</>
							}
						>
							<select
								value={notificationsStore.state.config.audio_device ?? ""}
								onChange={(e) => {
									const val = e.currentTarget.value;
									notificationsStore.setAudioDevice(val || null);
								}}
							>
								<option value="">{t("notifications.option.systemDefault", "System Default")}</option>
								<For each={devices()}>
									{(device) => (
										<option value={device.name}>
											{device.name}
											{device.is_default ? ` (${t("notifications.option.currentDefault", "current default")})` : ""}
										</option>
									)}
								</For>
							</select>
							<button
								class={s.testBtn}
								style={{ "margin-top": "6px" }}
								disabled={loadingDevices()}
								onClick={loadDevices}
							>
								{t("notifications.btn.refreshDevices", "Refresh")}
							</button>
						</Show>
					</div>
				</Show>

				<div class={s.group}>
					<label>{t("notifications.label.notificationEvents", "Notification Events")}</label>
					<p class={s.hint} style={{ "margin-bottom": "12px" }}>
						{t("notifications.hint.notificationEvents", "Choose which events play a sound")}
					</p>
					<For each={sounds}>
						{(sound) => (
							<div class={s.soundRow}>
								<div class={s.toggle}>
									<input
										type="checkbox"
										checked={notificationsStore.state.config.sounds[sound.key]}
										onChange={(e) => notificationsStore.setSoundEnabled(sound.key, e.currentTarget.checked)}
									/>
									<span>{sound.label}</span>
								</div>
								<SoundPatternSvg sound={sound.key} />
								<button class={s.testBtn} onClick={() => notificationsStore.testSound(sound.key)}>
									{t("notifications.btn.test", "Test")}
								</button>
							</div>
						)}
					</For>
				</div>

				<div class={s.actions}>
					<button onClick={() => notificationsStore.reset()}>
						{t("notifications.btn.resetDefaults", "Reset Defaults")}
					</button>
				</div>
			</Show>
		</div>
	);
};
