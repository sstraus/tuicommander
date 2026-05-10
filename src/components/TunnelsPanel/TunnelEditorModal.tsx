import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import type { ForwardSpec, ProfileOptions, TunnelProfile } from "../../stores/tunnels";
import { tunnelsStore } from "../../stores/tunnels";
import s from "../SettingsPanel/Settings.module.css";
import d from "../shared/dialog.module.css";

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
	};
}

function emptyForward(): ForwardSpec {
	return { type: "Local", bind_port: 0 };
}

export const TunnelEditorModal: Component<TunnelEditorModalProps> = (props) => {
	const isEdit = () => !!props.profile;

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
				forwards: forwards(),
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
					class={d.body}
					style={{
						display: "flex",
						"flex-direction": "column",
						gap: "12px",
						"max-height": "60vh",
						"overflow-y": "auto",
					}}
				>
					<div class={s.group}>
						<label class={s.label}>Name</label>
						<input value={name()} onInput={(e) => setName(e.currentTarget.value)} />
					</div>

					<div style={{ display: "flex", gap: "8px" }}>
						<div class={s.group} style={{ flex: "1" }}>
							<label class={s.label}>Host</label>
							<input value={host()} onInput={(e) => setHost(e.currentTarget.value)} list="ssh-hosts-list" />
							<datalist id="ssh-hosts-list">
								<For each={sshHosts()}>{(h) => <option value={h} />}</For>
							</datalist>
						</div>
						<div class={s.group} style={{ width: "80px" }}>
							<label class={s.label}>Port</label>
							<input
								type="number"
								value={port()}
								onInput={(e) => setPort(Number.parseInt(e.currentTarget.value, 10) || 22)}
							/>
						</div>
					</div>

					<div class={s.group}>
						<label class={s.label}>User</label>
						<input value={user()} onInput={(e) => setUser(e.currentTarget.value)} />
					</div>

					<div class={s.group}>
						<label class={s.label}>Identity / Authentication</label>
						<div style={{ display: "flex", gap: "6px" }}>
							<input
								style={{ flex: "1" }}
								value={identityFile()}
								onInput={(e) => setIdentityFile(e.currentTarget.value)}
								placeholder="Leave empty to use SSH agent"
							/>
							<button
								type="button"
								class={d.cancelBtn}
								style={{
									flex: "none",
									padding: "4px 10px",
									"font-size": "var(--font-sm)",
									border: "none",
									"border-radius": "var(--radius-md)",
									cursor: "pointer",
								}}
								onClick={browseIdentityFile}
								title="Browse for key file"
							>
								Browse…
							</button>
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
							<button
								type="button"
								class={d.cancelBtn}
								style={{
									flex: "none",
									padding: "2px 10px",
									"font-size": "var(--font-sm)",
									border: "none",
									"border-radius": "var(--radius-md)",
									cursor: "pointer",
								}}
								onClick={addForward}
							>
								+ Add
							</button>
						</div>
						<For each={forwards()}>
							{(fw, idx) => (
								<div class={s.group} style={{ display: "flex", gap: "6px", "align-items": "center" }}>
									<select
										style={{ width: "80px" }}
										value={fw.type}
										onChange={(e) => updateForward(idx(), { type: e.currentTarget.value as "Local" | "Remote" })}
									>
										<option value="Local">Local</option>
										<option value="Remote">Remote</option>
									</select>
									<input
										inputMode="numeric"
										pattern="[0-9]*"
										placeholder="bind"
										style={{ width: "70px" }}
										value={fw.bind_port || ""}
										onInput={(e) =>
											updateForward(idx(), {
												bind_port: Number.parseInt(e.currentTarget.value, 10) || 0,
											})
										}
									/>
									<span style={{ color: "var(--fg-muted)" }}>:</span>
									<input
										placeholder="remote host"
										style={{ flex: "1" }}
										value={fw.remote_host ?? ""}
										onInput={(e) => updateForward(idx(), { remote_host: e.currentTarget.value })}
									/>
									<span style={{ color: "var(--fg-muted)" }}>:</span>
									<input
										inputMode="numeric"
										pattern="[0-9]*"
										placeholder="port"
										style={{ width: "70px" }}
										value={fw.remote_port ?? ""}
										onInput={(e) =>
											updateForward(idx(), {
												remote_port: Number.parseInt(e.currentTarget.value, 10) || 0,
											})
										}
									/>
									<button
										type="button"
										class={d.cancelBtn}
										style={{
											flex: "none",
											padding: "2px 8px",
											"font-size": "var(--font-sm)",
											border: "none",
											"border-radius": "var(--radius-md)",
											cursor: "pointer",
										}}
										onClick={() => removeForward(idx())}
									>
										x
									</button>
								</div>
							)}
						</For>
					</div>

					{/* Options */}
					<div style={{ display: "flex", gap: "8px" }}>
						<div class={s.group} style={{ flex: "1" }}>
							<label class={s.label}>ServerAliveInterval</label>
							<input
								type="number"
								value={options().server_alive_interval}
								onInput={(e) =>
									setOptions((o) => ({
										...o,
										server_alive_interval: Number.parseInt(e.currentTarget.value, 10) || 15,
									}))
								}
							/>
						</div>
						<div class={s.group} style={{ width: "160px" }}>
							<label class={s.label}>StrictHostKeyChecking</label>
							<select
								value={options().strict_host_key_checking}
								onChange={(e) =>
									setOptions((o) => ({
										...o,
										strict_host_key_checking: e.currentTarget.value as "Yes" | "AcceptNew",
									}))
								}
							>
								<option value="AcceptNew">AcceptNew</option>
								<option value="Yes">Yes</option>
							</select>
						</div>
					</div>

					<label class={s.toggle}>
						<input type="checkbox" checked={autoConnect()} onChange={(e) => setAutoConnect(e.currentTarget.checked)} />
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
					<button class={d.cancelBtn} onClick={props.onClose}>
						Cancel
					</button>
					<button class={d.primaryBtn} onClick={handleSave} disabled={saving()}>
						{saving() ? "Saving..." : "Save"}
					</button>
				</div>
			</div>
		</div>
	);
};
