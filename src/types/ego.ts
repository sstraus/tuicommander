/**
 * What `ego_cli.rs` sends the Providers tab.
 *
 * A mirror of the Rust types, not a second opinion about them: the join
 * between ego's three commands — which provider a model belongs to, which
 * credential a provider has, which model is the default — is computed in Rust
 * and arrives finished. Nothing here reshapes it.
 */

/** Whether a provider can be used, as `ego doctor` reported it. */
export type EgoCredential =
	| { state: "stored"; detail: string }
	/** ego refreshes this by itself on its next run; nobody has to log in again. */
	| { state: "expired"; detail: string }
	| { state: "missing" }
	/** doctor could not read the store. Not the same as "nothing is there". */
	| { state: "unknown"; detail: string };

export interface EgoModel {
	/** The whole `provider/model` id — what ego stores as its `model` key. */
	slug: string;
	/** The half after the first separator. Split in Rust, never here. */
	name: string;
	available: boolean;
	/** Why not, in ego's own words. */
	unavailable: string | null;
}

export interface EgoProvider {
	name: string;
	credential: EgoCredential;
	models: EgoModel[];
}

export interface EgoProviders {
	/** ego's `model` key: the default a run starts from. */
	defaultModel: string | null;
	providers: EgoProvider[];
	/** Non-blocking health check result, independent of model selection. */
	diagnostic?: string | null;
}

export type EgoRootAccess = "read" | "read-write";

export interface EgoRootsEdit {
	rootDir: string;
	rootAccess: EgoRootAccess;
	readAllowlist: string;
	writableDirs: string;
}

export interface EgoEffectivePerimeter {
	workspace: string;
	roots: { path: string; access: EgoRootAccess; source: string }[];
	sandbox: string;
	sandbox_source: string;
	backend: string;
	capabilities: unknown;
	capabilities_reason?: string;
	probe_evidence?: string;
	network: string;
	warnings: string[];
}

export interface EgoPerimeterView {
	profile: string | null;
	roots: EgoRootsEdit;
	networkEnabled: boolean;
	execEnforcement: "enforcedByOs" | "promptOnly" | "notChecked";
	effective: EgoEffectivePerimeter;
	preview: string;
}

export type EgoCliErrorCode = "notConfigured" | "invalidInput" | "launchFailed" | "commandFailed" | "unreadableOutput";

/** A failure of the ego command line, carrying what it printed. */
export interface EgoCliError {
	code: EgoCliErrorCode;
	message: string;
	command: string;
	stdout: string;
	stderr: string;
	exitCode: number | null;
}

/** Whether a rejected promise carries an ego failure or something else. */
export function isEgoCliError(value: unknown): value is EgoCliError {
	if (typeof value !== "object" || value === null) return false;
	const candidate = value as Partial<EgoCliError>;
	return typeof candidate.code === "string" && typeof candidate.message === "string";
}

/**
 * The ego failure a rejection carries, whichever transport delivered it.
 *
 * Desktop IPC rejects with the `EgoCliError` itself. The HTTP transport throws
 * an `HttpRpcError` whose `body` is that same error as JSON (`ego_routes.rs`).
 * Matched structurally so this file stays free of the transport module.
 */
export function asEgoCliError(value: unknown): EgoCliError | null {
	if (isEgoCliError(value)) return value;
	const body = (value as { body?: unknown } | null)?.body;
	if (typeof body !== "string") return null;
	try {
		const parsed: unknown = JSON.parse(body);
		return isEgoCliError(parsed) ? parsed : null;
	} catch {
		return null;
	}
}
