import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { appLogger } from "../../../../stores/appLogger";
import {
	type ConnectionState,
	type DeployMode,
	type DiscoveredSshHost,
	type DiscoveredSshHosts,
	type RemoteConnection,
	type RemoteTransport,
	remoteConnectionsStore,
	type SshAgentInfo,
	type SshHostStatus,
} from "../../../../stores/remoteConnections";
import s from "../../Settings.module.css";

// ---------------------------------------------------------------------------
// Remote Machines panel
// ---------------------------------------------------------------------------

/** Status dot color for remote connection state */
export function remoteStatusColor(status: string): string {
	switch (status) {
		case "connected":
			return "var(--success)";
		case "connecting":
		case "deploying":
			return "var(--activity)";
		// Reachable, but every route but /health answers 401 — the same amber as
		// "connecting" would read as progress, and green would be a lie.
		case "unauthenticated":
			return "var(--warning)";
		case "error":
			return "var(--error)";
		default:
			return "var(--fg-muted)";
	}
}

/** Human-readable label for remote connection status */
export function remoteStatusLabel(status: string, step?: string): string {
	switch (status) {
		case "connected":
			return "Connected";
		case "connecting":
			return "Connecting...";
		case "deploying":
			return `Deploying: ${step ?? "preparing"}`;
		case "unauthenticated":
			return "Not authenticated";
		case "error":
			return "Error";
		default:
			return "Disconnected";
	}
}

/** Transport summary string */
export function transportSummary(transport: RemoteTransport): string {
	if (transport.type === "Ssh") {
		return `${transport.ssh_user}@${transport.ssh_host}:${transport.ssh_port}`;
	}
	return transport.url;
}

/** Blank form state for adding/editing a remote machine */
export function emptyRemoteForm() {
	return {
		name: "",
		transportType: "Ssh" as "Ssh" | "Direct",
		sshHost: "",
		sshPort: 22,
		sshUser: "",
		identityFile: "",
		remoteDaemonPort: 9876,
		directUrl: "",
		authUsername: "",
		authPassword: "",
		deploy: "never" as DeployMode,
		surviveMinutes: 30,
		autoUpdate: false,
	};
}

/** Probe state of a discovered host, matched by resolved target and port (never by display name). */
export function discoveredHostAuth(host: DiscoveredSshHost, statuses: SshHostStatus[]): SshHostStatus["auth"] | null {
	return statuses.find((st) => st.target === host.target && st.port === host.port)?.auth ?? null;
}

/** Replace the status of the same target and port, or append it. */
export function withStatus(statuses: SshHostStatus[], status: SshHostStatus): SshHostStatus[] {
	return [...statuses.filter((st) => !(st.target === status.target && st.port === status.port)), status];
}

/** Add-form state prefilled from a discovered host. */
export function formFromDiscoveredHost(host: DiscoveredSshHost) {
	return {
		...emptyRemoteForm(),
		name: host.host,
		sshHost: host.host,
		sshPort: host.port ?? 22,
		sshUser: host.user ?? "",
	};
}

export const RemoteMachinesPanel: Component = () => {
	const [showAdd, setShowAdd] = createSignal(false);
	const [form, setForm] = createSignal(emptyRemoteForm());
	const [saving, setSaving] = createSignal(false);
	const [error, setError] = createSignal("");
	const [editingId, setEditingId] = createSignal<string | null>(null);
	const [editForm, setEditForm] = createSignal(emptyRemoteForm());
	const [passwordStored, setPasswordStored] = createSignal(false);
	const [sshHosts, setSshHosts] = createSignal<SshHostStatus[]>([]);
	const [probingHosts, setProbingHosts] = createSignal(false);
	const [discovered, setDiscovered] = createSignal<DiscoveredSshHosts>({ hosts: [], hashed_count: 0 });
	const [agentInfo, setAgentInfo] = createSignal<SshAgentInfo | null>(null);
	const [serviceBusyId, setServiceBusyId] = createSignal<string | null>(null);
	const [updateErrors, setUpdateErrors] = createSignal<Record<string, string>>({});

	onMount(() => {
		remoteConnectionsStore.hydrate();
		// Reads ~/.ssh/config and known_hosts and asks ssh-add; no host is contacted.
		remoteConnectionsStore
			.discoverSshHosts()
			.then(setDiscovered)
			.catch((e) => appLogger.error("settings", "Failed to discover SSH hosts", { error: String(e) }));
		remoteConnectionsStore
			.sshAgentInfo()
			.then(setAgentInfo)
			.catch((e) => appLogger.error("settings", "Failed to list SSH agent keys", { error: String(e) }));
	});

	function connectionList(): ConnectionState[] {
		const conns = remoteConnectionsStore.getConnections();
		return Object.values(conns);
	}

	async function addConnection() {
		const f = form();
		const name = f.name.trim();
		if (!name) {
			setError("Name is required");
			return;
		}
		if (f.transportType === "Ssh" && !f.sshHost.trim()) {
			setError("SSH host is required");
			return;
		}
		if (f.transportType === "Direct" && !f.directUrl.trim()) {
			setError("URL is required");
			return;
		}

		const transport: RemoteTransport =
			f.transportType === "Ssh"
				? {
						type: "Ssh",
						ssh_host: f.sshHost.trim(),
						ssh_port: f.sshPort,
						ssh_user: f.sshUser.trim(),
						identity_file: f.identityFile.trim() || null,
						remote_daemon_port: f.remoteDaemonPort,
					}
				: { type: "Direct", url: f.directUrl.trim() };

		const conn: RemoteConnection = {
			id: crypto.randomUUID(),
			name,
			transport,
			auth_username: f.authUsername.trim(),
			enabled: true,
			auto_update: f.autoUpdate,
			deploy: f.transportType === "Ssh" ? f.deploy : "never",
			survive_secs: Math.max(60, Math.round(f.surviveMinutes * 60)),
		};

		setSaving(true);
		setError("");
		try {
			await remoteConnectionsStore.addConnection(conn);
			// After the connection exists: the vault key is its id, so a password
			// stored first would belong to an id nothing would ever look up again.
			if (f.authPassword) await remoteConnectionsStore.setPassword(conn.id, f.authPassword);
			setForm(emptyRemoteForm());
			setShowAdd(false);
		} catch (e) {
			setError(String(e));
		} finally {
			setSaving(false);
		}
	}

	async function startEdit(conn: RemoteConnection) {
		setEditingId(conn.id);
		// The password itself is unreadable by design; all the form can show is
		// whether one is there, so a blank field means "keep it" instead of "none".
		setPasswordStored(await remoteConnectionsStore.hasPassword(conn.id).catch(() => false));
		setEditForm({
			name: conn.name,
			transportType: conn.transport.type,
			sshHost: conn.transport.type === "Ssh" ? conn.transport.ssh_host : "",
			sshPort: conn.transport.type === "Ssh" ? conn.transport.ssh_port : 22,
			sshUser: conn.transport.type === "Ssh" ? conn.transport.ssh_user : "",
			identityFile: conn.transport.type === "Ssh" ? (conn.transport.identity_file ?? "") : "",
			remoteDaemonPort: conn.transport.type === "Ssh" ? conn.transport.remote_daemon_port : 9876,
			directUrl: conn.transport.type === "Direct" ? conn.transport.url : "",
			authUsername: conn.auth_username,
			authPassword: "",
			deploy: conn.deploy,
			surviveMinutes: Math.max(1, Math.round(conn.survive_secs / 60)),
			autoUpdate: conn.auto_update ?? false,
		});
	}

	async function saveEdit(connState: ConnectionState) {
		const f = editForm();
		const transport: RemoteTransport =
			f.transportType === "Ssh"
				? {
						type: "Ssh",
						ssh_host: f.sshHost.trim(),
						ssh_port: f.sshPort,
						ssh_user: f.sshUser.trim(),
						identity_file: f.identityFile.trim() || null,
						remote_daemon_port: f.remoteDaemonPort,
					}
				: { type: "Direct", url: f.directUrl.trim() };

		const updated: RemoteConnection = {
			...connState.connection,
			name: f.name.trim(),
			transport,
			auth_username: f.authUsername.trim(),
			deploy: f.transportType === "Ssh" ? f.deploy : "never",
			survive_secs: Math.max(60, Math.round(f.surviveMinutes * 60)),
			auto_update: f.autoUpdate,
		};

		setSaving(true);
		setError("");
		try {
			// Disconnect first if connected, then save the updated connection
			if (connState.status !== "disconnected") {
				await remoteConnectionsStore.disconnect(connState.connection.id);
			}
			await remoteConnectionsStore.addConnection(updated);
			// Blank means "keep what is in the vault" — the field can never show it.
			if (f.authPassword) await remoteConnectionsStore.setPassword(updated.id, f.authPassword);
			setEditingId(null);
		} catch (e) {
			setError(String(e));
		} finally {
			setSaving(false);
		}
	}

	async function probeHosts() {
		setProbingHosts(true);
		setError("");
		try {
			setSshHosts(await remoteConnectionsStore.probeSshHosts());
		} catch (e) {
			setError(String(e));
		} finally {
			setProbingHosts(false);
		}
	}

	async function probeOneHost(host: DiscoveredSshHost) {
		setError("");
		try {
			const status = await remoteConnectionsStore.probeSshHost(host);
			setSshHosts((current) => withStatus(current, status));
		} catch (e) {
			setError(String(e));
		}
	}

	async function toggleInstalled(conn: RemoteConnection) {
		setServiceBusyId(conn.id);
		setError("");
		try {
			if (conn.deploy === "installed") await remoteConnectionsStore.uninstall(conn.id);
			else await remoteConnectionsStore.install(conn.id);
		} catch (e) {
			setError(String(e));
		} finally {
			setServiceBusyId(null);
		}
	}

	async function updateRemote(connState: ConnectionState) {
		if (connState.updateInProgress) return;
		const id = connState.connection.id;
		setServiceBusyId(id);
		setError("");
		setUpdateErrors((current) => ({ ...current, [id]: "" }));
		try {
			const { session_count, desktop_build, remote_build, source } = await remoteConnectionsStore.prepareUpdate(id);
			const details =
				`Remote: ${remote_build?.version ?? "unknown"} (${remote_build?.target ?? "unknown target"})\n` +
				`Selected: ${desktop_build.version} (${desktop_build.target}, ${source})\n` +
				`${session_count} live sessions will be lost. Update and restart remote?`;
			if (!window.confirm(details)) return;
			await remoteConnectionsStore.updateAndRestart(id, session_count, desktop_build.sha256);
		} catch (reason) {
			setUpdateErrors((current) => ({ ...current, [id]: String(reason) }));
		} finally {
			setServiceBusyId(null);
		}
	}

	async function removeConnection(id: string, name: string) {
		let confirmed: boolean;
		try {
			const { confirm } = await import("@tauri-apps/plugin-dialog");
			confirmed = await confirm(`Remove remote machine "${name}"?`, {
				title: "Remove remote machine",
				kind: "warning",
			});
		} catch {
			confirmed = window.confirm(`Remove remote machine "${name}"?`);
		}
		if (!confirmed) return;
		try {
			await remoteConnectionsStore.removeConnection(id);
		} catch (e) {
			appLogger.error("settings", "Failed to remove remote connection", { error: String(e) });
		}
	}

	/** Render transport-specific form fields */
	function TransportFields(props: {
		formData: ReturnType<typeof emptyRemoteForm>;
		setFormData: (updater: (f: ReturnType<typeof emptyRemoteForm>) => ReturnType<typeof emptyRemoteForm>) => void;
		/** A password is already in the vault — the field then means "replace it". */
		passwordStored?: boolean;
		sshHosts: SshHostStatus[];
		hostListId: string;
		probingHosts: boolean;
		onProbeHosts: () => void;
	}) {
		return (
			<>
				<select
					class={s.input}
					value={props.formData.transportType}
					onChange={(e) =>
						props.setFormData((f) => ({ ...f, transportType: e.currentTarget.value as "Ssh" | "Direct" }))
					}
				>
					<option value="Ssh">SSH</option>
					<option value="Direct">Direct</option>
				</select>
				<Show when={props.formData.transportType === "Ssh"}>
					<input
						type="text"
						class={s.input}
						list={props.hostListId}
						placeholder="Host (e.g. 192.168.1.100)"
						value={props.formData.sshHost}
						onInput={(e) => props.setFormData((f) => ({ ...f, sshHost: e.currentTarget.value }))}
					/>
					<datalist id={props.hostListId}>
						<For each={props.sshHosts}>
							{(host) => <option value={host.host} label={`${host.host} — ${host.auth.replaceAll("_", " ")}`} />}
						</For>
					</datalist>
					<button class={s.textBtn} type="button" onClick={props.onProbeHosts} disabled={props.probingHosts}>
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
							<path d="M7.5 1a6.5 6.5 0 1 0 4.05 11.58l2.44 2.43 1.02-1.02-2.43-2.44A6.5 6.5 0 0 0 7.5 1m0 1.5a5 5 0 1 1 0 10 5 5 0 0 1 0-10" />
						</svg>
						{props.probingHosts ? " Probing..." : " Probe SSH hosts"}
					</button>
					<div style={{ display: "flex", gap: "8px" }}>
						<div style={{ flex: 1 }}>
							<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Port</label>
							<input
								type="number"
								class={s.input}
								value={props.formData.sshPort}
								min={1}
								max={65535}
								onInput={(e) =>
									props.setFormData((f) => ({ ...f, sshPort: parseInt(e.currentTarget.value, 10) || 22 }))
								}
							/>
						</div>
						<div style={{ flex: 2 }}>
							<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>User</label>
							<input
								type="text"
								class={s.input}
								placeholder="SSH user"
								value={props.formData.sshUser}
								onInput={(e) => props.setFormData((f) => ({ ...f, sshUser: e.currentTarget.value }))}
							/>
						</div>
					</div>
					<input
						type="text"
						class={s.input}
						placeholder="Identity file (optional, e.g. ~/.ssh/id_rsa)"
						value={props.formData.identityFile}
						onInput={(e) => props.setFormData((f) => ({ ...f, identityFile: e.currentTarget.value }))}
					/>
					<div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
						<label style={{ "font-size": "12px", color: "var(--text-dimmed)", "white-space": "nowrap" }}>
							Remote daemon port
						</label>
						<input
							type="number"
							class={s.input}
							value={props.formData.remoteDaemonPort}
							min={1}
							max={65535}
							style={{ width: "90px" }}
							onInput={(e) =>
								props.setFormData((f) => ({ ...f, remoteDaemonPort: parseInt(e.currentTarget.value, 10) || 9876 }))
							}
						/>
					</div>
				</Show>
				<Show when={props.formData.transportType === "Direct"}>
					<input
						type="text"
						class={s.input}
						placeholder="URL (e.g. http://192.168.1.100:9876)"
						value={props.formData.directUrl}
						onInput={(e) => props.setFormData((f) => ({ ...f, directUrl: e.currentTarget.value }))}
					/>
				</Show>
				<Show when={props.formData.transportType === "Ssh"}>
					<label style={{ display: "grid", gap: "4px" }}>
						<span>Deployment</span>
						<select
							class={s.input}
							value={props.formData.deploy}
							onChange={(e) => props.setFormData((f) => ({ ...f, deploy: e.currentTarget.value as DeployMode }))}
						>
							<option value="never">Never deploy</option>
							<option value="on_connect">Deploy on connect</option>
							<option value="installed">Installed service</option>
						</select>
					</label>
					<label style={{ display: "grid", gap: "4px" }}>
						<span>Keep ephemeral daemon alive (minutes)</span>
						<input
							type="number"
							class={s.input}
							min={1}
							value={props.formData.surviveMinutes}
							onInput={(e) =>
								props.setFormData((f) => ({
									...f,
									surviveMinutes: Math.max(1, parseInt(e.currentTarget.value, 10) || 1),
								}))
							}
						/>
					</label>
				</Show>
				<label style={{ display: "flex", gap: "8px", "align-items": "center" }}>
					Auto-update remote daemons
					<input
						type="checkbox"
						style={{ order: -1 }}
						checked={props.formData.autoUpdate}
						onChange={(e) => props.setFormData((f) => ({ ...f, autoUpdate: e.currentTarget.checked }))}
					/>
				</label>
				<input
					type="text"
					class={s.input}
					placeholder="Auth username (for TUIC API authentication)"
					value={props.formData.authUsername}
					onInput={(e) => props.setFormData((f) => ({ ...f, authUsername: e.currentTarget.value }))}
				/>
				<input
					type="password"
					class={s.input}
					autocomplete="off"
					placeholder={props.passwordStored ? "Password (stored — leave blank to keep it)" : "Auth password"}
					value={props.formData.authPassword}
					onInput={(e) => props.setFormData((f) => ({ ...f, authPassword: e.currentTarget.value }))}
				/>
				<p class={s.hint} style={{ margin: 0 }}>
					The password is kept in the OS credential vault, never in connections.json. It is traded for the daemon's
					session token on every connect — <code>tuic-remote</code> authenticates every request, so a connection without
					one reaches only <code>/health</code>.
				</p>
			</>
		);
	}

	return (
		<div>
			<div class={s.group}>
				<div style={{ display: "flex", "align-items": "center", gap: "8px", "justify-content": "space-between" }}>
					<p class={s.hint} style={{ margin: 0 }}>
						Connect to TUIC instances running on other machines. SSH connections create an encrypted tunnel
						automatically.
					</p>
					<button
						class={s.copyBtn}
						onClick={() => {
							setShowAdd((v) => !v);
							setError("");
						}}
						title="Add remote machine"
						style={{ "font-size": "18px", "line-height": 1 }}
					>
						{showAdd() ? "−" : "+"}
					</button>
				</div>
			</div>

			{/* Discovered SSH hosts */}
			<div class={s.group}>
				<Show when={agentInfo()}>
					{(info) => (
						<p
							class={s.hint}
							style={{ margin: "0 0 6px", color: info().keys.length === 0 ? "var(--warning)" : undefined }}
							role={info().keys.length === 0 ? "alert" : undefined}
						>
							{info().keys.length === 0
								? "No SSH agent identities loaded: key-based connections will fail authentication. Add a key with ssh-add."
								: `SSH agent keys loaded (${info().keys.length}): ${info()
										.keys.map((k) => k.comment || k.fingerprint)
										.join(", ")}`}
						</p>
					)}
				</Show>
				<div style={{ display: "flex", "align-items": "center", gap: "8px", "justify-content": "space-between" }}>
					<span style={{ "font-weight": 500, "font-size": "13px" }}>
						Discovered SSH hosts ({discovered().hosts.length})
					</span>
					<button class={s.textBtn} type="button" onClick={probeHosts} disabled={probingHosts()}>
						{probingHosts() ? "Probing..." : "Probe config hosts"}
					</button>
				</div>
				<p class={s.hint} style={{ margin: "4px 0 0" }}>
					Only ssh config hosts are probed in bulk. A known_hosts host is contacted only when you press its own Probe.
				</p>
				<Show when={discovered().hashed_count > 0}>
					<p class={s.hint} style={{ margin: "4px 0 0" }}>
						{discovered().hashed_count} known_hosts entries are hashed and cannot be listed.
					</p>
				</Show>
				<div style={{ "max-height": "220px", "overflow-y": "auto", "margin-top": "6px" }}>
					<For each={discovered().hosts}>
						{(host) => {
							const auth = () => discoveredHostAuth(host, sshHosts());
							return (
								<div style={{ display: "flex", gap: "4px", "align-items": "center" }}>
									<button
										type="button"
										class={s.textBtn}
										title="Prefill the Add form with this host"
										style={{ display: "flex", flex: 1, "justify-content": "space-between", gap: "8px" }}
										onClick={() => {
											setForm(formFromDiscoveredHost(host));
											setShowAdd(true);
											setError("");
										}}
									>
										<span style={{ "font-family": "monospace", "font-size": "11px" }}>
											{host.user ? `${host.user}@` : ""}
											{host.host}
											{host.port ? `:${host.port}` : ""}
										</span>
										<span style={{ "font-size": "11px", color: "var(--text-dimmed)" }}>
											{auth() ? auth()?.replaceAll("_", " ") : host.source === "config" ? "ssh config" : "known_hosts"}
										</span>
									</button>
									<button
										type="button"
										class={s.textBtn}
										title="Contact this host once to check authentication"
										onClick={() => probeOneHost(host)}
									>
										Probe
									</button>
								</div>
							);
						}}
					</For>
				</div>
			</div>

			{/* Add form */}
			<Show when={showAdd()}>
				<div
					class={s.group}
					style={{ background: "var(--bg-secondary, rgba(255,255,255,0.03))", padding: "12px", "border-radius": "6px" }}
				>
					<div style={{ display: "grid", gap: "8px" }}>
						<input
							type="text"
							class={s.input}
							placeholder="Name (e.g. dev-server, staging)"
							value={form().name}
							onInput={(e) => setForm((f) => ({ ...f, name: e.currentTarget.value }))}
						/>
						<TransportFields
							formData={form()}
							setFormData={setForm}
							sshHosts={sshHosts()}
							hostListId="remote-ssh-hosts-add"
							probingHosts={probingHosts()}
							onProbeHosts={probeHosts}
						/>
						<div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
							<button class={s.textBtn} onClick={addConnection} disabled={saving()}>
								{saving() ? "Saving..." : "Save"}
							</button>
							<button
								class={s.textBtn}
								onClick={() => {
									setShowAdd(false);
									setForm(emptyRemoteForm());
									setError("");
								}}
							>
								Cancel
							</button>
						</div>
						<Show when={error()}>
							<p class={s.hint} style={{ color: "var(--error, #e06c75)" }}>
								{error()}
							</p>
						</Show>
					</div>
				</div>
			</Show>

			{/* Empty state */}
			<Show when={connectionList().length === 0 && !showAdd()}>
				<p class={s.hint} style={{ color: "var(--text-dimmed)" }}>
					No remote machines configured. Click <strong>+</strong> to add one.
				</p>
			</Show>

			{/* Connection list */}
			<For each={connectionList()}>
				{(connState) => {
					const conn = () => connState.connection;
					const isEditing = () => editingId() === conn().id;
					return (
						<div style={{ "border-bottom": "1px solid var(--border-subtle, rgba(255,255,255,0.06))" }}>
							<div class={s.group} style={{ display: "flex", "align-items": "center", gap: "8px", padding: "8px 0" }}>
								{/* Status dot */}
								<span
									style={{
										display: "inline-block",
										width: "8px",
										height: "8px",
										"border-radius": "50%",
										background: remoteStatusColor(connState.status),
										"flex-shrink": "0",
									}}
									title={remoteStatusLabel(connState.status, connState.deployStep)}
								/>
								{/* Info */}
								<div style={{ flex: 1, "min-width": 0 }}>
									<div style={{ display: "flex", "align-items": "center", gap: "6px", "flex-wrap": "wrap" }}>
										<span style={{ "font-weight": 500, "font-size": "13px" }}>{conn().name}</span>
										<span
											style={{
												"font-size": "10px",
												padding: "1px 5px",
												"border-radius": "3px",
												background:
													conn().transport.type === "Ssh"
														? "color-mix(in srgb, var(--activity) 15%, transparent)"
														: "color-mix(in srgb, var(--success) 15%, transparent)",
												color: conn().transport.type === "Ssh" ? "var(--activity)" : "var(--success)",
											}}
										>
											{conn().transport.type === "Ssh" ? "SSH" : "DIRECT"}
										</span>
										<Show when={connState.status !== "disconnected"}>
											<span style={{ "font-size": "11px", color: remoteStatusColor(connState.status) }}>
												{remoteStatusLabel(connState.status, connState.deployStep)}
											</span>
										</Show>
										<Show when={connState.outOfDate}>
											<span style={{ "font-size": "11px", color: "var(--attention)" }}>Remote out of date</span>
										</Show>
										<Show
											when={connState.outOfDate && connState.liveSessions !== undefined && connState.liveSessions > 0}
										>
											<span style={{ "font-size": "11px", color: "var(--attention)" }}>
												{connState.liveSessions} live sessions. Update available.
											</span>
										</Show>
									</div>
									<div
										class={s.hint}
										style={{
											margin: 0,
											"font-family": "monospace",
											"font-size": "11px",
											overflow: "hidden",
											"text-overflow": "ellipsis",
											"white-space": "nowrap",
										}}
									>
										{transportSummary(conn().transport)}
									</div>
									<Show
										when={(connState.status === "error" || connState.status === "unauthenticated") && connState.error}
									>
										<div class={s.hint} style={{ margin: 0, "font-size": "11px", color: "var(--error)" }}>
											{connState.error}
										</div>
									</Show>
									<Show
										when={
											connState.retryAfterSecs !== undefined &&
											(connState.status === "error" || connState.status === "disconnected")
										}
									>
										<div class={s.hint} role="status">
											Retry delay: {connState.retryAfterSecs}s
										</div>
									</Show>
									<Show when={connState.updateNotice}>
										<div class={s.hint} style={{ margin: 0, "font-size": "11px" }} role="status">
											{connState.updateNotice}
										</div>
									</Show>
									<Show when={updateErrors()[conn().id]}>
										<div class={s.hint} style={{ margin: 0, "font-size": "11px", color: "var(--error)" }} role="alert">
											{updateErrors()[conn().id]}
										</div>
									</Show>
								</div>
								{/* Connect / Disconnect */}
								<Show when={connState.status === "connected"}>
									<button
										class={s.textBtn}
										disabled={serviceBusyId() === conn().id || connState.updateInProgress}
										onClick={() => updateRemote(connState)}
									>
										{serviceBusyId() === conn().id ? "Updating..." : "Update & restart remote"}
									</button>
								</Show>
								<button
									class={s.textBtn}
									onClick={() => {
										// "unauthenticated" still holds a tunnel and a baseUrl, so it
										// disconnects like any live connection rather than re-dialling.
										if (connState.status === "disconnected" || connState.status === "error") {
											remoteConnectionsStore.connect(conn().id);
										} else {
											remoteConnectionsStore.disconnect(conn().id);
										}
									}}
								>
									{connState.status === "disconnected" || connState.status === "error" ? "Connect" : "Disconnect"}
								</button>
								<Show when={conn().transport.type === "Ssh"}>
									<button
										class={s.textBtn}
										disabled={serviceBusyId() === conn().id}
										onClick={() => toggleInstalled(conn())}
										title={conn().deploy === "installed" ? "Remove persistent service" : "Install persistent service"}
									>
										<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
											<path d="M7.25 1h1.5v8.1l2.65-2.65 1.06 1.06L8 11.97 3.54 7.51 4.6 6.45 7.25 9.1zM2 13h12v2H2z" />
										</svg>
										{serviceBusyId() === conn().id
											? " Working..."
											: conn().deploy === "installed"
												? " Uninstall"
												: " Install"}
									</button>
								</Show>
								{/* Edit */}
								<button
									class={s.copyBtn}
									style={{ "flex-shrink": 0 }}
									title="Edit"
									onClick={() => (isEditing() ? setEditingId(null) : startEdit(conn()))}
								>
									<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
										<path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293z" />
									</svg>
								</button>
								{/* Delete */}
								<button
									class={s.copyBtn}
									title="Remove"
									onClick={() => removeConnection(conn().id, conn().name)}
									style={{ color: "var(--error, #e06c75)", "flex-shrink": 0 }}
								>
									<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
										<path d="M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5m2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5m3 .5a.5.5 0 0 0-1 0v6a.5.5 0 0 0 1 0z" />
										<path d="M14.5 3a1 1 0 0 1-1 1H13v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4h-.5a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1H6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1h3.5a1 1 0 0 1 1 1zM4.118 4 4 4.059V13a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.059L11.882 4zM2.5 3h11V2h-11z" />
									</svg>
								</button>
							</div>
							{/* Edit inline panel */}
							<Show when={isEditing()}>
								<div
									style={{
										background: "var(--bg-secondary, rgba(255,255,255,0.03))",
										padding: "12px",
										"border-radius": "6px",
										"margin-bottom": "8px",
									}}
								>
									<div style={{ display: "grid", gap: "8px" }}>
										<TransportFields
											formData={editForm()}
											setFormData={setEditForm}
											passwordStored={passwordStored()}
											sshHosts={sshHosts()}
											hostListId={`remote-ssh-hosts-${conn().id}`}
											probingHosts={probingHosts()}
											onProbeHosts={probeHosts}
										/>
										<div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
											<button class={s.textBtn} onClick={() => saveEdit(connState)} disabled={saving()}>
												{saving() ? "Saving..." : "Save"}
											</button>
											<button
												class={s.textBtn}
												onClick={() => {
													setEditingId(null);
													setError("");
												}}
											>
												Cancel
											</button>
										</div>
										<Show when={error()}>
											<p class={s.hint} style={{ color: "var(--error, #e06c75)", margin: "4px 0 0" }}>
												{error()}
											</p>
										</Show>
									</div>
								</div>
							</Show>
						</div>
					);
				}}
			</For>
		</div>
	);
};
