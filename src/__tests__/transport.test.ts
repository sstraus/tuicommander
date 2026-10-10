import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import * as ts from "@typescript/typescript6";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	buildHttpUrl,
	DEDICATED_WS_COMMANDS,
	INTENTIONALLY_UNMAPPED,
	isTauri,
	mapCommandToHttp,
	owningConnectionFor,
} from "../transport";
import {
	setRemoteBaseUrlLookup,
	setRemoteTokenLookup,
	setRepoConnectionLookup,
	setSessionConnectionLookup,
	setTransportLogger,
} from "../transportRuntime";

function readRepoFile(relativePath: string): string {
	return readFileSync(join(process.cwd(), relativePath), "utf8");
}

function findNodes<T extends ts.Node>(root: ts.Node, guard: (node: ts.Node) => node is T): T[] {
	const found: T[] = [];
	const visit = (node: ts.Node) => {
		if (guard(node)) found.push(node);
		ts.forEachChild(node, visit);
	};
	visit(root);
	return found;
}

function extractCommandTableCommands(transportSource = readRepoFile("src/transport/commandTable.ts")): Set<string> {
	const sourceFile = ts.createSourceFile("transport.ts", transportSource, ts.ScriptTarget.Latest, true);
	const declaration = sourceFile.statements
		.filter(ts.isVariableStatement)
		.flatMap((statement) => [...statement.declarationList.declarations])
		.find((entry) => ts.isIdentifier(entry.name) && entry.name.text === "COMMAND_TABLE");
	let value = declaration?.initializer;
	while (value && !ts.isObjectLiteralExpression(value)) {
		if (ts.isParenthesizedExpression(value)) value = value.expression;
		else if (ts.isConditionalExpression(value)) value = value.whenFalse;
		else if (ts.isBinaryExpression(value) && value.operatorToken.kind === ts.SyntaxKind.CommaToken) value = value.right;
		else throw new Error("COMMAND_TABLE initializer is not an object");
	}
	if (!value) throw new Error("COMMAND_TABLE initializer not found");
	return new Set(
		value.properties
			.filter(ts.isPropertyAssignment)
			.map((property) => property.name)
			.filter(ts.isIdentifier)
			.map((name) => name.text),
	);
}

describe("COMMAND_TABLE source scan", () => {
	it("finds space-indented commands without treating a nested object key as a command", () => {
		const source = `const COMMAND_TABLE = {
  first_command: { map: () => ({ body: {
    nested_key: {}
  } }) },
  second_command: { map: () => ({ method: "GET" }) },
};`;
		expect(extractCommandTableCommands(source)).toEqual(new Set(["first_command", "second_command"]));
	});
});

describe("orphan cleanup transport", () => {
	it("maps the assessment and pending answer to the same repo path", () => {
		const repoPath = "/repo with space";
		expect(mapCommandToHttp("assess_orphan_cleanup", { repoPath })).toEqual({
			method: "GET",
			path: "/repo/orphan-cleanup-assessment?repoPath=%2Frepo%20with%20space",
		});
		expect(mapCommandToHttp("begin_orphan_cleanup", { repoPath, paths: ["/wt/a"] })).toEqual({
			method: "POST",
			path: "/repo/orphan-cleanup/begin",
			body: { repoPath, paths: ["/wt/a"] },
		});
		expect(mapCommandToHttp("pending_orphan_cleanup_answer", { repoPath })).toEqual({
			method: "GET",
			path: "/repo/orphan-cleanup/pending?repoPath=%2Frepo%20with%20space",
		});
	});
});

function extractRegisteredTauriCommands(): Set<string> {
	const libSource = readRepoFile("src-tauri/src/lib.rs");
	const handlerStart = libSource.indexOf("tauri::generate_handler![");
	if (handlerStart < 0) {
		throw new Error("tauri::generate_handler![ block not found");
	}
	const listStart = libSource.indexOf("[", handlerStart);
	const listEnd = libSource.indexOf("\n        ])", listStart);
	if (listStart < 0 || listEnd < 0) {
		throw new Error("tauri::generate_handler![ command list bounds not found");
	}

	const commandList = libSource
		.slice(listStart + 1, listEnd)
		.replace(/\/\/.*$/gm, "")
		.split(",")
		.map((entry) => entry.trim())
		.filter((entry) => entry.length > 0)
		.map((entry) => {
			const parts = entry.split("::");
			const rustName = parts.at(-1)!;
			const renamedCommand = libSource.match(
				new RegExp(
					`#\\[tauri::command\\(rename\\s*=\\s*"([^"]+)"\\)\\]\\s*(?:pub\\(super\\)\\s+)?async\\s+fn\\s+${rustName}\\b`,
				),
			)?.[1];
			return renamedCommand ?? rustName;
		});

	return new Set(commandList);
}

/** Every .ts/.tsx under src/, excluding the test tree itself. */
function collectFrontendSources(): { path: string; source: string }[] {
	const root = join(process.cwd(), "src");
	const files: { path: string; source: string }[] = [];
	const walk = (dir: string) => {
		for (const entry of readdirSync(dir, { withFileTypes: true })) {
			const full = join(dir, entry.name);
			if (entry.isDirectory()) {
				if (entry.name === "__tests__" || entry.name === "node_modules") continue;
				walk(full);
			} else if (entry.name.endsWith(".ts") || entry.name.endsWith(".tsx")) {
				files.push({ path: full, source: readFileSync(full, "utf8") });
			}
		}
	};
	walk(root);
	return files;
}

/**
 * Per-session Tauri event names the frontend subscribes to, as `pty-<name>-`
 * prefixes. Two spellings reach the same events:
 *   - a literal `` listen(`pty-foo-${sessionId}`) ``
 *   - `transport.onEvent("foo")`, which TauriTransport expands to
 *     `pty-foo-${sessionId}` (canvasTerminalTransport.ts)
 */
function extractSubscribedPtyEvents(sources = collectFrontendSources()): Map<string, string[]> {
	const subscribed = new Map<string, string[]>();
	const add = (name: string, path: string) => {
		const where = subscribed.get(name) ?? [];
		where.push(path.replace(`${process.cwd()}/`, ""));
		subscribed.set(name, where);
	};
	for (const { path, source } of sources) {
		const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
		for (const call of findNodes(file, ts.isCallExpression)) {
			const argument = call.arguments[0];
			if (argument && ts.isIdentifier(call.expression) && call.expression.text === "listen") {
				for (const template of findNodes(argument, ts.isTemplateExpression)) {
					const eventName = /^(pty-[a-z0-9-]+)-$/.exec(template.head.text)?.[1];
					if (eventName) add(eventName, path);
				}
			}
			if (argument && ts.isPropertyAccessExpression(call.expression) && call.expression.name.text === "onEvent") {
				for (const name of findNodes(argument, ts.isStringLiteral)) {
					if (/^[a-z0-9-]+$/.test(name.text)) add(`pty-${name.text}`, path);
				}
			}
		}
	}
	return subscribed;
}

describe("PTY event source scan", () => {
	it("finds the activity listener when instrumentation wraps its template argument", () => {
		// biome-ignore lint/suspicious/noTemplateCurlyInString: The placeholder is source text for the parser.
		const source = "listen(mutant ? `` : (coverage(), `pty-activity-${sessionId}`), () => {});";
		expect(extractSubscribedPtyEvents([{ path: "src/transport.ts", source }]).has("pty-activity")).toBe(true);
	});
});

describe("transport", () => {
	/**
	 * A listener whose emitter has been deleted fails silently and forever: the
	 * callback simply stops running. That is not hypothetical — commit cda39f31
	 * removed the Rust `pty-output` emit and left `subscribePty` subscribed to
	 * it, freezing desktop `lastDataAt` and the background-tab unread flag for a
	 * commit with nothing red (story 625-56b0).
	 *
	 * So: every per-session event the frontend listens for must be emitted by
	 * Rust. This asserts the direction that broke. The reverse (an emit nobody
	 * consumes) is wasteful but harmless, and is deliberately not asserted.
	 */
	describe("per-session Tauri event parity", () => {
		it("every pty-* event the frontend subscribes to is emitted by Rust", () => {
			const rustSources = [
				"src-tauri/src/pty.rs",
				"src-tauri/src/state.rs",
				"src-tauri/crates/tuic-terminal/src/terminal_grid.rs",
			]
				.map((relative) => readRepoFile(relative))
				.join("\n");

			const subscribed = extractSubscribedPtyEvents();
			expect(subscribed.size).toBeGreaterThan(0);

			const orphaned = [...subscribed.entries()].filter(([name]) => !rustSources.includes(`${name}-{session_id}`));

			expect(orphaned.map(([name, where]) => `${name}-{session_id} (listened in ${where.join(", ")})`)).toEqual([]);
		});

		it("includes the activity pulse, which is the signal that regressed", () => {
			// Guards the guard: if the extraction above silently stopped matching,
			// the parity test would pass vacuously for the very event it exists for.
			expect([...extractSubscribedPtyEvents().keys()]).toContain("pty-activity");
		});
	});

	/**
	 * The desktop app receives `repo-changed` over Tauri IPC; browser, PWA and
	 * remote clients receive it over `/events` SSE. They are two transports for
	 * one event, and the same store code consumes both — so the payload keys
	 * must be identical, not merely similar.
	 *
	 * This is the shape the `kind` field was added to. Adding a field to the
	 * Tauri struct and forgetting the SSE arm (or vice versa) is silent: the
	 * desktop build keeps working and only remote clients degrade, which is
	 * exactly the class of drift nobody notices locally.
	 */
	describe("repo-changed cross-transport payload parity", () => {
		/** Field names of the `RepoChangedPayload` struct the Tauri emit sends. */
		function tauriPayloadFields(): string[] {
			const source = readRepoFile("src-tauri/src/repo_watcher.rs");
			const struct = source.match(/pub\(crate\) struct RepoChangedPayload \{([\s\S]*?)\n\}/);
			expect(struct, "RepoChangedPayload struct not found — the extractor is stale").not.toBeNull();
			return [...struct![1].matchAll(/pub (\w+):/g)].map((m) => m[1]).sort();
		}

		/** JSON keys the `/events` SSE arm sends for `AppEvent::RepoChanged`. */
		function ssePayloadKeys(): string[] {
			const source = readRepoFile("src-tauri/src/mcp_http/sse_routes.rs");
			const arm = source.match(/AppEvent::RepoChanged \{[^}]*\} => \{\s*serde_json::json!\(\{([^}]*)\}\)/);
			expect(arm, "RepoChanged SSE arm not found — the extractor is stale").not.toBeNull();
			return [...arm![1].matchAll(/"(\w+)":/g)].map((m) => m[1]).sort();
		}

		it("the Tauri emit and the SSE arm carry the identical field set", () => {
			expect(ssePayloadKeys()).toEqual(tauriPayloadFields());
		});

		it("that field set is the one the frontend reads", () => {
			// Guards the guard: both extractors returning [] would make the
			// equality above pass vacuously. These are the two keys
			// useAppInit.ts destructures.
			expect(tauriPayloadFields()).toEqual(["kind", "repo_path"]);
		});
	});

	describe("isTauri()", () => {
		const original = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;

		afterEach(() => {
			if (original !== undefined) {
				(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = original;
			} else {
				delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
			}
		});

		it("returns true when __TAURI_INTERNALS__ exists", () => {
			(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
			expect(isTauri()).toBe(true);
		});

		it("returns false when __TAURI_INTERNALS__ is absent", () => {
			delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
			expect(isTauri()).toBe(false);
		});
	});

	describe("buildHttpUrl()", () => {
		it("builds URL with current origin by default", () => {
			const url = buildHttpUrl("/health");
			// In test env, location.origin may be empty string, so just check it ends with /health
			expect(url).toContain("/health");
		});
	});

	describe("mapCommandToHttp()", () => {
		it("maps one complete managed reply to the atomic session route", () => {
			expect(mapCommandToHttp("submit_agent_reply", { sessionId: "s1", input: "answer" })).toEqual({
				method: "POST",
				path: "/sessions/s1/submit",
				body: { input: "answer" },
			});
		});
		it("maps every Design Mode command with matching IPC request fields", () => {
			expect(mapCommandToHttp("start_design_mode", { sessionId: "agent-1" })).toEqual({
				method: "POST",
				path: "/design-mode/start",
				body: { sessionId: "agent-1" },
			});
			expect(mapCommandToHttp("stop_design_mode", { repoPath: "/repo" })).toEqual({
				method: "POST",
				path: "/design-mode/stop",
				body: { repoPath: "/repo" },
			});
			expect(mapCommandToHttp("get_design_mode_status", {})).toEqual({
				method: "GET",
				path: "/design-mode",
			});
		});
		it("maps report_progress_event without reshaping its request", () => {
			const report = { type: "milestone", summary: "HTTP parity works.", workstream: "Progress" };
			const result = mapCommandToHttp("report_progress_event", { project: "/repo with space", report });
			expect(result).toEqual({
				method: "POST",
				path: "/progress/report?path=%2Frepo%20with%20space",
				body: report,
			});
		});

		it("maps mobile Progress project discovery to its authenticated HTTP route", () => {
			expect(mapCommandToHttp("progress_projects", {})).toEqual({
				method: "GET",
				path: "/progress/projects",
			});
		});

		it("routes native story actions to the owning project", () => {
			expect(mapCommandToHttp("story_capabilities", {})).toEqual({
				method: "GET",
				path: "/stories/capabilities",
			});
			const action = { action: "get_story", story_id: "story-1" };
			expect(mapCommandToHttp("story_action_command", { project: "/repo a", action })).toEqual({
				method: "POST",
				path: "/stories/action?path=%2Frepo%20a",
				body: { action },
			});
			const remove = {
				action: "remove_dependency",
				story_id: "dependent",
				dependency_id: "cancelled",
				expected_revision: 3,
			};
			expect(mapCommandToHttp("story_action_command", { project: "/repo a", action: remove })).toEqual({
				method: "POST",
				path: "/stories/action?path=%2Frepo%20a",
				body: { action: remove },
			});
		});
		it("routes versioned workflow definitions to their owning project", () => {
			const action = { action: "get_published", id: "flow-1", revision: 2 };
			expect(mapCommandToHttp("workflow_definition_action", { project: "/repo a", action })).toEqual({
				method: "POST",
				path: "/workflows/definition/action?path=%2Frepo%20a",
				body: action,
			});
		});
		it.each([
			{ action: "events", run_id: "run-1", after_sequence: 4, limit: 20 },
			{ action: "incidents", run_id: "run-1" },
		])("routes workflow run actions to their owning project: $action", (action) => {
			expect(mapCommandToHttp("workflow_run_action", { project: "/repo a", action })).toEqual({
				method: "POST",
				path: "/workflows/run/action?path=%2Frepo%20a",
				body: action,
			});
		});

		it("preserves pinned graph start and recovery payloads through HTTP", () => {
			// Catches: graph controls being dropped or reshaped on the browser transport.
			for (const action of [
				{
					action: "start_graph",
					target: { type: "story", id: "story-1" },
					expected_revision: 3,
					definition_id: "flow-1",
					definition_revision: 2,
					request_id: "start-key",
				},
				{
					action: "command",
					run_id: "run-1",
					command_id: "resume-key",
					expected_sequence: 4,
					command: { action: "resume_graph", execution_id: "root", activation_id: "a3", resolution: "Reviewed" },
				},
				{
					action: "command",
					run_id: "run-1",
					command_id: "cancel-key",
					expected_sequence: 4,
					command: { action: "cancel" },
				},
			])
				expect(mapCommandToHttp("workflow_run_action", { project: "/repo a", action })).toEqual({
					method: "POST",
					path: "/workflows/run/action?path=%2Frepo%20a",
					body: action,
				});
		});

		it("maps typed project progress controls", () => {
			// The whole Progress surface: record, list, delete, and the divider.
			// Every control the rejected design added — status, pause, resume,
			// clear, update, read, export — is gone from both transports.
			for (const command of ["progress_list", "progress_delete"]) {
				const input =
					command === "progress_list" ? { blockedOnly: false, ptyId: "pty-a", limit: 8, cursor: 42 } : { ids: [1] };
				expect(mapCommandToHttp(command, { project: "/repo a", input })).toEqual({
					method: "POST",
					path: `/progress/${command.slice(9)}?path=%2Frepo%20a`,
					body: input,
				});
			}
			expect(mapCommandToHttp("progress_mark_viewed", { project: "/repo a" })).toEqual({
				method: "POST",
				path: "/progress/viewed?path=%2Frepo%20a",
			});
			expect(mapCommandToHttp("progress_mark_viewed", { project: "/repo a", ptyId: "pty-a" })).toEqual({
				method: "POST",
				path: "/progress/viewed?path=%2Frepo%20a&ptyId=pty-a",
			});
			const report = { type: "done", text: "Shipped." };
			expect(mapCommandToHttp("report_progress_event", { project: "/repo a", report })).toEqual({
				method: "POST",
				path: "/progress/report?path=%2Frepo%20a",
				body: report,
			});
			// The Flow view: one read for the whole sequence, one for the full
			// text behind a subagent arrow. Neither reshapes its input.
			const flowInput = { ptyId: "pty-a" };
			expect(mapCommandToHttp("progress_flow", { project: "/repo a", input: flowInput })).toEqual({
				method: "POST",
				path: "/progress/flow?path=%2Frepo%20a",
				body: flowInput,
			});
			const detail = { ptyId: "pty-a", agentId: "a1", part: "report" };
			expect(mapCommandToHttp("progress_flow_detail", { input: detail })).toEqual({
				method: "POST",
				path: "/progress/flow/detail",
				body: detail,
			});
			for (const gone of ["progress_status", "progress_pause", "progress_clear", "progress_export"]) {
				expect(() => mapCommandToHttp(gone, { project: "/repo a" })).toThrow(/No HTTP mapping/);
			}
		});

		it("maps create_pty to POST /sessions", () => {
			const result = mapCommandToHttp("create_pty", { config: { rows: 24, cols: 80, shell: null, cwd: "/tmp" } });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions");
			expect(result.body).toEqual({ rows: 24, cols: 80, shell: null, cwd: "/tmp" });
		});

		// The desktop path reads `alias` off `PtyConfig`; the HTTP route reads it off
		// the same body under the same name. A restore over either transport has to
		// reserve the address the tab already had, so the field cannot be dropped in
		// the mapper.
		it("carries the requested alias through to POST /sessions", () => {
			const result = mapCommandToHttp("create_pty", {
				config: { rows: 24, cols: 80, shell: null, cwd: "/tmp", tuic_session: "tab-uuid", alias: "tu-3" },
			});
			expect(result.body).toEqual({
				rows: 24,
				cols: 80,
				shell: null,
				cwd: "/tmp",
				tuic_session: "tab-uuid",
				alias: "tu-3",
			});
		});

		it("maps write_pty to POST /sessions/{id}/write", () => {
			const result = mapCommandToHttp("write_pty", { sessionId: "abc", data: "hello" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/abc/write");
			expect(result.body).toEqual({ data: "hello" });
		});

		it("maps enqueue_agent_command to POST /sessions/{id}/queue", () => {
			const result = mapCommandToHttp("enqueue_agent_command", { sessionId: "abc", text: "run tests" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/abc/queue");
			expect(result.body).toEqual({ text: "run tests" });
		});

		// Catches: HTTP drops the idempotency key supplied to the desktop command.
		it("preserves queue idempotency keys across HTTP and Tauri arguments", () => {
			const result = mapCommandToHttp("enqueue_agent_command", {
				sessionId: "abc",
				text: "run tests",
				idempotencyKey: "bg-job-1",
			});
			expect(result.body).toEqual({ text: "run tests", idempotencyKey: "bg-job-1" });
		});

		it("maps clear_queued_agent_commands to DELETE /sessions/{id}/queue", () => {
			const result = mapCommandToHttp("clear_queued_agent_commands", { sessionId: "abc" });
			expect(result.method).toBe("DELETE");
			expect(result.path).toBe("/sessions/abc/queue");
		});

		it("maps list_queued_agent_commands to GET /sessions/{id}/queue", () => {
			const result = mapCommandToHttp("list_queued_agent_commands", { sessionId: "abc" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/abc/queue");
		});

		it("maps remove_queued_agent_command to DELETE /sessions/{id}/queue/{commandId}", () => {
			const result = mapCommandToHttp("remove_queued_agent_command", { sessionId: "abc", commandId: 7 });
			expect(result.method).toBe("DELETE");
			expect(result.path).toBe("/sessions/abc/queue/7");
		});

		it("maps session names with their custom-name origin", () => {
			const result = mapCommandToHttp("set_session_name", {
				sessionId: "abc",
				name: "Ollama audit",
				isCustom: false,
			});
			expect(result).toEqual({
				method: "PUT",
				path: "/sessions/abc/name",
				body: { name: "Ollama audit", isCustom: false },
			});
		});

		it("maps resize_pty to POST /sessions/{id}/resize", () => {
			const result = mapCommandToHttp("resize_pty", { sessionId: "abc", rows: 40, cols: 120 });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/abc/resize");
			expect(result.body).toEqual({ rows: 40, cols: 120 });
		});

		it("maps pause_pty to POST /sessions/{id}/pause", () => {
			const result = mapCommandToHttp("pause_pty", { sessionId: "abc" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/abc/pause");
		});

		it("maps resume_pty to POST /sessions/{id}/resume", () => {
			const result = mapCommandToHttp("resume_pty", { sessionId: "abc" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/abc/resume");
		});

		it("maps session_suspend_response to POST /mcp/suspend-response", () => {
			const result = mapCommandToHttp("session_suspend_response", {
				requestId: "r1",
				ok: false,
				reason: "agent working",
			});
			expect(result).toMatchObject({
				method: "POST",
				path: "/mcp/suspend-response",
				body: { request_id: "r1", ok: false, reason: "agent working" },
			});
		});

		it("maps close_pty to DELETE /sessions/{id}", () => {
			const result = mapCommandToHttp("close_pty", { sessionId: "abc", cleanupWorktree: false });
			expect(result.method).toBe("DELETE");
			expect(result.path).toBe("/sessions/abc");
		});

		it("maps get_session_foreground_process to GET /sessions/{id}/foreground", () => {
			const result = mapCommandToHttp("get_session_foreground_process", { sessionId: "abc" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/abc/foreground");
			expect(result.transform).toBeDefined();
			expect(result.transform?.({ agent: "claude" })).toBe("claude");
			expect(result.transform?.({ agent: null })).toBeNull();
		});

		it("maps get_pty_capture to GET /diagnostics/capture", () => {
			const result = mapCommandToHttp("get_pty_capture", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/diagnostics/capture");
		});

		it("maps set_pty_capture to POST /diagnostics/capture with a session filter", () => {
			const result = mapCommandToHttp("set_pty_capture", { enabled: true, sessionId: "abc" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/diagnostics/capture");
			expect(result.body).toEqual({ enabled: true, session_id: "abc" });
		});

		it("maps set_pty_capture without a session to an unfiltered tap", () => {
			const result = mapCommandToHttp("set_pty_capture", { enabled: false });
			expect(result.body).toEqual({ enabled: false, session_id: null });
		});

		it("maps get_orchestrator_stats to GET /stats", () => {
			const result = mapCommandToHttp("get_orchestrator_stats", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/stats");
		});

		it("maps get_session_metrics to GET /metrics", () => {
			const result = mapCommandToHttp("get_session_metrics", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/metrics");
		});

		it("maps list_active_sessions to GET /sessions", () => {
			const result = mapCommandToHttp("list_active_sessions", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions");
		});

		it("maps can_spawn_session to GET /stats", () => {
			const result = mapCommandToHttp("can_spawn_session", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/stats");
		});

		it("maps load_config to GET /config", () => {
			const result = mapCommandToHttp("load_config", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/config");
		});

		it("maps save_config to PUT /config", () => {
			const cfg = { font_family: "JetBrains Mono" };
			const result = mapCommandToHttp("save_config", { base: { font_family: "Menlo" }, config: cfg });
			expect(result.method).toBe("PUT");
			expect(result.path).toBe("/config");
			expect(result.body).toEqual({ base: { font_family: "Menlo" }, config: cfg });
		});

		it("maps upstream saves with both the loaded base and desired config", () => {
			const base = { servers: [{ id: "a", enabled: true }] };
			const config = { servers: [{ id: "a", enabled: false }] };
			const result = mapCommandToHttp("save_mcp_upstreams", { base, config });
			expect(result.method).toBe("PUT");
			expect(result.path).toBe("/mcp/upstreams");
			expect(result.body).toEqual({ base, config });
		});

		it("throws for unknown commands", () => {
			expect(() => mapCommandToHttp("unknown_cmd", {})).toThrow("No HTTP mapping for command: unknown_cmd");
		});

		it("maps previously browser-unsupported commands to HTTP", () => {
			const dictation = mapCommandToHttp("start_dictation", { source: "fn" });
			expect(dictation.method).toBe("POST");
			expect(dictation.path).toBe("/dictation/start");
			expect(dictation.body).toEqual({ source: "fn" });

			const openInApp = mapCommandToHttp("open_in_app", { path: "/tmp/x", app: "vscode" });
			expect(openInApp.method).toBe("POST");
			expect(openInApp.path).toBe("/agents/open-in-app");
		});

		/**
		 * The 19 paths whose axum routes were missing entirely: `mod dictation_routes`
		 * was never declared, so its 12 handlers never compiled, and the 7 others had
		 * a COMMAND_TABLE entry pointing at nothing. `rpc()` reads a 404 as a failed
		 * call, so every one of these was dead in browser mode.
		 *
		 * Method and path are asserted here; that they resolve to a registered route
		 * is asserted on the Rust side by
		 * `mcp_http::tests::every_dictation_and_os_integration_path_has_a_route`.
		 */
		describe("dictation and OS-integration mappings", () => {
			it.each([
				["get_dictation_status", {}, "GET", "/dictation/status"],
				["get_model_info", {}, "GET", "/dictation/models"],
				["get_speech_assets", {}, "GET", "/dictation/speech/assets"],
				["stop_speech", {}, "POST", "/dictation/speech/stop"],
				["pause_speech", {}, "POST", "/dictation/speech/pause"],
				["resume_speech", {}, "POST", "/dictation/speech/resume"],
				["get_speech_status", {}, "GET", "/dictation/speech/status"],
				// Asking about one reply is a query parameter rather than a
				// second route: it is the same question with a narrower answer,
				// and a caller polling its own utterance still wants to know
				// whether speech is available at all.
				["get_speech_status", { utterance: "7" }, "GET", "/dictation/speech/status?utterance=7"],
				// The language is a Whisper code, as for every other speech command.
				["get_speech_voices", { language: "it" }, "GET", "/dictation/speech/voices?language=it"],
				["get_edge_voices", { language: "it" }, "GET", "/dictation/speech/edge-voices?language=it"],
				["start_dictation", {}, "POST", "/dictation/start"],
				["stop_dictation_and_transcribe", {}, "POST", "/dictation/stop"],
				["get_correction_map", {}, "GET", "/dictation/corrections"],
				["list_audio_devices", {}, "GET", "/dictation/devices"],
				["get_dictation_config", {}, "GET", "/dictation/config"],
				["get_hands_free_status", {}, "GET", "/dictation/hands-free"],
				["get_hands_free_default_notice", {}, "GET", "/dictation/hands-free/default-notice"],
				["disarm_hands_free_dictation", {}, "POST", "/dictation/hands-free/disarm"],
				["get_relay_status", {}, "GET", "/system/relay-status"],
				["get_config_defaults", {}, "GET", "/config/defaults"],
				["check_update_channel", { channel: "nightly" }, "GET", "/system/check-update?channel=nightly"],
				["get_session_shell_family", { sessionId: "s1" }, "GET", "/sessions/s1/shell-family"],
			])("maps %s to %s %s", (command, args, method, path) => {
				const result = mapCommandToHttp(command as string, args as Record<string, unknown>);
				expect(result.method).toBe(method);
				expect(result.path).toBe(path);
			});

			it.each([
				["download_whisper_model", { modelName: "small" }, "POST", "/dictation/models/download", { model: "small" }],
				["delete_whisper_model", { modelName: "small" }, "POST", "/dictation/models/delete", { model: "small" }],
				// The wire key is `asset` on both transports, unlike the whisper
				// pair above where the IPC argument is `modelName` and the body
				// key is `model`. Keeping them the same here is deliberate: the
				// mismatch above is a wart nobody should copy.
				[
					"download_speech_asset",
					{ asset: "italian" },
					"POST",
					"/dictation/speech/assets/download",
					{ asset: "italian" },
				],
				[
					"cancel_speech_download",
					{ asset: "italian" },
					"POST",
					"/dictation/speech/assets/cancel",
					{ asset: "italian" },
				],
				["delete_speech_asset", { asset: "italian" }, "POST", "/dictation/speech/assets/delete", { asset: "italian" }],
				// The voice file travels as base64 so the body is the same JSON on
				// both transports; `dataBase64` is how Tauri spells `data_base64`.
				[
					"import_speech_voice",
					{ language: "it", name: "nonna", dataBase64: "AAAA" },
					"POST",
					"/dictation/speech/voices/import",
					{ language: "it", name: "nonna", dataBase64: "AAAA" },
				],
				[
					"delete_speech_voice",
					{ language: "it", name: "nonna" },
					"POST",
					"/dictation/speech/voices/delete",
					{ language: "it", name: "nonna" },
				],
				[
					"preview_speech_voice",
					{ language: "it", voice: "giovanni", text: "Ciao." },
					"POST",
					"/dictation/speech/voices/preview",
					{ language: "it", voice: "giovanni", text: "Ciao." },
				],
				// `turn` rides in the body rather than being derived: a reply
				// written for a turn the user has already talked over must be
				// refusable, and only the caller knows which turn it answered.
				["speak_reply", { text: "Fatto.", turn: 3 }, "POST", "/dictation/speech/speak", { text: "Fatto.", turn: 3 }],
				["set_correction_map", { map: { teh: "the" } }, "PUT", "/dictation/corrections", { map: { teh: "the" } }],
				["inject_text", { text: "hello" }, "POST", "/dictation/inject", { text: "hello" }],
				[
					"set_dictation_config",
					{ base: { enabled: false }, config: { enabled: true } },
					"PUT",
					"/dictation/config",
					{ base: { enabled: false }, config: { enabled: true } },
				],
				// The body is camelCase on both transports: the Rust request type
				// renames its fields to match, so one store works unchanged.
				[
					"arm_hands_free_dictation",
					{ sessionId: "s1", owner: "desktop" },
					"POST",
					"/dictation/hands-free/arm",
					{ sessionId: "s1", owner: "desktop" },
				],
				[
					"open_in_app",
					{ path: "/tmp/x", app: "vscode", line: 12, col: 3 },
					"POST",
					"/agents/open-in-app",
					{ path: "/tmp/x", app: "vscode", line: 12, col: 3 },
				],
				[
					"detect_all_agent_binaries",
					{ binaries: ["claude", "codex"] },
					"POST",
					"/agents/detect-all",
					{ binaries: ["claude", "codex"] },
				],
				[
					"run_setup_script",
					{ script: "pnpm i", cwd: "/repo" },
					"POST",
					"/worktrees/run-script",
					{ script: "pnpm i", cwd: "/repo" },
				],
			])("maps %s to %s %s with an IPC-identical body", (command, args, method, path, body) => {
				const result = mapCommandToHttp(command as string, args as Record<string, unknown>);
				expect(result.method).toBe(method);
				expect(result.path).toBe(path);
				expect(result.body).toEqual(body);
			});

			// `device` is the audio output the user picked. The Rust command has always
			// taken it and notifications.ts has always sent it; leaving it out of the
			// HTTP body sent every browser-mode sound to the default device instead.
			it("carries the chosen output device on play_notification_sound", () => {
				const chosen = mapCommandToHttp("play_notification_sound", {
					sound: "question",
					volume: 0.5,
					device: "Studio Display Speakers",
				});
				expect(chosen.method).toBe("POST");
				expect(chosen.path).toBe("/system/notification-sound");
				expect(chosen.body).toEqual({
					sound: "question",
					volume: 0.5,
					device: "Studio Display Speakers",
				});
			});

			it("sends a null device when the user picked none", () => {
				const dflt = mapCommandToHttp("play_notification_sound", { sound: "completion", volume: 1, device: null });
				expect(dflt.body).toEqual({ sound: "completion", volume: 1, device: null });
			});
		});

		it("maps hash_password to POST /config/hash-password with transform", () => {
			const result = mapCommandToHttp("hash_password", { password: "secret" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/config/hash-password");
			expect(result.body).toEqual({ password: "secret" });
			expect(result.transform).toBeDefined();
			expect(result.transform?.({ hash: "abc123" })).toBe("abc123");
		});

		it("maps can_spawn_session with transform", () => {
			const result = mapCommandToHttp("can_spawn_session", {});
			expect(result.transform).toBeDefined();
			expect(result.transform?.({ active_sessions: 2, max_sessions: 5 })).toBe(true);
			expect(result.transform?.({ active_sessions: 5, max_sessions: 5 })).toBe(false);
		});

		// The password never travels back: `set` takes it, `exists` answers a bare
		// boolean and `token` answers the daemon's session token. All three keep the
		// IPC shape (`json_result`), so no mapping needs a transform.
		describe("remote-connection credential mappings", () => {
			it("maps a remote edit with its loaded base", () => {
				const base = { id: "c1", name: "before", auto_update: false };
				const connection = { ...base, auto_update: true };
				const result = mapCommandToHttp("save_remote_connection", { base, connection });
				expect(result).toMatchObject({
					method: "PUT",
					path: "/config/remote-connections",
					body: { base, connection },
				});
			});

			it("maps set_remote_connection_password to PUT with the password in the body", () => {
				const result = mapCommandToHttp("set_remote_connection_password", { id: "c1", password: "hunter2" });
				expect(result.method).toBe("PUT");
				expect(result.path).toBe("/config/remote-connections/c1/password");
				expect(result.body).toEqual({ password: "hunter2" });
			});

			it("maps remote_connection_password_exists to a GET that carries no secret", () => {
				const result = mapCommandToHttp("remote_connection_password_exists", { id: "c1" });
				expect(result.method).toBe("GET");
				expect(result.path).toBe("/config/remote-connections/c1/password");
				expect(result.body).toBeUndefined();
			});

			it("maps fetch_remote_connection_token to POST with the daemon it must ask", () => {
				const result = mapCommandToHttp("fetch_remote_connection_token", {
					id: "c1",
					baseUrl: "http://mac-mint:9877",
					username: "stefano",
				});
				expect(result.method).toBe("POST");
				expect(result.path).toBe("/config/remote-connections/c1/token");
				expect(result.body).toEqual({ baseUrl: "http://mac-mint:9877", username: "stefano" });
			});
		});

		it("maps persistent remote daemon install and uninstall", () => {
			expect(mapCommandToHttp("install_remote_daemon", { id: "c1" })).toMatchObject({
				method: "POST",
				path: "/config/remote-connections/c1/install",
			});
			expect(mapCommandToHttp("uninstall_remote_daemon", { id: "c1" })).toMatchObject({
				method: "DELETE",
				path: "/config/remote-connections/c1/install",
			});
		});

		it("maps the on-demand SSH host status probe", () => {
			expect(mapCommandToHttp("probe_ssh_config_hosts", {})).toMatchObject({
				method: "GET",
				path: "/tunnels/ssh-hosts/status",
			});
		});

		it("maps the discovered SSH hosts listing", () => {
			expect(mapCommandToHttp("list_discovered_ssh_hosts", {})).toMatchObject({
				method: "GET",
				path: "/tunnels/ssh-hosts/discovered",
			});
		});

		it("maps the per-host SSH probe to a POST with the identity in the body", () => {
			expect(mapCommandToHttp("probe_discovered_ssh_host", { target: "10.0.0.5", port: 2222 })).toMatchObject({
				method: "POST",
				path: "/tunnels/ssh-hosts/probe",
				body: { target: "10.0.0.5", port: 2222 },
			});
		});

		it("maps detect_agents to GET /agents", () => {
			const result = mapCommandToHttp("detect_agents", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/agents");
		});

		it("maps get_repo_info to GET /repo/info?path=", () => {
			const result = mapCommandToHttp("get_repo_info", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/info?path=%2Fmy%2Frepo");
		});

		it("maps get_git_diff to GET /repo/diff?path=", () => {
			const result = mapCommandToHttp("get_git_diff", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/diff?path=%2Fmy%2Frepo");
		});

		it("maps get_diff_stats to GET /repo/diff-stats?path=", () => {
			const result = mapCommandToHttp("get_diff_stats", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/diff-stats?path=%2Fmy%2Frepo");
		});

		it("maps get_changed_files to GET /repo/files?path=", () => {
			const result = mapCommandToHttp("get_changed_files", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/files?path=%2Fmy%2Frepo");
		});

		it("maps get_github_status to GET /repo/github?path=", () => {
			const result = mapCommandToHttp("get_github_status", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/github?path=%2Fmy%2Frepo");
		});

		it("maps get_repo_pr_statuses to GET /repo/prs?path=", () => {
			const result = mapCommandToHttp("get_repo_pr_statuses", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/prs?path=%2Fmy%2Frepo");
		});

		it("maps get_git_branches to GET /repo/branches?path=", () => {
			const result = mapCommandToHttp("get_git_branches", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/branches?path=%2Fmy%2Frepo");
		});

		it("maps get_ci_checks to GET /repo/ci?path=&pr_number=", () => {
			const result = mapCommandToHttp("get_ci_checks", { path: "/my/repo", prNumber: 42 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/ci?path=%2Fmy%2Frepo&pr_number=42");
		});

		it("maps search_content to GET /fs/search-content", () => {
			const result = mapCommandToHttp("search_content", {
				repoPath: "/my/repo",
				query: "hello",
				caseSensitive: true,
				useRegex: false,
				wholeWord: false,
			});
			expect(result.method).toBe("GET");
			expect(result.path).toContain("/fs/search-content");
			expect(result.path).toContain("repoPath=%2Fmy%2Frepo");
			expect(result.path).toContain("query=hello");
			expect(result.path).toContain("caseSensitive=true");
		});

		// --- Terminal grid commands ---

		it("maps terminal_scroll to POST /sessions/{id}/terminal/scroll", () => {
			const result = mapCommandToHttp("terminal_scroll", { sessionId: "s1", delta: -5 });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/terminal/scroll");
			expect(result.body).toEqual({ delta: -5 });
		});

		it("maps terminal_scroll_to to POST /sessions/{id}/terminal/scroll-to", () => {
			const result = mapCommandToHttp("terminal_scroll_to", { sessionId: "s1", line: 42 });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/terminal/scroll-to");
			expect(result.body).toEqual({ line: 42 });
		});

		it("maps terminal_scroll_info to GET /sessions/{id}/terminal/scroll-info", () => {
			const result = mapCommandToHttp("terminal_scroll_info", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/scroll-info");
		});

		it("maps chat_view_snapshot to GET /sessions/{id}/chat-view with the cursor", () => {
			const first = mapCommandToHttp("chat_view_snapshot", { sessionId: "s1", epoch: null, fromSeq: 0 });
			expect(first.method).toBe("GET");
			expect(first.path).toBe("/sessions/s1/chat-view?from_seq=0");
			const next = mapCommandToHttp("chat_view_snapshot", { sessionId: "s1", epoch: 2, fromSeq: 17 });
			expect(next.path).toBe("/sessions/s1/chat-view?from_seq=17&epoch=2");
		});

		it("maps terminal_search to POST with transform", () => {
			const result = mapCommandToHttp("terminal_search", { sessionId: "s1", query: "foo" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/terminal/search");
			expect(result.body).toEqual({ query: "foo" });
			expect(result.transform?.({ matches: [{ row: 0, col: 1 }] })).toEqual([{ row: 0, col: 1 }]);
		});

		it("maps terminal_search_buffer to POST with transform", () => {
			const result = mapCommandToHttp("terminal_search_buffer", { sessionId: "s1", query: "bar" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/terminal/search-buffer");
			expect(result.body).toEqual({ query: "bar" });
			expect(result.transform?.({ matches: [] })).toEqual([]);
		});

		it("maps terminal_get_row_text to GET with transform", () => {
			const result = mapCommandToHttp("terminal_get_row_text", { sessionId: "s1", row: 5 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/row-text?row=5");
			expect(result.transform?.({ text: "hello" })).toBe("hello");
		});

		it("maps the selection history snapshot used to rebase rows after eviction", () => {
			const result = mapCommandToHttp("terminal_get_selection_text", {
				sessionId: "s1",
				startRow: 1,
				startCol: 2,
				endRow: 3,
				endCol: 4,
				historyBase: 99,
			});
			expect(result.path).toBe(
				"/sessions/s1/terminal/selection-text?startRow=1&startCol=2&endRow=3&endCol=4&historyBase=99",
			);
		});

		it("maps terminal_get_lines to GET with transform", () => {
			const result = mapCommandToHttp("terminal_get_lines", { sessionId: "s1", start: 0, end: 3 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/lines?start=0&end=3");
			expect(result.transform?.({ lines: ["a", "b"] })).toEqual(["a", "b"]);
		});

		it("maps terminal_get_cursor_line to GET with transform", () => {
			const result = mapCommandToHttp("terminal_get_cursor_line", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/cursor-line");
			expect(result.transform?.({ text: "$ " })).toBe("$ ");
		});

		it("maps terminal_hyperlink_at to GET with transform", () => {
			const result = mapCommandToHttp("terminal_hyperlink_at", { sessionId: "s1", row: 2, col: 10 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/hyperlink?row=2&col=10");
			expect(result.transform?.({ url: "https://example.com" })).toBe("https://example.com");
			expect(result.transform?.({ url: null })).toBeNull();
		});

		it("maps terminal_request_frame to POST /sessions/{id}/terminal/request-frame", () => {
			const result = mapCommandToHttp("terminal_request_frame", { sessionId: "s1" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/terminal/request-frame");
		});

		it("maps get_agent_hook_state to GET and unwraps {state}", () => {
			const result = mapCommandToHttp("get_agent_hook_state", { agentType: "claude" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/config/agents/claude/hook-instrumentation");
			expect(result.transform?.({ state: "installed" })).toBe("installed");
		});

		it("maps set_agent_hook_instrumentation to PUT with {enabled} body", () => {
			const result = mapCommandToHttp("set_agent_hook_instrumentation", { agentType: "claude", enabled: true });
			expect(result.method).toBe("PUT");
			expect(result.path).toBe("/config/agents/claude/hook-instrumentation");
			expect(result.body).toEqual({ enabled: true });
		});

		it("maps native status signal commands with exact GET/PUT parity", () => {
			const get = mapCommandToHttp("get_agent_native_status_signals", { agentType: "claude" });
			expect(get.method).toBe("GET");
			expect(get.path).toBe("/config/agents/claude/native-status-signals");
			expect(get.transform?.({ enabled: true })).toBe(true);
			const put = mapCommandToHttp("set_agent_native_status_signals", { agentType: "codex", enabled: false });
			expect(put.method).toBe("PUT");
			expect(put.path).toBe("/config/agents/codex/native-status-signals");
			expect(put.body).toEqual({ enabled: false });
		});

		it("maps read_plugin_data to GET /api/plugins/{id}/data/{path} with notFoundAsNull", () => {
			const result = mapCommandToHttp("read_plugin_data", {
				pluginId: "my-plugin",
				path: "credential-consent-anthropic",
			});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/api/plugins/my-plugin/data/credential-consent-anthropic");
			expect(result.notFoundAsNull).toBe(true);
			// Faithful Option<String> bridge: plain strings pass through, non-strings stringify, null stays null.
			expect(result.transform?.("allowed")).toBe("allowed");
			expect(result.transform?.({ a: 1 })).toBe('{"a":1}');
			expect(result.transform?.(null)).toBeNull();
		});

		it("maps write_plugin_data to POST with content body", () => {
			const result = mapCommandToHttp("write_plugin_data", {
				pluginId: "my-plugin",
				path: "credential-consent-anthropic",
				content: "allowed",
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/api/plugins/my-plugin/data/credential-consent-anthropic");
			expect(result.body).toEqual({ content: "allowed" });
		});

		it("maps resolve_terminal_path to GET with null-passthrough transform", () => {
			const result = mapCommandToHttp("resolve_terminal_path", { cwd: "/repo", candidate: "src/x.ts" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/fs/resolve-terminal-path?cwd=%2Frepo&candidate=src%2Fx.ts");
			expect(result.transform?.({ absolute_path: "/repo/src/x.ts", is_directory: false })).toEqual({
				absolute_path: "/repo/src/x.ts",
				is_directory: false,
			});
			expect(result.transform?.(null)).toBeNull();
		});

		// POST, not GET: a screenful of candidates does not belong in a query
		// string, and the whole point of the batch is that it can be large.
		it("maps resolve_terminal_paths to POST with the candidates in the body", () => {
			const result = mapCommandToHttp("resolve_terminal_paths", {
				cwd: "/repo",
				candidates: ["src/x.ts", "missing.ts"],
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/resolve-terminal-paths");
			expect(result.body).toEqual({ cwd: "/repo", candidates: ["src/x.ts", "missing.ts"] });
		});

		it("maps Markdown link resolution to the shared HTTP endpoint", () => {
			const result = mapCommandToHttp("resolve_markdown_link", {
				root: "/repo",
				currentFile: "docs/review.md",
				href: "../guide.md#intro",
			});
			expect(result).toMatchObject({
				method: "POST",
				path: "/fs/resolve-markdown-link",
				body: { root: "/repo", currentFile: "docs/review.md", href: "../guide.md#intro" },
			});
		});

		it("maps stat_path to GET /fs/stat?path=", () => {
			const result = mapCommandToHttp("stat_path", { path: "/repo/file.md" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/fs/stat?path=%2Frepo%2Ffile.md");
		});

		it("maps warm_content_index to POST /fs/warm-index", () => {
			const result = mapCommandToHttp("warm_content_index", { repoPath: "/repo" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/warm-index");
			expect(result.body).toEqual({ repoPath: "/repo" });
		});

		it("maps write_external_file to POST /fs/write-external", () => {
			const result = mapCommandToHttp("write_external_file", { path: "/repo/a.md", content: "hi" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/write-external");
			expect(result.body).toEqual({ path: "/repo/a.md", content: "hi" });
		});

		it("maps write_file_if_unchanged to POST /fs/write-if-unchanged with the expected text", () => {
			const result = mapCommandToHttp("write_file_if_unchanged", {
				repoPath: "/repo",
				file: "a.md",
				expected: "- [ ] a",
				content: "- [x] a",
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/write-if-unchanged");
			expect(result.body).toEqual({ repoPath: "/repo", file: "a.md", expected: "- [ ] a", content: "- [x] a" });
		});

		it("maps copy_path_abs to POST /fs/copy-abs", () => {
			const result = mapCommandToHttp("copy_path_abs", { from: "/a/x", to: "/b/x" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/copy-abs");
			expect(result.body).toEqual({ from: "/a/x", to: "/b/x" });
		});

		it("maps move_path_abs to POST /fs/move-abs", () => {
			const result = mapCommandToHttp("move_path_abs", { from: "/a/x", to: "/b/x" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/move-abs");
			expect(result.body).toEqual({ from: "/a/x", to: "/b/x" });
		});

		it("maps fs_transfer_paths to POST /fs/transfer", () => {
			const result = mapCommandToHttp("fs_transfer_paths", {
				destDir: "/repo/dst",
				paths: ["/a/x", "/a/y"],
				mode: "move",
				allowRecursive: true,
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/transfer");
			expect(result.body).toEqual({
				destDir: "/repo/dst",
				paths: ["/a/x", "/a/y"],
				mode: "move",
				allowRecursive: true,
			});
		});

		// Catches: exposing Finder source paths to HTTP token holders and enabling exfiltration.
		it("keeps remote copy coordination desktop-only", () => {
			expect(INTENTIONALLY_UNMAPPED.has("fs_transfer_remote_paths")).toBe(true);
			expect(() =>
				mapCommandToHttp("fs_transfer_remote_paths", {
					connectionId: "mint",
					destDir: "/repo/dst",
					paths: ["/Mac/file"],
					allowRecursive: false,
				}),
			).toThrow(/native\/host-only/);
		});

		// --- PTY/terminal read commands (story 062) ---
		it("maps get_shell_state to GET with {state} unwrap transform", () => {
			const result = mapCommandToHttp("get_shell_state", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/shell-state");
			expect(result.transform?.({ state: "busy" })).toBe("busy");
			expect(result.transform?.({ state: null })).toBeNull();
		});

		// Catches: inspector RPC routed to the latest typed prompt or wrong PTY.
		it("maps get_prompt_receipt to the stored session receipt", () => {
			const result = mapCommandToHttp("get_prompt_receipt", { sessionId: "launch-1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/launch-1/prompt-receipt");
		});

		it("maps get_last_prompt to GET with {prompt} unwrap transform", () => {
			const result = mapCommandToHttp("get_last_prompt", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/last-prompt");
			expect(result.transform?.({ prompt: "do the thing" })).toBe("do the thing");
			expect(result.transform?.({ prompt: null })).toBeNull();
		});

		it("maps get_input_buffer_content to GET with {content} unwrap transform", () => {
			const result = mapCommandToHttp("get_input_buffer_content", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/input-buffer");
			expect(result.transform?.({ content: "ls -la" })).toBe("ls -la");
		});

		it("maps get_session_leaf_pid to GET with {pid} unwrap transform", () => {
			const result = mapCommandToHttp("get_session_leaf_pid", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/leaf-pid");
			expect(result.transform?.({ pid: 4321 })).toBe(4321);
			expect(result.transform?.({ pid: null })).toBeNull();
		});

		it("maps has_foreground_process to GET with {process} unwrap transform", () => {
			const result = mapCommandToHttp("has_foreground_process", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/has-foreground");
			expect(result.transform?.({ process: "htop" })).toBe("htop");
			expect(result.transform?.({ process: null })).toBeNull();
		});

		it("maps set_session_visible to POST /sessions/{id}/visible", () => {
			const result = mapCommandToHttp("set_session_visible", { sessionId: "s1", visible: false });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/visible");
			expect(result.body).toEqual({ visible: false });
		});

		it("maps get_process_stats to GET /process/stats", () => {
			const result = mapCommandToHttp("get_process_stats", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/process/stats");
		});

		it("maps terminal_get_selection_text to GET with {text} unwrap transform", () => {
			const result = mapCommandToHttp("terminal_get_selection_text", {
				sessionId: "s1",
				startRow: 1,
				startCol: 2,
				endRow: 3,
				endCol: 4,
			});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/selection-text?startRow=1&startCol=2&endRow=3&endCol=4");
			expect(result.transform?.({ text: "hello" })).toBe("hello");
		});

		it("maps terminal_get_logical_line to GET (tuple array, no transform)", () => {
			const result = mapCommandToHttp("terminal_get_logical_line", { sessionId: "s1", row: 7 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/logical-line?row=7");
			expect(result.transform).toBeUndefined();
		});

		it("maps terminal_hyperlink_span to GET with null-passthrough transform", () => {
			const result = mapCommandToHttp("terminal_hyperlink_span", { sessionId: "s1", row: 2, col: 5 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/hyperlink-span?row=2&col=5");
			expect(result.transform?.([2, 9, "https://x.dev"])).toEqual([2, 9, "https://x.dev"]);
			expect(result.transform?.(null)).toBeNull();
		});

		// --- Claude Usage dashboard (story 063) ---
		it("maps get_claude_usage_api to GET /claude/usage", () => {
			const result = mapCommandToHttp("get_claude_usage_api", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/claude/usage");
		});

		it("routes a Claude session usage request to its owning backend", () => {
			const result = mapCommandToHttp("get_claude_usage_api", { sessionId: "private/session" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/claude/usage?sessionId=private%2Fsession");
		});

		it("maps get_claude_project_list to GET /claude/projects", () => {
			const result = mapCommandToHttp("get_claude_project_list", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/claude/projects");
		});

		it("maps get_claude_usage_timeline to GET with scope + days", () => {
			const result = mapCommandToHttp("get_claude_usage_timeline", { scope: "all", days: 7 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/claude/timeline?scope=all&days=7");
		});

		it("maps get_claude_usage_timeline omitting days when absent", () => {
			const result = mapCommandToHttp("get_claude_usage_timeline", { scope: "my-proj" });
			expect(result.path).toBe("/claude/timeline?scope=my-proj");
		});

		it("maps get_claude_session_stats to GET with scope", () => {
			const result = mapCommandToHttp("get_claude_session_stats", { scope: "current" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/claude/session-stats?scope=current");
		});

		// --- Git panel (story 064) ---
		it("maps get_gutter_changes to GET with optional scope", () => {
			const a = mapCommandToHttp("get_gutter_changes", { path: "/r", file: "a.ts", scope: "head" });
			expect(a.method).toBe("GET");
			expect(a.path).toBe("/repo/gutter-changes?path=%2Fr&file=a.ts&scope=head");
			const b = mapCommandToHttp("get_gutter_changes", { path: "/r", file: "a.ts" });
			expect(b.path).toBe("/repo/gutter-changes?path=%2Fr&file=a.ts");
		});

		it("maps get_branches_detail to GET /repo/branches-detail", () => {
			const result = mapCommandToHttp("get_branches_detail", { path: "/r" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/branches-detail?path=%2Fr");
		});

		it("maps get_recent_branches with optional limit", () => {
			expect(mapCommandToHttp("get_recent_branches", { path: "/r", limit: 5 }).path).toBe(
				"/repo/recent-branches?path=%2Fr&limit=5",
			);
			expect(mapCommandToHttp("get_recent_branches", { path: "/r" }).path).toBe("/repo/recent-branches?path=%2Fr");
		});

		it("maps get_branch_base to GET with null-passthrough transform", () => {
			const result = mapCommandToHttp("get_branch_base", { path: "/r", branchName: "feat" });
			expect(result.path).toBe("/repo/branch-base?path=%2Fr&branchName=feat");
			expect(result.transform?.("main")).toBe("main");
			expect(result.transform?.(null)).toBeNull();
		});

		// Addressed by workspace id, not branch: two workspaces may share a branch,
		// and this answer gates an irreversible cleanup (#726-5ac7).
		it("maps check_worktree_dirty to GET keyed by workspace id", () => {
			const result = mapCommandToHttp("check_worktree_dirty", { repoPath: "/r", workspaceId: "feat~a1b2c3d4" });
			expect(result.path).toBe("/repo/worktree-dirty?repoPath=%2Fr&workspaceId=feat~a1b2c3d4");
		});

		// The one worktree command whose identifier rides in the PATH, so a
		// regression here sends a DELETE to a URL that names the wrong workspace —
		// or, if the id ever reverts to a branch, to whichever workspace git
		// happens to list first (#727-2085).
		it("maps remove_worktree to a DELETE addressed by workspace id", () => {
			const result = mapCommandToHttp("remove_worktree", {
				repoPath: "/r",
				workspaceId: "feat~a1b2c3d4",
				deleteBranch: true,
			});
			expect(result.method).toBe("DELETE");
			expect(result.path).toBe("/worktrees/feat~a1b2c3d4?repoPath=%2Fr&deleteBranch=true");
		});

		it("forwards an explicit lock override separately from dirty-file force", () => {
			const result = mapCommandToHttp("remove_worktree", {
				repoPath: "/r",
				workspaceId: "locked",
				deleteBranch: true,
				force: true,
				overrideLock: true,
			});
			expect(result.path).toBe("/worktrees/locked?repoPath=%2Fr&deleteBranch=true&force=true&overrideLock=true");
		});

		it("forwards the confirmed worktree fingerprint to the HTTP removal route", () => {
			const result = mapCommandToHttp("remove_worktree", {
				repoPath: "/r",
				workspaceId: "dirty",
				force: true,
				expectedFingerprint: "abc123",
			});
			expect(result.path).toBe(
				"/worktrees/dirty?repoPath=%2Fr&deleteBranch=false&force=true&expectedFingerprint=abc123",
			);
		});

		it("forwards missing-checkout confirmation without a fingerprint", () => {
			const result = mapCommandToHttp("remove_worktree", {
				repoPath: "/r",
				workspaceId: "missing",
				force: true,
				confirmMissingCheckout: true,
			});
			expect(result.path).toBe(
				"/worktrees/missing?repoPath=%2Fr&deleteBranch=false&force=true&confirmMissingCheckout=true",
			);
		});

		it("keeps the branch by default when force only discards checkout files", () => {
			const result = mapCommandToHttp("remove_worktree", {
				repoPath: "/r",
				workspaceId: "dirty",
				force: true,
			});
			expect(result.path).toBe("/worktrees/dirty?repoPath=%2Fr&deleteBranch=false&force=true");
		});

		// Creation is the one command that takes a branch and no id — the id does
		// not exist yet. It comes back in the response.
		it("maps create_worktree to POST carrying the branch name", () => {
			const result = mapCommandToHttp("create_worktree", { baseRepo: "/r", branchName: "feat" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/worktrees");
			expect(result.body).toEqual({ base_repo: "/r", branch_name: "feat" });
		});

		it("maps list_base_ref_options to GET", () => {
			expect(mapCommandToHttp("list_base_ref_options", { repoPath: "/r" }).path).toBe(
				"/repo/base-ref-options?repoPath=%2Fr",
			);
		});

		it("maps generate_clone_branch_name_cmd to POST", () => {
			const result = mapCommandToHttp("generate_clone_branch_name_cmd", {
				sourceBranch: "main",
				existingNames: ["a", "b"],
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/clone-branch-name");
			expect(result.body).toEqual({ sourceBranch: "main", existingNames: ["a", "b"] });
		});

		it("maps get_commit_graph with optional count", () => {
			expect(mapCommandToHttp("get_commit_graph", { path: "/r", count: 200 }).path).toBe(
				"/repo/commit-graph?path=%2Fr&count=200",
			);
			expect(mapCommandToHttp("get_commit_graph", { path: "/r" }).path).toBe("/repo/commit-graph?path=%2Fr");
		});

		it("maps create_branch to POST", () => {
			const result = mapCommandToHttp("create_branch", {
				path: "/r",
				name: "feat",
				startPoint: "main",
				checkout: true,
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/create-branch");
			expect(result.body).toEqual({ path: "/r", name: "feat", startPoint: "main", checkout: true });
		});

		it("maps delete_branch to POST", () => {
			const result = mapCommandToHttp("delete_branch", { path: "/r", name: "feat", force: false });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/delete-branch");
			expect(result.body).toEqual({ path: "/r", name: "feat", force: false });
		});

		it("maps delete_local_branch to POST", () => {
			// Two identifiers, two fields: the branch is the ref to delete, the
			// workspace is the checkout to dispose of (#726-5ac7).
			const result = mapCommandToHttp("delete_local_branch", {
				repoPath: "/r",
				branchName: "feat",
				workspaceId: "feat~a1b2c3d4",
				keepWorktree: true,
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/delete-local-branch");
			expect(result.body).toEqual({
				repoPath: "/r",
				branchName: "feat",
				workspaceId: "feat~a1b2c3d4",
				keepWorktree: true,
			});
		});

		it("maps update_from_base to POST", () => {
			const result = mapCommandToHttp("update_from_base", {
				path: "/r",
				branchName: "feat",
				strategy: "rebase",
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/update-from-base");
			expect(result.body).toEqual({ path: "/r", branchName: "feat", strategy: "rebase" });
		});

		it("maps switch_branch to POST", () => {
			const result = mapCommandToHttp("switch_branch", {
				repoPath: "/r",
				branchName: "feat",
				force: false,
				stash: true,
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/switch-branch");
			expect(result.body).toEqual({ repoPath: "/r", branchName: "feat", force: false, stash: true });
		});

		it("maps merge_and_archive_worktree to POST", () => {
			const result = mapCommandToHttp("merge_and_archive_worktree", {
				repoPath: "/r",
				branchName: "feat",
				targetBranch: "main",
				afterMerge: "archive",
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/merge-archive-worktree");
			expect(result.body).toEqual({
				repoPath: "/r",
				branchName: "feat",
				targetBranch: "main",
				afterMerge: "archive",
				force: undefined,
			});
		});

		it("forwards the merge_and_archive_worktree force flag over HTTP", () => {
			const result = mapCommandToHttp("merge_and_archive_worktree", {
				repoPath: "/r",
				branchName: "feat",
				targetBranch: "main",
				afterMerge: "archive",
				force: true,
			});
			expect(result.body).toEqual({
				repoPath: "/r",
				branchName: "feat",
				targetBranch: "main",
				afterMerge: "archive",
				force: true,
			});
		});

		it("forwards the finalize_merged_worktree force flag over HTTP", () => {
			// Finalize ends in `git worktree remove --force` just like merge-and-archive,
			// so the confirmation override has to reach the backend on both transports.
			const guarded = mapCommandToHttp("finalize_merged_worktree", {
				repoPath: "/r",
				workspaceId: "feat~a1b2c3d4",
				action: "archive",
			});
			expect(guarded.method).toBe("POST");
			expect(guarded.path).toBe("/worktrees/finalize");
			expect(guarded.body).toEqual({
				repoPath: "/r",
				workspaceId: "feat~a1b2c3d4",
				action: "archive",
				force: undefined,
			});

			const forced = mapCommandToHttp("finalize_merged_worktree", {
				repoPath: "/r",
				workspaceId: "feat~a1b2c3d4",
				action: "archive",
				force: true,
			});
			expect(forced.body).toEqual({
				repoPath: "/r",
				workspaceId: "feat~a1b2c3d4",
				action: "archive",
				force: true,
			});
		});

		it("maps close_issue to POST", () => {
			const result = mapCommandToHttp("close_issue", { repoPath: "/r", issueNumber: 42 });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/issues/close");
			expect(result.body).toEqual({ repoPath: "/r", issueNumber: 42 });
		});

		it("maps reopen_issue to POST", () => {
			const result = mapCommandToHttp("reopen_issue", { repoPath: "/r", issueNumber: 42 });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/issues/reopen");
			expect(result.body).toEqual({ repoPath: "/r", issueNumber: 42 });
		});

		it("maps get_issue_detail to GET", () => {
			const result = mapCommandToHttp("get_issue_detail", { repoPath: "/r", issueNumber: 42 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/issue-detail?repoPath=%2Fr&issueNumber=42");
		});

		it("maps GitHub write primitives to HTTP", () => {
			const pr = mapCommandToHttp("create_pr", {
				repoPath: "/r",
				title: "Fix bug",
				body: "Details",
				base: "main",
				head: "fix/bug",
				draft: true,
			});
			expect(pr.method).toBe("POST");
			expect(pr.path).toBe("/repo/create-pr");
			expect(pr.body).toEqual({
				repoPath: "/r",
				title: "Fix bug",
				body: "Details",
				base: "main",
				head: "fix/bug",
				draft: true,
			});

			const issue = mapCommandToHttp("create_issue", { repoPath: "/r", title: "Bug", body: "Broken" });
			expect(issue.method).toBe("POST");
			expect(issue.path).toBe("/repo/create-issue");
			expect(issue.body).toEqual({ repoPath: "/r", title: "Bug", body: "Broken" });

			const review = mapCommandToHttp("post_pr_review", {
				repoPath: "/r",
				prNumber: 42,
				body: "Review",
				event: "COMMENT",
				comments: [{ path: "src/main.rs", line: 10, side: "RIGHT", body: "Check this" }],
			});
			expect(review.method).toBe("POST");
			expect(review.path).toBe("/repo/post-pr-review");
			expect(review.body).toEqual({
				repoPath: "/r",
				prNumber: 42,
				body: "Review",
				event: "COMMENT",
				comments: [{ path: "src/main.rs", line: 10, side: "RIGHT", body: "Check this" }],
			});
		});

		it("maps get_merged_prs to GET with optional sinceTag", () => {
			const noTag = mapCommandToHttp("get_merged_prs", { repoPath: "/r" });
			expect(noTag.method).toBe("GET");
			expect(noTag.path).toBe("/repo/merged-prs?path=%2Fr");

			const withTag = mapCommandToHttp("get_merged_prs", { repoPath: "/r", sinceTag: "v1.2.0" });
			expect(withTag.path).toBe("/repo/merged-prs?path=%2Fr&sinceTag=v1.2.0");
		});

		it("maps start_conflict_assist to POST", () => {
			const result = mapCommandToHttp("start_conflict_assist", { repoPath: "/r", prNumber: 7 });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/conflict-assist");
			expect(result.body).toEqual({ repoPath: "/r", prNumber: 7 });
		});

		it("maps get_github_viewer_login to GET", () => {
			const result = mapCommandToHttp("get_github_viewer_login", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/github/viewer-login");
		});

		it("maps fetch_ci_failure_logs to GET with query", () => {
			const result = mapCommandToHttp("fetch_ci_failure_logs", { repoPath: "/r", branch: "feat" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/ci-failure-logs?repoPath=%2Fr&branch=feat");
			expect(
				mapCommandToHttp("fetch_ci_failure_logs", {
					repoPath: "/r",
					branch: "feat",
					checkUrl: "https://circleci.com/gh/a/b/1",
					headSha: "abc",
				}).path,
			).toBe(
				"/repo/ci-failure-logs?repoPath=%2Fr&branch=feat&checkUrl=https%3A%2F%2Fcircleci.com%2Fgh%2Fa%2Fb%2F1&headSha=abc",
			);
		});

		it("maps github_set_pr_hide_drafts to POST", () => {
			const result = mapCommandToHttp("github_set_pr_hide_drafts", { hide: true });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/github/pr-hide-drafts");
			expect(result.body).toEqual({ hide: true });
		});

		it("maps github device-code auth flow", () => {
			expect(mapCommandToHttp("github_start_login", {}).path).toBe("/github/auth/start");
			expect(mapCommandToHttp("github_start_login", {}).method).toBe("POST");
			const poll = mapCommandToHttp("github_poll_login", { deviceCode: "abc" });
			expect(poll.method).toBe("POST");
			expect(poll.path).toBe("/github/auth/poll");
			expect(poll.body).toEqual({ deviceCode: "abc" });
			const addPoll = mapCommandToHttp("github_poll_add_account", { deviceCode: "def" });
			expect(addPoll.method).toBe("POST");
			expect(addPoll.path).toBe("/github/accounts/poll");
			expect(addPoll.body).toEqual({ deviceCode: "def" });
			expect(mapCommandToHttp("github_logout", {}).path).toBe("/github/auth/logout");
			expect(mapCommandToHttp("github_disconnect", {}).path).toBe("/github/auth/disconnect");
			expect(mapCommandToHttp("github_auth_status", {}).path).toBe("/github/auth/status");
			expect(mapCommandToHttp("github_auth_status", {}).method).toBe("GET");
			expect(mapCommandToHttp("github_diagnostics", {}).path).toBe("/github/diagnostics");
		});

		it("maps multi-account accounts + repo bindings", () => {
			expect(mapCommandToHttp("github_list_accounts", {}).method).toBe("GET");
			expect(mapCommandToHttp("github_list_accounts", {}).path).toBe("/github/accounts");

			const add = mapCommandToHttp("github_add_account", { host: "ghe.acme.com", pat: "ghp_x" });
			expect(add.method).toBe("POST");
			expect(add.path).toBe("/github/accounts");
			expect(add.body).toEqual({ host: "ghe.acme.com", pat: "ghp_x" });

			const rm = mapCommandToHttp("github_remove_account", { id: "ghe.acme.com" });
			expect(rm.method).toBe("POST");
			expect(rm.path).toBe("/github/accounts/remove");
			expect(rm.body).toEqual({ id: "ghe.acme.com" });

			expect(mapCommandToHttp("github_list_bindings", {}).path).toBe("/github/bindings");
			expect(mapCommandToHttp("github_list_bindings", {}).method).toBe("GET");

			const bind = mapCommandToHttp("github_bind_repo", {
				repoPath: "/my/repo",
				accountId: "ghe.acme.com",
				remoteName: "origin",
			});
			expect(bind.method).toBe("POST");
			expect(bind.path).toBe("/github/bindings");
			expect(bind.body).toEqual({ repoPath: "/my/repo", accountId: "ghe.acme.com", remoteName: "origin" });

			const unbind = mapCommandToHttp("github_unbind_repo", { repoPath: "/my/repo" });
			expect(unbind.method).toBe("POST");
			expect(unbind.path).toBe("/github/bindings/remove");
			expect(unbind.body).toEqual({ repoPath: "/my/repo" });

			const resolve = mapCommandToHttp("github_resolve_repo", { repoPath: "/my/repo" });
			expect(resolve.method).toBe("GET");
			expect(resolve.path).toBe("/github/resolve-repo?repoPath=%2Fmy%2Frepo");

			const resolveBatch = mapCommandToHttp("github_resolve_repos", { repoPaths: ["/a", "/b"] });
			expect(resolveBatch.method).toBe("POST");
			expect(resolveBatch.path).toBe("/github/resolve-repos");
			expect(resolveBatch.body).toEqual({ repoPaths: ["/a", "/b"] });
		});

		it("maps note asset commands", () => {
			const img = mapCommandToHttp("save_note_image", {
				noteId: "n1",
				dataBase64: "AAA",
				extension: "png",
			});
			expect(img.path).toBe("/config/note-image");
			expect(img.body).toEqual({ noteId: "n1", dataBase64: "AAA", extension: "png" });
			expect(mapCommandToHttp("delete_note_assets", { noteId: "n1" }).path).toBe("/config/note-assets/delete");
			const batch = mapCommandToHttp("delete_note_assets_batch", { noteIds: ["a", "b"] });
			expect(batch.path).toBe("/config/note-assets/delete-batch");
			expect(batch.body).toEqual({ noteIds: ["a", "b"] });
		});

		it("maps config/themes/mcp-upstreams commands", () => {
			expect(mapCommandToHttp("list_themes", {}).path).toBe("/config/themes");
			const rlc = mapCommandToHttp("save_repo_local_config", { repoPath: "/r" });
			expect(rlc.method).toBe("POST");
			expect(rlc.body).toEqual({ repoPath: "/r" });
			const bl = mapCommandToHttp("set_branch_label", {
				repoPath: "/r",
				branchName: "feat",
				label: "x",
			});
			expect(bl.path).toBe("/config/branch-label");
			expect(bl.body).toEqual({ repoPath: "/r", branchName: "feat", label: "x" });
			const up = mapCommandToHttp("set_project_mcp_upstreams", {
				repoPath: "/r",
				upstreamNames: ["a"],
			});
			expect(up.path).toBe("/config/project-mcp-upstreams");
			expect(up.body).toEqual({ repoPath: "/r", upstreamNames: ["a"] });
		});

		it("maps misc command parity (shell/audio/agent/generators/registry)", () => {
			const sh = mapCommandToHttp("execute_shell_script", {
				scriptContent: "echo hi",
				timeoutMs: 5000,
				repoPath: "/r",
			});
			expect(sh.method).toBe("POST");
			expect(sh.path).toBe("/exec/shell-script");
			expect(sh.body).toEqual({ scriptContent: "echo hi", timeoutMs: 5000, repoPath: "/r" });
			expect(mapCommandToHttp("list_audio_output_devices", {}).path).toBe("/audio/output-devices");
			const disc = mapCommandToHttp("discover_agent_session", {
				agentType: "claude",
				cwd: "/r",
				claimedIds: [],
				agentPid: 123,
				envOverrides: {},
			});
			expect(disc.path).toBe("/agent/discover-session");
			expect(disc.body).toEqual({
				agentType: "claude",
				cwd: "/r",
				claimedIds: [],
				agentPid: 123,
				envOverrides: {},
			});
			expect(mapCommandToHttp("claude_project_dir", { cwd: "/r", claudeConfigDir: null }).path).toBe(
				"/agent/claude-project-dir",
			);
			const oic = mapCommandToHttp("open_in_custom", {
				executable: "code",
				args: ["-g"],
				ctx: { repo: "/r" },
			});
			expect(oic.path).toBe("/agent/open-in-custom");
			expect(oic.body).toEqual({ executable: "code", args: ["-g"], ctx: { repo: "/r" } });
			const gen = mapCommandToHttp("generate_value", { request: { type: "password" } });
			expect(gen.path).toBe("/generators/generate");
			expect(gen.body).toEqual({ request: { type: "password" } });
			expect(mapCommandToHttp("fetch_plugin_registry", {}).path).toBe("/registry/plugins");
		});

		it("maps plugin RPC commands (story 071)", () => {
			// plugin_read_file
			const rf = mapCommandToHttp("plugin_read_file", { pluginId: "my-plugin", path: "/home/user/f.txt" });
			expect(rf.method).toBe("GET");
			expect(rf.path).toBe("/api/plugins/my-plugin/fs/read?path=%2Fhome%2Fuser%2Ff.txt");

			// plugin_read_files — the batch read goes in the body: a query string
			// cannot carry hundreds of paths.
			const rfs = mapCommandToHttp("plugin_read_files", {
				pluginId: "my-plugin",
				paths: ["/home/user/a.md", "/home/user/b.md"],
			});
			expect(rfs.method).toBe("POST");
			expect(rfs.path).toBe("/api/plugins/my-plugin/fs/read-batch");
			expect(rfs.body).toEqual({ paths: ["/home/user/a.md", "/home/user/b.md"] });

			// plugin_read_file_base64
			const rfb = mapCommandToHttp("plugin_read_file_base64", { pluginId: "my-plugin", path: "/home/user/f.docx" });
			expect(rfb.method).toBe("GET");
			expect(rfb.path).toBe("/api/plugins/my-plugin/fs/read-base64?path=%2Fhome%2Fuser%2Ff.docx");
			const largeRfb = mapCommandToHttp("plugin_read_file_base64", {
				pluginId: "my-plugin",
				path: "/home/user/database.sqlite",
				maxBytes: 268_435_456,
			});
			expect(largeRfb.path).toBe(
				"/api/plugins/my-plugin/fs/read-base64?path=%2Fhome%2Fuser%2Fdatabase.sqlite&maxBytes=268435456",
			);

			// plugin_read_file_tail
			const tail = mapCommandToHttp("plugin_read_file_tail", {
				pluginId: "my-plugin",
				path: "/home/user/f.log",
				maxBytes: 4096,
			});
			expect(tail.method).toBe("GET");
			expect(tail.path).toBe("/api/plugins/my-plugin/fs/tail?path=%2Fhome%2Fuser%2Ff.log&maxBytes=4096");

			// plugin_list_directory — with optional params
			const listBase = mapCommandToHttp("plugin_list_directory", { pluginId: "my-plugin", path: "/home/user/dir" });
			expect(listBase.method).toBe("GET");
			expect(listBase.path).toBe("/api/plugins/my-plugin/fs/list?path=%2Fhome%2Fuser%2Fdir");
			const listFull = mapCommandToHttp("plugin_list_directory", {
				pluginId: "my-plugin",
				path: "/home/user/dir",
				pattern: "*.log",
				sortBy: "mtime",
			});
			expect(listFull.path).toContain("pattern=*.log");
			expect(listFull.path).toContain("sortBy=mtime");

			// plugin_write_file
			const wf = mapCommandToHttp("plugin_write_file", {
				pluginId: "my-plugin",
				path: "/home/user/out.txt",
				content: "hello",
			});
			expect(wf.method).toBe("POST");
			expect(wf.path).toBe("/api/plugins/my-plugin/fs/write");
			expect(wf.body).toEqual({ path: "/home/user/out.txt", content: "hello" });

			const binaryWrite = mapCommandToHttp("plugin_write_file_base64", {
				pluginId: "my-plugin",
				path: "/home/user/database.sqlite",
				content: "U1FMaXRl",
				maxBytes: 268435456,
			});
			expect(binaryWrite.method).toBe("POST");
			expect(binaryWrite.path).toBe("/api/plugins/my-plugin/fs/write-base64");
			expect(binaryWrite.body).toEqual({
				path: "/home/user/database.sqlite",
				content: "U1FMaXRl",
				maxBytes: 268435456,
			});

			// plugin_rename_path
			const rn = mapCommandToHttp("plugin_rename_path", {
				pluginId: "my-plugin",
				from: "/home/user/a.txt",
				to: "/home/user/b.txt",
			});
			expect(rn.method).toBe("POST");
			expect(rn.path).toBe("/api/plugins/my-plugin/fs/rename");
			expect(rn.body).toEqual({ from: "/home/user/a.txt", to: "/home/user/b.txt" });

			// scan_build_artifacts
			const scan = mapCommandToHttp("scan_build_artifacts", {
				pluginId: "build-cleaner",
				repoPaths: ["/home/user/repoA", "/home/user/repoB"],
			});
			expect(scan.method).toBe("POST");
			expect(scan.path).toBe("/api/plugins/build-cleaner/build-artifacts/scan");
			expect(scan.body).toEqual({ repoPaths: ["/home/user/repoA", "/home/user/repoB"] });
			const forcedScan = mapCommandToHttp("scan_build_artifacts", {
				pluginId: "build-cleaner",
				repoPaths: ["/home/user/repoA"],
				forceRefresh: true,
			});
			expect(forcedScan.body).toEqual({ repoPaths: ["/home/user/repoA"], forceRefresh: true });

			// delete_build_artifact
			const del = mapCommandToHttp("delete_build_artifact", {
				pluginId: "build-cleaner",
				path: "/home/user/repoA/target",
				repoPaths: ["/home/user/repoA"],
			});
			expect(del.method).toBe("POST");
			expect(del.path).toBe("/api/plugins/build-cleaner/build-artifacts/delete");
			expect(del.body).toEqual({ path: "/home/user/repoA/target", repoPaths: ["/home/user/repoA"] });

			// trim_build_artifact — same body as delete, different route. A browser
			// client must be able to reclaim intermediates without the desktop app.
			const trim = mapCommandToHttp("trim_build_artifact", {
				pluginId: "build-cleaner",
				path: "/home/user/repoA/target",
				repoPaths: ["/home/user/repoA"],
			});
			expect(trim.method).toBe("POST");
			expect(trim.path).toBe("/api/plugins/build-cleaner/build-artifacts/trim");
			expect(trim.body).toEqual({ path: "/home/user/repoA/target", repoPaths: ["/home/user/repoA"] });

			// plugin_exec_cli
			const ex = mapCommandToHttp("plugin_exec_cli", {
				pluginId: "my-plugin",
				binary: "mdkb",
				args: ["--version"],
				cwd: "/home/user",
			});
			expect(ex.method).toBe("POST");
			expect(ex.path).toBe("/api/plugins/my-plugin/exec");
			expect(ex.body).toEqual({ binary: "mdkb", args: ["--version"], cwd: "/home/user" });

			// plugin_http_fetch
			const hf = mapCommandToHttp("plugin_http_fetch", {
				pluginId: "my-plugin",
				url: "https://api.example.com/data",
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: "{}",
			});
			expect(hf.method).toBe("POST");
			expect(hf.path).toBe("/api/plugins/my-plugin/http");
			expect(hf.body).toEqual({
				url: "https://api.example.com/data",
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: "{}",
			});

			// plugin_read_session_output — with and without maxLines
			const pty = mapCommandToHttp("plugin_read_session_output", {
				pluginId: "my-plugin",
				sessionId: "sess-1",
			});
			expect(pty.method).toBe("GET");
			expect(pty.path).toBe("/api/plugins/my-plugin/pty/output?sessionId=sess-1");
			const ptyLines = mapCommandToHttp("plugin_read_session_output", {
				pluginId: "my-plugin",
				sessionId: "sess-1",
				maxLines: 100,
			});
			expect(ptyLines.path).toContain("maxLines=100");

			// register_loaded_plugin
			const reg = mapCommandToHttp("register_loaded_plugin", {
				pluginId: "my-plugin",
				capabilities: ["fs:read", "net:http"],
			});
			expect(reg.method).toBe("POST");
			expect(reg.path).toBe("/api/plugins/my-plugin/register");
			expect(reg.body).toEqual({ capabilities: ["fs:read", "net:http"] });

			// unregister_loaded_plugin
			const unreg = mapCommandToHttp("unregister_loaded_plugin", { pluginId: "my-plugin" });
			expect(unreg.method).toBe("POST");
			expect(unreg.path).toBe("/api/plugins/my-plugin/unregister");

			// get_plugin_readme_path — null passthrough transform
			const readme = mapCommandToHttp("get_plugin_readme_path", { id: "my-plugin" });
			expect(readme.method).toBe("GET");
			expect(readme.path).toBe("/api/plugins/my-plugin/readme");
			expect(readme.transform?.("/path/to/README.md")).toBe("/path/to/README.md");
			expect(readme.transform?.(null)).toBeNull();
		});

		it("maps the remote home lookup to the machine serving the file browser", () => {
			expect(mapCommandToHttp("get_home_directory", {})).toEqual({
				method: "GET",
				path: "/system/home-directory",
			});
		});

		it("maps agent detection and spawn aliases to HTTP", () => {
			const launchArgs = mapCommandToHttp("prepare_agent_launch_args", {
				agentType: "codex",
				binaryPath: "/opt/bin/codex",
				args: ["resume"],
			});
			expect(launchArgs.method).toBe("POST");
			expect(launchArgs.path).toBe("/agents/launch-args");
			expect(launchArgs.body).toEqual({
				agentType: "codex",
				binaryPath: "/opt/bin/codex",
				args: ["resume"],
			});
			const detectClaude = mapCommandToHttp("detect_claude_binary", {});
			expect(detectClaude.method).toBe("GET");
			expect(detectClaude.path).toBe("/agents/detect?binary=claude");
			expect(detectClaude.transform?.({ path: "/usr/local/bin/claude" })).toBe("/usr/local/bin/claude");

			const spawn = mapCommandToHttp("spawn_agent", {
				pty_config: {
					rows: 30,
					cols: 100,
					shell: null,
					cwd: "/repo",
					tuic_session: null,
					env: { PROFILE: "work" },
					agent_type: "codex",
					alias: null,
				},
				agent_config: {
					prompt: "fix it",
					cwd: "/agent",
					agent_type: "codex",
					model: "gpt-5",
					print_mode: false,
					output_format: null,
					binary_path: null,
					args: null,
				},
			});
			expect(spawn.method).toBe("POST");
			expect(spawn.path).toBe("/sessions/agent");
			expect(JSON.parse(JSON.stringify(spawn.body))).toEqual(
				JSON.parse(readRepoFile("src-tauri/tests/fixtures/spawn_agent_http_body.json")),
			);
			expect(spawn.transform?.({ session_id: "s1" })).toBe("s1");
		});

		it("uses PTY cwd when agent cwd is null", () => {
			const spawn = mapCommandToHttp("spawn_agent", {
				pty_config: { rows: 24, cols: 80, cwd: "/repo", shell: null, tuic_session: null, alias: null },
				agent_config: { prompt: "inspect", cwd: null, print_mode: false },
			});
			expect(JSON.parse(JSON.stringify(spawn.body))).toEqual({
				rows: 24,
				cols: 80,
				cwd: "/repo",
				prompt: "inspect",
				print_mode: false,
			});
		});

		it("omits desktop-only PTY identity fields when populated", () => {
			const spawn = mapCommandToHttp("spawn_agent", {
				pty_config: {
					rows: 24,
					cols: 80,
					shell: "/bin/zsh",
					tuic_session: "persistent-session",
					alias: "tu-7",
				},
				agent_config: { prompt: "inspect" },
			});
			expect(JSON.parse(JSON.stringify(spawn.body))).toEqual({ rows: 24, cols: 80, prompt: "inspect" });
		});
	});

	it("maps CircleCI token commands without exposing a token in status", () => {
		expect(mapCommandToHttp("circleci_token_status", {})).toEqual({ method: "GET", path: "/circleci/token" });
		expect(mapCommandToHttp("circleci_set_token", { token: "read-only" })).toEqual({
			method: "POST",
			path: "/circleci/token",
			body: { token: "read-only" },
		});
	});

	describe("ACP (ego) routes", () => {
		const CONNECTION = "01932d5e-0000-7000-8000-0000000000e1";
		const SESSION = "01932d5e-0000-7000-8000-0000000000aa";
		const REQUEST = "01932d5e-0000-7000-8000-0000000000f1";
		const authority = { cwd: "/repo", additionalDirectories: [], mcpServers: [] };

		// The routes the architecture contract fixes, spelled out here rather
		// than derived from the mappers: a test that rebuilt the path the same
		// way the code does would agree with any typo.
		it.each([
			["acp_workspace_root", {}, "GET", "/acp/workspace", undefined],
			[
				"acp_chat_open",
				{ request: { profile: "coordinator", workspace: "/repo", executable: "/bin/ego" } },
				"POST",
				"/acp/chat/open",
				{ profile: "coordinator", workspace: "/repo", executable: "/bin/ego" },
			],
			["acp_connect", { root: "/repo" }, "POST", "/acp/connections", { root: "/repo" }],
			["acp_connection_snapshot", { connectionId: CONNECTION }, "GET", `/acp/connections/${CONNECTION}`, undefined],
			["acp_disconnect", { connectionId: CONNECTION }, "DELETE", `/acp/connections/${CONNECTION}`, undefined],
			["acp_kill", { connectionId: CONNECTION }, "POST", `/acp/connections/${CONNECTION}/kill`, undefined],
			[
				"acp_reconnect",
				{ connectionId: CONNECTION, root: "/repo" },
				"POST",
				`/acp/connections/${CONNECTION}/reconnect`,
				{ root: "/repo" },
			],
			[
				"acp_session_new",
				{ connectionId: CONNECTION, authority },
				"POST",
				`/acp/connections/${CONNECTION}/sessions`,
				{ authority },
			],
			["acp_session_list", { connectionId: CONNECTION }, "GET", `/acp/connections/${CONNECTION}/sessions`, undefined],
			[
				"acp_session_load",
				{ connectionId: CONNECTION, sessionId: SESSION, authority },
				"POST",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/load`,
				{ authority },
			],
			[
				"acp_session_resume",
				{ connectionId: CONNECTION, sessionId: SESSION, authority },
				"POST",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/resume`,
				{ authority },
			],
			[
				"acp_session_fork",
				{ connectionId: CONNECTION, sessionId: SESSION, authority },
				"POST",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/fork`,
				{ authority },
			],
			[
				"acp_session_fork",
				{ connectionId: CONNECTION, sessionId: SESSION, authority, atMessageId: "reply-id" },
				"POST",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/fork`,
				{ authority, atMessageId: "reply-id" },
			],
			[
				"acp_session_delete",
				{ connectionId: CONNECTION, sessionId: SESSION },
				"DELETE",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}`,
				undefined,
			],
			[
				"acp_session_close",
				{ connectionId: CONNECTION, sessionId: SESSION },
				"POST",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/close`,
				undefined,
			],
			[
				"acp_session_prompt",
				{
					connectionId: CONNECTION,
					sessionId: SESSION,
					prompt: [{ type: "text", text: "hi" }],
					viewedRepo: "/repo/viewed",
				},
				"POST",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/prompt`,
				{ prompt: [{ type: "text", text: "hi" }], viewedRepo: "/repo/viewed" },
			],
			[
				"acp_session_cancel",
				{ connectionId: CONNECTION, sessionId: SESSION },
				"POST",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/cancel`,
				undefined,
			],
			[
				"acp_queued_prompt_cancel",
				{ connectionId: CONNECTION, sessionId: SESSION, turnId: "01932d5e-0000-7000-8000-0000000000b1" },
				"DELETE",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/queue/01932d5e-0000-7000-8000-0000000000b1`,
				undefined,
			],
			[
				"acp_session_set_config_option",
				{ connectionId: CONNECTION, sessionId: SESSION, configId: "model", value: { value: "opus" } },
				"POST",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/config`,
				{ configId: "model", value: { value: "opus" } },
			],
			[
				"acp_turn_pause",
				{ connectionId: CONNECTION, sessionId: SESSION, requestId: REQUEST },
				"POST",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/pause`,
				{ requestId: REQUEST },
			],
			[
				"acp_turn_resume",
				{ connectionId: CONNECTION, sessionId: SESSION, requestId: REQUEST },
				"POST",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/resume-turn`,
				{ requestId: REQUEST },
			],
			[
				"acp_session_compact",
				{ connectionId: CONNECTION, sessionId: SESSION, requestId: REQUEST },
				"POST",
				`/acp/connections/${CONNECTION}/sessions/${SESSION}/compact`,
				{ requestId: REQUEST },
			],
			[
				"acp_pending_interactions",
				{ connectionId: CONNECTION },
				"GET",
				`/acp/connections/${CONNECTION}/interactions`,
				undefined,
			],
			[
				"acp_respond_permission",
				{ connectionId: CONNECTION, requestId: REQUEST, outcome: { outcome: "cancelled" } },
				"POST",
				`/acp/connections/${CONNECTION}/permissions/${REQUEST}/response`,
				{ outcome: { outcome: "cancelled" } },
			],
			[
				"acp_respond_elicitation",
				{ connectionId: CONNECTION, requestId: REQUEST, action: "cancel" },
				"POST",
				`/acp/connections/${CONNECTION}/elicitations/${REQUEST}/response`,
				{ action: "cancel" },
			],
		])("maps %s onto its final route", (command, args, method, path, body) => {
			const mapping = mapCommandToHttp(command as string, args as Record<string, unknown>);
			expect(mapping.method).toBe(method);
			expect(mapping.path).toBe(path);
			expect(mapping.body).toEqual(body);
		});

		it("carries the list filters as query parameters when they are given", () => {
			const mapping = mapCommandToHttp("acp_session_list", {
				connectionId: CONNECTION,
				cwd: "/repo",
				cursor: "page-2",
			});
			expect(mapping.path).toBe(`/acp/connections/${CONNECTION}/sessions?cwd=%2Frepo&cursor=page-2`);
		});

		it("refuses a request that names no connection instead of building a path with undefined in it", () => {
			expect(() => mapCommandToHttp("acp_connection_snapshot", {})).toThrow(/missing required argument/);
		});

		// The stream is the one ACP command with no request/response route. It
		// must not be classified as host-only: a browser needs it, and saying
		// otherwise would report a feature gap that does not exist.
		it("classifies the stream as a dedicated WebSocket, not as host-only", () => {
			expect(DEDICATED_WS_COMMANDS.has("acp_subscribe")).toBe(true);
			expect(INTENTIONALLY_UNMAPPED.has("acp_subscribe")).toBe(false);
			expect(DEDICATED_WS_COMMANDS.get("acp_subscribe")?.({ connectionId: CONNECTION, afterSequence: 7 })).toBe(
				`/acp/connections/${CONNECTION}/stream?after=7`,
			);
		});

		it("points a caller at the socket instead of raising a generic missing-mapping error", () => {
			expect(() => mapCommandToHttp("acp_subscribe", { connectionId: CONNECTION, afterSequence: 0 })).toThrow(
				new RegExp(`dedicated WebSocket.*/acp/connections/${CONNECTION}/stream\\?after=0`),
			);
		});
	});

	describe("ego command line (Providers)", () => {
		// Catches: browser writes lose roots/network fields or hit the provider endpoint.
		it("keeps the perimeter payload identical to IPC on dedicated routes", () => {
			expect(mapCommandToHttp("ego_perimeter", {})).toEqual({ method: "GET", path: "/ego/perimeter" });
			const roots = { rootDir: "~/Gits", rootAccess: "read", readAllowlist: "/reference", writableDirs: "/scratch" };
			expect(mapCommandToHttp("ego_set_perimeter_roots", { roots })).toEqual({
				method: "POST",
				path: "/ego/perimeter/roots",
				body: { roots },
			});
			expect(mapCommandToHttp("ego_set_perimeter_network", { enabled: false })).toEqual({
				method: "POST",
				path: "/ego/perimeter/network",
				body: { enabled: false },
			});
		});

		it("reads the providers without asking ego to re-enumerate its sources", () => {
			const mapping = mapCommandToHttp("ego_providers", {});
			expect(mapping.method).toBe("GET");
			expect(mapping.path).toBe("/ego/providers");
		});

		// A refresh is the one thing on this surface that reaches a provider over
		// the network, so it has to be asked for rather than implied.
		it("asks for a refresh only when one was requested", () => {
			expect(mapCommandToHttp("ego_providers", { refresh: true }).path).toBe("/ego/providers?refresh=true");
			expect(mapCommandToHttp("ego_providers", { refresh: false }).path).toBe("/ego/providers");
		});

		it("writes the default model as the body of its own route", () => {
			const mapping = mapCommandToHttp("ego_set_default_model", { model: "anthropic/claude-opus-5" });
			expect(mapping.method).toBe("POST");
			expect(mapping.path).toBe("/ego/providers/model");
			expect(mapping.body).toEqual({ model: "anthropic/claude-opus-5" });
		});
	});

	describe("INTENTIONALLY_UNMAPPED (native/host-only commands)", () => {
		it("classifies renamed async wrappers by their public IPC name", () => {
			const registeredCommands = extractRegisteredTauriCommands();

			expect(registeredCommands.has("load_activity")).toBe(true);
			expect(registeredCommands.has("load_activity_async")).toBe(false);
		});

		it("classifies every registered Tauri command as HTTP-mapped or intentionally host-only", () => {
			const mappedCommands = extractCommandTableCommands();
			const registeredCommands = extractRegisteredTauriCommands();
			const uncoveredCommands = Array.from(registeredCommands)
				.filter(
					(command) =>
						!mappedCommands.has(command) && !INTENTIONALLY_UNMAPPED.has(command) && !DEDICATED_WS_COMMANDS.has(command),
				)
				.sort();

			expect(uncoveredCommands).toEqual([]);
		});

		it("raises a precise native-only error, not a generic missing-mapping error", () => {
			for (const command of INTENTIONALLY_UNMAPPED) {
				expect(() => mapCommandToHttp(command, {})).toThrow(/native\/host-only/);
			}
		});

		it("covers the documented native-only command families", () => {
			// Sentinels from each group in the story 073 spec.
			for (const cmd of [
				"open_panel_window",
				"write_clipboard_text",
				"read_clipboard_text",
				"start_native_drag",
				"block_sleep",
				"set_global_hotkey",
				"check_microphone_permission",
				"get_connect_url",
				"regenerate_session_token",
				"get_tailscale_status",
				"mcp_oauth_callback",
				"install_cli",
				"set_last_seen_version",
				"install_mdkb",
				"subscribe_terminal_grid",
				"ack_terminal_frame",
				// story 071 desktop-only plugin commands
				"plugin_watch_path",
				"plugin_unwatch",
				"plugin_read_credential",
				"install_plugin_from_zip",
				"install_plugin_from_folder",
				"install_plugin_from_url",
				"uninstall_plugin",
				"delete_plugin_data",
			]) {
				expect(INTENTIONALLY_UNMAPPED.has(cmd)).toBe(true);
			}
		});

		it("does not also have a COMMAND_TABLE mapping (would be contradictory)", () => {
			// If a command were both mapped and listed unmapped, mapCommandToHttp would
			// succeed and the native-only error would be dead. Guard against that drift.
			for (const command of INTENTIONALLY_UNMAPPED) {
				let mapped = true;
				try {
					mapCommandToHttp(command, {});
				} catch {
					mapped = false;
				}
				expect(mapped).toBe(false);
			}
		});
	});

	/**
	 * Half one of the COMMAND_TABLE → axum-router gate (story 643).
	 *
	 * The table is TypeScript, the router is Rust, so neither language can see
	 * both. Splitting the gate is what makes it able to fail:
	 *
	 *   - here, we EXECUTE every mapper (no source parsing: the mappers build
	 *     paths from template literals, ternaries and query builders, so a regex
	 *     over them would be guesswork) and snapshot the resulting path set;
	 *   - in Rust, `command_table_paths_all_hit_a_registered_route` reads that
	 *     same file and PATCH-probes each path against the real router.
	 *
	 * Adding a COMMAND_TABLE entry therefore fails HERE first (the snapshot is
	 * stale). Regenerate with `pnpm vitest run src/__tests__/transport.test.ts -u`
	 * and, if the route was never registered, the Rust half then fails. Both
	 * halves run in `make check`, so neither can be skipped.
	 */
	describe("COMMAND_TABLE → router parity", () => {
		/**
		 * Every argument resolves to the same URL-safe placeholder, so a mapper
		 * yields one path per branch-free shape. `p()` throws on a missing
		 * required argument, which is exactly what this avoids.
		 */
		const PLACEHOLDER_ARGS = new Proxy({} as Record<string, unknown>, {
			get: () => "x",
			has: () => true,
		});

		function commandHttpPaths(): { paths: string[]; failed: string[] } {
			const paths = new Set<string>();
			const failed: string[] = [];
			for (const command of Array.from(extractCommandTableCommands()).sort()) {
				try {
					// Query strings are stripped: axum matches on the path alone, and
					// keeping them would churn the snapshot on unrelated argument edits.
					paths.add(mapCommandToHttp(command, PLACEHOLDER_ARGS).path.split("?")[0] ?? "");
				} catch (error) {
					failed.push(`${command}: ${error instanceof Error ? error.message : String(error)}`);
				}
			}
			return { paths: Array.from(paths).sort(), failed };
		}

		it("evaluates every mapper — a mapper that throws would silently drop its path", () => {
			// Fail-closed: a path missing from the snapshot is a path the Rust probe
			// never tests, which is the drift this gate exists to prevent.
			expect(commandHttpPaths().failed).toEqual([]);
		});

		it("excludes INTENTIONALLY_UNMAPPED commands, which have no route by design", () => {
			// These are native/host-only and deliberately absent from COMMAND_TABLE.
			// Asserting the disjointness makes the exclusion explicit rather than
			// incidental: were one ever added to the table, it would enter the
			// snapshot and the Rust probe would demand a route that must not exist.
			const mapped = extractCommandTableCommands();
			const leaked = Array.from(INTENTIONALLY_UNMAPPED)
				.filter((command) => mapped.has(command))
				.sort();
			expect(leaked).toEqual([]);
		});

		it("snapshots the path set the Rust router probe reads", async () => {
			const { paths } = commandHttpPaths();
			// Guard against a scanner that quietly stopped matching: an empty or
			// tiny snapshot would make the Rust probe pass by testing nothing.
			expect(paths.length).toBeGreaterThan(200);
			const body = [
				"# Generated by src/__tests__/transport.test.ts — do not edit by hand.",
				"# One HTTP path per COMMAND_TABLE entry, query stripped, deduplicated.",
				"# Read by mcp_http::tests::command_table_paths_all_hit_a_registered_route.",
				"# Regenerate: pnpm vitest run src/__tests__/transport.test.ts -u",
				...paths,
				"",
			].join("\n");
			await expect(body).toMatchFileSnapshot("../../src-tauri/src/mcp_http/command_table_paths.txt");
		});
	});

	describe("rpc()", () => {
		const originalFetch = globalThis.fetch;
		const originalTauri = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
		const discoverArgs = {
			agentType: "claude",
			cwd: "/repo",
			claimedIds: [],
			agentPid: 123,
			envOverrides: {},
		};

		function jsonResponse(body: string, status = 200, statusText = "OK") {
			return {
				ok: status >= 200 && status < 300,
				status,
				statusText,
				headers: new Headers({ "content-type": "application/json" }),
				text: vi.fn().mockResolvedValue(body),
			};
		}

		beforeEach(() => {
			// Ensure non-Tauri mode for HTTP tests
			delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
		});

		afterEach(() => {
			globalThis.fetch = originalFetch;
			if (originalTauri !== undefined) {
				(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = originalTauri;
			} else {
				delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
			}
		});

		it("uses fetch in non-Tauri mode with JSON response", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: true,
				headers: new Headers({ "content-type": "application/json" }),
				text: vi.fn().mockResolvedValue('{"sessions":[]}'),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			const result = await rpc<{ sessions: unknown[] }>("list_active_sessions");
			expect(result).toEqual({ sessions: [] });
			expect(globalThis.fetch).toHaveBeenCalledWith(
				expect.stringContaining("/sessions"),
				expect.objectContaining({ method: "GET" }),
			);
		});

		// Catches: a fixed client-side AbortController (was 30 s) shorter than a backend
		// deadline such as ego's 60 s initialize, surfacing "signal is aborted" instead of the backend message.
		it("does not abort a request before the backend answers, however long it takes", async () => {
			vi.useFakeTimers();
			try {
				const { rpc } = await import("../transport");
				let signal: AbortSignal | undefined;
				globalThis.fetch = vi.fn().mockImplementation((_url: string, init: RequestInit) => {
					signal = init.signal ?? undefined;
					return new Promise((resolve) => {
						setTimeout(() => resolve(jsonResponse('{"ok":true}')), 90_000);
					});
				});

				const pending = rpc<{ ok: boolean }>("list_active_sessions");
				await vi.advanceTimersByTimeAsync(89_000);

				expect(signal?.aborted ?? false).toBe(false);
				await vi.advanceTimersByTimeAsync(2_000);
				await expect(pending).resolves.toEqual({ ok: true });
			} finally {
				vi.useRealTimers();
			}
		});

		it("sends the selected Claude profile root when verifying a browser resume", async () => {
			const { rpc } = await import("../transport");
			globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse("true"));

			await rpc("verify_agent_session", {
				agentType: "claude",
				sessionId: "af467730-5e79-49d9-8a17-ebd94c99f262",
				cwd: "/work/project",
				agentPid: null,
				envOverrides: { CLAUDE_CONFIG_DIR: "/profiles/work" },
			});

			const [url, request] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0];
			expect(url).toContain("/agents/verify-session");
			expect(JSON.parse(request.body)).toEqual({
				agentType: "claude",
				sessionId: "af467730-5e79-49d9-8a17-ebd94c99f262",
				cwd: "/work/project",
				agentPid: null,
				envOverrides: { CLAUDE_CONFIG_DIR: "/profiles/work" },
			});
		});

		it("sends a live agent PID and Codex home when verifying through HTTP", async () => {
			const { rpc } = await import("../transport");
			globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse("false"));

			await rpc("verify_agent_session", {
				agentType: "codex",
				sessionId: "af467730-5e79-49d9-8a17-ebd94c99f262",
				cwd: "/work/project",
				agentPid: 4321,
				envOverrides: { CODEX_HOME: "/profiles/codex" },
			});

			const [, request] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0];
			expect(JSON.parse(request.body)).toMatchObject({
				agentPid: 4321,
				envOverrides: { CODEX_HOME: "/profiles/codex" },
			});
		});

		it("preserves an empty override map for a default-profile resume", async () => {
			const { rpc } = await import("../transport");
			globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse("false"));

			await rpc("verify_agent_session", {
				agentType: "gemini",
				sessionId: "af467730-5e79-49d9-8a17-ebd94c99f262",
				cwd: "/work/project",
				agentPid: null,
				envOverrides: {},
			});

			const [, request] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0];
			expect(JSON.parse(request.body)).toMatchObject({ agentPid: null, envOverrides: {} });
		});

		// The defect this story fixes: rpcImpl used to fetch a remote baseUrl with
		// no Authorization header, no cookie and no token, so `/health` passed and
		// every real call 401'd while the connection still read "connected".
		describe("remote calls carry the connection's credential", () => {
			afterEach(() => {
				setRemoteBaseUrlLookup(() => undefined);
				setRemoteTokenLookup(() => undefined);
			});

			it("signs a remote call with the session token", async () => {
				const { rpc } = await import("../transport");
				setRemoteBaseUrlLookup((id) => (id === "c1" ? "http://remote.test:9876" : undefined));
				setRemoteTokenLookup((id) => (id === "c1" ? "tok-abc" : undefined));
				globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse("[]"));

				await rpc("list_active_sessions", {}, "c1");

				expect(globalThis.fetch).toHaveBeenCalledWith(
					"http://remote.test:9876/sessions?token=tok-abc",
					expect.objectContaining({ method: "GET" }),
				);
			});

			it("leaves a local call unsigned — it has no connection id", async () => {
				const { rpc } = await import("../transport");
				setRemoteTokenLookup(() => "tok-abc");
				globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse("[]"));

				await rpc("list_active_sessions");

				expect(String((globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0][0])).not.toContain("token=");
			});

			it("refuses to call a connection that is not connected", async () => {
				const { rpc } = await import("../transport");
				globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse("[]"));

				await expect(rpc("list_active_sessions", {}, "c-gone")).rejects.toThrow("not connected");
				expect(globalThis.fetch).not.toHaveBeenCalled();
			});
		});

		it("returns a decoded JSON null response as null", async () => {
			const { rpc } = await import("../transport");
			globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse("null"));

			await expect(rpc<string | null>("discover_agent_session", discoverArgs)).resolves.toBeNull();
		});

		it("preserves a decoded non-null JSON response", async () => {
			const { rpc } = await import("../transport");
			globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse('"agent-session-1"'));

			await expect(rpc<string | null>("discover_agent_session", discoverArgs)).resolves.toBe("agent-session-1");
		});

		it("rejects a zero-length JSON response with command context", async () => {
			const { rpc } = await import("../transport");
			globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse(""));

			await expect(rpc("discover_agent_session", discoverArgs)).rejects.toThrow(
				"RPC discover_agent_session: empty response body",
			);
		});

		it("rejects malformed JSON with command context", async () => {
			const { rpc } = await import("../transport");
			globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse("{"));

			await expect(rpc("discover_agent_session", discoverArgs)).rejects.toThrow(
				"RPC discover_agent_session: invalid JSON response",
			);
		});

		it("rejects a non-success JSON response with command context", async () => {
			const { rpc } = await import("../transport");
			globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse("backend unavailable", 503, "Service Unavailable"));

			await expect(rpc("discover_agent_session", discoverArgs)).rejects.toThrow(
				"RPC discover_agent_session failed: 503 backend unavailable",
			);
		});

		/** Drives a burst of writes with the first request held open, and reports
		 *  each request as the list of inputs it carried. */
		async function burstWrites(
			rpc: (c: string, a: Record<string, unknown>) => Promise<unknown>,
			datas: string[],
		): Promise<{ url: string; parts: string[] }[]> {
			const requests: { url: string; parts: string[] }[] = [];
			let releaseFirst: (() => void) | undefined;
			const firstSent = new Promise<void>((resolve) => {
				releaseFirst = resolve;
			});
			globalThis.fetch = vi.fn().mockImplementation((url: string, init: { body: string }) => {
				const body = JSON.parse(init.body);
				requests.push({ url, parts: body.parts ?? [body.data] });
				const settle = requests.length === 1 ? firstSent : Promise.resolve();
				return settle.then(() => ({
					ok: true,
					headers: new Headers({ "content-type": "application/json" }),
					text: vi.fn().mockResolvedValue("{}"),
				}));
			});
			const writes = datas.map((data) => rpc("write_pty", { sessionId: "s1", data }));
			releaseFirst?.();
			await Promise.all(writes);
			return requests;
		}

		it("coalesces keystrokes typed while a write is in flight", async () => {
			// Browser mode posts one HTTP request per keystroke and awaits each before
			// sending the next, so typing speed is capped at one character per RTT.
			const { rpc } = await import("../transport");

			const requests = await burstWrites(rpc, ["h", "e", "l", "l", "o"]);

			expect(requests.flatMap((r) => r.parts).join("")).toBe("hello");
			expect(requests.length).toBeLessThan(5);
			expect(requests[0].parts).toEqual(["h"]);
		});

		it("keeps coalesced keystrokes separate instead of joining them", async () => {
			// The bytes reaching the PTY are the same either way, but `write_pty` is
			// not a byte pipe: the backend runs its per-input bookkeeping once per
			// REQUEST, and that is not a function of the concatenated bytes. A lone
			// "/" opens slash mode; an Escape dismisses it. Joined into "\x1b/" the
			// backend reads a dismissal and the slash menu never opens — so what
			// piles up must travel as parts, not as one string.
			const { rpc } = await import("../transport");

			const requests = await burstWrites(rpc, ["\x1b", "/", "h"]);

			const coalesced = requests.slice(1);
			expect(coalesced.length).toBeGreaterThan(0);
			for (const request of coalesced) {
				expect(request.url).toContain("/write-parts");
			}
			expect(coalesced.flatMap((r) => r.parts)).toEqual(["/", "h"]);
		});

		it("sends a solitary keystroke on the single-input route", async () => {
			// Nothing piled up behind it, so there is no batch — and routing it
			// through the N-ary path would change nothing except the shape.
			const { rpc } = await import("../transport");
			const requests = await burstWrites(rpc, ["x"]);
			expect(requests).toHaveLength(1);
			expect(requests[0].url).toContain("/write");
			expect(requests[0].url).not.toContain("/write-parts");
			expect(requests[0].parts).toEqual(["x"]);
		});

		it("collapses a resize burst to the newest dimensions", async () => {
			// A drag-resize fires one resize_pty per frame and never awaits the last
			// one. The backend reflows on the blocking pool, so two resizes in flight
			// race for the per-session lock and can be applied newest-first: the PTY
			// is left at the OLDER size while the frontend has already recorded the
			// newer one, and nothing corrects it until the next physical resize.
			// Only the newest size means anything, so only the newest may follow the
			// request already in flight — an intermediate size in flight is an
			// intermediate size that can land last.
			const { rpc } = await import("../transport");

			const sent: Array<{ rows: number; cols: number }> = [];
			let releaseFirst: (() => void) | undefined;
			const firstSent = new Promise<void>((resolve) => {
				releaseFirst = resolve;
			});
			globalThis.fetch = vi.fn().mockImplementation((_url: string, init: { body: string }) => {
				const { rows, cols } = JSON.parse(init.body);
				sent.push({ rows, cols });
				const settle = sent.length === 1 ? firstSent : Promise.resolve();
				return settle.then(() => ({
					ok: true,
					headers: new Headers({ "content-type": "application/json" }),
					text: vi.fn().mockResolvedValue("{}"),
				}));
			});

			const resizes = [
				rpc("resize_pty", { sessionId: "s1", rows: 10, cols: 40 }),
				rpc("resize_pty", { sessionId: "s1", rows: 20, cols: 80 }),
				rpc("resize_pty", { sessionId: "s1", rows: 30, cols: 120 }),
				rpc("resize_pty", { sessionId: "s1", rows: 40, cols: 160 }),
			];
			releaseFirst?.();
			await Promise.all(resizes);

			expect(sent[0]).toEqual({ rows: 10, cols: 40 });
			expect(sent.at(-1)!).toEqual({ rows: 40, cols: 160 });
			expect(sent).toHaveLength(2);
		});

		it("keeps one session's resize out of another's", async () => {
			// The queue is keyed per session for the same reason the write queue is:
			// collapsing across sessions would drop a real resize, not a stale one.
			const { rpc } = await import("../transport");

			const seen: Array<{ url: string; rows: number }> = [];
			globalThis.fetch = vi.fn().mockImplementation((url: string, init: { body: string }) => {
				seen.push({ url, rows: JSON.parse(init.body).rows });
				return Promise.resolve({
					ok: true,
					headers: new Headers({ "content-type": "application/json" }),
					text: vi.fn().mockResolvedValue("{}"),
				});
			});

			await Promise.all([
				rpc("resize_pty", { sessionId: "a", rows: 10, cols: 40 }),
				rpc("resize_pty", { sessionId: "b", rows: 20, cols: 80 }),
			]);

			expect(seen.filter((s) => s.url.includes("/sessions/a/")).map((s) => s.rows)).toEqual([10]);
			expect(seen.filter((s) => s.url.includes("/sessions/b/")).map((s) => s.rows)).toEqual([20]);
		});

		it("keeps each session's keystrokes to itself", async () => {
			const { rpc } = await import("../transport");
			const seen: Array<{ url: string; data: string }> = [];
			globalThis.fetch = vi.fn().mockImplementation((url: string, init: { body: string }) => {
				seen.push({ url, data: JSON.parse(init.body).data });
				return Promise.resolve({
					ok: true,
					headers: new Headers({ "content-type": "application/json" }),
					text: vi.fn().mockResolvedValue("{}"),
				});
			});

			await Promise.all([
				rpc("write_pty", { sessionId: "a", data: "1" }),
				rpc("write_pty", { sessionId: "b", data: "2" }),
				rpc("write_pty", { sessionId: "a", data: "3" }),
			]);

			const a = seen.filter((s) => s.url.includes("/sessions/a/")).map((s) => s.data);
			const b = seen.filter((s) => s.url.includes("/sessions/b/")).map((s) => s.data);
			expect(a.join("")).toBe("13");
			expect(b.join("")).toBe("2");
		});

		it("sends body for POST requests", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: true,
				headers: new Headers({ "content-type": "application/json" }),
				text: vi.fn().mockResolvedValue('{"id":"sess-1"}'),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			await rpc("create_pty", { config: { rows: 24, cols: 80, shell: null, cwd: "/tmp" } });
			const fetchCall = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0];
			expect(fetchCall[1].body).toBeDefined();
			expect(JSON.parse(fetchCall[1].body)).toEqual({ rows: 24, cols: 80, shell: null, cwd: "/tmp" });
		});

		// Styled row chunks are packed bytes, ~141 KB each. Reading them with
		// `text()` (the pre-binary path) hands the decoder a mojibake string, and
		// `json()` throws. The content-type is what tells the two apart.
		it("reads an octet-stream response as an ArrayBuffer", async () => {
			const { rpc } = await import("../transport");

			const payload = new Uint8Array([26, 0, 200, 7]);
			const mockResponse = {
				ok: true,
				headers: new Headers({ "content-type": "application/octet-stream" }),
				arrayBuffer: vi.fn().mockResolvedValue(payload.buffer),
				json: vi.fn(),
				text: vi.fn(),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			const result = await rpc<ArrayBuffer>("terminal_styled_rows", {
				sessionId: "s1",
				start: 0,
				count: 64,
			});
			expect(result).toBeInstanceOf(ArrayBuffer);
			expect([...new Uint8Array(result)]).toEqual([26, 0, 200, 7]);
			expect(mockResponse.text).not.toHaveBeenCalled();
			expect(mockResponse.json).not.toHaveBeenCalled();
		});

		// A dead session answers with zero bytes. That is a valid empty chunk, and
		// the generic "empty response body" guard must not turn it into a throw.
		it("accepts an empty octet-stream body", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: true,
				headers: new Headers({ "content-type": "application/octet-stream" }),
				arrayBuffer: vi.fn().mockResolvedValue(new ArrayBuffer(0)),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			const result = await rpc<ArrayBuffer>("terminal_styled_rows", {
				sessionId: "s1",
				start: 0,
				count: 64,
			});
			expect(result.byteLength).toBe(0);
		});

		it("handles text response without content-type as JSON fallback", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: true,
				headers: new Headers({}),
				text: vi.fn().mockResolvedValue('{"result":"ok"}'),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			const result = await rpc("get_orchestrator_stats");
			expect(result).toEqual({ result: "ok" });
		});

		// Catches: a remote HTML page is returned as the home path, leaking its token in diagnostics.
		it("rejects HTML 200 for get_home_directory with a credential-free URL and content type", async () => {
			const { rpc } = await import("../transport");
			setRemoteBaseUrlLookup(() => "http://remote.test:9877");
			setRemoteTokenLookup(() => "private-token");
			globalThis.fetch = vi.fn().mockResolvedValue(
				new Response('<!doctype html><html lang="en">', {
					headers: { "content-type": "text/html; charset=utf-8" },
				}),
			);
			try {
				await expect(rpc("get_home_directory", {}, "verification")).rejects.toThrow(
					"RPC get_home_directory: expected JSON from http://remote.test:9877/system/home-directory, received text/html; charset=utf-8",
				);
			} finally {
				setRemoteBaseUrlLookup(() => undefined);
				setRemoteTokenLookup(() => undefined);
			}
		});

		// Catches: untyped non-JSON bodies silently become JSON RPC return values.
		it("rejects plain text without content-type for JSON RPCs", async () => {
			const { rpc } = await import("../transport");
			globalThis.fetch = vi.fn().mockResolvedValue({
				ok: true,
				headers: new Headers({}),
				text: vi.fn().mockResolvedValue("plain text response"),
			});
			await expect(rpc("get_orchestrator_stats")).rejects.toThrow(/expected JSON from .*received missing content-type/);
		});

		// Catches: enforcing JSON for every route breaks the plugin data string contract.
		it.each(["plain text response", "null", "123", '"quoted"'])("preserves plugin text data %s", async (body) => {
			const { rpc } = await import("../transport");
			globalThis.fetch = vi.fn().mockResolvedValue(
				new Response(body, {
					headers: { "content-type": "text/plain; charset=utf-8" },
				}),
			);
			expect(await rpc("read_plugin_data", { pluginId: "p", path: "content" })).toBe(body);
		});

		// Catches: the raw-text exception lets a proxy HTML page overwrite plugin content.
		it("rejects HTML for plugin data despite its text response contract", async () => {
			const { rpc } = await import("../transport");
			globalThis.fetch = vi.fn().mockResolvedValue(
				new Response("<!doctype html>", {
					headers: { "content-type": "text/html" },
				}),
			);
			await expect(rpc("read_plugin_data", { pluginId: "p", path: "content" })).rejects.toThrow(
				/expected JSON from .*received text\/html/,
			);
		});

		it("throws on non-ok response", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: false,
				status: 500,
				statusText: "Internal Server Error",
				text: vi.fn().mockResolvedValue("Something went wrong"),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			await expect(rpc("get_orchestrator_stats")).rejects.toThrow("RPC get_orchestrator_stats failed: 500");
		});

		it("applies transform when present", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: true,
				headers: new Headers({ "content-type": "application/json" }),
				text: vi.fn().mockResolvedValue('{"active_sessions":2,"max_sessions":5}'),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			const result = await rpc<boolean>("can_spawn_session");
			expect(result).toBe(true);
		});

		it("returns null on 404 when notFoundAsNull is set (read_plugin_data)", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: false,
				status: 404,
				statusText: "Not Found",
				text: vi.fn().mockResolvedValue(""),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			const result = await rpc<string | null>("read_plugin_data", { pluginId: "p", path: "missing-key" });
			expect(result).toBeNull();
		});

		it("still throws on non-404 errors even with notFoundAsNull", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: false,
				status: 400,
				statusText: "Bad Request",
				text: vi.fn().mockResolvedValue("bad path"),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			await expect(rpc("read_plugin_data", { pluginId: "p", path: "../escape" })).rejects.toThrow(
				"RPC read_plugin_data failed: 400",
			);
		});

		it("handles resp.text() failure in error path", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: false,
				status: 502,
				statusText: "Bad Gateway",
				text: vi.fn().mockRejectedValue(new Error("read failed")),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			await expect(rpc("get_orchestrator_stats")).rejects.toThrow("Bad Gateway");
		});
	});

	/**
	 * On desktop, `isTauri()` is true and there is no HTTP transport at all —
	 * `rpcImpl` always goes to Tauri `invoke()`. Before this fix, every single
	 * desktop RPC still ran `isIdempotentRpc()` -> `mapCommandToHttp()`: a
	 * COMMAND_TABLE lookup, and for any command not in that table (most of
	 * them — desktop-only commands aren't there) a thrown-and-caught Error.
	 * `vi.spyOn` on `mapCommandToHttp` was tried and does not observe internal
	 * same-module calls under this project's Vite/Vitest transform (verified:
	 * the spy reports zero calls even on the HTTP path where the call is
	 * unconditional), so the control-flow ordering is asserted directly from
	 * source instead — same rationale as `canvasTerminalMountGuards.test.ts`.
	 */
	describe("rpc() desktop short-circuit", () => {
		const source = ts.createSourceFile("http.ts", readRepoFile("src/transport/http.ts"), ts.ScriptTarget.Latest, true);
		const functionNamed = (name: string) =>
			findNodes(source, ts.isFunctionDeclaration).find((node) => node.name?.text === name);
		const callNamed = (node: ts.Node, name: string) =>
			findNodes(node, ts.isCallExpression).some(
				(call) => ts.isIdentifier(call.expression) && call.expression.text === name,
			);

		it("returns to rpcImpl before reaching isIdempotentRpc's HTTP-table lookup", () => {
			const rpc = functionNamed("rpc");
			expect(rpc?.body).toBeDefined();
			const shortCircuit = findNodes(rpc!.body!, ts.isIfStatement).find(
				(statement) =>
					findNodes(statement.expression, ts.isBinaryExpression).some(
						(expression) =>
							expression.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken &&
							findNodes(expression.left, ts.isPrefixUnaryExpression).some(
								(prefix) =>
									prefix.operator === ts.SyntaxKind.ExclamationToken &&
									ts.isIdentifier(prefix.operand) &&
									prefix.operand.text === "connectionId",
							) &&
							callNamed(expression.right, "isTauri"),
					) &&
					findNodes(statement.thenStatement, ts.isReturnStatement).some(
						(statement) => statement.expression && callNamed(statement.expression, "rpcImpl"),
					),
			);
			const idempotentCheck = findNodes(rpc!.body!, ts.isCallExpression).find(
				(call) => ts.isIdentifier(call.expression) && call.expression.text === "isIdempotentRpc",
			);
			expect(shortCircuit).toBeDefined();
			expect(idempotentCheck).toBeDefined();
			expect(shortCircuit!.getEnd()).toBeLessThan(idempotentCheck!.getStart(source));
		});

		it("caches the @tauri-apps/api/core import instead of re-importing it on every call", () => {
			const rpcImpl = functionNamed("rpcImpl");
			expect(rpcImpl?.body).toBeDefined();
			const cacheGuard = findNodes(rpcImpl!.body!, ts.isIfStatement).find(
				(statement) =>
					findNodes(statement.expression, ts.isPrefixUnaryExpression).some(
						(prefix) =>
							prefix.operator === ts.SyntaxKind.ExclamationToken &&
							ts.isIdentifier(prefix.operand) &&
							prefix.operand.text === "cachedTauriInvoke",
					) &&
					findNodes(statement.thenStatement, ts.isCallExpression).some(
						(call) =>
							call.expression.kind === ts.SyntaxKind.ImportKeyword &&
							call.arguments[0] &&
							ts.isStringLiteral(call.arguments[0]) &&
							call.arguments[0].text === "@tauri-apps/api/core",
					),
			);
			const cachedInvoke = findNodes(rpcImpl!.body!, ts.isCallExpression).find(
				(call) =>
					ts.isIdentifier(call.expression) &&
					call.expression.text === "cachedTauriInvoke" &&
					call.typeArguments?.[0]?.getText(source) === "T" &&
					call.arguments[0]?.getText(source) === "command" &&
					call.arguments[1]?.getText(source) === "args",
			);
			expect(cacheGuard).toBeDefined();
			expect(cachedInvoke).toBeDefined();
		});

		it("still resolves desktop RPCs via the cached invoke reference (regression)", async () => {
			const { rpc } = await import("../transport");
			const { mockInvoke } = await import("./mocks/tauri");
			mockInvoke.mockClear();
			mockInvoke.mockResolvedValueOnce({ enabled: true });

			const result = await rpc<{ enabled: boolean }>("get_dictation_status");

			expect(result).toEqual({ enabled: true });
			expect(mockInvoke).toHaveBeenCalledWith("get_dictation_status");
		});
	});

	describe("subscribePty()", () => {
		const originalTauri = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;

		beforeEach(() => {
			// Ensure non-Tauri mode for WebSocket tests
			delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
		});

		afterEach(() => {
			if (originalTauri !== undefined) {
				(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = originalTauri;
			} else {
				delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
			}
		});

		it("creates WebSocket in browser mode and subscribes to events", async () => {
			const { subscribePty } = await import("../transport");

			let wsInstance: {
				onopen: (() => void) | null;
				onmessage: ((event: { data: string }) => void) | null;
				onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null;
				onerror: ((e: unknown) => void) | null;
				close: () => void;
			};

			class MockWebSocket {
				onopen: (() => void) | null = null;
				onmessage: ((event: { data: string }) => void) | null = null;
				onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null = null;
				onerror: ((e: unknown) => void) | null = null;
				close = vi.fn();
				constructor() {
					wsInstance = this;
				}
			}

			const origWs = globalThis.WebSocket;
			globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;

			const onData = vi.fn();
			const onExit = vi.fn();

			const subscribePromise = subscribePty("sess-1", onData, onExit);

			// Trigger onopen to resolve
			wsInstance!.onopen!();
			const unsub = await subscribePromise;

			// Simulate data
			wsInstance!.onmessage!({ data: "hello" });
			expect(onData).toHaveBeenCalledWith("hello");

			// Simulate clean close
			wsInstance!.onclose!({ wasClean: true, code: 1000, reason: "" });
			expect(onExit).toHaveBeenCalled();

			// Unsubscribe closes WS
			unsub();
			expect(wsInstance!.close).toHaveBeenCalled();

			globalThis.WebSocket = origWs;
		});

		it("routes the WebSocket activity frame to onActivity, not onData", async () => {
			const { subscribePty } = await import("../transport");

			let wsInstance: {
				onopen: (() => void) | null;
				onmessage: ((event: { data: string }) => void) | null;
				onclose: (() => void) | null;
				onerror: unknown;
				close: () => void;
			};

			class MockWebSocket {
				onopen: (() => void) | null = null;
				onmessage: ((event: { data: string }) => void) | null = null;
				onclose: (() => void) | null = null;
				onerror: unknown = null;
				close = vi.fn();
				constructor() {
					wsInstance = this as never;
				}
			}

			const origWs = globalThis.WebSocket;
			globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;

			const onData = vi.fn();
			const onActivity = vi.fn();
			const subscribePromise = subscribePty("sess-1", onData, vi.fn(), { onActivity });
			wsInstance!.onopen!();
			const unsub = await subscribePromise;

			wsInstance!.onmessage!({ data: JSON.stringify({ type: "activity", session_id: "sess-1" }) });

			expect(onActivity).toHaveBeenCalledTimes(1);
			// The pulse carries no output; anything else would mean the browser is
			// still deriving activity from bytes while desktop is not.
			expect(onData).not.toHaveBeenCalled();

			unsub();
			globalThis.WebSocket = origWs;
		});

		it("routes the Tauri activity event to onActivity and subscribes to no output event", async () => {
			const { listen } = await import("@tauri-apps/api/event");
			const handlers = new Map<string, (event: { payload: unknown }) => void>();
			vi.mocked(listen).mockImplementation((async (name: string, handler: never) => {
				handlers.set(name, handler);
				return vi.fn();
			}) as never);

			(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
			const { subscribePty } = await import("../transport");

			const onData = vi.fn();
			const onActivity = vi.fn();
			const unsub = await subscribePty("sess-1", onData, vi.fn(), { onActivity });

			expect([...handlers.keys()]).toContain("pty-activity-sess-1");
			// The regression: a listener for an event Rust no longer emits.
			expect([...handlers.keys()].filter((name) => name.startsWith("pty-output"))).toEqual([]);

			handlers.get("pty-activity-sess-1")!({ payload: { session_id: "sess-1" } });
			expect(onActivity).toHaveBeenCalledTimes(1);
			expect(onData).not.toHaveBeenCalled();

			unsub();
			vi.mocked(listen).mockReset();
			vi.mocked(listen).mockResolvedValue(vi.fn());
		});

		it("logs warning and schedules reconnect on abnormal WebSocket close", async () => {
			const { subscribePty } = await import("../transport");

			let wsInstance: {
				onopen: (() => void) | null;
				onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null;
				onmessage: unknown;
				onerror: unknown;
				close: () => void;
			};

			class MockWebSocket {
				onopen: (() => void) | null = null;
				onmessage: unknown = null;
				onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null = null;
				onerror: unknown = null;
				close = vi.fn();
				constructor() {
					wsInstance = this;
				}
			}

			const origWs = globalThis.WebSocket;
			globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;

			const debugSpy = vi.fn();
			setTransportLogger({ debug: debugSpy, warn: vi.fn() });
			const onExit = vi.fn();

			const subscribePromise = subscribePty("sess-1", vi.fn(), onExit);
			wsInstance!.onopen!();
			const unsub = await subscribePromise;

			// Abnormal close triggers reconnect, not onExit
			wsInstance!.onclose!({ wasClean: false, code: 1006, reason: "" });
			expect(debugSpy).toHaveBeenCalledWith("network", expect.stringContaining("abnormally"));
			// onExit is NOT called on abnormal close — the transport schedules a reconnect instead
			expect(onExit).not.toHaveBeenCalled();

			unsub();
			setTransportLogger({ debug: vi.fn(), warn: vi.fn() });
			globalThis.WebSocket = origWs;
		});

		it("log mode reconnect resumes from the tracked cursor, not the mount offset", async () => {
			const { subscribePty } = await import("../transport");
			vi.useFakeTimers();

			const instances: {
				url: string;
				onopen: (() => void) | null;
				onmessage: ((e: { data: string }) => void) | null;
				onclose: ((e: { code: number; reason?: string }) => void) | null;
				onerror: unknown;
				close: () => void;
			}[] = [];

			class MockWebSocket {
				url: string;
				onopen: (() => void) | null = null;
				onmessage: ((e: { data: string }) => void) | null = null;
				onclose: ((e: { code: number; reason?: string }) => void) | null = null;
				onerror: unknown = null;
				close = vi.fn();
				constructor(url: string) {
					this.url = url;
					instances.push(this as never);
				}
			}

			const origWs = globalThis.WebSocket;
			globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;

			// Mount in log mode with the HTTP-fetched offset (50).
			const subscribePromise = subscribePty("sess-1", vi.fn(), vi.fn(), { format: "log", logOffset: 50 });
			instances[0].onopen?.();
			const unsub = await subscribePromise;
			expect(instances[0].url).toContain("offset=50");

			// Server advances the monotonic line cursor to 80 via a log frame.
			instances[0].onmessage?.({
				data: JSON.stringify({ type: "log", lines: [{ spans: [{ text: "x" }] }], offset: 50, total_lines: 80 }),
			});

			// Abnormal close → reconnect after backoff.
			instances[0].onclose?.({ code: 1006 });
			await vi.advanceTimersByTimeAsync(1000);

			// Reconnect must resume from the consumed cursor (80), NOT replay from mount (50).
			expect(instances.length).toBe(2);
			expect(instances[1].url).toContain("offset=80");
			expect(instances[1].url).not.toContain("offset=50");

			// Complete the reconnect handshake so the in-flight connect() promise settles.
			// (A real browser WebSocket fires onclose on close(); the mock does not, so an
			// unsettled connect() promise would otherwise leak past the test.)
			instances[1].onopen?.();

			unsub();
			globalThis.WebSocket = origWs;
			vi.useRealTimers();
		});

		/**
		 * A backgrounded PWA must stop draining the socket, and the naive way to
		 * do that ships two silent bugs: `ws.close()` looks exactly like a session
		 * exit to the close handler, and re-subscribing from scratch replays from
		 * the MOUNT offset because the live cursor is closure-private. So pause and
		 * resume live here, next to the cursor they have to preserve.
		 */
		describe("pause/resume", () => {
			interface FakeWs {
				url: string;
				onopen: (() => void) | null;
				onmessage: ((e: { data: string }) => void) | null;
				onclose: ((e: { code: number; reason?: string }) => void) | null;
				onerror: unknown;
				close: ReturnType<typeof vi.fn>;
			}

			let instances: FakeWs[] = [];
			let origWs: typeof WebSocket;

			beforeEach(() => {
				instances = [];
				class MockWebSocket {
					url: string;
					onopen: (() => void) | null = null;
					onmessage: ((e: { data: string }) => void) | null = null;
					onclose: ((e: { code: number; reason?: string }) => void) | null = null;
					onerror: unknown = null;
					close = vi.fn();
					constructor(url: string) {
						this.url = url;
						instances.push(this as unknown as FakeWs);
					}
				}
				origWs = globalThis.WebSocket;
				globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;
			});

			afterEach(() => {
				globalThis.WebSocket = origWs;
			});

			/** Mount in log mode at the given offset and settle the handshake. */
			async function mount(onExit = vi.fn(), opts: Record<string, unknown> = {}) {
				const { subscribePty } = await import("../transport");
				const onLogLines = vi.fn();
				const pending = subscribePty("sess-1", vi.fn(), onExit, {
					format: "log",
					logOffset: 50,
					onLogLines,
					...opts,
				});
				instances[0].onopen?.();
				return { sub: await pending, onExit, onLogLines };
			}

			it("closes the socket on pause without reporting a session exit", async () => {
				const { sub, onExit } = await mount();

				sub.pause();
				// A real socket answers close() with onclose, and 1000 is the code the
				// live handler reads as "the session is over".
				instances[0].onclose?.({ code: 1000 });

				expect(instances[0].close).toHaveBeenCalled();
				expect(onExit).not.toHaveBeenCalled();
			});

			it("delivers nothing that arrives after a pause", async () => {
				const { sub, onLogLines } = await mount();

				sub.pause();
				instances[0].onmessage?.({
					data: JSON.stringify({ type: "log", lines: [{ spans: [{ text: "late" }] }], total_lines: 99 }),
				});

				expect(onLogLines).not.toHaveBeenCalled();
			});

			it("resumes from the live cursor, not the mount offset", async () => {
				const { sub } = await mount();

				// Server advances the consumed line cursor to 80.
				instances[0].onmessage?.({
					data: JSON.stringify({ type: "log", lines: [{ spans: [{ text: "x" }] }], total_lines: 80 }),
				});
				sub.pause();
				instances[0].onclose?.({ code: 1000 });
				sub.resume();

				expect(instances.length).toBe(2);
				expect(instances[1].url).toContain("offset=80");
				expect(instances[1].url).not.toContain("offset=50");
				instances[1].onopen?.();
			});

			it("delivers again after a resume", async () => {
				const { sub, onLogLines } = await mount();

				sub.pause();
				sub.resume();
				instances[1].onopen?.();
				instances[1].onmessage?.({
					data: JSON.stringify({ type: "log", lines: [{ spans: [{ text: "back" }] }], total_lines: 81 }),
				});

				expect(onLogLines).toHaveBeenCalledTimes(1);
			});

			it("does not open a second socket when resume follows no pause", async () => {
				const { sub } = await mount();

				sub.resume();

				expect(instances.length).toBe(1);
			});

			it("does not reconnect while paused", async () => {
				vi.useFakeTimers();
				const { sub } = await mount();

				sub.pause();
				// An abnormal code is the reconnect trigger; a paused subscription
				// must not race the backoff timer against its own resume.
				instances[0].onclose?.({ code: 1006 });
				await vi.advanceTimersByTimeAsync(60_000);

				expect(instances.length).toBe(1);
				vi.useRealTimers();
			});

			it("cancels a pending reconnect when it is paused mid-backoff", async () => {
				vi.useFakeTimers();
				const { sub } = await mount();

				instances[0].onclose?.({ code: 1006 });
				sub.pause();
				await vi.advanceTimersByTimeAsync(60_000);

				expect(instances.length).toBe(1);
				vi.useRealTimers();
			});

			/**
			 * Pause suppresses DATA, never lifecycle. Swallowing the exit frame
			 * would leave the view showing a live session until the reconnect
			 * backoff finally gave up — ten attempts, roughly three minutes — and
			 * on desktop, where there is no socket to fail, forever.
			 */
			it("reports a session exit that arrives while paused", async () => {
				const { sub, onExit } = await mount();

				sub.pause();
				instances[0].onmessage?.({ data: JSON.stringify({ type: "exit" }) });

				expect(onExit).toHaveBeenCalledTimes(1);
			});

			it("does not reopen the socket after an exit seen while paused", async () => {
				const { sub } = await mount();

				sub.pause();
				instances[0].onmessage?.({ data: JSON.stringify({ type: "exit" }) });
				sub.resume();

				expect(instances.length).toBe(1);
			});

			it("still reports the exit on desktop while paused", async () => {
				const { listen } = await import("@tauri-apps/api/event");
				const handlers = new Map<string, (event: { payload: unknown }) => void>();
				vi.mocked(listen).mockImplementation((async (name: string, handler: never) => {
					handlers.set(name, handler);
					return vi.fn();
				}) as never);
				(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};

				const { subscribePty } = await import("../transport");
				const onExit = vi.fn();
				const onActivity = vi.fn();
				const sub = await subscribePty("sess-1", vi.fn(), onExit, { onActivity });

				sub.pause();
				handlers.get("pty-activity-sess-1")?.({ payload: {} });
				handlers.get("pty-exit-sess-1")?.({ payload: {} });

				expect(onActivity).not.toHaveBeenCalled();
				expect(onExit).toHaveBeenCalledTimes(1);

				sub();
				delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
				vi.mocked(listen).mockReset();
				vi.mocked(listen).mockResolvedValue(vi.fn());
			});

			/**
			 * `close()` is asynchronous: the socket a pause dropped can still fire
			 * its handlers after the resume that replaced it. A `paused` flag alone
			 * cannot see that — by then the flag is false again — so the guard has
			 * to be socket identity, not subscription state.
			 */
			it("ignores data from the socket it already paused, even after resume", async () => {
				const { sub, onLogLines } = await mount();

				instances[0].onmessage?.({
					data: JSON.stringify({ type: "log", lines: [{ spans: [{ text: "a" }] }], total_lines: 80 }),
				});
				sub.pause();
				sub.resume();
				instances[1].onopen?.();
				onLogLines.mockClear();

				// The dropped socket flushes what it had buffered, late.
				instances[0].onmessage?.({
					data: JSON.stringify({ type: "log", lines: [{ spans: [{ text: "stale" }] }], total_lines: 200 }),
				});

				expect(onLogLines).not.toHaveBeenCalled();
			});

			it("does not let a stale socket's cursor rewrite the live one", async () => {
				const { sub } = await mount();

				instances[0].onmessage?.({
					data: JSON.stringify({ type: "log", lines: [{ spans: [{ text: "a" }] }], total_lines: 80 }),
				});
				sub.pause();
				sub.resume();
				instances[1].onopen?.();
				// A late frame from the dropped socket claiming a cursor we never
				// consumed. If it were tracked, the NEXT reconnect would resume past
				// lines the user never saw.
				instances[0].onmessage?.({
					data: JSON.stringify({ type: "log", lines: [{ spans: [{ text: "stale" }] }], total_lines: 999 }),
				});

				sub.pause();
				sub.resume();
				instances[2].onopen?.();

				expect(instances[2].url).toContain("offset=80");
				expect(instances[2].url).not.toContain("offset=999");
			});

			it("does not read a stale socket's close as a session exit", async () => {
				const { sub, onExit } = await mount();

				sub.pause();
				sub.resume();
				instances[1].onopen?.();
				instances[0].onclose?.({ code: 1000 });

				expect(onExit).not.toHaveBeenCalled();
			});

			it("does not reconnect on a stale socket's abnormal close", async () => {
				vi.useFakeTimers();
				const { sub } = await mount();

				sub.pause();
				sub.resume();
				instances[1].onopen?.();
				instances[0].onclose?.({ code: 1006 });
				await vi.advanceTimersByTimeAsync(60_000);

				expect(instances.length).toBe(2);
				vi.useRealTimers();
			});

			/**
			 * The window a pause can land in is not just "connected": a backoff
			 * timer may already have called connect(), which assigns the socket
			 * synchronously but resolves much later. That in-flight attempt has to
			 * be superseded, or its late failure schedules a reconnect of its own
			 * and the session ends up on two live sockets at once.
			 */
			it("does not open a parallel socket when a pause lands mid-connect", async () => {
				vi.useFakeTimers();
				const { sub } = await mount();

				instances[0].onclose?.({ code: 1006 });
				await vi.advanceTimersByTimeAsync(1000);
				expect(instances.length).toBe(2); // the backoff attempt, still opening

				sub.pause();
				sub.resume();
				instances[2].onopen?.();
				// The superseded attempt now reports its failure, late.
				instances[1].onclose?.({ code: 1006 });
				await vi.advanceTimersByTimeAsync(60_000);

				expect(instances.length).toBe(3);
				vi.useRealTimers();
			});

			it("closes an attempt that opens after it was superseded", async () => {
				vi.useFakeTimers();
				const { sub } = await mount();

				instances[0].onclose?.({ code: 1006 });
				await vi.advanceTimersByTimeAsync(1000);
				sub.pause();
				sub.resume();
				instances[2].onopen?.();
				instances[1].close.mockClear();
				// The superseded attempt completes its handshake anyway. Left open it
				// would stream a second copy of the session at the server's expense.
				instances[1].onopen?.();

				expect(instances[1].close).toHaveBeenCalled();
				vi.useRealTimers();
			});

			it("gives up for good once the retry budget is spent", async () => {
				vi.useFakeTimers();
				const { sub, onExit } = await mount();

				// Ten failures is MAX_RETRIES; the eleventh close is the one that
				// finds the budget spent.
				for (let i = 0; i < 11; i++) {
					instances.at(-1)!.onclose?.({ code: 1006 });
					await vi.advanceTimersByTimeAsync(60_000);
				}
				expect(onExit).toHaveBeenCalledTimes(1);

				const opened = instances.length;
				sub.pause();
				sub.resume();
				await vi.advanceTimersByTimeAsync(60_000);

				// A dead subscription stays dead. Refilling the budget on every
				// hide/show would let a session that is gone retry forever, and
				// report its exit again each time the budget ran out.
				expect(instances.length).toBe(opened);
				expect(onExit).toHaveBeenCalledTimes(1);
				vi.useRealTimers();
			});

			/**
			 * Exit frames and retry exhaustion are terminal. A clean close is the
			 * same news arriving by a third route, so it has to be terminal too —
			 * otherwise the session is declared exited to the consumer while the
			 * subscription still believes it can be reopened.
			 */
			it("treats a clean close as terminal, so hide/show cannot reopen it", async () => {
				const { sub, onExit } = await mount();

				instances[0].onclose?.({ code: 1000 });
				expect(onExit).toHaveBeenCalledTimes(1);

				sub.pause();
				sub.resume();

				expect(instances.length).toBe(1);
				expect(onExit).toHaveBeenCalledTimes(1);
			});

			it("reports the reconnection that a pause interrupted", async () => {
				const onReconnecting = vi.fn();
				const onReconnected = vi.fn();
				const { sub } = await mount(vi.fn(), { onReconnecting, onReconnected });

				instances[0].onclose?.({ code: 1006 });
				expect(onReconnecting).toHaveBeenCalledTimes(1);

				// The user backgrounds the page mid-backoff and comes back. The
				// socket is healthy again, so a consumer told "reconnecting" must be
				// told it finished — otherwise its banner never comes down.
				sub.pause();
				sub.resume();
				instances[1].onopen?.();
				await Promise.resolve();

				expect(onReconnected).toHaveBeenCalledTimes(1);
			});

			it("does not announce a reconnection for a resume that never lost one", async () => {
				const onReconnected = vi.fn();
				const { sub } = await mount(vi.fn(), { onReconnected });

				sub.pause();
				sub.resume();
				instances[1].onopen?.();
				await Promise.resolve();

				expect(onReconnected).not.toHaveBeenCalled();
			});

			/**
			 * The reconnect callbacks sit on the same promise chain as `connect()`.
			 * A consumer that throws inside `onReconnected` would land in the
			 * rejection handler and be read as a failed connection — announcing a
			 * reconnect that never broke and opening a second socket alongside the
			 * healthy one. A consumer's bug must not become a transport failure.
			 */
			it("does not read a throwing onReconnected as a failed connection", async () => {
				const onReconnecting = vi.fn();
				const onReconnected = vi.fn(() => {
					throw new Error("consumer blew up");
				});
				const { sub } = await mount(vi.fn(), { onReconnecting, onReconnected });

				instances[0].onclose?.({ code: 1006 });
				sub.pause();
				sub.resume();
				instances[1].onopen?.();
				await Promise.resolve();
				await Promise.resolve();

				expect(onReconnected).toHaveBeenCalledTimes(1);
				// One announcement, from the abnormal close that really happened.
				expect(onReconnecting).toHaveBeenCalledTimes(1);
			});

			it("stays disposed when pause or resume arrive after unsubscribe", async () => {
				const { sub } = await mount();

				sub();
				sub.pause();
				sub.resume();

				expect(instances.length).toBe(1);
			});
		});
	});
});

/**
 * The gate between "this call belongs to a remote machine" and "this call can
 * actually be sent there".
 *
 * `resolveOwningConnection` answers the first question from the call's
 * arguments; this function answers the second from the command name. Only a
 * command with an HTTP mapping can be routed — a host-only one has no
 * request/response route, so routing it would turn a working local call into a
 * throw.
 */
describe("owningConnectionFor", () => {
	const SESSION = "sess-on-tycho";
	const REPO = "/Volumes/work/api";
	const TYCHO = "conn-tycho";

	beforeEach(() => {
		setSessionConnectionLookup((id) => (id === SESSION ? TYCHO : undefined));
		setRepoConnectionLookup((path) => (path.startsWith(REPO) ? TYCHO : undefined));
	});

	afterEach(() => {
		setSessionConnectionLookup(() => undefined);
		setRepoConnectionLookup(() => undefined);
	});

	it("routes a mapped command to the machine its arguments name", () => {
		expect(owningConnectionFor("write_pty", { sessionId: SESSION, data: "ls\r" })).toBe(TYCHO);
		expect(owningConnectionFor("get_branches_detail", { repoPath: REPO })).toBe(TYCHO);
	});

	it("leaves a call about nothing remote alone", () => {
		expect(owningConnectionFor("write_pty", { sessionId: "local-session", data: "ls\r" })).toBeUndefined();
		expect(owningConnectionFor("get_branches_detail", { repoPath: "/local/repo" })).toBeUndefined();
	});

	// A host-only command keeps running here rather than being routed into a
	// throw — but silently would leave a remote repo looking local, so it says
	// so. Once per command: it is reached once per call, and a per-call warning
	// would be one line per keystroke for a command like `start_native_drag`.
	it("warns exactly once for a host-only command that names a remote machine", () => {
		const warn = vi.fn();
		setTransportLogger({ debug: vi.fn(), warn });
		// Chosen off the real set rather than invented, so the test cannot pass
		// against a command that is no longer host-only.
		const hostOnly = [...INTENTIONALLY_UNMAPPED].find((command) => command === "start_native_drag");
		expect(hostOnly).toBe("start_native_drag");

		expect(owningConnectionFor("start_native_drag", { repoPath: REPO })).toBeUndefined();
		expect(owningConnectionFor("start_native_drag", { sessionId: SESSION })).toBeUndefined();

		expect(warn).toHaveBeenCalledTimes(1);
		expect(warn.mock.calls[0][1]).toContain("start_native_drag");
		expect(warn.mock.calls[0][2]).toMatchObject({ command: "start_native_drag", connectionId: TYCHO });
	});
});

// A private native bootstrap must not become a public nonce-issuing endpoint.
describe("private secret transport boundary", () => {
	it("keeps native nonce bootstrap unmapped and submits the same envelope over HTTP", () => {
		expect(INTENTIONALLY_UNMAPPED.has("secret_form_bootstrap")).toBe(true);
		const request = mapCommandToHttp("secret_form_submit", {
			submission: { nonce: "one-time", status: "declined", values: {}, template: null },
		});
		expect(request).toEqual({
			method: "POST",
			path: "/secrets/forms/submit",
			body: { nonce: "one-time", status: "declined", values: {}, template: null },
		});
	});
});

// Catches: phone setup drops the action or reaches a different config surface than IPC.
describe("Telegram Settings transport", () => {
	it("maps reads and writes to the same guarded settings route", () => {
		expect(mapCommandToHttp("telegram_settings", {})).toEqual({ method: "GET", path: "/config/telegram" });
		const change = { action: "add_chat", chat_id: "123" };
		expect(mapCommandToHttp("telegram_setup", { change })).toEqual({
			method: "PUT",
			path: "/config/telegram",
			body: { change },
		});
	});
});

// Add-account must never select the default-login HTTP handler.
it("add-account polling uses a separate HTTP endpoint", () => {
	const add = mapCommandToHttp("github_poll_add_account", { deviceCode: "additional" });
	const login = mapCommandToHttp("github_poll_login", { deviceCode: "default" });
	expect(add).toEqual({ method: "POST", path: "/github/accounts/poll", body: { deviceCode: "additional" } });
	expect(login).toEqual({ method: "POST", path: "/github/auth/poll", body: { deviceCode: "default" } });
});
