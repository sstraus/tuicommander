import { type Component, createEffect, createSignal, For, Index, on, onCleanup, Show } from "solid-js";
import { useConfirmDialog } from "../../../../hooks/useConfirmDialog";
import { settingOptions } from "../../../../services/settingsCatalogue";
import { reportSettingsConflicts, settingsRefresh } from "../../../../services/settingsRefresh";
import { appLogger } from "../../../../stores/appLogger";
import {
	rpc,
	type UpstreamHeader,
	type UpstreamMcpConfig,
	type UpstreamMcpServer,
	type UpstreamTransport,
} from "../../../../transport";
import { mergeSettingsSnapshot } from "../../../../utils/mergeSettingsSnapshot";
import { handleOpenUrl } from "../../../../utils/openUrl";
import { ConfirmDialog } from "../../../ConfirmDialog";
import { Button } from "../../../ui/Button";
import { Input } from "../../../ui/Input";
import { Select } from "../../../ui/Select";
import { Switch } from "../../../ui/Switch";
import { ContextualHelp } from "../../ContextualHelp";
import { MachineSelector } from "../../MachineSelector";
import s from "../../Settings.module.css";
import { SectionCard } from "../../SettingsPrimitives";
import { SETTINGS_SECTION_UPSTREAM_MCP } from "../../sections";
import layout from "./ServiceRestyle.module.css";

/** Pure helper: should the Authorize button be shown for this server+status? */
export function shouldShowAuthorize(
	authType: string | undefined,
	status: string | undefined,
	enabled: boolean,
): boolean {
	// A disabled upstream is never connected, so authorizing it is meaningless.
	if (!enabled) return false;
	return authType === "oauth2" || status === "needs_auth" || status === "authenticating";
}

type UpstreamAuthForm = Pick<
	ReturnType<typeof emptyForm>,
	"transportType" | "authMethod" | "oauthClientId" | "oauthClientSecret" | "oauthScopes"
>;

/** Build the persisted auth block from the editor. An empty OAuth client ID
 * deliberately means no explicit auth block: the backend then performs DCR
 * after the upstream advertises OAuth. This must also clear a previous Bearer
 * block instead of silently retaining it. */
export function authFromUpstreamForm(
	form: UpstreamAuthForm,
	existingAuth?: UpstreamMcpServer["auth"],
): UpstreamMcpServer["auth"] {
	if (form.transportType !== "http" || form.authMethod !== "oauth2") return undefined;
	const clientId = form.oauthClientId.trim();
	if (!clientId) return undefined;
	const scopes = form.oauthScopes.trim()
		? form.oauthScopes
				.trim()
				.split(/[\s,]+/)
				.filter(Boolean)
		: undefined;
	const secret = form.oauthClientSecret.trim() || undefined;
	const existingOAuth = existingAuth?.type === "oauth2" ? existingAuth : undefined;
	return {
		type: "oauth2",
		client_id: clientId,
		...(secret ? { client_secret: secret } : {}),
		...(scopes?.length ? { scopes } : {}),
		...(existingOAuth?.authorization_endpoint ? { authorization_endpoint: existingOAuth.authorization_endpoint } : {}),
		...(existingOAuth?.token_endpoint ? { token_endpoint: existingOAuth.token_endpoint } : {}),
	};
}

interface HeaderRow extends UpstreamHeader {
	value: string;
	saved: boolean;
}

/** Mirror HTTP token syntax for immediate form feedback; backend is authoritative. */
export function headerFormError(rows: HeaderRow[], hasBearer: boolean): string {
	const names = new Set<string>();
	const reserved = new Set([
		"host",
		"content-length",
		"transfer-encoding",
		"connection",
		"content-type",
		"accept",
		"mcp-session-id",
		"mcp-protocol-version",
		"proxy-authorization",
		"cookie",
	]);
	for (const row of rows) {
		const name = row.name.toLowerCase();
		if (!/^[!#$%&'*+.^_`|~0-9a-z-]+$/i.test(row.name) || reserved.has(name))
			return "Invalid or reserved custom header name";
		if (names.has(name) || (hasBearer && name === "authorization"))
			return "Duplicate custom header or Authorization conflicts with Bearer";
		if (!row.saved && !row.value) return "A secret value is required for each new header";
		if (/[\r\n]/.test(row.value)) return "Invalid custom header value";
		names.add(name);
	}
	return "";
}

const HeaderRows: Component<{
	rows: HeaderRow[];
	onChange: (rows: HeaderRow[]) => void;
	onError: (message: string) => void;
}> = (props) => (
	<div style={{ display: "grid", gap: "8px" }}>
		<div style={{ display: "flex", "align-items": "center", "justify-content": "space-between" }}>
			<span class={s.hint}>Secret headers</span>
			<Button
				data-non-setting="Add or remove a secret-header draft row; the owner Save commits it"
				type="button"
				class={s.copyBtn}
				title="Add secret header"
				onClick={() =>
					props.onChange([...props.rows, { name: "", credential_ref: crypto.randomUUID(), value: "", saved: false }])
				}
			>
				+
			</Button>
		</div>
		<Index each={props.rows}>
			{(row, index) => (
				<div style={{ display: "grid", "grid-template-columns": "minmax(0, 1fr) minmax(0, 1fr) auto", gap: "8px" }}>
					<Input
						data-setting-id="upstreams.servers.{id}.headers.{name}.name"
						type="text"
						class={s.input}
						aria-label={`Header name ${index + 1}`}
						placeholder="Header name"
						onPaste={(e) => {
							if (/[\r\n]/.test(e.clipboardData?.getData("text") ?? "")) {
								e.preventDefault();
								props.onError("Invalid or reserved custom header name");
							}
						}}
						value={row().name}
						onInput={(e) =>
							props.onChange(props.rows.map((r, i) => (i === index ? { ...r, name: e.currentTarget.value } : r)))
						}
					/>
					<Input
						type="password"
						class={s.input}
						aria-label={`Header secret ${index + 1}`}
						autocomplete="new-password"
						placeholder={row().saved ? "Saved secret (leave blank to keep)" : "Secret value"}
						data-setting-id="upstreams.servers.{id}.credentials.headers.{name}.status"
						value={row().value}
						onInput={(e) =>
							props.onChange(props.rows.map((r, i) => (i === index ? { ...r, value: e.currentTarget.value } : r)))
						}
					/>
					<Button
						data-non-setting="Add or remove a secret-header draft row; the owner Save commits it"
						type="button"
						class={s.textBtn}
						title={`Remove header ${index + 1}`}
						onClick={() => props.onChange(props.rows.filter((_, i) => i !== index))}
					>
						−
					</Button>
				</div>
			)}
		</Index>
	</div>
);

interface StartOAuthResponse {
	authorization_url: string;
	state: string;
	cross_domain_as: boolean;
}

export async function startAuthorizeFlow(
	name: string,
	confirmAuthorization: (origin: string, name: string, crossDomain: boolean) => Promise<boolean>,
): Promise<void> {
	let resp: StartOAuthResponse;
	try {
		resp = await rpc<StartOAuthResponse>("start_mcp_upstream_oauth", { name });
	} catch (e) {
		const msg = String(e);
		appLogger.warn("network", `start_mcp_upstream_oauth failed: ${msg}`);
		const { message } = await import("@tauri-apps/plugin-dialog");
		await message(msg, { title: `Authorization failed for "${name}"`, kind: "error" });
		return;
	}

	// Surface the AS origin before opening the browser so the user sees where
	// they're being sent (AS mix-up defence, #1268-40e8). The backend never
	// blocks an off-domain authorization server — gateways and hosted IdPs make
	// that routine — it flags it via `cross_domain_as` so this dialog can say so
	// and let the user decide.
	let asOrigin = resp.authorization_url;
	try {
		asOrigin = new URL(resp.authorization_url).origin;
	} catch {
		// keep full URL if parsing fails
	}
	const proceed = await confirmAuthorization(asOrigin, name, resp.cross_domain_as);
	if (!proceed) {
		try {
			await rpc("cancel_mcp_upstream_oauth", { name });
		} catch (e) {
			appLogger.warn("network", `cancel_mcp_upstream_oauth failed: ${String(e)}`);
		}
		return;
	}
	handleOpenUrl(resp.authorization_url);
}

export interface UpstreamStatusEntry {
	name: string;
	status: "connecting" | "ready" | "circuit_open" | "disabled" | "failed" | "authenticating" | "needs_auth";
	transport: { type: string; url?: string; command?: string; args?: string[] };
	tool_count: number;
	tools: string[];
	metrics: { call_count: number; error_count: number; last_latency_ms: number };
	last_error?: string | null;
}

// ---------------------------------------------------------------------------
// Upstream MCP Servers panel (Tauri-only — uses OS keyring)
// ---------------------------------------------------------------------------

/** Blank form state for adding a new upstream */
function emptyForm() {
	return {
		name: "",
		transportType: "http" as "http" | "stdio",
		url: "",
		command: "",
		args: "",
		cwd: "",
		credential: "",
		timeout: 30,
		authMethod: "bearer" as "bearer" | "oauth2",
		headers: [] as HeaderRow[],
		oauthClientId: "",
		oauthClientSecret: "",
		oauthScopes: "",
	};
}

export const UpstreamMcpPanel: Component = () => {
	const oauthDialog = useConfirmDialog();
	/**
	 * Which machine's upstream list this panel edits.
	 *
	 * An upstream MCP server is dialled by the backend that holds it, so the list
	 * belongs to a machine the same way `agents.json` does. OAuth is the one part
	 * that cannot follow: it opens a browser, and a headless daemon has none.
	 */
	const [machine, setMachine] = createSignal<string | undefined>(undefined);
	const [upstreams, setUpstreams] = createSignal<UpstreamMcpServer[]>([]);
	const [upstreamStatus, setUpstreamStatus] = createSignal<UpstreamStatusEntry[]>([]);
	const [showAdd, setShowAdd] = createSignal(false);
	const [form, setForm] = createSignal(emptyForm());
	const [saving, setSaving] = createSignal(false);
	const [error, setError] = createSignal("");
	const [editingId, setEditingId] = createSignal<string | null>(null);
	const [editForm, setEditForm] = createSignal(emptyForm());
	let pendingEdit: UpstreamMcpServer | undefined;
	const confirmAuthorization = (origin: string, name: string, crossDomain: boolean) =>
		oauthDialog.confirm({
			title: "Authorize MCP server",
			message: crossDomain
				? `About to open ${origin} to authorize "${name}".\n\nThis authorization server is on a different domain than the MCP server. That is normal for gateways and hosted identity providers — but only continue if you recognise it.\n\nContinue?`
				: `About to open ${origin} to authorize "${name}".\n\nContinue?`,
			okLabel: "Continue",
			cancelLabel: "Cancel",
			kind: crossDomain ? "warning" : "info",
		});

	const refreshStatus = async () => {
		try {
			const snapshot = await rpc<{ upstreams: UpstreamStatusEntry[] }>("get_mcp_upstream_status", {}, machine());
			setUpstreamStatus(snapshot.upstreams ?? []);
		} catch {
			// Upstream status not available (e.g. server not running)
		}
	};

	// Load upstream config and preserve the existing three-second status cadence.
	// Re-runs when the machine changes: the list and the statuses are that
	// machine's, so both have to be re-read rather than merged.
	createEffect(
		on(machine, () => {
			onCleanup(
				settingsRefresh.register({
					section: "upstreams",
					connectionId: machine(),
					read: () => rpc<UpstreamMcpConfig>("load_mcp_upstreams", {}, machine()),
					apply: (config) => {
						const edited = config.servers?.find((server) => server.id === editingId());
						if (
							edited &&
							pendingEdit &&
							JSON.stringify(serverForm(edited)) === JSON.stringify(serverForm(pendingEdit))
						) {
							// This is our redacted save acknowledgement, not a competing draft.
							editBase = serverForm(edited);
						} else if (edited) {
							const incoming = serverForm(edited);
							const merged = mergeSettingsSnapshot(editBase, editForm(), incoming);
							editBase = incoming;
							setEditForm(merged.value);
							reportSettingsConflicts(merged.conflicts);
						}
						setUpstreams(config.servers ?? []);
					},
				}),
			);
			setUpstreams([]);
			setUpstreamStatus([]);
			rpc<UpstreamMcpConfig>("load_mcp_upstreams", {}, machine())
				.then((config) => setUpstreams(config.servers ?? []))
				.catch(() => {
					// Not in Tauri — silently skip
				});
			void refreshStatus();
			const interval = setInterval(refreshStatus, 3000);
			onCleanup(() => clearInterval(interval));
		}),
	);

	async function saveUpstreams(
		servers: UpstreamMcpServer[],
		credentials: { server_id: string; token: string; header?: UpstreamHeader }[] = [],
	): Promise<boolean> {
		setSaving(true);
		setError("");
		const base = { servers: [...upstreams()] };
		try {
			appLogger.info("mcp", "saveUpstreams: calling RPC", {
				serverCount: servers.length,
				names: servers.map((s) => s.name),
			});
			await rpc("save_mcp_upstreams", { base, config: { servers }, credentials }, machine());
			appLogger.info("mcp", "saveUpstreams: RPC succeeded");
			setUpstreams(servers);
			return true;
		} catch (e) {
			appLogger.error("mcp", "saveUpstreams: RPC failed", { error: String(e) });
			setError(String(e));
			return false;
		} finally {
			setSaving(false);
		}
	}

	async function saveHeaderRows(name: string, url: string, rows: HeaderRow[]): Promise<UpstreamHeader[]> {
		const headers: UpstreamHeader[] = [];
		for (const row of rows) {
			const header = { name: row.name, credential_ref: row.value ? crypto.randomUUID() : row.credential_ref };
			if (row.value) await rpc("save_mcp_upstream_credential", { name, url, token: row.value, header }, machine());
			headers.push(header);
		}
		return headers;
	}

	async function removeHeaderCredentials(name: string, headers: UpstreamHeader[]) {
		for (const header of headers) await rpc("delete_mcp_upstream_credential", { name, header }, machine());
	}

	async function addUpstream() {
		const f = form();
		const headerError =
			f.transportType === "http" ? headerFormError(f.headers, f.authMethod === "oauth2" || !!f.credential) : "";
		if (headerError) {
			setError(headerError);
			return;
		}
		const name = f.name.trim();
		if (!name) {
			setError("Name is required");
			return;
		}
		if (!/^[a-z0-9_-]+$/.test(name)) {
			setError("Name must be lowercase letters, digits, hyphens, or underscores only");
			return;
		}
		if (f.transportType === "http" && !f.url.trim()) {
			setError("URL is required for HTTP transport");
			return;
		}
		if (f.transportType === "stdio" && !f.command.trim()) {
			setError("Command is required for stdio transport");
			return;
		}
		const transport: UpstreamTransport =
			f.transportType === "http"
				? { type: "http", url: f.url.trim() }
				: {
						type: "stdio",
						command: f.command.trim(),
						args: f.args.trim() ? f.args.trim().split(/\s+/) : [],
						...(f.cwd.trim() ? { cwd: f.cwd.trim() } : {}),
					};

		const server: UpstreamMcpServer = {
			id: crypto.randomUUID(),
			name: f.name.trim(),
			transport,
			enabled: true,
			timeout_secs: f.timeout,
		};

		server.auth = authFromUpstreamForm(f);

		const credentials: { server_id: string; token: string; header?: UpstreamHeader }[] = [];
		if (f.transportType === "http") {
			server.headers = f.headers.map((row) => {
				const header = { name: row.name, credential_ref: row.value ? crypto.randomUUID() : row.credential_ref };
				if (row.value) credentials.push({ server_id: server.id, token: row.value, header });
				return header;
			});
			if (f.credential && f.authMethod === "bearer") {
				credentials.push({ server_id: server.id, token: f.credential });
				server.auth = { type: "bearer", token: "" };
			}
		}
		const ok = await saveUpstreams([...upstreams(), server], credentials);
		if (!ok) return;
		setForm(emptyForm());
		setShowAdd(false);
	}

	/** Get live status entry for an upstream by name */
	function getStatus(name: string): UpstreamStatusEntry | undefined {
		return upstreamStatus().find((u) => u.name === name);
	}

	/** Status dot color based on upstream connection state */
	function statusColor(st: string | undefined): string {
		switch (st) {
			case "ready":
				return "var(--green, #98c379)";
			case "connecting":
				return "var(--warning, #e5c07b)";
			case "authenticating":
				return "var(--info, #61afef)";
			case "needs_auth":
				return "var(--warning, #e5c07b)";
			case "circuit_open":
			case "failed":
				return "var(--error, #e06c75)";
			default:
				return "var(--text-dimmed)";
		}
	}

	/** Human-readable label for a status string. */
	function statusLabel(st: string | undefined): string {
		switch (st) {
			case "ready":
				return "Connected";
			case "connecting":
				return "Connecting…";
			case "authenticating":
				return "Awaiting authorization…";
			case "needs_auth":
				return "Authorize to connect";
			case "circuit_open":
				return "Retrying…";
			case "failed":
				return "Failed";
			case "disabled":
				return "Disabled";
			default:
				return st ?? "Unknown";
		}
	}

	function serverForm(server: UpstreamMcpServer): ReturnType<typeof emptyForm> {
		const isOAuth = server.auth?.type === "oauth2";
		return {
			name: server.name,
			transportType: server.transport.type,
			url: server.transport.type === "http" ? server.transport.url : "",
			command: server.transport.type === "stdio" ? server.transport.command : "",
			args: server.transport.type === "stdio" ? (server.transport.args?.join(" ") ?? "") : "",
			cwd: server.transport.type === "stdio" ? (server.transport.cwd ?? "") : "",
			credential: "",
			headers: (server.headers ?? []).map((header) => ({ ...header, value: "", saved: true })),
			timeout: server.timeout_secs,
			authMethod: isOAuth ? "oauth2" : "bearer",
			oauthClientId: isOAuth ? (server.auth as { client_id: string }).client_id : "",
			oauthClientSecret: isOAuth ? ((server.auth as { client_secret?: string }).client_secret ?? "") : "",
			oauthScopes: isOAuth ? ((server.auth as { scopes?: string[] }).scopes?.join(" ") ?? "") : "",
		};
	}

	let editBase = emptyForm();
	function startEdit(server: UpstreamMcpServer) {
		pendingEdit = undefined;
		setEditingId(server.id);
		editBase = serverForm(server);
		setEditForm(editBase);
	}

	async function saveEdit(server: UpstreamMcpServer) {
		const f = editForm();
		if (server.transport.type === "http" && f.transportType === "http" && (server.auth || server.headers?.length)) {
			try {
				if (new URL(server.transport.url).origin !== new URL(f.url.trim()).origin) {
					setError(
						"Cannot change the origin of an upstream with credentials. Add a new upstream with a different name for a different provider.",
					);
					return;
				}
			} catch {
				setError("Enter a valid HTTP URL");
				return;
			}
		}
		const hasBearer = f.authMethod === "oauth2" || !!f.credential || server.auth?.type === "bearer";
		const headerError = f.transportType === "http" ? headerFormError(f.headers, hasBearer) : "";
		if (headerError) {
			setError(headerError);
			return;
		}
		const transport: UpstreamTransport =
			f.transportType === "http"
				? { type: "http", url: f.url.trim() }
				: {
						type: "stdio",
						command: f.command.trim(),
						args: f.args.trim() ? f.args.trim().split(/\s+/) : [],
						...(f.cwd.trim() ? { cwd: f.cwd.trim() } : {}),
					};

		const updated: UpstreamMcpServer = {
			...server,
			transport,
			timeout_secs: f.timeout,
		};

		updated.auth = authFromUpstreamForm(f, server.auth);
		const oldMethod = server.auth?.type === "oauth2" ? "oauth2" : "bearer";
		const methodChanged = oldMethod !== f.authMethod;
		const replaceBearer = f.transportType === "http" && f.authMethod === "bearer" && !!f.credential;
		try {
			updated.headers = f.transportType === "http" ? await saveHeaderRows(server.name, f.url.trim(), f.headers) : [];
			if (f.transportType === "http" && f.authMethod === "bearer") {
				if (f.credential || server.auth?.type === "bearer") updated.auth = { type: "bearer", token: "" };
			}
		} catch {
			setError("Failed to update upstream credentials");
			return;
		}
		pendingEdit = updated;
		const ok = await saveUpstreams(upstreams().map((s) => (s.id === server.id ? updated : s)));
		if (!ok) {
			pendingEdit = undefined;
			return;
		}
		try {
			// Replacing the shared auth slot is destructive too: persist the config first.
			if (replaceBearer)
				await rpc(
					"save_mcp_upstream_credential",
					{ name: server.name, url: f.url.trim(), token: f.credential, replace: true },
					machine(),
				);
			else if (methodChanged) await rpc("delete_mcp_upstream_credential", { name: server.name }, machine());
			await removeHeaderCredentials(
				server.name,
				(server.headers ?? []).filter((old) => !updated.headers?.some((h) => h.credential_ref === old.credential_ref)),
			);
			await rpc("reconnect_mcp_upstream", { name: server.name }, machine());
		} catch {
			pendingEdit = undefined;
			setError("Settings saved, but credential cleanup or reconnect failed");
			return;
		}
		setEditForm(emptyForm());
		setEditingId(null);
		pendingEdit = undefined;
	}

	async function toggleUpstream(id: string, enabled: boolean) {
		const updated = upstreams().map((s) => (s.id === id ? { ...s, enabled } : s));
		await saveUpstreams(updated);
	}

	async function removeUpstream(id: string, name: string) {
		let confirmed: boolean;
		try {
			const { confirm } = await import("@tauri-apps/plugin-dialog");
			confirmed = await confirm(`Remove upstream "${name}"?`, { title: "Remove MCP upstream", kind: "warning" });
		} catch {
			confirmed = window.confirm(`Remove upstream "${name}"?`);
		}
		if (!confirmed) return;
		const removed = upstreams().find((server) => server.id === id);
		const ok = await saveUpstreams(upstreams().filter((s) => s.id !== id));
		if (!ok) return;
		await removeHeaderCredentials(name, removed?.headers ?? []).catch(() =>
			setError("Failed to remove header credentials"),
		);
		await rpc("delete_mcp_upstream_credential", { name }, machine()).catch((e) =>
			appLogger.error("settings", "Failed to delete MCP upstream credential", { error: String(e) }),
		);
	}

	async function clearUpstreamCredential(name: string) {
		try {
			await rpc("delete_mcp_upstream_credential", { name }, machine());
			setEditForm((current) => ({ ...current, credential: "" }));
			await rpc("reconnect_mcp_upstream", { name }, machine());
			setError("");
		} catch (e) {
			setError(`Failed to clear saved credential: ${String(e)}`);
		}
	}

	return (
		<div id={SETTINGS_SECTION_UPSTREAM_MCP} class={`${s.section} ${layout.upstreams}`}>
			<SectionCard
				title="Upstream MCP Servers"
				headingActions={
					<Button
						data-non-setting="Open, close or cancel the upstream editor"
						class={s.copyBtn}
						onClick={() => {
							setShowAdd((v) => !v);
							setError("");
						}}
						title="Add upstream server"
						style={{ "font-size": "18px", "line-height": 1 }}
					>
						{showAdd() ? "−" : "+"}
					</Button>
				}
			>
				<MachineSelector value={machine()} label="Upstreams on" onChange={setMachine} />

				{/* Add upstream form */}
				<Show when={showAdd()}>
					<div
						class={s.group}
						style={{
							background: "var(--bg-secondary, rgba(255,255,255,0.03))",
							padding: "12px",
							"border-radius": "6px",
						}}
					>
						<div style={{ display: "grid", gap: "8px" }}>
							<Input
								data-setting-id="upstreams.servers.{id}.name"
								type="text"
								class={s.input}
								placeholder="Name (e.g. context7, github)"
								value={form().name}
								onInput={(e) => {
									const normalized = e.currentTarget.value.toLowerCase().replace(/[^a-z0-9_-]/g, "-");
									e.currentTarget.value = normalized;
									setForm((f) => ({ ...f, name: normalized }));
								}}
							/>
							<Select
								data-setting-id="upstreams.servers.{id}.transport.type"
								class={s.input}
								value={form().transportType}
								onChange={(e) => setForm((f) => ({ ...f, transportType: e.currentTarget.value as "http" | "stdio" }))}
							>
								<For
									each={settingOptions("upstreams.servers.{id}.transport.type", {
										http: "HTTP (Streamable MCP)",
										stdio: "stdio (process)",
									})}
								>
									{(option) => <option value={option.value}>{option.label}</option>}
								</For>
							</Select>
							<Show when={form().transportType === "http"}>
								<Input
									data-setting-id="upstreams.servers.{id}.transport.url"
									type="text"
									class={s.input}
									placeholder="URL (e.g. http://localhost:8080/mcp)"
									value={form().url}
									onInput={(e) => setForm((f) => ({ ...f, url: e.currentTarget.value }))}
								/>
								<div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
									<label style={{ "font-size": "12px", color: "var(--text-dimmed)", "min-width": "90px" }}>
										Authentication
									</label>
									<ContextualHelp label="Authentication" />
									<Select
										data-setting-id="upstreams.servers.{id}.auth.type"
										class={s.input}
										value={form().authMethod}
										onChange={(e) =>
											setForm((f) => ({ ...f, authMethod: e.currentTarget.value as "bearer" | "oauth2" }))
										}
										style={{ flex: 1 }}
									>
										<For
											each={settingOptions("upstreams.servers.{id}.auth.type", {
												bearer: "Bearer token",
												oauth2: "OAuth 2.1",
											})}
										>
											{(option) => <option value={option.value}>{option.label}</option>}
										</For>
									</Select>
								</div>
								<Show when={form().authMethod === "bearer"}>
									<Input
										data-setting-id="upstreams.servers.{id}.auth.token"
										type="password"
										class={s.input}
										placeholder="API key (optional)"
										value={form().credential}
										onInput={(e) => setForm((f) => ({ ...f, credential: e.currentTarget.value }))}
									/>
								</Show>
								<HeaderRows
									onError={setError}
									rows={form().headers}
									onChange={(headers) => setForm((f) => ({ ...f, headers }))}
								/>
								<Show when={form().authMethod === "oauth2"}>
									<Input
										data-setting-id="upstreams.servers.{id}.auth.client_id"
										type="text"
										class={s.input}
										placeholder="Client ID (leave blank to auto-register)"
										value={form().oauthClientId}
										onInput={(e) => setForm((f) => ({ ...f, oauthClientId: e.currentTarget.value }))}
									/>
									<Input
										data-setting-id="upstreams.servers.{id}.auth.client_secret"
										type="password"
										class={s.input}
										placeholder="Client Secret (optional, for confidential clients)"
										value={form().oauthClientSecret}
										onInput={(e) => setForm((f) => ({ ...f, oauthClientSecret: e.currentTarget.value }))}
									/>
									<Input
										data-setting-id="upstreams.servers.{id}.auth.scopes"
										type="text"
										class={s.input}
										placeholder="Scopes (optional, space-separated)"
										value={form().oauthScopes}
										onInput={(e) => setForm((f) => ({ ...f, oauthScopes: e.currentTarget.value }))}
									/>
									<p class={s.hint} style={{ margin: "2px 0 0" }}>
										Authorization will begin after saving. The browser opens for sign-in.
									</p>
								</Show>
							</Show>
							<Show when={form().transportType === "stdio"}>
								<Input
									data-setting-id="upstreams.servers.{id}.transport.command"
									type="text"
									class={s.input}
									placeholder="Command (e.g. npx)"
									value={form().command}
									onInput={(e) => setForm((f) => ({ ...f, command: e.currentTarget.value }))}
								/>
								<Input
									data-setting-id="upstreams.servers.{id}.transport.args"
									type="text"
									class={s.input}
									placeholder="Args (space-separated, e.g. -y @modelcontextprotocol/server-filesystem /path)"
									value={form().args}
									onInput={(e) => setForm((f) => ({ ...f, args: e.currentTarget.value }))}
								/>
								<Input
									data-setting-id="upstreams.servers.{id}.transport.cwd"
									type="text"
									class={s.input}
									placeholder="Working directory (optional)"
									value={form().cwd}
									onInput={(e) => setForm((f) => ({ ...f, cwd: e.currentTarget.value }))}
								/>
							</Show>
							<div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
								<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Timeout (s):</label>
								<ContextualHelp label="Timeout (s):" />
								<Input
									data-setting-id="upstreams.servers.{id}.timeout_secs"
									type="number"
									class={s.input}
									value={form().timeout}
									min={0}
									max={300}
									style={{ width: "70px" }}
									onInput={(e) => setForm((f) => ({ ...f, timeout: parseInt(e.currentTarget.value, 10) || 30 }))}
								/>
							</div>
							{/* Same shape as the edit form below: primary + secondary on their own
						    right-aligned row, so the pair is never squeezed by the timeout field
						    and both buttons use the shared save/cancel styling. */}
							<div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
								<Button
									variant="primary"
									data-non-setting="Create, update or remove a stable upstream through its owner"
									class={s.saveBtn}
									onClick={addUpstream}
									disabled={saving()}
								>
									{saving() ? "Adding…" : "Add"}
								</Button>
								<Button
									data-non-setting="Open, close or cancel the upstream editor"
									class={s.testBtn}
									onClick={() => {
										setShowAdd(false);
										setForm(emptyForm());
										setError("");
									}}
								>
									Cancel
								</Button>
							</div>
							<Show when={error()}>
								<p class={s.hint} style={{ color: "var(--error, #e06c75)" }}>
									{error()}
								</p>
							</Show>
						</div>
					</div>
				</Show>

				{/* Upstream list */}
				<Show when={upstreams().length === 0 && !showAdd()}>
					<p class={s.hint} style={{ color: "var(--text-dimmed)" }}>
						No upstream servers configured.
					</p>
				</Show>

				<For each={upstreams()}>
					{(server) => {
						const st = () => getStatus(server.name);
						const isEditing = () => editingId() === server.id;
						return (
							<div style={{ "border-bottom": "1px solid var(--border-subtle, rgba(255,255,255,0.06))" }}>
								<div class={s.group} style={{ display: "flex", "align-items": "center", gap: "8px", padding: "8px 0" }}>
									{/* Enable toggle */}
									<div class={s.toggle} style={{ "margin-right": "4px" }}>
										<Switch
											role="checkbox"
											data-setting-id="upstreams.servers.{id}.enabled"
											checked={server.enabled}
											onChange={(e) => toggleUpstream(server.id, e.currentTarget.checked)}
										/>
									</div>
									{/* Info */}
									<div style={{ flex: 1, "min-width": 0 }}>
										<div style={{ display: "flex", "align-items": "center", gap: "6px", "flex-wrap": "wrap" }}>
											<span style={{ "font-weight": 500, "font-size": "13px" }}>{server.name}</span>
											<span
												style={{
													"font-size": "10px",
													padding: "1px 5px",
													"border-radius": "3px",
													background:
														server.transport.type === "http" ? "rgba(97,175,239,0.15)" : "rgba(152,195,121,0.15)",
													color: server.transport.type === "http" ? "#61afef" : "#98c379",
												}}
											>
												{server.transport.type.toUpperCase()}
											</span>
											{/* Status dot + label */}
											<Show when={st()}>
												{(entry) => (
													<>
														<span
															style={{
																display: "inline-block",
																width: "7px",
																height: "7px",
																"border-radius": "50%",
																background: statusColor(entry().status),
															}}
															title={statusLabel(entry().status)}
														/>
														<Show when={server.enabled && entry().status !== "disabled"}>
															<span style={{ "font-size": "11px", color: statusColor(entry().status) }}>
																{statusLabel(entry().status)}
															</span>
														</Show>
													</>
												)}
											</Show>
											<Show when={!server.enabled}>
												<span
													style={{
														"font-size": "10px",
														padding: "1px 5px",
														"border-radius": "3px",
														background: "rgba(255,255,255,0.05)",
														color: "var(--text-dimmed)",
													}}
												>
													Disabled
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
											{server.transport.type === "http"
												? server.transport.url
												: server.transport.command +
													(server.transport.args?.length ? " " + server.transport.args.join(" ") : "")}
										</div>
										{/* Metrics line */}
										<Show when={st()?.metrics}>
											{(m) => (
												<div class={s.hint} style={{ margin: 0, "font-size": "11px" }}>
													{st()!.tool_count} tools · {m().call_count} calls · {m().error_count} errors
													{m().last_latency_ms > 0 ? ` · ${m().last_latency_ms}ms` : ""}
												</div>
											)}
										</Show>
									</div>
									{/* Action buttons — never shrink */}
									{/* Authorize — show for explicit OAuth2 config OR when server auto-detected needs_auth (DCR case).
								    Local only: the flow opens a browser and listens on a loopback redirect,
								    neither of which a headless daemon has. `start_mcp_upstream_oauth` has no
								    HTTP route for the same reason. */}
									<Show when={!machine() && shouldShowAuthorize(server.auth?.type, st()?.status, server.enabled)}>
										<Show
											when={st()?.status === "authenticating"}
											fallback={
												<Button
													data-non-setting="Open, close or cancel the upstream editor"
													class={s.textBtn}
													style={{
														color: st()?.status === "needs_auth" ? "var(--warning, #e5c07b)" : "var(--info, #61afef)",
														"font-weight": st()?.status === "needs_auth" ? "600" : undefined,
													}}
													title={
														st()?.status === "needs_auth"
															? "Upstream requires authorization — click to open the provider's consent page"
															: st()?.status === "ready"
																? "Re-authorize — sign in again to switch account"
																: "Authorize via OAuth 2.1"
													}
													onClick={() => {
														startAuthorizeFlow(server.name, confirmAuthorization).catch((e) =>
															appLogger.warn("network", String(e)),
														);
													}}
												>
													{st()?.status === "ready" ? "Re-authorize" : "Authorize"}
												</Button>
											}
										>
											<Button
												data-non-setting="Typed upstream credential or OAuth workflow"
												class={s.textBtn}
												style={{ color: "var(--warning, #e5c07b)" }}
												title="Cancel authorization"
												onClick={() =>
													rpc("cancel_mcp_upstream_oauth", { name: server.name }).catch((e) =>
														appLogger.warn("network", String(e)),
													)
												}
											>
												Cancel
											</Button>
										</Show>
									</Show>
									<Button
										data-non-setting="Open, close or cancel the upstream editor"
										class={s.copyBtn}
										style={{ "flex-shrink": 0 }}
										title="Edit"
										onClick={() => (isEditing() ? setEditingId(null) : startEdit(server))}
									>
										<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
											<path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293z" />
										</svg>
									</Button>
									{/* Reconnect */}
									<Button
										data-non-setting="Reconnect the current upstream runtime without editing configuration"
										class={s.copyBtn}
										style={{ "flex-shrink": 0 }}
										title="Reconnect"
										onClick={() =>
											rpc("reconnect_mcp_upstream", { name: server.name }, machine()).catch((e) =>
												appLogger.warn("network", String(e)),
											)
										}
									>
										<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
											<path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2z" />
											<path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466" />
										</svg>
									</Button>
									{/* Remove */}
									<Button
										data-non-setting="Create, update or remove a stable upstream through its owner"
										class={s.copyBtn}
										title="Remove"
										onClick={() => removeUpstream(server.id, server.name)}
										style={{ color: "var(--error, #e06c75)", "flex-shrink": 0 }}
									>
										<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
											<path d="M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5m2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5m3 .5a.5.5 0 0 0-1 0v6a.5.5 0 0 0 1 0z" />
											<path d="M14.5 3a1 1 0 0 1-1 1H13v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4h-.5a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1H6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1h3.5a1 1 0 0 1 1 1zM4.118 4 4 4.059V13a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.059L11.882 4zM2.5 3h11V2h-11z" />
										</svg>
									</Button>
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
											<Show when={editForm().transportType === "http"}>
												<div>
													<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>URL</label>
													<ContextualHelp label="URL" />
													<Input
														data-setting-id="upstreams.servers.{id}.transport.url"
														type="text"
														class={s.input}
														value={editForm().url}
														onInput={(e) => setEditForm((f) => ({ ...f, url: e.currentTarget.value }))}
													/>
												</div>
												<div>
													<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Authentication</label>
													<ContextualHelp label="Authentication" />
													<Select
														data-setting-id="upstreams.servers.{id}.auth.type"
														class={s.input}
														value={editForm().authMethod}
														onChange={(e) =>
															setEditForm((f) => ({ ...f, authMethod: e.currentTarget.value as "bearer" | "oauth2" }))
														}
													>
														<For
															each={settingOptions("upstreams.servers.{id}.auth.type", {
																bearer: "Bearer token",
																oauth2: "OAuth 2.1",
															})}
														>
															{(option) => <option value={option.value}>{option.label}</option>}
														</For>
													</Select>
												</div>
												<Show when={editForm().authMethod === "bearer"}>
													<div>
														<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Bearer token</label>
														<ContextualHelp label="Bearer token" />
														<Input
															data-setting-id="upstreams.servers.{id}.auth.token"
															type="password"
															class={s.input}
															placeholder="Enter new token (leave blank to keep current)"
															value={editForm().credential}
															onInput={(e) => setEditForm((f) => ({ ...f, credential: e.currentTarget.value }))}
														/>
														<p class={s.hint} style={{ margin: "2px 0 0" }}>
															Leave blank to keep current token.
														</p>
														<Button
															data-non-setting="Typed upstream credential or OAuth workflow"
															type="button"
															class={s.textBtn}
															style={{ "margin-top": "4px" }}
															onClick={() => void clearUpstreamCredential(server.name)}
														>
															Clear saved token
														</Button>
													</div>
												</Show>
												<HeaderRows
													onError={setError}
													rows={editForm().headers}
													onChange={(headers) => setEditForm((f) => ({ ...f, headers }))}
												/>
												<Show when={editForm().authMethod === "oauth2"}>
													<div>
														<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>OAuth client ID</label>
														<Input
															data-setting-id="upstreams.servers.{id}.auth.client_id"
															type="text"
															class={s.input}
															placeholder="Leave blank to auto-register"
															value={editForm().oauthClientId}
															onInput={(e) => setEditForm((f) => ({ ...f, oauthClientId: e.currentTarget.value }))}
														/>
													</div>
													<div>
														<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Client Secret</label>
														<ContextualHelp label="Client Secret" />
														<Input
															data-setting-id="upstreams.servers.{id}.auth.client_secret"
															type="password"
															class={s.input}
															placeholder="Optional, for confidential clients"
															value={editForm().oauthClientSecret}
															onInput={(e) => setEditForm((f) => ({ ...f, oauthClientSecret: e.currentTarget.value }))}
														/>
													</div>
													<div>
														<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Scopes</label>
														<ContextualHelp label="Scopes" />
														<Input
															data-setting-id="upstreams.servers.{id}.auth.scopes"
															type="text"
															class={s.input}
															placeholder="Optional, space-separated"
															value={editForm().oauthScopes}
															onInput={(e) => setEditForm((f) => ({ ...f, oauthScopes: e.currentTarget.value }))}
														/>
													</div>
												</Show>
											</Show>
											<Show when={editForm().transportType === "stdio"}>
												<div>
													<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Command</label>
													<ContextualHelp label="Command" />
													<Input
														data-setting-id="upstreams.servers.{id}.transport.command"
														type="text"
														class={s.input}
														value={editForm().command}
														onInput={(e) => setEditForm((f) => ({ ...f, command: e.currentTarget.value }))}
													/>
												</div>
												<div>
													<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Args</label>
													<Input
														data-setting-id="upstreams.servers.{id}.transport.args"
														type="text"
														class={s.input}
														value={editForm().args}
														onInput={(e) => setEditForm((f) => ({ ...f, args: e.currentTarget.value }))}
													/>
												</div>
												<div>
													<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Working directory</label>
													<ContextualHelp label="Working directory" />
													<Input
														data-setting-id="upstreams.servers.{id}.transport.cwd"
														type="text"
														class={s.input}
														value={editForm().cwd}
														placeholder="Optional"
														onInput={(e) => setEditForm((f) => ({ ...f, cwd: e.currentTarget.value }))}
													/>
												</div>
											</Show>
											<div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
												<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>Timeout (s):</label>
												<ContextualHelp label="Timeout (s):" />
												<Input
													data-setting-id="upstreams.servers.{id}.timeout_secs"
													type="number"
													class={s.input}
													value={editForm().timeout}
													min={0}
													max={300}
													style={{ width: "70px" }}
													onInput={(e) =>
														setEditForm((f) => ({ ...f, timeout: parseInt(e.currentTarget.value, 10) || 30 }))
													}
												/>
											</div>
											{/* Discovered tools */}
											<Show when={st()?.tools?.length}>
												<div>
													<label style={{ "font-size": "12px", color: "var(--text-dimmed)" }}>
														Discovered tools ({st()!.tools.length})
													</label>
													<div
														style={{
															"font-family": "monospace",
															"font-size": "11px",
															color: "var(--text-dimmed)",
															"margin-top": "4px",
															"line-height": "1.6",
														}}
													>
														{st()!.tools.join(", ")}
													</div>
												</div>
											</Show>
											<div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
												<Button
													variant="primary"
													data-non-setting="Create, update or remove a stable upstream through its owner"
													class={s.saveBtn}
													onClick={() => saveEdit(server)}
													disabled={saving()}
												>
													{saving() ? "Saving…" : "Save"}
												</Button>
												<Button
													data-non-setting="Open, close or cancel the upstream editor"
													class={s.testBtn}
													onClick={() => {
														setEditingId(null);
														setEditForm(emptyForm());
														setError("");
													}}
												>
													Cancel
												</Button>
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
				<Show when={oauthDialog.dialogState()}>
					{(dialog) => (
						<ConfirmDialog
							visible={true}
							title={dialog().title}
							message={dialog().message}
							confirmLabel={dialog().confirmLabel}
							cancelLabel={dialog().cancelLabel}
							kind={dialog().kind}
							defaultButton={dialog().defaultButton}
							onConfirm={oauthDialog.handleConfirm}
							onClose={oauthDialog.handleClose}
						/>
					)}
				</Show>
			</SectionCard>
		</div>
	);
};
