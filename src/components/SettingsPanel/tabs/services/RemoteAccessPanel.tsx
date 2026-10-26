import QRCode from "qrcode";
import { type Component, createEffect, createResource, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { useConfirmDialog } from "../../../../hooks/useConfirmDialog";
import { t } from "../../../../i18n";
import { reportSettingsConflicts, settingsRefresh } from "../../../../services/settingsRefresh";
import { appLogger } from "../../../../stores/appLogger";
import { rpc } from "../../../../transport";
import { cx } from "../../../../utils";
import { writeClipboard } from "../../../../utils/clipboard";
import { mergeSettingsSnapshot } from "../../../../utils/mergeSettingsSnapshot";
import { ConfirmDialog } from "../../../ConfirmDialog";
import { Button } from "../../../ui/Button";
import { Input } from "../../../ui/Input";
import { Select } from "../../../ui/Select";
import { ContextualHelp } from "../../ContextualHelp";
import { ExpertSetting } from "../../ExpertSetting";
import { SettingInput, SettingSelect, SettingToggle } from "../../SettingFields";
import s from "../../Settings.module.css";
import { DangerZone, SectionCard, SettingPageHeader, SettingRow } from "../../SettingsPrimitives";
import layout from "./ServiceRestyle.module.css";
import type { AppConfig } from "./servicesShared";
import { saveConfigField, useMcpStatusPoll } from "./servicesShared";

/** Username the placeholder advertises, and the value used when the field is left blank. */
export const DEFAULT_AUTH_USERNAME = "admin";

/**
 * Normalize the remote-access username before it reaches the config.
 *
 * An empty username makes `validate_basic_auth` return `NotConfigured`, so the
 * server answers every Basic Auth attempt with 401 no matter what the client
 * sends — while the field still shows the `admin` placeholder, which reads as a
 * default it is not. A password saved on its own used to land exactly there.
 */
export const normalizeAuthUsername = (input: string): string => input.trim() || DEFAULT_AUTH_USERNAME;

interface LocalIpEntry {
	ip: string;
	label: string;
}

/**
 * Remote Access page: enablement, authentication, network settings,
 * Tailscale HTTPS, the QR/connect URL, and the cloud relay.
 */
export const RemoteAccessPanel: Component = () => {
	const { status, relayConnected, refresh } = useMcpStatusPoll();
	const [localIps, { refetch: refetchLocalIps }] = createResource(() => rpc<LocalIpEntry[]>("get_local_ips"));
	const availableIps = () => (localIps.error ? undefined : localIps());
	const [selectedIp, setSelectedIp] = createSignal<string>("");

	// Remote access form state
	const [raEnabled, setRaEnabled] = createSignal(false);
	const [raPort, setRaPort] = createSignal(9876);
	const [raUsername, setRaUsername] = createSignal("");
	const [raPassword, setRaPassword] = createSignal("");
	const [raHasPassword, setRaHasPassword] = createSignal(false);
	const [raShowPassword, setRaShowPassword] = createSignal(false);
	const [qrDataUrl, setQrDataUrl] = createSignal<string | null>(null);
	const [tokenDuration, setTokenDuration] = createSignal(2592000);
	const [ipv6Enabled, setIpv6Enabled] = createSignal(false);
	const [urlCopied, setUrlCopied] = createSignal(false);
	const [regenerating, setRegenerating] = createSignal(false);

	// Tailscale state (mirrors Rust TailscaleState enum serialization)
	type TailscaleStatus =
		| { state: "NotInstalled" }
		| { state: "NotRunning" }
		| { state: "Running"; fqdn: string; https_enabled: boolean };

	const [tailscaleState, setTailscaleState] = createSignal<TailscaleStatus | null>(null);

	// Relay state
	const [relayEnabled, setRelayEnabled] = createSignal(false);
	const [relayUrl, setRelayUrl] = createSignal("wss://relay.tuicommander.com");
	const [relayToken, setRelayToken] = createSignal("");
	const [relaySessionId, setRelaySessionId] = createSignal("");

	// Auto-select best IP when list loads (prefer Tailscale, then LAN/Wi-Fi)
	createEffect(() => {
		const ips = availableIps();
		if (!ips?.length || selectedIp()) return;
		const preferred =
			ips.find((e) => e.label.includes("Tailscale")) ??
			ips.find((e) => e.label.includes("Wi-Fi") || e.label.includes("LAN")) ??
			ips[0];
		setSelectedIp(preferred.ip);
	});

	const activeIp = () => (localIps.error ? undefined : selectedIp() || availableIps()?.[0]?.ip);

	// Connect URL fetched from backend (token never reaches JS)
	const [connectUrl, setConnectUrl] = createSignal<string | null>(null);
	// Bumped after token regeneration to re-fetch the URL
	const [qrVersion, setQrVersion] = createSignal(0);

	/** Fetch connect URL from backend so the raw token stays server-side. */
	createEffect(() => {
		const ip = activeIp();
		void qrVersion(); // subscribe — re-run after token regeneration
		void status()?.tls_active; // A listener restart can change the URL scheme.
		if (!ip) {
			setConnectUrl(null);
			setQrDataUrl(null);
			return;
		}
		let cancelled = false;
		onCleanup(() => {
			cancelled = true;
		});
		rpc<string>("get_connect_url", { ip })
			.then((url) => {
				if (cancelled) return;
				setConnectUrl(url);
				return QRCode.toDataURL(url, { width: 160, margin: 2, color: { dark: "#ffffff", light: "#1e1e1e" } });
			})
			.then((dataUrl) => {
				if (!cancelled && dataUrl) setQrDataUrl(dataUrl);
			})
			.catch(() => {
				if (cancelled) return;
				setConnectUrl(null);
				setQrDataUrl(null);
			});
	});

	function formSnapshot() {
		return {
			enabled: raEnabled(),
			port: raPort(),
			username: raUsername(),
			tokenDuration: tokenDuration(),
			ipv6: ipv6Enabled(),
			relayEnabled: relayEnabled(),
			relayUrl: relayUrl(),
			relayToken: relayToken(),
			relaySessionId: relaySessionId(),
		};
	}
	let acknowledgedForm = formSnapshot();
	function adoptConfig(config: AppConfig) {
		const incoming = {
			enabled: config.services.server.enabled,
			port: config.services.server.port,
			username: config.services.auth.username,
			tokenDuration: config.services.auth.session_token_duration_secs ?? 2592000,
			ipv6: config.services.server.ipv6_enabled ?? false,
			relayEnabled: config.services.relay.enabled ?? false,
			relayUrl: config.services.relay.url || "wss://relay.tuicommander.com",
			relayToken: config.services.relay.token ?? "",
			relaySessionId: config.services.relay.session_id ?? "",
		};
		const merged = mergeSettingsSnapshot(acknowledgedForm, formSnapshot(), incoming);
		acknowledgedForm = incoming;
		const value = merged.value;
		setRaEnabled(value.enabled);
		setRaPort(value.port);
		setRaUsername(value.username);
		setRaHasPassword(config.services.auth.password_hash.length > 0);
		setTokenDuration(value.tokenDuration);
		setIpv6Enabled(value.ipv6);
		setRelayEnabled(value.relayEnabled);
		setRelayUrl(value.relayUrl);
		setRelayToken(value.relayToken);
		setRelaySessionId(value.relaySessionId);
		reportSettingsConflicts(merged.conflicts);
	}
	onCleanup(
		settingsRefresh.register({ section: "app", read: () => rpc<AppConfig>("load_config"), apply: adoptConfig }),
	);

	const loadRemoteConfig = async () => {
		try {
			const config = await rpc<AppConfig>("load_config");
			adoptConfig(config);
		} catch (e) {
			appLogger.warn("config", "Failed to load remote access config, using defaults", e);
		}
	};

	onMount(async () => {
		loadRemoteConfig();

		// Load Tailscale status
		try {
			const ts = await rpc<TailscaleStatus>("get_tailscale_status");
			setTailscaleState(ts);
		} catch {
			/* Tailscale detection not available */
		}
	});

	/** Hash and save a new password */
	const savePassword = async (password: string) => {
		if (!password) return;
		try {
			const hash = await rpc<string>("hash_password", { password });
			// A hash without a username disables Basic Auth entirely, so persist both.
			const username = normalizeAuthUsername(raUsername());
			setRaUsername(username);
			await saveConfigField((c) => {
				c.services.auth.username = username;
				c.services.auth.password_hash = hash;
			});
			setRaPassword("");
			setRaHasPassword(true);
		} catch (e) {
			appLogger.error("config", "Failed to hash password", e);
		}
	};

	const copyUrl = async () => {
		const url = connectUrl();
		if (!url) return;
		try {
			await writeClipboard(url);
			setUrlCopied(true);
			setTimeout(() => setUrlCopied(false), 2000);
		} catch {
			// Fallback: select text for manual copy
		}
	};

	const tokenDialog = useConfirmDialog();
	const regenerateToken = async () => {
		if (
			!(await tokenDialog.confirm({
				title: "Regenerate remote access token?",
				message: "This revokes shared token URLs and disconnects all active remote sessions on this machine.",
				okLabel: "Regenerate token",
				kind: "warning",
				defaultButton: "cancel",
			}))
		)
			return;
		setRegenerating(true);
		try {
			await rpc("regenerate_session_token");
			setQrVersion((v) => v + 1); // re-fetch QR URL with new token
			await refresh();
		} catch (e) {
			appLogger.error("config", "Failed to regenerate token", e);
		} finally {
			setRegenerating(false);
		}
	};

	/** Token duration options */
	const TOKEN_DURATIONS = [
		{ value: 3600, label: t("services.tokenDuration.1h", "1 hour") },
		{ value: 86400, label: t("services.tokenDuration.24h", "24 hours") },
		{ value: 604800, label: t("services.tokenDuration.7d", "7 days") },
		{ value: 2592000, label: t("services.tokenDuration.30d", "30 days") },
		{ value: 31536000, label: t("services.tokenDuration.never", "Never") },
	];

	return (
		<div class={`${s.section} ${layout.page}`}>
			<SettingPageHeader title="Remote Access" scope="This machine" breadcrumb="Integrations / Remote Access" />
			<SectionCard title="Connection status">
				<SettingToggle
					settingId="app.services.server.enabled"
					row
					checked={raEnabled()}
					onChange={(val) => {
						setRaEnabled(val);
						saveConfigField((c) => {
							c.services.server.enabled = val;
						});
					}}
					label={t("services.toggle.enableRemoteAccess", "Enable remote access")}
					hint={t(
						"services.hint.remoteAccessWarning",
						"Warning: exposes a web interface on your local network. Secure with a strong password.",
					)}
					hintStyle={{ color: "var(--warning, #e5c07b)" }}
				/>

				<Show when={raEnabled()}>
					<div class={s.raBody}>
						<div class={s.raFields}>
							<ExpertSetting configKey="app.services.server.port" value={raPort()}>
								<SettingRow label={t("services.label.port", "Port")}>
									{(id, descriptionId) => (
										<>
											<Input
												data-setting-id="app.services.server.port"
												id={id}
												aria-describedby={descriptionId()}
												type="number"
												class={s.input}
												value={raPort()}
												min={1024}
												max={65535}
												onInput={(e) => setRaPort(parseInt(e.currentTarget.value, 10) || 9876)}
												onChange={() =>
													saveConfigField((c) => {
														c.services.server.port = raPort();
													})
												}
											/>
											<p class={s.hint}>{t("services.hint.port", "TCP port for the remote access web server")}</p>
										</>
									)}
								</SettingRow>
							</ExpertSetting>

							{/* Username + Password side by side */}
							<div>
								<SettingRow label={t("services.label.username", "Username")}>
									{(id, descriptionId) => (
										<>
											<Input
												data-setting-id="app.services.auth.username"
												id={id}
												aria-describedby={descriptionId()}
												type="text"
												class={s.input}
												value={raUsername()}
												placeholder={t("services.placeholder.username", "admin")}
												onInput={(e) => setRaUsername(e.currentTarget.value)}
												onChange={() => {
													// Blank it out while a password is set and Basic Auth silently stops working.
													const username = raHasPassword() ? normalizeAuthUsername(raUsername()) : raUsername().trim();
													setRaUsername(username);
													saveConfigField((c) => {
														c.services.auth.username = username;
													});
												}}
											/>
										</>
									)}
								</SettingRow>
								<SettingRow label={t("services.label.password", "Password")}>
									{(id, descriptionId) => (
										<>
											<div class={s.passwordRow}>
												<Input
													data-setting-id="app.services.auth.password_hash"
													id={id}
													aria-describedby={descriptionId()}
													type={raShowPassword() ? "text" : "password"}
													class={s.input}
													value={raPassword()}
													placeholder={
														raHasPassword()
															? t("services.placeholder.passwordSet", "Password set — enter to change")
															: t("services.placeholder.passwordEnter", "Enter password")
													}
													onInput={(e) => setRaPassword(e.currentTarget.value)}
													onChange={() => savePassword(raPassword())}
												/>
												<Button
													data-non-setting="Change only visibility of the unsaved password input"
													class={s.toggleBtn}
													onClick={() => setRaShowPassword(!raShowPassword())}
													title={
														raShowPassword()
															? t("services.btn.hidePassword", "Hide password")
															: t("services.btn.showPassword", "Show password")
													}
												>
													{raShowPassword() ? (
														/* eye-slash */ <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
															<path d="M13.359 11.238C15.06 9.72 16 8 16 8s-3-5.5-8-5.5a7.028 7.028 0 0 0-2.79.588l.77.771A5.944 5.944 0 0 1 8 3.5c2.12 0 3.879 1.168 5.168 2.457A13.134 13.134 0 0 1 14.828 8c-.058.087-.122.183-.195.288-.335.48-.83 1.12-1.465 1.755-.165.165-.337.328-.517.486l.708.709z" />
															<path d="M11.297 9.176a3.5 3.5 0 0 0-4.474-4.474l.823.823a2.5 2.5 0 0 1 2.829 2.829l.822.822zm-2.943 1.299.822.822a3.5 3.5 0 0 1-4.474-4.474l.823.823a2.5 2.5 0 0 0 2.829 2.829z" />
															<path d="M3.35 5.47c-.18.16-.353.322-.518.487A13.134 13.134 0 0 0 1.172 8l.195.288c.335.48.83 1.12 1.465 1.755C4.121 11.332 5.881 12.5 8 12.5c.716 0 1.39-.133 2.02-.36l.77.772A7.029 7.029 0 0 1 8 13.5C3 13.5 0 8 0 8s.939-1.721 2.641-3.238l.708.709zm10.296 8.884-12-12 .708-.708 12 12-.708.708z" />
														</svg>
													) : (
														/* eye */ <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
															<path d="M16 8s-3-5.5-8-5.5S0 8 0 8s3 5.5 8 5.5S16 8 16 8zM1.173 8a13.133 13.133 0 0 1 1.66-2.043C4.12 4.668 5.88 3.5 8 3.5c2.12 0 3.879 1.168 5.168 2.457A13.133 13.133 0 0 1 14.828 8c-.058.087-.122.183-.195.288-.335.48-.83 1.12-1.465 1.755C11.879 11.332 10.119 12.5 8 12.5c-2.12 0-3.879-1.168-5.168-2.457A13.134 13.134 0 0 1 1.172 8z" />
															<path d="M8 5.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5zM4.5 8a3.5 3.5 0 1 1 7 0 3.5 3.5 0 0 1-7 0z" />
														</svg>
													)}
												</Button>
											</div>
											<p class={s.hint}>
												{t("services.hint.passwordLeaveBlank", "Leave blank to keep the current password")}
											</p>
										</>
									)}
								</SettingRow>
							</div>

							<Show when={raEnabled()}>
								<SettingRow label={t("services.label.networkInterface", "Network Interface")}>
									{(id, descriptionId) => (
										<>
											<Show
												when={!localIps.error}
												fallback={
													<div>
														<p role="alert" class={layout.discoveryError}>
															Local IP discovery failed: {String(localIps.error)}
														</p>
														<Button
															type="button"
															class={s.testBtn}
															data-non-setting="Retry local network discovery"
															onClick={() => void Promise.resolve(refetchLocalIps()).catch(() => {})}
														>
															Retry
														</Button>
													</div>
												}
											>
												<Show
													when={(availableIps()?.length ?? 0) > 1}
													fallback={<code class={s.url}>{activeIp() ?? "Not available"}</code>}
												>
													<Select
														data-non-setting="Displayed connection URL address selection; no persistence"
														id={id}
														aria-describedby={descriptionId()}
														class={s.input}
														value={selectedIp()}
														onChange={(e) => setSelectedIp(e.currentTarget.value)}
													>
														<For each={availableIps()}>
															{(entry) => (
																<option value={entry.ip}>
																	{entry.label} — {entry.ip}
																</option>
															)}
														</For>
													</Select>
												</Show>
											</Show>
											<p class={s.hint} style={{ "margin-top": "4px" }}>
												{t("services.hint.qrScan", "Scan the QR code to connect from another device")}
											</p>
											<Show when={status()?.reachable === false}>
												<p class={s.hint} style={{ color: "var(--warning, #e5c07b)", "margin-top": "4px" }}>
													{t("services.hint.firewallWarning", "Port may be blocked by a firewall")}
												</p>
											</Show>
											<Show when={status()?.reachable === true}>
												<p class={s.hint} style={{ color: "var(--green, #98c379)", "margin-top": "4px" }}>
													{t("services.hint.serverReachable", "Server is reachable from the network")}
												</p>
											</Show>
										</>
									)}
								</SettingRow>
							</Show>

							<ExpertSetting configKey="app.services.auth.session_token_duration_secs" value={tokenDuration()}>
								<SettingSelect
									settingId="app.services.auth.session_token_duration_secs"
									row
									label={t("services.label.tokenDuration", "Session Token Duration")}
									value={String(tokenDuration())}
									onChange={(v) => {
										const val = parseInt(v, 10);
										setTokenDuration(val);
										saveConfigField((c) => {
											c.services.auth.session_token_duration_secs = val;
										});
									}}
									options={TOKEN_DURATIONS.map((o) => ({ value: String(o.value), label: o.label }))}
									hint={t(
										"services.hint.tokenDuration",
										"How long a device may stay unused before it must log in again. Every use renews it.",
									)}
								/>
							</ExpertSetting>
						</div>

						<Show when={connectUrl()}>
							<div class={s.qr}>
								<Show when={qrDataUrl()}>
									{(url) => (
										<img
											src={url()}
											width={120}
											height={120}
											alt={t("services.alt.qrCode", "QR code")}
											title={t("services.title.qrCode", "Scan to connect")}
										/>
									)}
								</Show>
								<span class={s.qrLabel}>{t("services.label.scanToConnect", "Scan to connect")}</span>
								{/* Connection URL right under QR code */}
								<div class={s.urlCopyRow} style={{ "margin-top": "8px", "max-width": "200px" }}>
									<code class={s.urlFull} style={{ "font-size": "10px", "word-break": "break-all" }}>
										{connectUrl()}
									</code>
									<Button
										data-non-setting="Copy the displayed connection URL"
										class={s.copyBtn}
										onClick={copyUrl}
										title={t("services.btn.copyUrl", "Copy URL to clipboard")}
									>
										{urlCopied() ? (
											<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
												<path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 0 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0z" />
											</svg>
										) : (
											<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
												<path d="M0 6.75C0 5.784.784 5 1.75 5h1.5a.75.75 0 0 1 0 1.5h-1.5a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-1.5a.75.75 0 0 1 1.5 0v1.5A1.75 1.75 0 0 1 9.25 16h-7.5A1.75 1.75 0 0 1 0 14.25Z" />
												<path d="M5 1.75C5 .784 5.784 0 6.75 0h7.5C15.216 0 16 .784 16 1.75v7.5A1.75 1.75 0 0 1 14.25 11h-7.5A1.75 1.75 0 0 1 5 9.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z" />
											</svg>
										)}
									</Button>
								</div>
							</div>
						</Show>
					</div>

					<ExpertSetting configKey="app.services.server.ipv6_enabled" value={ipv6Enabled()}>
						<SettingToggle
							settingId="app.services.server.ipv6_enabled"
							row
							checked={ipv6Enabled()}
							onChange={(val) => {
								setIpv6Enabled(val);
								saveConfigField((c) => {
									c.services.server.ipv6_enabled = val;
								});
							}}
							label={t("services.toggle.enableIpv6", "Enable IPv6 (dual-stack)")}
							hint={t(
								"services.hint.ipv6Description",
								"Binds the server to both IPv4 and IPv6 addresses. Requires save + server restart.",
							)}
						/>
					</ExpertSetting>
				</Show>
			</SectionCard>
			<SectionCard title="Tailscale HTTPS" help={<ContextualHelp label="Tailscale HTTPS" />}>
				<Show when={!tailscaleState()}>
					<p class={s.hint}>Tailscale status not checked.</p>
				</Show>
				{/* ── Tailscale TLS ── */}
				<Show when={tailscaleState()}>
					{(() => {
						const ts = tailscaleState()!;
						const statusText =
							ts.state === "Running"
								? status()?.tls_active
									? `HTTPS active (${ts.fqdn})`
									: ts.https_enabled
										? "Running (HTTPS unavailable)"
										: "Running (HTTPS not enabled)"
								: ts.state === "NotRunning"
									? "Not running"
									: "Not installed";
						const showHint = ts.state === "Running" && !ts.https_enabled;
						return (
							<>
								<div class={s.group}>
									<label>{t("services.label.tailscaleStatus", "Status")}</label>
									<div class={s.mcpStatusRow}>
										<span class={s.mcpStatusText}>{statusText}</span>
										<Button
											data-non-setting="Explicit Tailscale runtime discovery"
											class={s.testBtn}
											onClick={async () => {
												try {
													const updated = await rpc<TailscaleStatus>("recheck_tailscale_status");
													setTailscaleState(updated);
												} catch (e) {
													appLogger.error("tailscale", "Recheck failed", e);
												}
											}}
											title={t("services.action.recheckTailscale", "Recheck Tailscale status")}
										>
											{t("services.action.recheck", "Recheck")}
										</Button>
									</div>
									<Show when={ts.state === "Running" && ts.https_enabled && !status()?.tls_active}>
										<p class={s.hint}>
											Enable remote access, then recheck Tailscale. If HTTPS remains unavailable, check the Tailscale
											certificate errors in the app logs.
										</p>
									</Show>
									<Show when={showHint}>
										<p class={s.hint}>
											{t(
												"services.hint.tailscaleEnableHttps",
												"Enable HTTPS certificates in your Tailscale admin console to serve the PWA over HTTPS.",
											)}
										</p>
									</Show>
								</div>
							</>
						);
					})()}
				</Show>

				{/* ── Cloud Relay ── */}
			</SectionCard>
			<Show when={raEnabled()}>
				{" "}
				<DangerZone
					title="Danger zone"
					scope="All remote sessions on this machine"
					consequence="Regenerating the token revokes shared token URLs and disconnects active remote sessions."
				>
					<ContextualHelp label="Regenerate remote token" />
					<Button
						data-setting-id="app.services.auth.session_token"
						class={s.testBtn}
						disabled={regenerating()}
						onClick={regenerateToken}
					>
						{regenerating()
							? t("services.btn.regenerating", "Regenerating...")
							: t("services.btn.regenerateToken", "Regenerate Token")}
					</Button>
					<p class={s.hint}>
						{t("services.hint.regenerateToken", "Generates a new token, disconnecting all active remote sessions")}
					</p>
				</DangerZone>
			</Show>

			<SectionCard title={t("services.heading.cloudRelay", "Cloud Relay")}>
				<SettingToggle
					settingId="app.services.relay.enabled"
					row
					checked={relayEnabled()}
					onChange={(val) => {
						setRelayEnabled(val);
						if (val && !relaySessionId()) {
							const id = crypto.randomUUID();
							setRelaySessionId(id);
							saveConfigField((c) => {
								c.services.relay.enabled = val;
								c.services.relay.session_id = id;
							});
						} else {
							saveConfigField((c) => {
								c.services.relay.enabled = val;
							});
						}
					}}
					label={t("services.toggle.enableRelay", "Enable cloud relay")}
					hint={t(
						"services.hint.relayDescription",
						"Connect from anywhere via an encrypted WebSocket relay. No port forwarding or VPN needed. Note: traffic is encrypted in transit, but the relay operator can derive the key — this is not end-to-end encryption.",
					)}
				/>

				{/* URL and token render with the relay off: it starts only once enabled
			    AND both are set (relay_client.rs), so they are configured first. */}
				<SettingInput
					settingId="app.services.relay.url"
					row
					label={t("services.label.relayUrl", "Relay Server URL")}
					value={relayUrl()}
					onInput={(v) => {
						setRelayUrl(v);
						saveConfigField((c) => {
							c.services.relay.url = v;
						});
					}}
					placeholder="wss://relay.tuicommander.com"
				/>

				<SettingInput
					settingId="app.services.relay.token"
					row
					label={t("services.label.relayToken", "Bearer Token")}
					value={relayToken()}
					onInput={(v) => {
						setRelayToken(v);
						saveConfigField((c) => {
							c.services.relay.token = v;
							c.services.relay.token_exists = v.length > 0;
						});
					}}
					type="password"
					placeholder={t("services.placeholder.relayToken", "Paste token from relay server registration")}
					hint={t(
						"services.hint.relayToken",
						"Obtained from the relay server's /register endpoint. Used for both authentication and encryption key derivation — because the relay receives this token, it can derive the key, so traffic is not end-to-end encrypted.",
					)}
				/>

				<Show when={relayEnabled()}>
					<div class={s.group}>
						<div class={s.mcpStatusRow}>
							<span class={cx(s.mcpStatusDot, relayConnected() && s.running)} />
							<span class={s.mcpStatusText}>
								{relayConnected()
									? t("services.relay.connected", "Connected")
									: t("services.relay.disconnected", "Disconnected")}
							</span>
						</div>
						<p class={s.hint} style={{ color: "var(--warning, #e5c07b)" }}>
							{t("services.hint.relayRestart", "Changes require an app restart to take effect.")}
						</p>
					</div>

					<div class={s.group}>
						<label>{t("services.label.relaySessionId", "Session ID")}</label>
						<div class={s.passwordRow}>
							<Input
								data-non-setting="Read-only relay-issued session identity"
								type="text"
								class={s.input}
								value={relaySessionId()}
								readOnly
							/>
							<Button
								data-non-setting="Rotate the runtime relay-session identity through its existing owner"
								class={s.toggleBtn}
								onClick={() => {
									const id = crypto.randomUUID();
									setRelaySessionId(id);
									saveConfigField((c) => {
										c.services.relay.session_id = id;
									});
								}}
								title={t("services.btn.regenerateSessionId", "Generate new session ID")}
							>
								<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
									<path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2z" />
									<path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466" />
								</svg>
							</Button>
						</div>
						<p class={s.hint}>
							{t(
								"services.hint.relaySessionId",
								"Shared with the mobile client to join the same relay room. Regenerating disconnects the current mobile session.",
							)}
						</p>
					</div>
				</Show>
			</SectionCard>
			<Show when={tokenDialog.dialogState()}>
				{(dialog) => (
					<ConfirmDialog
						visible={true}
						title={dialog().title}
						message={dialog().message}
						confirmLabel={dialog().confirmLabel}
						cancelLabel={dialog().cancelLabel}
						kind={dialog().kind}
						defaultButton={dialog().defaultButton}
						onConfirm={tokenDialog.handleConfirm}
						onClose={tokenDialog.handleClose}
					/>
				)}
			</Show>
		</div>
	);
};
