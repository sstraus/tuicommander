import { createSignal, onCleanup, onMount } from "solid-js";
import { appLogger } from "../../../../stores/appLogger";
import { rpc } from "../../../../transport";
import { updateAppConfig } from "../../../../utils/updateAppConfig";

/** Config shapes shared by the MCP and Remote Access pages (both read/write
 * slices of the same persisted `AppConfig`). */
export interface ServerConfig {
	enabled: boolean;
	port: number;
	ipv6_enabled: boolean;
}

export interface AuthConfig {
	username: string;
	password_hash: string;
	session_token_duration_secs: number;
	session_token_exists?: boolean;
	lan_auth_bypass: boolean;
}

export interface RelayConfig {
	enabled: boolean;
	url: string;
	token?: string;
	token_exists?: boolean;
	session_id: string;
}

export interface ServicesConfig {
	server: ServerConfig;
	auth: AuthConfig;
	relay: RelayConfig;
}

export interface AppConfig {
	shell: string | null;
	font_family: string;
	font_size: number;
	theme: string;
	mcp_server_enabled: boolean;
	services: ServicesConfig;
	mcp_capabilities: { denied: string[]; migration_unknown_tools: string[] };
	settings_mcp_enabled: boolean;
	collapse_tools: boolean;
	mcp_call_audit_enabled: boolean;
}

/** Save a single config field (load-modify-save pattern matching other tabs) */
export async function saveConfigField(updater: (config: AppConfig) => void): Promise<void> {
	try {
		await updateAppConfig<AppConfig>(updater);
	} catch (e) {
		appLogger.error("config", "Failed to save config", e);
	}
}

export interface NativeMcpTool {
	name: string;
	summary: string;
	description: string;
}

export interface McpCapability {
	name: string;
	title: string;
	description: string;
	domain: string;
	parent: string | null;
	section: "always" | "rare" | "conditional";
	available?: boolean;
	availability?: string;
}

export interface McpStatus {
	/** True only while the backend TCP task serves with loaded TLS configuration. */
	tls_active: boolean;
	native_capabilities: McpCapability[];
	/** Unfiltered native registry, including tools disabled in config. */
	native_tools: NativeMcpTool[];
	enabled: boolean;
	running: boolean;
	remote_port: number | null;
	active_sessions: number;
	/** Connected MCP protocol clients (reaped after 1h idle) */
	mcp_clients: number;
	max_sessions: number;
	/** null = remote disabled, true = TCP reachable, false = likely firewalled */
	reachable?: boolean | null;
}

interface RelayStatus {
	enabled: boolean;
	connected: boolean;
	url: string;
	session_id: string;
}

/**
 * Poll `get_mcp_status` + `get_relay_status` every 3s while the calling
 * component is mounted, and clear the interval on unmount.
 *
 * The MCP and Remote Access pages both need pieces of this snapshot
 * (`running` for MCP, `reachable` for Remote Access) but are never mounted at
 * the same time — they are separate, mutually exclusive Settings tabs. This
 * is the one poll implementation both call into, so neither page hand-rolls
 * its own interval.
 */
export function useMcpStatusPoll() {
	const [status, setStatus] = createSignal<McpStatus | null>(null);
	const [relayConnected, setRelayConnected] = createSignal(false);

	const refresh = async () => {
		try {
			const s = await rpc<McpStatus>("get_mcp_status");
			setStatus(s);
		} catch (e) {
			// Transient poll failures are normal during app startup
			appLogger.debug("config", "MCP status refresh failed", e);
		}
		try {
			const rs = await rpc<RelayStatus>("get_relay_status");
			setRelayConnected(rs.connected);
		} catch {
			// Relay status not available
		}
	};

	onMount(() => {
		refresh();
		const interval = setInterval(refresh, 3000);
		onCleanup(() => clearInterval(interval));
	});

	return { status, relayConnected, refresh };
}
