import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import { settingOptions } from "../../services/settingsCatalogue";
import { appLogger } from "../../stores/appLogger";
import { registerModal } from "../../stores/modalStack";
import type { ForwardSpec, ProfileOptions, TunnelProfile } from "../../stores/tunnels";
import { tunnelsStore } from "../../stores/tunnels";
import { openDialog as openFileDialog } from "../../utils/nativeDialog";
import s from "../SettingsPanel/Settings.module.css";
import d from "../shared/dialog.module.css";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import layout from "./TunnelEditorModal.module.css";

interface AgentKey {
	fingerprint: string;
	comment: string;
	key_type: string;
}

interface SshAgentInfo {
	keys: AgentKey[];
	agent_type: string;
}

interface TunnelEditorModalProps {
	profile?: TunnelProfile;
	onClose: () => void;
}

function defaultOptions(): ProfileOptions {
	return {
		server_alive_interval: 15,
		server_alive_count_max: 3,
		strict_host_key_checking: "Yes",
		compression: true,
	};
}

function emptyForward(): ForwardSpec {
	return { type: "Local", bind_port: 0 };
}

function parsePort(value: string, fallback: number): number {
	const parsed = Number.parseInt(value, 10);
	return Number.isFinite(parsed) ? parsed : fallback;
}

export function normalizeForwardForType(forward: ForwardSpec): ForwardSpec {
	if (forward.type === "Remote") {
		return {
			type: "Remote",
			bind_port: forward.bind_port,
			local_host: forward.local_host ?? forward.remote_host ?? "",
			local_port: forward.local_port ?? forward.remote_port ?? 0,
		};
	}

	return {
		type: "Local",
		bind_port: forward.bind_port,
		remote_host: forward.remote_host ?? forward.local_host ?? "",
		remote_port: forward.remote_port ?? forward.local_port ?? 0,
	};
}

export function convertForwardType(
	forward: ForwardSpec,
	type: ForwardSpec["type"],
	defaultRemoteHost = "",
): ForwardSpec {
	if (type === "Remote") {
		return {
			type,
			bind_port: forward.bind_port,
			local_host: forward.local_host ?? "127.0.0.1",
			local_port: forward.local_port ?? forward.remote_port ?? 0,
		};
	}

	return {
		type,
		bind_port: forward.bind_port,
		remote_host: forward.remote_host ?? defaultRemoteHost,
		remote_port: forward.remote_port ?? forward.local_port ?? 0,
	};
}

export const TunnelEditorModal: Component<TunnelEditorModalProps> = (props) => {
	const isEdit = () => !!props.profile;
	// Escape-to-close handled centrally (stores/modalStack): closes this modal and
	// keeps Escape from reaching the terminal underneath.
	registerModal(props.onClose);

	const [name, setName] = createSignal(props.profile?.name ?? "");
	const [host, setHost] = createSignal(props.profile?.host ?? "");
	const [port, setPort] = createSignal(props.profile?.port ?? 22);
	const [user, setUser] = createSignal(props.profile?.user ?? "");
	const [identityFile, setIdentityFile] = createSignal(props.profile?.identity_file ?? "");
	const [forwards, setForwards] = createSignal<ForwardSpec[]>(props.profile?.forwards ?? []);
	const [autoConnect, setAutoConnect] = createSignal(props.profile?.auto_connect ?? false);
	const [options, setOptions] = createSignal<ProfileOptions>(props.profile?.options ?? defaultOptions());
	const [saving, setSaving] = createSignal(false);
	const [error, setError] = createSignal("");
	const [sshHosts, setSshHosts] = createSignal<string[]>([]);
	const [agentInfo, setAgentInfo] = createSignal<SshAgentInfo>({ keys: [], agent_type: "" });

	onMount(async () => {
		try {
			const [hosts, info] = await Promise.all([
				invoke<string[]>("list_ssh_config_hosts").catch(() => []),
				invoke<SshAgentInfo>("list_ssh_agent_keys").catch(() => ({ keys: [], agent_type: "" })),
			]);
			if (hosts?.length) setSshHosts(hosts);
			setAgentInfo(info);
		} catch {
			// best-effort
		}
	});

	const browseIdentityFile = async () => {
		const home = await invoke<string | null>("resolve_terminal_path", { path: "~/.ssh" }).catch(() => null);
		const selected = await openFileDialog({
			title: "Select SSH Identity File",
			defaultPath: home ?? undefined,
			multiple: false,
		});
		if (selected) setIdentityFile(selected);
	};

	const addForward = () => setForwards((f) => [...f, { ...emptyForward(), remote_host: host().trim() || undefined }]);
	const removeForward = (idx: number) => setForwards((f) => f.filter((_, i) => i !== idx));
	const updateForward = (idx: number, patch: Partial<ForwardSpec>) => {
		setForwards((f) => f.map((fw, i) => (i === idx ? { ...fw, ...patch } : fw)));
	};
	const replaceForward = (idx: number, next: ForwardSpec) => {
		setForwards((f) => f.map((fw, i) => (i === idx ? next : fw)));
	};

	const handleSave = async () => {
		const trimmedName = name().trim();
		const trimmedHost = host().trim();
		const trimmedUser = user().trim();

		if (!trimmedName || !trimmedHost || !trimmedUser) {
			setError("Name, host, and user are required.");
			return;
		}

		setSaving(true);
		setError("");

		try {
			const data = {
				name: trimmedName,
				host: trimmedHost,
				port: port(),
				user: trimmedUser,
				identity_file: identityFile().trim() || null,
				forwards: forwards().map(normalizeForwardForType),
				options: options(),
				auto_connect: autoConnect(),
			};

			if (isEdit() && props.profile) {
				await tunnelsStore.updateProfile({ id: props.profile.id, ...data });
			} else {
				await tunnelsStore.createProfile(data);
			}
			props.onClose();
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			setError(msg);
			appLogger.error("store", "TunnelEditor save failed", err);
		} finally {
			setSaving(false);
		}
	};

	return (
		<div class={d.overlay} onClick={(e) => e.target === e.currentTarget && props.onClose()}>
			<div class={d.popover} style={{ width: "520px" }}>
				<div class={d.header}>
					<h4>{isEdit() ? "Edit Tunnel" : "New Tunnel"}</h4>
				</div>

				<div
					class={`${d.body} ${layout.body}`}
					style={{
						display: "flex",
						"flex-direction": "column",
						gap: "12px",
						"max-height": "60vh",
						"overflow-y": "auto",
					}}
				>
					<div class={`${s.group} ${layout.field}`}>
						<label class={s.label}>Name</label>
						<Input
							data-setting-id="tunnels.profiles.{id}.name"
							value={name()}
							onInput={(e) => setName(e.currentTarget.value)}
						/>
					</div>

					<div style={{ display: "flex", gap: "8px" }}>
						<div class={`${s.group} ${layout.field}`} style={{ flex: "1" }}>
							<label class={s.label}>Host</label>
							<Input
								data-setting-id="tunnels.profiles.{id}.host"
								value={host()}
								onInput={(e) => setHost(e.currentTarget.value)}
								list="ssh-hosts-list"
							/>
							<datalist id="ssh-hosts-list">
								<For each={sshHosts()}>{(h) => <option value={h} />}</For>
							</datalist>
						</div>
						<div class={`${s.group} ${layout.field}`} style={{ width: "80px" }}>
							<label class={s.label}>Port</label>
							<Input
								data-setting-id="tunnels.profiles.{id}.port"
								type="number"
								value={port()}
								onInput={(e) => setPort(parsePort(e.currentTarget.value, 22))}
							/>
						</div>
					</div>

					<div class={`${s.group} ${layout.field}`}>
						<label class={s.label}>User</label>
						<Input
							data-setting-id="tunnels.profiles.{id}.user"
							value={user()}
							onInput={(e) => setUser(e.currentTarget.value)}
						/>
					</div>

					<div class={`${s.group} ${layout.field}`}>
						<label class={s.label}>Identity / Authentication</label>
						<div style={{ display: "flex", gap: "6px" }}>
							<Input
								data-setting-id="tunnels.profiles.{id}.identity_file"
								style={{ flex: "1" }}
								value={identityFile()}
								onInput={(e) => setIdentityFile(e.currentTarget.value)}
								placeholder="Leave empty to use SSH agent"
							/>
							<Button
								data-non-setting="Choose a key-file path for the existing tunnel draft"
								type="button"
								class={d.cancelBtn}
								style={{ flex: "none" }}
								onClick={browseIdentityFile}
								title="Browse for key file"
							>
								Browse…
							</Button>
						</div>
						<Show when={agentInfo().agent_type}>
							<div style={{ "margin-top": "6px", "font-size": "var(--font-sm)", color: "var(--fg-muted)" }}>
								<span style={{ color: agentInfo().keys.length > 0 ? "var(--success)" : "var(--fg-muted)" }}>
									{agentInfo().agent_type}:
								</span>{" "}
								<Show when={agentInfo().keys.length > 0} fallback="no keys loaded">
									{agentInfo()
										.keys.map((k) => `${k.comment} (${k.key_type})`)
										.join(", ")}
								</Show>
							</div>
						</Show>
					</div>

					{/* Port Forwards */}
					<div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
						<div style={{ display: "flex", "align-items": "center", "justify-content": "space-between" }}>
							<label class={s.label}>Port Forwards</label>
							<Button
								data-non-setting="Add or remove a typed forward in the tunnel draft"
								type="button"
								class={d.cancelBtn}
								style={{ flex: "none" }}
								onClick={addForward}
							>
								+ Add
							</Button>
						</div>
						<For each={forwards()}>
							{(fw, idx) => (
								<div
									class={`${s.group} ${layout.field}`}
									style={{ display: "flex", gap: "6px", "align-items": "center" }}
								>
									<Select
										data-setting-id="tunnels.profiles.{id}.forwards"
										compact
										style={{ width: "80px" }}
										value={fw.type}
										onChange={(e) =>
											replaceForward(
												idx(),
												convertForwardType(fw, e.currentTarget.value as ForwardSpec["type"], host().trim()),
											)
										}
									>
										<option value="Local">Local</option>
										<option value="Remote">Remote</option>
									</Select>
									<Input
										data-setting-id="tunnels.profiles.{id}.forwards"
										compact
										inputMode="numeric"
										pattern="[0-9]*"
										placeholder="bind"
										style={{ width: "70px" }}
										value={fw.bind_port || ""}
										onInput={(e) =>
											updateForward(idx(), {
												bind_port: parsePort(e.currentTarget.value, 0),
											})
										}
									/>
									<span style={{ color: "var(--fg-muted)" }}>:</span>
									<Input
										data-setting-id="tunnels.profiles.{id}.forwards"
										compact
										placeholder={fw.type === "Remote" ? "local host" : "remote host"}
										style={{ flex: "1" }}
										value={fw.type === "Remote" ? (fw.local_host ?? fw.remote_host ?? "") : (fw.remote_host ?? "")}
										onInput={(e) =>
											updateForward(
												idx(),
												fw.type === "Remote"
													? { local_host: e.currentTarget.value }
													: { remote_host: e.currentTarget.value },
											)
										}
									/>
									<span style={{ color: "var(--fg-muted)" }}>:</span>
									<Input
										data-setting-id="tunnels.profiles.{id}.forwards"
										compact
										inputMode="numeric"
										pattern="[0-9]*"
										placeholder="port"
										style={{ width: "70px" }}
										value={fw.type === "Remote" ? (fw.local_port ?? fw.remote_port ?? "") : (fw.remote_port ?? "")}
										onInput={(e) =>
											updateForward(
												idx(),
												fw.type === "Remote"
													? { local_port: parsePort(e.currentTarget.value, 0) }
													: { remote_port: parsePort(e.currentTarget.value, 0) },
											)
										}
									/>
									<Button
										data-non-setting="Add or remove a typed forward in the tunnel draft"
										type="button"
										class={d.cancelBtn}
										style={{ flex: "none" }}
										size="compact"
										onClick={() => removeForward(idx())}
									>
										x
									</Button>
								</div>
							)}
						</For>
					</div>

					{/* Options */}
					<div style={{ display: "flex", gap: "8px" }}>
						<div class={`${s.group} ${layout.field}`} style={{ flex: "1" }}>
							<label class={s.label}>ServerAliveInterval</label>
							<Input
								data-setting-id="tunnels.profiles.{id}.options.server_alive_interval"
								type="number"
								value={options().server_alive_interval}
								onInput={(e) =>
									setOptions((o) => ({
										...o,
										server_alive_interval: parsePort(e.currentTarget.value, 15),
									}))
								}
							/>
						</div>
						<div class={`${s.group} ${layout.field}`} style={{ width: "160px" }}>
							<label class={s.label}>StrictHostKeyChecking</label>
							<Select
								data-setting-id="tunnels.profiles.{id}.options.strict_host_key_checking"
								value={options().strict_host_key_checking}
								onChange={(e) =>
									setOptions((o) => ({
										...o,
										strict_host_key_checking: e.currentTarget.value as "Yes" | "AcceptNew",
									}))
								}
							>
								<For
									each={settingOptions("tunnels.profiles.{id}.options.strict_host_key_checking", {
										AcceptNew: "AcceptNew",
										Yes: "Yes",
									})}
								>
									{(option) => <option value={option.value}>{option.label}</option>}
								</For>
							</Select>
						</div>
					</div>

					<label class={s.toggle}>
						<input
							data-setting-id="tunnels.profiles.{id}.options.compression"
							type="checkbox"
							checked={options().compression}
							onChange={(e) => setOptions((o) => ({ ...o, compression: e.currentTarget.checked }))}
						/>
						<span>Compress the channel (ssh -C)</span>
					</label>

					<label class={s.toggle}>
						<input
							data-setting-id="tunnels.profiles.{id}.auto_connect"
							type="checkbox"
							checked={autoConnect()}
							onChange={(e) => setAutoConnect(e.currentTarget.checked)}
						/>
						<span>Connect automatically on startup</span>
					</label>

					<Show when={error()}>
						<div
							class={d.error}
							ref={(el) => requestAnimationFrame(() => el.scrollIntoView({ behavior: "smooth", block: "nearest" }))}
						>
							{error()}
						</div>
					</Show>
				</div>

				<div class={d.actions}>
					<Button
						data-non-setting="Cancel the tunnel editor without persistence"
						class={d.cancelBtn}
						onClick={props.onClose}
					>
						Cancel
					</Button>
					<Button
						data-non-setting="Commit the complete typed tunnel draft through its owner"
						variant="primary"
						class={d.primaryBtn}
						onClick={handleSave}
						disabled={saving()}
					>
						{saving() ? "Saving..." : "Save"}
					</Button>
				</div>
			</div>
		</div>
	);
};
