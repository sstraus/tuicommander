import { afterEach, describe, expect, it, vi } from "vitest";
import { HttpRpcError, rpc } from "../transport";

describe("HTTP RPC error detail", () => {
	// Catches: a JSON error envelope is exposed as raw HTTP metadata in the UI.
	it("extracts the server's text error", () => {
		const error = new HttpRpcError("create_session", 500, '{"error":"Working directory does not exist"}');
		expect(error.detail).toBe("Working directory does not exist");
	});

	// Catches: malformed or non-string error payloads are rendered as empty or object text.
	it.each(["plain failure", "null", '{"error":true}', '{"other":"permission denied"}'])(
		"keeps the response body when it has no string error: %s",
		(body) => {
			expect(new HttpRpcError("list_directory", 500, body).detail).toBe(body);
		},
	);
});

// Catches: a caller-owned health deadline is dropped or shares cancellation with another request.
it("keeps abortable health reads independent and passes their signals to fetch", async () => {
	vi.stubGlobal("__TAURI_SHIM__", true);
	const controllers = [new AbortController(), new AbortController()];
	const fetchStub = vi.fn(
		(_url: string, init?: RequestInit) =>
			new Promise<Response>((_resolve, reject) => {
				init?.signal?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), {
					once: true,
				});
			}),
	);
	vi.stubGlobal("fetch", fetchStub);
	const first = rpc("list_active_sessions", {}, undefined, controllers[0].signal);
	const second = rpc("list_active_sessions", {}, undefined, controllers[1].signal);
	const failures = Promise.allSettled([first, second]);
	expect(fetchStub).toHaveBeenCalledTimes(2);
	controllers.forEach((controller) => controller.abort());
	expect((await failures).map((result) => result.status)).toEqual(["rejected", "rejected"]);
});
afterEach(() => vi.unstubAllGlobals());
